//! The tools, and what each one does with its arguments.

use std::path::{Path, PathBuf};

use serde_json::{json, Map, Value};

use mosna_config::{get_config, write_config, RawConfig};

use crate::assortativity::{compare_assortativity, Tolerance};
use crate::backend::{Backend, Locations, Step};
use crate::check::config::{check_config, network_location, nodes_spec, Sections};
use crate::check::naming::Naming;
use crate::check::network::check_network;
use crate::check::nodes::{check_nodes, NodesSpec};
use crate::compare::{compare_networks, compare_partitions};
use crate::jobs::Jobs;
use crate::report::{Affects, Report, Severity};

/// Findings returned by default: enough to act on, few enough not to flood
/// the context of the model reading them.
const DEFAULT_MAX_FINDINGS: usize = 40;

pub struct Server {
    pub locations: Locations,
    pub jobs: Jobs,
    /// Directories no run may write into (`MOSNA_MCP_PROTECTED`).
    pub protected: Vec<PathBuf>,
}

impl Server {
    pub fn from_env() -> Self {
        Self {
            locations: Locations::from_env(),
            jobs: Jobs::default(),
            protected: std::env::var_os("MOSNA_MCP_PROTECTED")
                .map(|v| std::env::split_paths(&v).collect())
                .unwrap_or_default(),
        }
    }

    pub fn call(&self, tool: &str, args: &Value) -> anyhow::Result<Value> {
        let args = Args(args);
        match tool {
            "check_config" => self.check_config(&args),
            "check_nodes" => self.check_nodes(&args),
            "check_network" => self.check_network(&args),
            "run_step" => self.run_step(&args),
            "job_status" => match args.opt_str("job_id")? {
                Some(id) => self.jobs.status(id),
                None => Ok(self.jobs.list()),
            },
            "cancel_job" => self.jobs.cancel(args.str("job_id")?),
            "compare_networks" => {
                let naming = |ext: &str| -> anyhow::Result<Naming> {
                    Ok(Naming {
                        extension: args.opt_str(ext)?.unwrap_or("parquet").to_string(),
                        patient_column: args.str("patient_column")?.to_string(),
                        sample_column: args.opt_str("sample_column")?.map(str::to_string),
                    })
                };
                let (a, b) = (naming("extension_a")?, naming("extension_b")?);
                compare_networks(
                    (Path::new(args.str("dir_a")?), &a),
                    (Path::new(args.str("dir_b")?), &b),
                )
            }
            "compare_assortativity" => self.compare_assortativity(&args),
            "backends" => Ok(self.locations.describe()),
            "compare_partitions" => {
                let naming = Naming {
                    extension: args.opt_str("extension")?.unwrap_or("parquet").to_string(),
                    patient_column: args.str("patient_column")?.to_string(),
                    sample_column: args.opt_str("sample_column")?.map(str::to_string),
                };
                compare_partitions(
                    (
                        Path::new(args.str("dir_a")?),
                        &naming,
                        args.str("column_a")?,
                    ),
                    (
                        Path::new(args.str("dir_b")?),
                        &naming,
                        args.str("column_b")?,
                    ),
                    args.opt_bool("per_sample")?.unwrap_or(false),
                )
            }
            other => anyhow::bail!("unknown tool `{other}`"),
        }
    }

    fn compare_assortativity(&self, args: &Args) -> anyhow::Result<Value> {
        let from_config = match args.opt_str("config")? {
            Some(path) => {
                let config = get_config(path)?;
                Some(mosna_config::AssortativityConfig::from_raw(&config)?.effective_shuffles())
            }
            None => None,
        };
        let both = args.opt_usize("n_shuffle")?.or(from_config);
        let side = |key: &str| -> anyhow::Result<usize> {
            args.opt_usize(key)?.or(both).ok_or_else(|| {
                anyhow::anyhow!(
                    "the tolerance depends on the number of permutations: pass `n_shuffle` \
                     (or `n_shuffle_a` and `n_shuffle_b`), or the `config` of the runs"
                )
            })
        };
        let tolerance = Tolerance {
            sigmas: args.opt_f64("sigmas")?.unwrap_or(4.0),
            n_shuffle: (side("n_shuffle_a")?, side("n_shuffle_b")?),
            ..Tolerance::default()
        };
        if tolerance.sigmas <= 0.0 {
            anyhow::bail!("`sigmas` must be positive");
        }
        compare_assortativity(
            Path::new(args.str("a")?),
            Path::new(args.str("b")?),
            &tolerance,
        )
    }

    fn check_config(&self, args: &Args) -> anyhow::Result<Value> {
        let config = get_config(args.str("config")?)?;
        let (report, _) = check_config(&config);
        Ok(report.sorted().to_json(args.max_findings()?))
    }

    fn check_nodes(&self, args: &Args) -> anyhow::Result<Value> {
        let working_dir = args.working_dir()?;
        let base = match args.opt_str("config")? {
            Some(path) => {
                let config = get_config(path)?;
                let tysserand = mosna_config::TysserandConfig::from_raw(&config)?;
                Some((nodes_spec(&tysserand), tysserand.nodes_directory))
            }
            None => None,
        };

        let pick = |key: &str, from_base: Option<&str>| -> anyhow::Result<String> {
            match (args.opt_str(key)?, from_base) {
                (Some(value), _) => Ok(value.to_string()),
                (None, Some(value)) => Ok(value.to_string()),
                (None, None) => anyhow::bail!("`{key}` is required when no `config` is given"),
            }
        };
        let base_spec = base.as_ref().map(|(spec, _)| spec);
        let spec = NodesSpec {
            naming: Naming {
                extension: pick("extension", base_spec.map(|s| s.naming.extension.as_str()))?,
                patient_column: pick(
                    "patient_column",
                    base_spec.map(|s| s.naming.patient_column.as_str()),
                )?,
                sample_column: match args.opt_str("sample_column")? {
                    Some("") => None,
                    Some(value) => Some(value.to_string()),
                    None => base_spec.and_then(|s| s.naming.sample_column.clone()),
                },
            },
            x_column: pick("x_column", base_spec.map(|s| s.x_column.as_str()))?,
            y_column: pick("y_column", base_spec.map(|s| s.y_column.as_str()))?,
            phenotype_column: pick(
                "phenotype_column",
                base_spec.map(|s| s.phenotype_column.as_str()),
            )?,
            min_neighbors: args
                .opt_usize("min_neighbors")?
                .or(base_spec.map(|s| s.min_neighbors))
                .unwrap_or(3),
        };
        let nodes_dir = working_dir.join(pick(
            "nodes_dir",
            base.as_ref().map(|(_, dir)| dir.as_str()),
        )?);

        let (report, samples) = check_nodes(&nodes_dir, &spec);
        Ok(with_samples(
            report.sorted().to_json(args.max_findings()?),
            &nodes_dir,
            samples,
        ))
    }

    fn check_network(&self, args: &Args) -> anyhow::Result<Value> {
        let working_dir = args.working_dir()?;
        let (dir, naming, phenotype) = match args.opt_str("config")? {
            Some(path) => {
                let config = get_config(path)?;
                let step = Step::parse(args.opt_str("step")?.unwrap_or("assortativity"))?;
                network_input(&config, step, &working_dir)?
            }
            None => (
                working_dir.join(args.str("network_dir")?),
                Naming {
                    extension: args.opt_str("extension")?.unwrap_or("parquet").to_string(),
                    patient_column: args.str("patient_column")?.to_string(),
                    sample_column: args.opt_str("sample_column")?.map(str::to_string),
                },
                None,
            ),
        };
        let phenotype = args
            .opt_str("phenotype_column")?
            .map(str::to_string)
            .or(phenotype);

        let (report, samples) = check_network(&dir, &naming, phenotype.as_deref());
        Ok(with_samples(
            report.sorted().to_json(args.max_findings()?),
            &dir,
            samples,
        ))
    }

    fn run_step(&self, args: &Args) -> anyhow::Result<Value> {
        let step = Step::parse(args.str("step")?)?;
        let backend = Backend::parse(args.str("backend")?)?;
        let working_dir = args.working_dir()?;
        self.refuse_protected(&working_dir)?;
        std::fs::create_dir_all(&working_dir)?;

        let source = PathBuf::from(args.str("config")?);
        let mut config = get_config(&source)?;
        let config_path = match args.0.get("overrides") {
            Some(Value::Object(overrides)) if !overrides.is_empty() => {
                apply_overrides(&mut config, overrides)?;
                let dir = working_dir.join(".mosna-mcp");
                std::fs::create_dir_all(&dir)?;
                let stamp = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis())
                    .unwrap_or_default();
                let path = dir.join(format!("config-{}-{stamp}.yaml", step.as_str()));
                write_config(&config, &path)?;
                path
            }
            _ => source,
        };

        let preflight = if args.opt_bool("skip_preflight")?.unwrap_or(false) {
            None
        } else {
            let report = preflight(&config, step, &working_dir);
            let blocking = report.findings.iter().any(|f| {
                f.severity == Severity::Error
                    && (f.affects == Affects::Both || f.affects.as_str() == backend.as_str())
            });
            let summary = report.sorted().to_json(args.max_findings()?);
            if blocking {
                return Ok(json!({
                    "started": false,
                    "reason": "the inputs would make this step fail or give a wrong result; \
                               fix them, or pass skip_preflight to run anyway",
                    "preflight": summary,
                }));
            }
            Some(summary)
        };

        let command = self.locations.command(
            backend,
            step,
            &config_path,
            &working_dir,
            args.opt_str("figure_formats")?,
            args.opt_str("results_in")?,
        )?;
        let description = json!({
            "step": step.as_str(),
            "backend": backend.as_str(),
            "config": config_path,
            "working_dir": working_dir,
        });
        let id = self.jobs.spawn(command, description)?;
        Ok(json!({
            "started": true,
            "job_id": id,
            "config_used": config_path,
            "deterministic": backend.deterministic(step),
            "note": if backend.deterministic(step) {
                "rerunning this step on the same inputs gives the same result"
            } else {
                "MOSNA_GUI's niche step is unseeded: a rerun gives a different partition"
            },
            "preflight": preflight,
        }))
    }

    fn refuse_protected(&self, working_dir: &Path) -> anyhow::Result<()> {
        let target = canonical(working_dir);
        for protected in &self.protected {
            if target.starts_with(canonical(protected)) {
                anyhow::bail!(
                    "{} is under {}, which is protected (MOSNA_MCP_PROTECTED): \
                     it holds results that cannot be produced again",
                    working_dir.display(),
                    protected.display()
                );
            }
        }
        Ok(())
    }
}

/// Every check that applies before `step` runs.
fn preflight(config: &RawConfig, step: Step, working_dir: &Path) -> Report {
    let (full, sections) = check_config(config);

    // Only the step's own section: an unfinished niche section is no reason
    // to refuse building the networks. Every config finding names its section.
    let prefix = format!("[{}]", step.section());
    let mut report = Report {
        findings: full
            .findings
            .into_iter()
            .filter(|f| f.message.starts_with(&prefix))
            .collect(),
    };
    match step {
        Step::Network => {
            if let Some(tysserand) = &sections.tysserand {
                let dir = working_dir.join(&tysserand.nodes_directory);
                report.extend(check_nodes(&dir, &nodes_spec(tysserand)).0);
            }
        }
        Step::Assortativity | Step::Niches => {
            if let Ok((dir, naming, phenotype)) =
                network_from_sections(&sections, step, working_dir)
            {
                report.extend(check_network(&dir, &naming, phenotype.as_deref()).0);
            }
        }
    }
    report
}

fn network_input(
    config: &RawConfig,
    step: Step,
    working_dir: &Path,
) -> anyhow::Result<(PathBuf, Naming, Option<String>)> {
    let (report, sections) = check_config(config);
    network_from_sections(&sections, step, working_dir).map_err(|error| {
        let detail: Vec<String> = report.findings.iter().map(|f| f.message.clone()).collect();
        anyhow::anyhow!("{error}; {}", detail.join("; "))
    })
}

fn network_from_sections(
    sections: &Sections,
    step: Step,
    working_dir: &Path,
) -> anyhow::Result<(PathBuf, Naming, Option<String>)> {
    match step {
        Step::Assortativity => {
            let a = sections
                .assortativity
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("the Assortativity section is not usable"))?;
            let (dir, naming) = network_location(
                &a.network_directory,
                &a.extension,
                &a.patient_column,
                a.sample_column.as_deref(),
                working_dir,
            );
            Ok((dir, naming, Some(a.phenotype_column.clone())))
        }
        Step::Niches => {
            let n = sections
                .niches
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("the Niche Analysis section is not usable"))?;
            let (dir, naming) = network_location(
                &n.network_directory,
                &n.extension,
                &n.patient_column,
                n.sample_column.as_deref(),
                working_dir,
            );
            Ok((dir, naming, n.phenotype_column.clone()))
        }
        Step::Network => anyhow::bail!("step network reads nodes, not networks: use check_nodes"),
    }
}

/// Merge `{section: {key: value}}` into the configuration, recursing into
/// sub-sections such as `Aggregated nodes`.
pub fn apply_overrides(
    config: &mut RawConfig,
    overrides: &Map<String, Value>,
) -> anyhow::Result<()> {
    fn merge(target: &mut serde_yaml::Value, patch: &Map<String, Value>) -> anyhow::Result<()> {
        if !target.is_mapping() {
            *target = serde_yaml::Value::Mapping(Default::default());
        }
        let mapping = target.as_mapping_mut().expect("just made a mapping");
        for (key, value) in patch {
            let key_value = serde_yaml::Value::String(key.clone());
            match value {
                Value::Object(inner) => {
                    let slot = mapping
                        .entry(key_value)
                        .or_insert(serde_yaml::Value::Mapping(Default::default()));
                    merge(slot, inner)?;
                }
                other => {
                    mapping.insert(key_value, serde_yaml::to_value(other)?);
                }
            }
        }
        Ok(())
    }

    for (section, patch) in overrides {
        let Value::Object(patch) = patch else {
            anyhow::bail!("overrides.{section} must be an object of keys to values");
        };
        // A misspelt section would otherwise be created, and ignored by both
        // implementations without a word.
        if config.section(section).is_err() {
            anyhow::bail!(
                "no section `{section}` in the configuration; it has {:?}",
                config.section_names()
            );
        }
        merge(config.section_mut(section), patch)?;
    }
    Ok(())
}

fn with_samples(mut summary: Value, dir: &Path, samples: Value) -> Value {
    let count = samples.as_array().map(Vec::len).unwrap_or(0);
    summary["directory"] = json!(dir);
    summary["sample_count"] = json!(count);
    summary["samples"] = match samples {
        Value::Array(rows) => Value::Array(rows.into_iter().take(50).collect()),
        other => other,
    };
    summary
}

fn canonical(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

/// Typed access to a tool's arguments, with errors naming the argument.
struct Args<'a>(&'a Value);

impl Args<'_> {
    fn opt_str(&self, key: &str) -> anyhow::Result<Option<&str>> {
        match self.0.get(key) {
            None | Some(Value::Null) => Ok(None),
            Some(Value::String(s)) => Ok(Some(s)),
            Some(other) => anyhow::bail!("`{key}` must be a string, got {other}"),
        }
    }

    fn str(&self, key: &str) -> anyhow::Result<&str> {
        self.opt_str(key)?
            .ok_or_else(|| anyhow::anyhow!("`{key}` is required"))
    }

    fn opt_usize(&self, key: &str) -> anyhow::Result<Option<usize>> {
        match self.0.get(key) {
            None | Some(Value::Null) => Ok(None),
            Some(value) => value
                .as_u64()
                .map(|v| Some(v as usize))
                .ok_or_else(|| anyhow::anyhow!("`{key}` must be a non-negative integer")),
        }
    }

    fn opt_f64(&self, key: &str) -> anyhow::Result<Option<f64>> {
        match self.0.get(key) {
            None | Some(Value::Null) => Ok(None),
            Some(value) => value
                .as_f64()
                .map(Some)
                .ok_or_else(|| anyhow::anyhow!("`{key}` must be a number")),
        }
    }

    fn opt_bool(&self, key: &str) -> anyhow::Result<Option<bool>> {
        match self.0.get(key) {
            None | Some(Value::Null) => Ok(None),
            Some(Value::Bool(b)) => Ok(Some(*b)),
            Some(other) => anyhow::bail!("`{key}` must be true or false, got {other}"),
        }
    }

    fn working_dir(&self) -> anyhow::Result<PathBuf> {
        match self.opt_str("working_dir")? {
            Some(dir) => Ok(PathBuf::from(dir)),
            None => Ok(std::env::current_dir()?),
        }
    }

    fn max_findings(&self) -> anyhow::Result<usize> {
        Ok(self
            .opt_usize("max_findings")?
            .unwrap_or(DEFAULT_MAX_FINDINGS))
    }
}
