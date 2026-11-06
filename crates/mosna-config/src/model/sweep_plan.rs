//! The plan of a sensitivity analysis, as a file a person reads and a command
//! runs.
//!
//! # One file, two readers
//!
//! The interface writes it from the grid the sensitivity screen holds. Its
//! comments say, in words, everything the analysis is about to do — which runs,
//! in which order, which stage each one recomputes, where the results go — so
//! the plan can be checked before hours of computation are spent on it. Below
//! the comments is the same plan as YAML, which is what
//!
//! ```text
//! mosna sensitivity-analysis --log sensitivity_plan.log
//! ```
//!
//! reads and runs. Keeping both in one file is what guarantees they agree: the
//! explanation is generated from the data it sits above, never written apart
//! from it.
//!
//! # Self-contained on purpose
//!
//! The whole configuration every run starts from is copied into the file
//! (`base_configuration`). A plan written today and run tonight from a terminal
//! therefore runs what was reviewed, even if `configuration.yaml` has been
//! edited in between.

use std::path::{Path, PathBuf};

use serde_yaml::{Mapping, Value};

use crate::error::{ConfigError, Result};
use crate::model::raw::RawConfig;
use crate::section;

/// The `format` key of a plan this version writes and reads.
pub const FORMAT: &str = "mosna-sensitivity-plan/1";

/// The file name the interface gives a plan, beside the sweep's results.
pub const PLAN_FILE: &str = "sensitivity_plan.log";

/// One run of the grid: the keys written into the swept sub-section before it.
#[derive(Debug, Clone, PartialEq)]
pub struct PlannedRun {
    /// `(key, value)`, in the order the stages run.
    pub settings: Vec<(String, Value)>,
}

impl PlannedRun {
    /// The value of one key, if this run sets it.
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.settings
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value)
    }
}

/// What a run computes, given the run before it.
///
/// The three stages of a niche analysis are cached separately, each a pure
/// function of the stage above it, so a run only computes from the first stage
/// whose settings differ from the previous run's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Recomputes {
    /// Aggregation, projection and clustering.
    Everything,
    /// Projection and clustering; the aggregation is read from the cache.
    FromReduction,
    /// The clustering alone.
    Clustering,
}

impl Recomputes {
    /// The sentence the plan writes above the run.
    pub fn describe(self) -> &'static str {
        match self {
            Recomputes::Everything => "computes the aggregation, the projection and the clustering",
            Recomputes::FromReduction => {
                "reuses the aggregation, computes the projection and the clustering"
            }
            Recomputes::Clustering => "reuses the aggregation and the projection, re-clusters",
        }
    }
}

/// Which stage a key belongs to: 0 aggregation, 1 reduction, 2 clustering.
///
/// The same mapping as the interface's `stage_of`, which a test there checks
/// key by key: the cost the plan announces and the order the interface walks
/// the grid in must agree on what is expensive.
pub fn stage_rank(key: &str) -> u8 {
    match key {
        "order" | "stat_funcs" => 0,
        "reducer_type" | "n_neighbors" | "dim_clust" | "min_dist" | "metric" => 1,
        _ => 2,
    }
}

/// Everything a sensitivity analysis will do.
#[derive(Debug, Clone, PartialEq)]
pub struct SweepPlan {
    /// When the plan was written, as the interface spells it. Only shown.
    pub created: String,
    /// The working directory the runs read their networks from and write into.
    pub working_dir: PathBuf,
    /// The sub-directory of `Niche_Analysis` the results go to.
    pub results_in: String,
    /// The sub-section of `Niche Analysis` each run's settings are written into.
    pub sub_section: String,
    /// The normalisation every run draws its composition figures with.
    pub normalize: Option<String>,
    /// The formats each run's figures are written in.
    pub figure_formats: Vec<String>,
    /// Every key the grid varies, in stage order: the comparison table's columns.
    pub columns: Vec<String>,
    /// How many combinations were asked for, before duplicates were collapsed.
    pub requested: usize,
    pub runs: Vec<PlannedRun>,
    /// The configuration every run starts from.
    pub base: RawConfig,
}

impl SweepPlan {
    /// The configuration run `index` is started with: the base, with the run's
    /// settings and the sweep's normalisation written into the swept
    /// sub-section. Everything else is left exactly as the base has it.
    pub fn configuration(&self, index: usize) -> RawConfig {
        let mut updated = self.base.clone();
        let Some(run) = self.runs.get(index) else {
            return updated;
        };
        let Value::Mapping(niche) = updated.section_mut(section::NICHE_ANALYSIS) else {
            return updated;
        };
        let entry = niche
            .entry(Value::String(self.sub_section.clone()))
            .or_insert_with(|| Value::Mapping(Mapping::new()));
        if let Value::Mapping(sub) = entry {
            for (key, value) in &run.settings {
                sub.insert(Value::String(key.clone()), value.clone());
            }
            if let Some(normalize) = &self.normalize {
                sub.insert(
                    Value::String("normalize".to_string()),
                    Value::String(normalize.clone()),
                );
            }
        }
        updated
    }

    /// What each run computes, given the one before it.
    pub fn recomputes(&self) -> Vec<Recomputes> {
        let mut previous: Option<&PlannedRun> = None;
        self.runs
            .iter()
            .map(|run| {
                let cost = match previous {
                    None => Recomputes::Everything,
                    Some(before) => {
                        let differs = |rank: u8| {
                            run.settings
                                .iter()
                                .chain(before.settings.iter())
                                .filter(|(key, _)| stage_rank(key) == rank)
                                .any(|(key, _)| run.get(key) != before.get(key))
                        };
                        if differs(0) {
                            Recomputes::Everything
                        } else if differs(1) {
                            Recomputes::FromReduction
                        } else {
                            Recomputes::Clustering
                        }
                    }
                };
                previous = Some(run);
                cost
            })
            .collect()
    }

    /// How many aggregations, projections and clusterings the plan computes,
    /// at most.
    pub fn costs(&self) -> (usize, usize, usize) {
        let recomputes = self.recomputes();
        let aggregations = recomputes
            .iter()
            .filter(|r| **r == Recomputes::Everything)
            .count();
        let projections = recomputes
            .iter()
            .filter(|r| **r <= Recomputes::FromReduction)
            .count();
        (aggregations, projections, recomputes.len())
    }

    /// The distinct values each varied key takes, in the order the grid walks
    /// them.
    pub fn grid(&self) -> Vec<(String, Vec<String>)> {
        self.columns
            .iter()
            .map(|column| {
                let mut values: Vec<String> = Vec::new();
                for run in &self.runs {
                    if let Some(value) = run.get(column) {
                        let text = inline(value);
                        if !values.contains(&text) {
                            values.push(text);
                        }
                    }
                }
                (column.clone(), values)
            })
            .collect()
    }

    /// The command that runs this plan, once written at `path`.
    pub fn command(path: &Path) -> String {
        format!(
            "mosna sensitivity-analysis --log \"{}\"",
            path.to_string_lossy()
        )
    }

    /// The plan as the file a person reads and the command runs.
    ///
    /// `path` is where it will be written, so the explanation can name the
    /// command that runs it.
    pub fn render(&self, path: &Path) -> Result<String> {
        let mut out = String::new();
        let rule = format!("# {}\n", "=".repeat(76));
        let line = |out: &mut String, text: &str| {
            if text.is_empty() {
                out.push_str("#\n");
            } else {
                out.push_str("# ");
                out.push_str(text);
                out.push('\n');
            }
        };

        let niche = |key: &str| {
            self.base
                .get(section::NICHE_ANALYSIS, key)
                .map(inline)
                .unwrap_or_else(|| "—".to_string())
        };
        let results = self
            .working_dir
            .join("Niche_Analysis")
            .join(&self.results_in);
        let (aggregations, projections, clusterings) = self.costs();
        let total = self.runs.len();

        out.push_str(&rule);
        line(&mut out, "MOSNA — sensitivity analysis plan");
        out.push_str(&rule);
        line(&mut out, "");
        line(
            &mut out,
            &format!("Written on {}. Nothing has been run yet.", self.created),
        );
        line(
            &mut out,
            "This file says what the sensitivity analysis will do, and it is also",
        );
        line(&mut out, "what runs it, from a terminal:");
        line(&mut out, "");
        line(&mut out, &format!("    {}", Self::command(path)));
        line(&mut out, "");
        line(&mut out, "What will be done");
        line(&mut out, "-----------------");
        line(
            &mut out,
            &format!("Step 3 — the niche analysis — is run {total} time(s), once per combination"),
        );
        line(
            &mut out,
            "listed under `runs`, one after the other. Each is an ordinary niche analysis:",
        );
        line(
            &mut out,
            "same validation, same caches, same register and same outputs as a run",
        );
        line(
            &mut out,
            "started by hand. A run that fails is recorded as failed and the next one",
        );
        line(&mut out, "starts.");
        line(&mut out, "");
        line(
            &mut out,
            &format!("  Working directory  : {}", self.working_dir.display()),
        );
        line(
            &mut out,
            &format!("  Networks           : {}", niche("Network directory")),
        );
        line(
            &mut out,
            &format!("  Processing method  : {}", niche("Processing method")),
        );
        line(
            &mut out,
            &format!("  Niches method      : {}", niche("Niches method")),
        );
        line(
            &mut out,
            &format!("  Column to aggregate: {}", niche("Column to aggregate")),
        );
        line(
            &mut out,
            &format!(
                "  Settings written   : into `{}`, before each run",
                self.sub_section
            ),
        );
        line(
            &mut out,
            "  Everything else    : as in `base_configuration`, at the end of this file",
        );
        line(
            &mut out,
            &format!(
                "  Normalisation      : {}",
                self.normalize.as_deref().unwrap_or("as configured")
            ),
        );
        line(
            &mut out,
            &format!("  Figures            : {}", self.figure_formats.join(", ")),
        );
        line(
            &mut out,
            &format!("  Results            : {}/<run>/", results.display()),
        );
        line(&mut out, "");
        line(&mut out, "The grid");
        line(&mut out, "--------");
        let width = self.columns.iter().map(String::len).max().unwrap_or(0);
        for (column, values) in self.grid() {
            line(
                &mut out,
                &format!("  {column:<width$} : {}", values.join(", ")),
            );
        }
        let dropped = self.requested.saturating_sub(total);
        line(
            &mut out,
            &if dropped == 0 {
                format!("  {} combination(s), {total} run(s).", self.requested)
            } else {
                format!(
                    "  {} combination(s) asked for; {dropped} repeated one(s) dropped, {total} run(s).",
                    self.requested
                )
            },
        );
        line(&mut out, "");
        line(&mut out, "Cost");
        line(&mut out, "----");
        line(
            &mut out,
            "The aggregation, the projection and the clustering are cached separately,",
        );
        line(
            &mut out,
            "and the runs are ordered with the aggregation varying slowest, so a run",
        );
        line(
            &mut out,
            "only computes from the first stage whose settings changed. At most:",
        );
        line(&mut out, "");
        line(
            &mut out,
            &format!("  aggregations computed: {aggregations}   (the expensive stage)"),
        );
        line(&mut out, &format!("  projections computed : {projections}"));
        line(&mut out, &format!("  clusterings computed : {clusterings}"));
        line(&mut out, "");
        line(
            &mut out,
            "\"At most\": a stage an earlier analysis already computed with the same",
        );
        line(&mut out, "settings is read from the cache instead.");
        line(&mut out, "");
        line(&mut out, "Afterwards");
        line(&mut out, "----------");
        line(
            &mut out,
            &format!(
                "  {}/sensitivity.csv — one line per run: what it was",
                results.display()
            ),
        );
        line(
            &mut out,
            "    given, then the niches it found, the largest niche and the components",
        );
        line(&mut out, "    of its clustering graph.");
        line(
            &mut out,
            "  The comparison figures between runs (`compare-sweep`), once two runs",
        );
        line(&mut out, "    or more have succeeded.");
        line(&mut out, "");
        line(
            &mut out,
            "Each run below is preceded by what it computes. The YAML is what the",
        );
        line(
            &mut out,
            "command reads: edit it and the command runs the edited plan, but the",
        );
        line(&mut out, "comments above will no longer describe it.");
        out.push_str(&rule);
        out.push('\n');

        // The data.
        out.push_str(&format!("format: {}\n", inline(&Value::from(FORMAT))));
        out.push_str(&format!(
            "created: {}\n",
            inline(&Value::from(self.created.as_str()))
        ));
        out.push_str(&format!(
            "working_dir: {}\n",
            inline(&Value::from(self.working_dir.to_string_lossy().as_ref()))
        ));
        out.push_str(&format!(
            "results_in: {}\n",
            inline(&Value::from(self.results_in.as_str()))
        ));
        out.push_str(&format!(
            "sub_section: {}\n",
            inline(&Value::from(self.sub_section.as_str()))
        ));
        out.push_str(&format!(
            "normalize: {}\n",
            self.normalize
                .as_deref()
                .map(|n| inline(&Value::from(n)))
                .unwrap_or_else(|| "null".to_string())
        ));
        out.push_str(&format!(
            "figure_formats: {}\n",
            flow_list(&self.figure_formats)
        ));
        out.push_str(&format!("columns: {}\n", flow_list(&self.columns)));
        out.push_str(&format!("requested: {}\n", self.requested));
        out.push_str("runs:\n");
        for (index, (run, cost)) in self.runs.iter().zip(self.recomputes()).enumerate() {
            out.push_str(&format!(
                "  # Run {} of {total} — {}.\n",
                index + 1,
                cost.describe()
            ));
            if run.settings.is_empty() {
                out.push_str("  - {}\n");
                continue;
            }
            for (position, (key, value)) in run.settings.iter().enumerate() {
                let marker = if position == 0 { "  - " } else { "    " };
                out.push_str(&format!(
                    "{marker}{}: {}\n",
                    inline(&Value::from(key.as_str())),
                    inline(value)
                ));
            }
        }
        out.push_str("# The configuration every run starts from.\n");
        out.push_str("base_configuration:\n");
        let base = self
            .base
            .to_yaml_string()
            .map_err(|e| ConfigError::assertion(format!("cannot write the plan: {e}")))?;
        for text in base.lines() {
            if text.is_empty() {
                out.push('\n');
            } else {
                out.push_str("  ");
                out.push_str(text);
                out.push('\n');
            }
        }
        Ok(out)
    }

    /// Read a plan back.
    pub fn parse(text: &str) -> Result<Self> {
        let invalid =
            |what: &str| ConfigError::assertion(format!("invalid sensitivity plan: {what}"));
        let root: Value = serde_yaml::from_str(text)
            .map_err(|e| ConfigError::assertion(format!("invalid sensitivity plan: {e}")))?;
        let Value::Mapping(root) = root else {
            return Err(invalid("not a YAML mapping"));
        };
        let field = |key: &str| root.get(Value::String(key.to_string()));
        let string = |key: &str| -> Result<String> {
            field(key)
                .and_then(Value::as_str)
                .map(str::to_string)
                .ok_or_else(|| invalid(&format!("`{key}` is missing or not a string")))
        };
        let list = |key: &str| -> Vec<String> {
            field(key)
                .and_then(Value::as_sequence)
                .map(|items| {
                    items
                        .iter()
                        .filter_map(|item| item.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default()
        };

        let format = string("format")?;
        if format != FORMAT {
            return Err(invalid(&format!(
                "format `{format}` is not `{FORMAT}`, which this version reads"
            )));
        }

        let runs = field("runs")
            .and_then(Value::as_sequence)
            .ok_or_else(|| invalid("`runs` is missing or not a list"))?
            .iter()
            .enumerate()
            .map(|(index, run)| match run {
                Value::Mapping(map) => Ok(PlannedRun {
                    settings: map
                        .iter()
                        .filter_map(|(key, value)| Some((key.as_str()?.to_string(), value.clone())))
                        .collect(),
                }),
                _ => Err(invalid(&format!("run {} is not a mapping", index + 1))),
            })
            .collect::<Result<Vec<_>>>()?;

        let base = match field("base_configuration") {
            Some(Value::Mapping(map)) => RawConfig::from_mapping(map.clone()),
            _ => return Err(invalid("`base_configuration` is missing or not a mapping")),
        };
        if base.section(section::NICHE_ANALYSIS).is_err() {
            return Err(invalid(
                "`base_configuration` has no `Niche Analysis` section",
            ));
        }

        let figure_formats = list("figure_formats");
        Ok(Self {
            created: field("created")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            working_dir: PathBuf::from(string("working_dir")?),
            results_in: string("results_in")?,
            sub_section: string("sub_section")?,
            normalize: field("normalize")
                .and_then(Value::as_str)
                .map(str::to_string),
            figure_formats: if figure_formats.is_empty() {
                vec!["png".to_string()]
            } else {
                figure_formats
            },
            columns: list("columns"),
            requested: field("requested")
                .and_then(Value::as_u64)
                .map(|n| n as usize)
                .unwrap_or(runs.len()),
            runs,
            base,
        })
    }

    /// Write the plan at `path`, creating the directory if needed.
    pub fn write(&self, path: &Path) -> Result<()> {
        let text = self.render(path)?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|source| ConfigError::Write {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        std::fs::write(path, text).map_err(|source| ConfigError::Write {
            path: path.to_path_buf(),
            source,
        })
    }

    /// Read the plan at `path`.
    pub fn read(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path).map_err(|source| ConfigError::Read {
            path: path.to_path_buf(),
            source,
        })?;
        Self::parse(&text)
    }
}

/// A value on one line, as YAML writes it.
fn inline(value: &Value) -> String {
    match value {
        Value::Sequence(items) => {
            let parts: Vec<String> = items.iter().map(inline).collect();
            format!("[{}]", parts.join(", "))
        }
        other => serde_yaml::to_string(other)
            .unwrap_or_default()
            .trim_end()
            .to_string(),
    }
}

/// A list of strings as a YAML flow sequence.
fn flow_list(items: &[String]) -> String {
    inline(&Value::Sequence(
        items
            .iter()
            .map(|item| Value::from(item.as_str()))
            .collect(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: &str = "\
Tysserand:
  Nodes directory: /data
Niche Analysis:
  Network directory: Default
  Processing method: Aggregated nodes
  Niches method: NAS
  Column to aggregate: Cluster
  Aggregated nodes:
    order: '1'
    reducer_type: umap
    n_neighbors: 20
    clusterer_type: leiden
    resolution: 0.05
    normalize: niche
";

    fn run(settings: &[(&str, Value)]) -> PlannedRun {
        PlannedRun {
            settings: settings
                .iter()
                .map(|(k, v)| (k.to_string(), v.clone()))
                .collect(),
        }
    }

    fn plan() -> SweepPlan {
        SweepPlan {
            created: "2026-11-06 10:00".to_string(),
            working_dir: PathBuf::from("/work dir"),
            results_in: "sensitivity_analysis".to_string(),
            sub_section: "Aggregated nodes".to_string(),
            normalize: Some("total".to_string()),
            figure_formats: vec!["png".to_string()],
            columns: vec![
                "order".to_string(),
                "n_neighbors".to_string(),
                "clusterer_type".to_string(),
                "resolution".to_string(),
            ],
            requested: 5,
            runs: vec![
                run(&[
                    ("order", Value::from("1")),
                    ("n_neighbors", Value::from(10)),
                    ("clusterer_type", Value::from("leiden")),
                    ("resolution", Value::from(0.005)),
                ]),
                run(&[
                    ("order", Value::from("1")),
                    ("n_neighbors", Value::from(10)),
                    ("clusterer_type", Value::from("leiden")),
                    ("resolution", Value::from(0.05)),
                ]),
                run(&[
                    ("order", Value::from("1")),
                    ("n_neighbors", Value::from(30)),
                    ("clusterer_type", Value::from("leiden")),
                    ("resolution", Value::from(0.005)),
                ]),
                run(&[
                    ("order", Value::from("2")),
                    ("n_neighbors", Value::from(10)),
                    ("clusterer_type", Value::from("leiden")),
                    ("resolution", Value::from(0.005)),
                ]),
            ],
            base: RawConfig::from_yaml_str(BASE).unwrap(),
        }
    }

    /// The file the interface writes is the file the command reads: what is
    /// rendered parses back to the same plan.
    #[test]
    fn a_rendered_plan_reads_back_unchanged() {
        let original = plan();
        let text = original.render(Path::new("/work dir/plan.log")).unwrap();
        assert_eq!(SweepPlan::parse(&text).unwrap(), original);
    }

    #[test]
    fn each_run_says_which_stages_it_computes() {
        assert_eq!(
            plan().recomputes(),
            vec![
                Recomputes::Everything,
                Recomputes::Clustering,
                Recomputes::FromReduction,
                Recomputes::Everything,
            ]
        );
        assert_eq!(plan().costs(), (2, 3, 4));
    }

    #[test]
    fn the_explanation_covers_the_plan() {
        let text = plan().render(Path::new("/work dir/plan.log")).unwrap();
        for expected in [
            "mosna sensitivity-analysis --log \"/work dir/plan.log\"",
            "run 4 time(s)",
            "order",
            "n_neighbors    : 10, 30",
            "resolution     : 0.005, 0.05",
            "5 combination(s) asked for; 1 repeated one(s) dropped, 4 run(s).",
            "aggregations computed: 2",
            "projections computed : 3",
            "clusterings computed : 4",
            "# Run 2 of 4 — reuses the aggregation and the projection, re-clusters.",
            "/work dir/Niche_Analysis/sensitivity_analysis/sensitivity.csv",
            "Column to aggregate: Cluster",
        ] {
            assert!(text.contains(expected), "missing `{expected}` in:\n{text}");
        }
    }

    /// A run's configuration is the base with the run's keys and the sweep's
    /// normalisation replaced, and nothing else touched.
    #[test]
    fn a_run_is_the_base_with_its_settings_written_in() {
        let config = plan().configuration(2);
        let sub = config
            .get("Niche Analysis", "Aggregated nodes")
            .unwrap()
            .clone();
        assert_eq!(sub.get("n_neighbors"), Some(&Value::from(30)));
        assert_eq!(sub.get("resolution"), Some(&Value::from(0.005)));
        assert_eq!(sub.get("normalize"), Some(&Value::from("total")));
        assert_eq!(sub.get("reducer_type"), Some(&Value::from("umap")));
        assert_eq!(
            config.get("Tysserand", "Nodes directory"),
            Some(&Value::from("/data"))
        );
    }

    #[test]
    fn a_plan_of_another_format_is_refused() {
        let text = plan()
            .render(Path::new("/p.log"))
            .unwrap()
            .replace(FORMAT, "mosna-sensitivity-plan/99");
        let error = SweepPlan::parse(&text).unwrap_err().to_string();
        assert!(error.contains("mosna-sensitivity-plan/99"), "{error}");
    }

    #[test]
    fn a_plan_without_its_configuration_is_refused() {
        let text = plan().render(Path::new("/p.log")).unwrap();
        let cut = &text[..text.find("base_configuration:").unwrap()];
        let error = SweepPlan::parse(cut).unwrap_err().to_string();
        assert!(error.contains("base_configuration"), "{error}");
    }

    #[test]
    fn writing_and_reading_go_through_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join(PLAN_FILE);
        plan().write(&path).unwrap();
        assert_eq!(SweepPlan::read(&path).unwrap(), plan());
    }
}
