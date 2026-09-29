//! The grid a sensitivity analysis walks, and the order it walks it in.
//!
//! # What this is
//!
//! A sensitivity analysis asks how the niches move when a parameter does. The
//! user gives each parameter an interval and a number of values — `resolution`
//! from 0.005 to 0.05, four values — or, for a parameter that has no interval,
//! a set of the choices to try. This builds the configurations that asks for.
//!
//! Nothing here runs anything. A [`Plan`] is a list of configurations, each one
//! a complete `Niche Analysis` section that `mosna niche-analysis` accepts
//! unchanged — so a swept run is an ordinary run, writes its labels into the
//! nodes files under the ordinary `niches_1-2-4` name, and lands in the
//! ordinary register. There is no second kind of run to explain.
//!
//! # Why the order matters more than anything else here
//!
//! The three stages of a niche analysis are cached separately, and each is a
//! pure function of the stage before it: change the clustering and the
//! aggregation and the projection are read back from disk; change the
//! aggregation and everything below it is recomputed. Measured on a cohort of
//! 39 290 cells: a run that recomputes everything takes 14 s, a run that only
//! re-clusters takes 0.4 s.
//!
//! So the product is not generated in whatever order the axes were declared. It
//! is generated with the aggregation varying slowest, then the reduction, then
//! the clustering — [`Stage`] — which is what lets every run after the first in
//! a block read its features and its projection from the cache. The same grid
//! walked the other way round pays the full price for every point of it.
//!
//! # Why the values are deduplicated
//!
//! An interval of four values over an integer range of three — `n_clusters`
//! from 4 to 6 — produces `4, 4, 5, 6`. Left alone that is a run computed
//! twice, and two identical rows in the comparison table. Sampling collapses
//! repeats, and the plan says how many points were asked for against how many
//! are distinct, so the interface can say "12 combinations, 9 distinct runs"
//! before anything starts.

use std::collections::BTreeSet;

use mosna_config::model::niche_params::{IMPLEMENTED_CLUSTERERS, IMPLEMENTED_REDUCERS};
use mosna_config::RawConfig;

/// Which stage of the pipeline a parameter belongs to.
///
/// The order of the variants is the order the product is generated in, which
/// is what makes the cache pay off — see the module note.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Stage {
    /// The neighbourhood aggregation: the most expensive thing to redo.
    Aggregation,
    /// The projection, recomputed whenever the aggregation under it changes.
    Reduction,
    /// The partition, the cheapest stage and the one worth sweeping densely.
    Clustering,
}

impl Stage {
    pub fn title(self) -> &'static str {
        match self {
            Stage::Aggregation => "Aggregation",
            Stage::Reduction => "Reduction",
            Stage::Clustering => "Clustering",
        }
    }

    /// The sub-section key each stage's parameters live under.
    ///
    /// All three are in the same sub-section of the configuration; the stage is
    /// about what a change costs, not about where the key is written.
    pub fn all() -> [Stage; 3] {
        [Stage::Aggregation, Stage::Reduction, Stage::Clustering]
    }
}

/// How the points of an interval are spaced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scale {
    Linear,
    /// Constant ratio rather than constant step.
    ///
    /// The right default for a parameter that acts multiplicatively.
    /// `resolution` from 0.005 to 0.05 in four points is 0.005, 0.0108, 0.0232,
    /// 0.05 — a decade covered evenly. Linearly it is 0.005, 0.02, 0.035, 0.05,
    /// which puts three of the four points in the top half and barely samples
    /// the regime where the partition is coarse.
    Log,
}

/// What kind of value a swept parameter takes.
#[derive(Debug, Clone, PartialEq)]
pub enum Values {
    /// An interval and how many points to take in it.
    Interval {
        from: f64,
        to: f64,
        count: usize,
        scale: Scale,
        /// Whether the parameter is written back as a whole number.
        integer: bool,
    },
    /// A set of choices, of which any number may be tried.
    Choices { chosen: Vec<String> },
    /// A set of whole numbers, ticked from a menu.
    ///
    /// Kept apart from [`Values::Choices`] because the two reach the
    /// configuration differently: a choice is written as the text it is, and a
    /// whole number as a number. Writing `n_neighbors` as the string the menu
    /// stores it as failed every run of a sweep with `n_neighbors must be int`.
    Integers { chosen: Vec<i64> },
    /// The parameter is not swept: the configuration's own value is used.
    Fixed,
}

/// One parameter of the sweep.
#[derive(Debug, Clone, PartialEq)]
pub struct Axis {
    /// The configuration key, spelled as the YAML spells it.
    pub key: String,
    pub stage: Stage,
    pub values: Values,
}

impl Axis {
    /// An axis that is not swept.
    pub fn fixed(key: &str, stage: Stage) -> Self {
        Self {
            key: key.to_string(),
            stage,
            values: Values::Fixed,
        }
    }

    /// The values this axis contributes, deduplicated and in order.
    ///
    /// Empty when the axis is fixed: the caller leaves the configuration's own
    /// value in place rather than writing one.
    pub fn points(&self) -> Vec<Point> {
        match &self.values {
            Values::Fixed => Vec::new(),
            Values::Choices { chosen } => {
                let mut seen = BTreeSet::new();
                chosen
                    .iter()
                    .filter(|choice| seen.insert(choice.as_str().to_string()))
                    .map(|choice| Point::Text(choice.clone()))
                    .collect()
            }
            Values::Integers { chosen } => {
                let mut seen = BTreeSet::new();
                chosen
                    .iter()
                    .filter(|value| seen.insert(**value))
                    .map(|value| Point::Integer(*value))
                    .collect()
            }
            Values::Interval {
                from,
                to,
                count,
                scale,
                integer,
            } => sample(*from, *to, *count, *scale, *integer),
        }
    }

    /// How many points were asked for, before duplicates were collapsed.
    pub fn requested(&self) -> usize {
        match &self.values {
            Values::Fixed => 0,
            Values::Choices { chosen } => chosen.len(),
            Values::Integers { chosen } => chosen.len(),
            Values::Interval { count, .. } => *count,
        }
    }
}

/// One value on an axis.
#[derive(Debug, Clone, PartialEq)]
pub enum Point {
    Number(f64),
    Integer(i64),
    Text(String),
}

impl Point {
    /// How the value is written into the YAML.
    pub fn to_yaml(&self) -> serde_yaml::Value {
        match self {
            Point::Number(value) => serde_yaml::Value::Number((*value).into()),
            Point::Integer(value) => serde_yaml::Value::Number((*value).into()),
            Point::Text(value) => serde_yaml::Value::String(value.clone()),
        }
    }

    /// How the value is shown in a table or a label.
    pub fn label(&self) -> String {
        match self {
            // Enough digits to tell 0.0108 from 0.0109, not so many that a
            // column of them stops lining up.
            Point::Number(value) => format!("{value:.6}")
                .trim_end_matches('0')
                .trim_end_matches('.')
                .to_string(),
            Point::Integer(value) => value.to_string(),
            Point::Text(value) => value.clone(),
        }
    }
}

/// `count` points from `from` to `to`, inclusive of both ends.
///
/// Duplicates are collapsed, which is what an integer interval narrower than
/// the number of points asked for produces: `n_clusters` from 4 to 6 in four
/// points is 4, 4, 5, 6 and must be run three times, not four.
///
/// Asking for a single value is asking for the *first* field and nothing else:
/// the screen greys the second one out, so the bound the user can still see is
/// the one that must be used. Taking the lower of the two would silently run a
/// value the user had stopped being able to edit.
pub fn sample(from: f64, to: f64, count: usize, scale: Scale, integer: bool) -> Vec<Point> {
    if count == 0 {
        return Vec::new();
    }
    let (low, high) = if from <= to { (from, to) } else { (to, from) };

    // Log spacing needs both ends strictly positive: a ratio through zero is
    // not defined, and `min_dist` starts at zero. Falling back to linear is
    // what the user meant by "spread these points out".
    let scale = if scale == Scale::Log && (low <= 0.0 || high <= 0.0) {
        Scale::Linear
    } else {
        scale
    };

    let raw: Vec<f64> = if count == 1 {
        vec![from]
    } else {
        let last = (count - 1) as f64;
        (0..count)
            .map(|index| {
                let t = index as f64 / last;
                match scale {
                    Scale::Linear => low + (high - low) * t,
                    Scale::Log => low * (high / low).powf(t),
                }
            })
            .collect()
    };

    let mut seen = BTreeSet::new();
    raw.into_iter()
        .map(|value| {
            if integer {
                Point::Integer(value.round() as i64)
            } else {
                Point::Number(round_to(value, 6))
            }
        })
        .filter(|point| seen.insert(point.label()))
        .collect()
}

/// Round to `digits` decimals, so two points that differ in the fifteenth are
/// one point.
fn round_to(value: f64, digits: u32) -> f64 {
    let factor = 10f64.powi(digits as i32);
    (value * factor).round() / factor
}

/// The clusterers to try, and what to vary inside each of them.
///
/// # Why the clustering axes are per clusterer
///
/// `resolution` means something to leiden and nothing to gmm; `n_clusters` is
/// the other way round. A single set of clustering axes would either offer
/// leiden a parameter it ignores — which the register already refuses to record
/// as a difference, so the runs would silently collapse into one — or force the
/// sweep to one clusterer at a time.
#[derive(Debug, Clone, PartialEq)]
pub struct ClustererSweep {
    pub clusterer: String,
    pub axes: Vec<Axis>,
}

impl ClustererSweep {
    /// The parameters this clusterer reads, and nothing else.
    ///
    /// Taken from the same place `clustering_parameters` takes them, so an axis
    /// can never be offered for a value the register will not record.
    pub fn parameters_of(clusterer: &str) -> &'static [&'static str] {
        match clusterer {
            "leiden" => &["resolution", "k_cluster"],
            "gmm" | "spectral" => &["n_clusters"],
            _ => &[],
        }
    }

    /// A clusterer with every parameter it reads, none of them swept yet.
    pub fn new(clusterer: &str) -> Self {
        Self {
            clusterer: clusterer.to_string(),
            axes: Self::parameters_of(clusterer)
                .iter()
                .map(|key| Axis::fixed(key, Stage::Clustering))
                .collect(),
        }
    }
}

/// One configuration the sweep will run.
#[derive(Debug, Clone, PartialEq)]
pub struct Combination {
    /// The values to write, as `(key, value)` in the order the stages run.
    pub settings: Vec<(String, Point)>,
    /// Which clusterer this combination belongs to.
    pub clusterer: String,
}

impl Combination {
    /// The value of one key, for a comparison table's column.
    pub fn get(&self, key: &str) -> Option<&Point> {
        self.settings
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, point)| point)
    }
}

/// Everything the sweep will do, in the order it will do it.
#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    pub combinations: Vec<Combination>,
    /// How many points were asked for across the grid, before duplicates were
    /// collapsed — so the interface can say what it dropped.
    pub requested: usize,
}

impl Plan {
    pub fn len(&self) -> usize {
        self.combinations.len()
    }

    pub fn is_empty(&self) -> bool {
        self.combinations.is_empty()
    }

    /// Every key the plan varies, in stage order, for a table's columns.
    pub fn columns(&self) -> Vec<String> {
        let mut seen = BTreeSet::new();
        let mut columns = Vec::new();
        for combination in &self.combinations {
            for (key, _) in &combination.settings {
                if seen.insert(key.clone()) {
                    columns.push(key.clone());
                }
            }
        }
        columns
    }

    /// How many distinct aggregations the plan will compute.
    ///
    /// The expensive number: everything else is read from the cache. It is what
    /// turns "180 runs" into a duration a user can decide about.
    pub fn aggregations(&self) -> usize {
        self.distinct_prefixes(Stage::Aggregation)
    }

    /// How many distinct projections, on the same terms.
    pub fn reductions(&self) -> usize {
        self.distinct_prefixes(Stage::Reduction)
    }

    fn distinct_prefixes(&self, upto: Stage) -> usize {
        let mut seen = BTreeSet::new();
        for combination in &self.combinations {
            let prefix: Vec<String> = combination
                .settings
                .iter()
                .filter(|(key, _)| stage_of(key) <= upto)
                .map(|(key, point)| format!("{key}={}", point.label()))
                .collect();
            seen.insert(prefix);
        }
        seen.len()
    }
}

/// Which stage a key belongs to.
///
/// One place, so the ordering of the product and the cost estimate cannot
/// disagree about what is expensive.
pub fn stage_of(key: &str) -> Stage {
    match key {
        "order" | "stat_funcs" => Stage::Aggregation,
        "reducer_type" | "n_neighbors" | "dim_clust" | "min_dist" | "metric" => Stage::Reduction,
        _ => Stage::Clustering,
    }
}

/// Build the plan from the shared axes and the per-clusterer ones.
///
/// `shared` carries the aggregation and reduction axes, which every clusterer
/// is tried against; `clusterers` carries what varies inside each of them.
///
/// The product is generated aggregation-slowest, so consecutive runs share as
/// much of the cache as the grid allows — see the module note.
pub fn plan(shared: &[Axis], clusterers: &[ClustererSweep]) -> Plan {
    let mut requested = 1usize;
    for axis in shared.iter().filter(|a| a.requested() > 0) {
        requested = requested.saturating_mul(axis.requested());
    }
    let clusterer_requested: usize = clusterers
        .iter()
        .map(|sweep| {
            sweep
                .axes
                .iter()
                .filter(|a| a.requested() > 0)
                .fold(1usize, |total, a| total.saturating_mul(a.requested()))
        })
        .sum();
    requested =
        requested.saturating_mul(clusterer_requested.max(usize::from(!clusterers.is_empty())));

    // The shared axes, sorted so the aggregation varies slowest.
    let mut ordered: Vec<&Axis> = shared.iter().filter(|a| !a.points().is_empty()).collect();
    ordered.sort_by_key(|axis| (axis.stage, axis.key.clone()));

    let shared_rows = product(&ordered);
    let mut combinations = Vec::new();

    for row in shared_rows {
        for sweep in clusterers {
            let mut axes: Vec<&Axis> = sweep
                .axes
                .iter()
                .filter(|a| !a.points().is_empty())
                .collect();
            axes.sort_by_key(|axis| axis.key.clone());

            for tail in product(&axes) {
                let mut settings = row.clone();
                // The clusterer itself is a setting, written before the
                // parameters that only it reads.
                settings.push((
                    "clusterer_type".to_string(),
                    Point::Text(sweep.clusterer.clone()),
                ));
                settings.extend(tail);
                combinations.push(Combination {
                    settings,
                    clusterer: sweep.clusterer.clone(),
                });
            }
        }
    }

    Plan {
        combinations,
        requested,
    }
}

/// The cartesian product of the axes, first axis varying slowest.
fn product(axes: &[&Axis]) -> Vec<Vec<(String, Point)>> {
    let mut rows: Vec<Vec<(String, Point)>> = vec![Vec::new()];
    for axis in axes {
        let points = axis.points();
        let mut next = Vec::with_capacity(rows.len() * points.len());
        for row in &rows {
            for point in &points {
                let mut extended = row.clone();
                extended.push((axis.key.clone(), point.clone()));
                next.push(extended);
            }
        }
        rows = next;
    }
    rows
}

/// Which sub-section a run's settings are written into.
///
/// The sweep follows the configuration's own `Processing method` rather than
/// choosing for itself: the interface shows one mode, and a sweep that silently
/// ran the other would produce runs the user cannot find.
pub fn swept_subsection(config: &RawConfig) -> &'static str {
    match config
        .get(NICHE, "Processing method")
        .and_then(serde_yaml::Value::as_str)
        .unwrap_or_default()
    {
        "Per sample" => PER_SAMPLE,
        _ => AGGREGATED,
    }
}

/// The section and sub-section names, as the YAML spells them.
pub const NICHE: &str = "Niche Analysis";
pub const AGGREGATED: &str = "Aggregated nodes";
pub const PER_SAMPLE: &str = "Per sample";

/// The configuration one combination asks for.
///
/// Everything the user did not sweep is left exactly as the main interface has
/// it — a swept run is an ordinary run with a few keys replaced, so it writes
/// its labels into the nodes files under the ordinary `niches_1-2-4` name and
/// lands in the ordinary register. There is no second kind of run.
pub fn apply(config: &RawConfig, combination: &Combination, sub_section: &str) -> RawConfig {
    apply_with(config, combination, sub_section, None)
}

/// The same, also setting the normalisation every run of the sweep draws.
pub fn apply_with(
    config: &RawConfig,
    combination: &Combination,
    sub_section: &str,
    normalize: Option<&str>,
) -> RawConfig {
    let mut updated = config.clone();
    let section = updated.section_mut(NICHE);

    let serde_yaml::Value::Mapping(map) = section else {
        return updated;
    };
    let key = serde_yaml::Value::String(sub_section.to_string());
    let entry = map
        .entry(key)
        .or_insert_with(|| serde_yaml::Value::Mapping(Default::default()));
    if let serde_yaml::Value::Mapping(sub) = entry {
        for (name, point) in &combination.settings {
            sub.insert(serde_yaml::Value::String(name.clone()), point.to_yaml());
        }
        if let Some(normalize) = normalize {
            sub.insert(
                serde_yaml::Value::String("normalize".to_string()),
                serde_yaml::Value::String(normalize.to_string()),
            );
        }
    }
    updated
}

/// The parameters a sweep may vary, per stage, and how each is sampled.
///
/// # What is missing from this list, and why
///
/// `normalize` and `Phenotype column` are not here. They choose which
/// composition figures are drawn and change no part of the partition, so the
/// register records two runs differing only in them as *one* run — sweeping
/// them would produce a grid of points that all collapse onto each other. They
/// belong to the main interface, where they are a rendering choice.
///
/// `min_cluster_size` is not here either: only HDBSCAN reads it, and HDBSCAN
/// has no implementation.
pub fn sweepable(stage: Stage) -> &'static [(&'static str, Sampling)] {
    match stage {
        Stage::Aggregation => &[
            // Only the first and second neighbourhood orders are offered: they
            // are what the aggregation implements today, and an interval would
            // invite a third that does not exist.
            ("order", Sampling::Choice(&["1", "2"])),
            (
                "stat_funcs",
                Sampling::Choice(&["np.mean,np.std", "np.mean"]),
            ),
        ],
        Stage::Reduction => &[
            ("reducer_type", Sampling::Choice(IMPLEMENTED_REDUCERS)),
            ("n_neighbors", Sampling::Integer),
            ("dim_clust", Sampling::Integer),
            // Multiplicative in effect, but it starts at zero, where a ratio is
            // not defined — so linear is the only default that always works.
            ("min_dist", Sampling::Decimal(Scale::Linear)),
            (
                "metric",
                Sampling::Choice(&["euclidean", "manhattan", "cosine"]),
            ),
        ],
        Stage::Clustering => &[
            // The one parameter that is genuinely multiplicative: 0.005 to 0.05
            // is a decade, and linear spacing barely samples the coarse end.
            ("resolution", Sampling::Decimal(Scale::Log)),
            ("k_cluster", Sampling::Integer),
            ("n_clusters", Sampling::Integer),
        ],
    }
}

/// How one parameter is sampled by default.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sampling {
    /// A whole number chosen from a list rather than sampled from an interval.
    ///
    /// # Why these are not intervals
    ///
    /// `dim_clust` is two, or three, or whatever the data allows — a handful of
    /// values, each meaning something on its own. Asking for "four values
    /// between 2 and 6" then rounding them is a worse way of saying "2, 3, 5,
    /// 6": the count and the bounds have to be juggled to land on the numbers
    /// wanted, and an interval narrower than the count silently repeats itself.
    ///
    /// So the bounds give the candidates and the user ticks the ones to run,
    /// the way the main interface picks columns.
    Integer,
    Decimal(Scale),
    Choice(&'static [&'static str]),
}

/// What a parameter stands at when the configuration does not say.
///
/// # Why this has to exist
///
/// With no box to tick, a row whose fields are blank is a row that reports "not
/// a number" and stops the sweep being started. A configuration written before
/// a key existed — or one trimmed by hand — would therefore open a screen the
/// user cannot use and cannot obviously fix.
///
/// These are the values `NicheParams::from_value` falls back to, so a form
/// seeded from a sparse configuration asks for exactly what the pipeline would
/// have run anyway.
/// The top of the menu a whole-number parameter opens on.
///
/// The bound is editable — this is only where the list starts — but it has to
/// be somewhere useful, because a menu whose only entry is the current value
/// offers no choice at all. These are the ranges these parameters are actually
/// explored over.
pub fn default_ceiling(key: &str) -> &'static str {
    match key {
        "dim_clust" => "5",
        "n_neighbors" => "30",
        "n_clusters" => "20",
        "k_cluster" => "20",
        _ => "10",
    }
}

pub fn default_for(key: &str) -> &'static str {
    match key {
        "order" => "1",
        "stat_funcs" => "np.mean,np.std",
        "reducer_type" => "umap",
        "n_neighbors" => "15",
        "dim_clust" => "2",
        "min_dist" => "0.0",
        "metric" => "euclidean",
        "resolution" => "0.005",
        "k_cluster" => "8",
        "n_clusters" => "15",
        _ => "1",
    }
}

impl Sampling {
    /// The parameter this describes, for a key.
    pub fn of(key: &str) -> Sampling {
        Stage::all()
            .into_iter()
            .flat_map(sweepable)
            .find(|(name, _)| *name == key)
            .map(|(_, sampling)| *sampling)
            .unwrap_or(Sampling::Decimal(Scale::Linear))
    }
}

/// One parameter as the screen holds it while the user edits it.
///
/// # Why there is no box to tick
///
/// Every parameter is always part of the grid; what changes is how many values
/// it contributes. One value is a parameter held still, and the screen greys
/// out the second bound to say so — which is the same statement a tick-box
/// would have made, in a control the user was going to have to fill in anyway.
/// A screen of twenty parameters each needing to be enabled before it could be
/// typed into is twenty clicks that say nothing.
///
/// So a fresh form is the configuration the main interface is already showing:
/// every count is one, the grid is one run, and raising a count is what opens
/// the sweep out.
///
/// The bounds are kept as the text that was typed rather than as numbers: a
/// field being edited passes through states like `"-"` and `"1e"` that are not
/// numbers yet, and rounding them to something parseable on every keystroke
/// takes the cursor away from the user.
#[derive(Debug, Clone, PartialEq)]
pub struct AxisEdit {
    pub key: String,
    pub stage: Stage,
    pub sampling: Sampling,
    pub from: String,
    pub to: String,
    pub count: String,
    pub scale: Scale,
    /// For a categorical parameter: each choice, and whether it is picked.
    pub choices: Vec<(String, bool)>,
}

impl AxisEdit {
    pub fn new(key: &str, stage: Stage, sampling: Sampling) -> Self {
        let default = default_for(key);
        let mut axis = Self {
            key: key.to_string(),
            stage,
            sampling,
            from: default.to_string(),
            to: default.to_string(),
            count: "1".to_string(),
            scale: match sampling {
                Sampling::Decimal(scale) => scale,
                _ => Scale::Linear,
            },
            choices: match sampling {
                // The default is ticked, so a row opens on one value rather
                // than on none — which would be a row reporting a problem
                // before the user has touched it.
                Sampling::Choice(options) => options
                    .iter()
                    .map(|option| (option.to_string(), *option == default))
                    .collect(),
                _ => Vec::new(),
            },
        };
        if matches!(sampling, Sampling::Integer) {
            // The menu runs from the value in force to somewhere worth
            // exploring, with only the first ticked: one value to begin with,
            // and a list to open out.
            axis.to = default_ceiling(key).to_string();
            axis.refresh_candidates();
        }
        axis
    }

    pub fn is_categorical(&self) -> bool {
        matches!(self.sampling, Sampling::Choice(_))
    }

    /// Whether the values are ticked from a list rather than sampled.
    pub fn is_a_list(&self) -> bool {
        matches!(self.sampling, Sampling::Choice(_) | Sampling::Integer)
    }

    /// The whole numbers the bounds offer, for the menu to list.
    ///
    /// Capped, because the bounds are typed and `2` to `100000` is a menu
    /// nobody can use — and a list that long is an interval by another name.
    pub fn candidates(&self) -> Vec<String> {
        const MOST: i64 = 64;
        let (Ok(from), Ok(to)) = (
            self.from.trim().parse::<i64>(),
            self.to.trim().parse::<i64>(),
        ) else {
            return Vec::new();
        };
        let (low, high) = if from <= to { (from, to) } else { (to, from) };
        (low..=high)
            .take(MOST as usize)
            .map(|n| n.to_string())
            .collect()
    }

    /// Open a whole number's menu on the value the configuration holds.
    ///
    /// The lower bound becomes that value and the upper one is pushed above it,
    /// so the menu always offers a choice: a drop-down whose only entry is the
    /// value already in force is not a menu, and that is what a configured
    /// `k_cluster` of 20 produced against a ceiling of 20.
    ///
    /// Only the configured value is ticked. Refreshing alone would have kept
    /// whatever the row opened on — a `n_clusters` of 6 seeded from the
    /// configuration was left running the default of 15.
    pub fn open_menu_at(&mut self, value: &str) {
        if !matches!(self.sampling, Sampling::Integer) {
            return;
        }
        self.from = value.to_string();

        /// The smallest range that is still a range. Four above the value in
        /// force is enough for the menu to be worth opening, and the bound is
        /// editable the moment the user wants more.
        const LEAST_SPAN: i64 = 4;

        let ceiling = default_ceiling(&self.key).parse::<i64>().unwrap_or(0);
        if let Ok(from) = value.trim().parse::<i64>() {
            self.to = ceiling.max(from + LEAST_SPAN).to_string();
        }

        self.refresh_candidates();
        for (name, picked) in &mut self.choices {
            *picked = name == value;
        }
        // A configured value the bounds cannot hold — a negative, say — still
        // leaves the row producing a run.
        if !self.choices.iter().any(|(_, picked)| *picked) {
            if let Some((_, picked)) = self.choices.first_mut() {
                *picked = true;
            }
        }
    }

    /// Bring the ticked values back in line with the bounds.
    ///
    /// Called when a bound changes: a value that has fallen outside the range
    /// is dropped, and one that was already ticked keeps its tick. Without it a
    /// narrowed range would still run the values it no longer offers.
    pub fn refresh_candidates(&mut self) {
        if !matches!(self.sampling, Sampling::Integer) {
            return;
        }
        let candidates = self.candidates();
        let picked: Vec<String> = self
            .choices
            .iter()
            .filter(|(_, picked)| *picked)
            .map(|(name, _)| name.clone())
            .collect();

        self.choices = candidates
            .into_iter()
            .map(|value| {
                let was = picked.contains(&value);
                (value, was)
            })
            .collect();

        // A range that no longer holds anything the user had chosen falls back
        // to its first value, so the row keeps producing a run.
        if !self.choices.iter().any(|(_, picked)| *picked) {
            if let Some((_, picked)) = self.choices.first_mut() {
                *picked = true;
            }
        }
    }

    /// Whether the second bound is in play.
    ///
    /// One value uses the first field alone, and the screen greys the second
    /// one out rather than leaving a box that changes nothing.
    pub fn spans_an_interval(&self) -> bool {
        !self.is_a_list() && self.count.trim().parse::<usize>().is_ok_and(|n| n > 1)
    }

    /// Whether the scale changes anything at the number of values asked for.
    ///
    /// Two points of an interval are its ends however they are spaced, so the
    /// box only starts meaning something at three. Offering it before then is
    /// offering a control that does nothing when it is clicked — which reads as
    /// a fault in the screen rather than as arithmetic.
    pub fn scale_matters(&self) -> bool {
        matches!(self.sampling, Sampling::Decimal(_))
            && self.count.trim().parse::<usize>().is_ok_and(|n| n > 2)
    }

    /// An integer row always spans its bounds: both are the menu's ends.
    pub fn spans_bounds(&self) -> bool {
        matches!(self.sampling, Sampling::Integer) || self.spans_an_interval()
    }

    /// The axis this edit describes, or a fixed one when it is incomplete.
    ///
    /// An axis half-typed is not an error to report: the user is still typing,
    /// and the plan simply has one fewer dimension until the fields make sense.
    pub fn to_axis(&self) -> Axis {
        match self.sampling {
            // A whole number reaches the configuration as a number: every
            // integer check refuses the string the menu stores it as.
            Sampling::Integer => Axis {
                key: self.key.clone(),
                stage: self.stage,
                values: Values::Integers {
                    chosen: self
                        .choices
                        .iter()
                        .filter(|(_, picked)| *picked)
                        .filter_map(|(name, _)| name.trim().parse::<i64>().ok())
                        .collect(),
                },
            },
            // `order` is the exception the configuration itself makes: the
            // interface has always written it as a string, and the validation
            // accepts either.
            Sampling::Choice(_) => Axis {
                key: self.key.clone(),
                stage: self.stage,
                values: Values::Choices {
                    chosen: self
                        .choices
                        .iter()
                        .filter(|(_, picked)| *picked)
                        .map(|(name, _)| name.clone())
                        .collect(),
                },
            },
            _ => {
                let from = self.from.trim().parse::<f64>();
                // One value reads the first field alone, so a second bound left
                // blank — or stale — cannot keep the axis out of the grid.
                let to = if self.spans_an_interval() {
                    self.to.trim().parse::<f64>()
                } else {
                    from.clone()
                };
                let count = self.count.trim().parse::<usize>();
                match (from, to, count) {
                    (Ok(from), Ok(to), Ok(count)) if count > 0 => Axis {
                        key: self.key.clone(),
                        stage: self.stage,
                        values: Values::Interval {
                            from,
                            to,
                            count,
                            scale: self.scale,
                            integer: matches!(self.sampling, Sampling::Integer),
                        },
                    },
                    _ => Axis::fixed(&self.key, self.stage),
                }
            }
        }
    }

    /// What is wrong with the fields as typed, if anything.
    ///
    /// Shown beside the row rather than raised: the user is mid-edit, and a
    /// dialog for a bound that is not a number yet would be unusable.
    pub fn problem(&self) -> Option<String> {
        if self.is_a_list() {
            if matches!(self.sampling, Sampling::Integer) && self.candidates().is_empty() {
                return Some("the bounds are not whole numbers".to_string());
            }
            return self
                .choices
                .iter()
                .all(|(_, picked)| !picked)
                .then(|| "pick at least one value".to_string());
        }
        let mut fields = vec![(&self.from, "from")];
        if self.spans_an_interval() {
            fields.push((&self.to, "to"));
        }
        for (field, name) in fields {
            if field.trim().parse::<f64>().is_err() {
                return Some(format!("`{name}` is not a number"));
            }
        }
        match self.count.trim().parse::<usize>() {
            Ok(0) | Err(_) => Some("how many values?".to_string()),
            Ok(_) => None,
        }
    }
}

/// One clusterer, and what varies inside it, as the screen holds it.
#[derive(Debug, Clone, PartialEq)]
pub struct ClustererEdit {
    pub clusterer: String,
    pub selected: bool,
    pub axes: Vec<AxisEdit>,
}

impl ClustererEdit {
    pub fn new(clusterer: &str) -> Self {
        Self {
            clusterer: clusterer.to_string(),
            selected: false,
            axes: ClustererSweep::parameters_of(clusterer)
                .iter()
                .map(|key| AxisEdit::new(key, Stage::Clustering, Sampling::of(key)))
                .collect(),
        }
    }
}

/// The whole screen's state.
#[derive(Debug, Clone, PartialEq)]
pub struct SweepForm {
    /// The aggregation and reduction axes, which every clusterer is tried
    /// against.
    pub shared: Vec<AxisEdit>,
    pub clusterers: Vec<ClustererEdit>,
    /// Which composition figures every run draws.
    ///
    /// # Why this is one choice and not an axis
    ///
    /// `normalize` decides which composition figures are drawn and changes no
    /// part of the partition, so two runs differing only in it are *one* run —
    /// the register records them as the same, and the second adds a rendering
    /// to the first's directory rather than becoming a point of its own.
    /// Sweeping it would therefore build a grid whose extra dimension collapses
    /// the moment it runs.
    ///
    /// It belongs here all the same, because a sweep is when a user decides
    /// what every one of two hundred runs should draw. `all` is how to ask for
    /// several: the pipeline draws the five normalisations in one pass.
    pub normalize: String,
}

/// The normalisations a run can draw, as the configuration spells them.
pub const NORMALIZATIONS: &[&str] = &["total", "niche", "obs", "clr", "niche&obs", "all"];

impl Default for SweepForm {
    fn default() -> Self {
        Self::new()
    }
}

impl SweepForm {
    /// The form as the configuration currently stands: every count at one, so
    /// the grid is the single run the main interface would have started.
    ///
    /// Seeding the bounds from the configuration is what makes that true. A
    /// form of empty fields would be a screen the user has to fill in before it
    /// says anything, and the thing it would say first is "this is not a
    /// number".
    pub fn from_config(config: &RawConfig) -> Self {
        let mut form = Self::new();
        let sub_section = swept_subsection(config);
        let Some(sub) = config.get(NICHE, sub_section) else {
            return form;
        };

        let seed = |axis: &mut AxisEdit| {
            let Some(value) = sub.get(&axis.key) else {
                return;
            };
            let text = match value {
                serde_yaml::Value::String(text) => text.clone(),
                serde_yaml::Value::Number(number) => number.to_string(),
                serde_yaml::Value::Bool(flag) => flag.to_string(),
                _ => return,
            };
            if matches!(axis.sampling, Sampling::Integer) {
                axis.open_menu_at(&text);
            } else if axis.is_categorical() {
                // The value in force is the one already picked, so one value is
                // the configuration unchanged.
                let mut matched = false;
                for (name, picked) in &mut axis.choices {
                    *picked = *name == text;
                    matched |= *picked;
                }
                if !matched {
                    if let Some((_, picked)) = axis.choices.first_mut() {
                        *picked = true;
                    }
                }
            } else {
                axis.from = text.clone();
                axis.to = text;
            }
        };

        for axis in &mut form.shared {
            seed(axis);
        }
        for clusterer in &mut form.clusterers {
            for axis in &mut clusterer.axes {
                seed(axis);
            }
        }
        form.normalize = sub
            .get("normalize")
            .and_then(serde_yaml::Value::as_str)
            .unwrap_or("total")
            .to_string();
        form
    }

    pub fn new() -> Self {
        Self {
            shared: [Stage::Aggregation, Stage::Reduction]
                .into_iter()
                .flat_map(|stage| {
                    sweepable(stage)
                        .iter()
                        .map(move |(key, sampling)| AxisEdit::new(key, stage, *sampling))
                })
                .collect(),
            clusterers: available_clusterers()
                .iter()
                .map(|name| ClustererEdit::new(name))
                .collect(),
            normalize: "total".to_string(),
        }
    }

    /// The axes of one stage, for the box that draws them.
    pub fn stage(&self, stage: Stage) -> impl Iterator<Item = &AxisEdit> {
        self.shared.iter().filter(move |axis| axis.stage == stage)
    }

    pub fn stage_mut(&mut self, stage: Stage) -> impl Iterator<Item = &mut AxisEdit> {
        self.shared
            .iter_mut()
            .filter(move |axis| axis.stage == stage)
    }

    /// Whether no run of this sweep will compute a projection.
    ///
    /// True when `none` is the only reducer chosen. Half a grid that reduces is
    /// still a grid that reduces, so both choices together leave everything in
    /// play.
    pub fn reduction_is_inert(&self) -> bool {
        let Some(axis) = self.shared.iter().find(|a| a.key == "reducer_type") else {
            return false;
        };
        let picked: Vec<&str> = axis
            .choices
            .iter()
            .filter(|(_, picked)| *picked)
            .map(|(name, _)| name.as_str())
            .collect();
        picked == ["none"]
    }

    /// Whether a parameter decides nothing as the form currently stands.
    ///
    /// # What this is for
    ///
    /// Without a reduction there is no projection, so `dim_clust`, `min_dist`
    /// and `metric` are read by nobody — and a run that does not reduce records
    /// no reduction at all, which means a sweep over three `dim_clust` values
    /// would run three times and land on one entry in the register. A dimension
    /// that collapses the moment it runs is worse than no dimension: it costs
    /// the time and produces a table with three identical rows.
    ///
    /// `n_neighbors` is not among them. It caps leiden's graph degree through
    /// `effective_k_cluster`, so it still decides something with no reduction
    /// at all — which is the judgement the main interface already makes.
    pub fn is_inert(&self, key: &str) -> bool {
        matches!(key, "dim_clust" | "min_dist" | "metric") && self.reduction_is_inert()
    }

    /// The plan as the form currently reads.
    pub fn plan(&self) -> Plan {
        let shared: Vec<Axis> = self
            .shared
            .iter()
            .map(|axis| {
                if self.is_inert(&axis.key) {
                    Axis::fixed(&axis.key, axis.stage)
                } else {
                    axis.to_axis()
                }
            })
            .collect();
        let clusterers: Vec<ClustererSweep> = self
            .clusterers
            .iter()
            .filter(|edit| edit.selected)
            .map(|edit| ClustererSweep {
                clusterer: edit.clusterer.clone(),
                axes: edit.axes.iter().map(AxisEdit::to_axis).collect(),
            })
            .collect();
        plan(&shared, &clusterers)
    }

    /// Everything that stops the sweep being started, in the order it is drawn.
    pub fn problems(&self) -> Vec<String> {
        let mut problems: Vec<String> = Vec::new();
        if !self.clusterers.iter().any(|edit| edit.selected) {
            problems.push("choose at least one clusterer".to_string());
        }
        for axis in &self.shared {
            // A parameter nothing reads cannot be wrong.
            if self.is_inert(&axis.key) {
                continue;
            }
            if let Some(problem) = axis.problem() {
                problems.push(format!("{}: {problem}", axis.key));
            }
        }
        for clusterer in self.clusterers.iter().filter(|edit| edit.selected) {
            for axis in &clusterer.axes {
                if let Some(problem) = axis.problem() {
                    problems.push(format!("{} / {}: {problem}", clusterer.clusterer, axis.key));
                }
            }
        }
        problems
    }

    /// Whether the sweep can be started.
    pub fn is_runnable(&self) -> bool {
        self.problems().is_empty() && !self.plan().is_empty()
    }
}

/// What became of one combination.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunState {
    Pending,
    Running,
    Done,
    Failed,
}

impl RunState {
    pub fn as_str(&self) -> &'static str {
        match self {
            RunState::Pending => "pending",
            RunState::Running => "running",
            RunState::Done => "done",
            RunState::Failed => "failed",
        }
    }
}

/// One row of the comparison table: what was asked for, and what came back.
///
/// The right-hand half is read out of the run's own `run.json`, which records
/// the niche count, the niche sizes and the connected components of the
/// clustering graph — so nothing here recomputes anything, and a sweep resumed
/// from a directory that already holds its runs fills its table by reading.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub combination: Combination,
    /// The directory the run claimed, once it has said so.
    pub run: Option<String>,
    pub state: RunState,
    pub niches: Option<usize>,
    pub largest: Option<usize>,
    pub graph_components: Option<usize>,
}

impl Row {
    pub fn pending(combination: Combination) -> Self {
        Self {
            combination,
            run: None,
            state: RunState::Pending,
            niches: None,
            largest: None,
            graph_components: None,
        }
    }

    /// Fill in what a finished run recorded about itself.
    ///
    /// A record that cannot be read leaves the row's outcome empty rather than
    /// failing the sweep: the run may have succeeded and written a record this
    /// version does not understand, and the next hundred runs should not stop
    /// for that.
    pub fn absorb(&mut self, record: &serde_json::Value) {
        let result = &record["result"];
        self.niches = result["niches"].as_u64().map(|n| n as usize);
        self.largest = result["sizes"]
            .as_array()
            .and_then(|sizes| sizes.iter().filter_map(serde_json::Value::as_u64).max())
            .map(|n| n as usize);
        self.graph_components = result["graph_components"].as_u64().map(|n| n as usize);
    }
}

/// The comparison table, as the file a user takes away.
///
/// One line per run: everything the sweep varied, then everything it found.
/// This is the deliverable of a sensitivity analysis — the individual figures
/// answer "what did run 1-1-7 look like", and this answers "what did varying
/// the resolution do", which is the question that was asked.
pub fn to_csv(columns: &[String], rows: &[Row]) -> String {
    let mut out = String::new();

    out.push_str("run,status");
    for column in columns {
        out.push(',');
        out.push_str(&escape(column));
    }
    out.push_str(",niches,largest_niche,graph_components\n");

    for row in rows {
        out.push_str(&escape(row.run.as_deref().unwrap_or("")));
        out.push(',');
        out.push_str(row.state.as_str());
        for column in columns {
            out.push(',');
            out.push_str(&escape(
                &row.combination
                    .get(column)
                    .map(|p| p.label())
                    .unwrap_or_default(),
            ));
        }
        for value in [row.niches, row.largest, row.graph_components] {
            out.push(',');
            if let Some(value) = value {
                out.push_str(&value.to_string());
            }
        }
        out.push('\n');
    }
    out
}

/// A CSV field, quoted when it has to be.
///
/// Column names come from the configuration and values from the user, so a
/// comma or a quote in either is possible and must not shift every column
/// after it.
fn escape(field: &str) -> String {
    if field.contains([',', '"', '\n']) {
        format!("\"{}\"", field.replace('"', "\"\""))
    } else {
        field.to_string()
    }
}

/// A sweep under way: the queue, where it has got to, and what it has found.
///
/// # Why the progress is counted in runs and not in work
///
/// The runs of a sweep do not cost the same: the first of a block recomputes
/// its aggregation and its projection, the rest read both from the cache — 14 s
/// against 0.4 s on a cohort of 39 290 cells. A bar weighted by expected cost
/// would be more honest about time and less useful for the thing the user
/// actually wants to know, which is how much of the grid is left. So the bar
/// counts runs, and the screen says separately how many aggregations and
/// projections the plan holds, which is where the time goes.
#[derive(Debug, Clone, PartialEq)]
pub struct Sweep {
    pub rows: Vec<Row>,
    /// The row now running, if any.
    pub current: Option<usize>,
    /// Set when the user asks to stop; the run in flight is let finish.
    pub stopping: bool,
    /// Why the first run to fail failed.
    ///
    /// Kept because a grid usually fails for one reason — a column the
    /// configuration does not name, a network that is not there — and fails
    /// that way from end to end. Reporting only a count leaves the user with
    /// eighty-four failures and nothing to act on, and the single-run path's
    /// dialog never fires for a sweep.
    ///
    /// The first is kept rather than the last: it is the one that was not
    /// caused by whatever the earlier ones left behind.
    pub failure: Option<String>,
}

impl Sweep {
    pub fn new(plan: &Plan) -> Self {
        Self {
            rows: plan
                .combinations
                .iter()
                .cloned()
                .map(Row::pending)
                .collect(),
            current: None,
            stopping: false,
            failure: None,
        }
    }

    pub fn total(&self) -> usize {
        self.rows.len()
    }

    /// How many runs have finished, one way or the other.
    pub fn finished(&self) -> usize {
        self.rows
            .iter()
            .filter(|row| matches!(row.state, RunState::Done | RunState::Failed))
            .count()
    }

    pub fn failed(&self) -> usize {
        self.rows
            .iter()
            .filter(|row| row.state == RunState::Failed)
            .count()
    }

    /// The fraction done, for the bar.
    pub fn fraction(&self) -> f32 {
        if self.rows.is_empty() {
            return 0.0;
        }
        self.finished() as f32 / self.rows.len() as f32
    }

    /// The percentage done, as the caption says it.
    pub fn percent(&self) -> usize {
        (self.fraction() * 100.0).round() as usize
    }

    /// The next row to run, or `None` when the sweep is over.
    ///
    /// A sweep asked to stop hands out nothing more; the run already in flight
    /// is left to finish, because killing it half-way would leave a directory
    /// the register calls `running` and a partial set of figures.
    pub fn next_pending(&self) -> Option<usize> {
        if self.stopping {
            return None;
        }
        self.rows
            .iter()
            .position(|row| row.state == RunState::Pending)
    }

    /// Mark the row now running.
    pub fn begin(&mut self, index: usize) {
        if let Some(row) = self.rows.get_mut(index) {
            row.state = RunState::Running;
        }
        self.current = Some(index);
    }

    /// Record how the run in flight ended.
    pub fn end(
        &mut self,
        succeeded: bool,
        run: Option<String>,
        record: Option<&serde_json::Value>,
    ) {
        self.end_with(succeeded, run, record, None)
    }

    /// The same, noting why a run failed.
    pub fn end_with(
        &mut self,
        succeeded: bool,
        run: Option<String>,
        record: Option<&serde_json::Value>,
        reason: Option<String>,
    ) {
        // A run the user killed is not a run that failed: recording "Unknown
        // error" for a deliberate stop would put a fault on the screen where
        // there is none.
        if !succeeded && self.failure.is_none() && !self.stopping {
            self.failure = reason;
        }
        let Some(index) = self.current.take() else {
            return;
        };
        let Some(row) = self.rows.get_mut(index) else {
            return;
        };
        row.state = if succeeded {
            RunState::Done
        } else {
            RunState::Failed
        };
        if let Some(run) = run {
            row.run = Some(run);
        }
        if let Some(record) = record {
            row.absorb(record);
        }
    }

    /// Whether anything is left to do.
    pub fn is_over(&self) -> bool {
        self.current.is_none() && self.next_pending().is_none()
    }

    /// Whether the sweep is still working.
    ///
    /// The screen shows Stop while this is true and Start once it is not. The
    /// two used to be told apart by whether a sweep existed at all, so a
    /// finished one left a Stop button that could never do anything again and
    /// no way to start another — which is what "the stop button does not work"
    /// looks like from the outside.
    pub fn is_running(&self) -> bool {
        !self.is_over()
    }

    /// What the status line says about the sweep as a whole.
    pub fn caption(&self) -> String {
        let (done, total) = (self.finished(), self.total());
        let failed = self.failed();

        if self.is_over() {
            // A sweep the user stopped did not fail, however many of its runs
            // were cut short by the stopping.
            let what = if self.stopping { "stopped" } else { "finished" };
            let mut caption = if failed == 0 || self.stopping {
                format!("Sweep {what}: {done} of {total} run(s)")
            } else {
                format!(
                    "Sweep {what}: {} of {total} run(s), {failed} failed",
                    done - failed
                )
            };
            if let Some(reason) = &self.failure {
                caption.push_str(&format!(" — {reason}"));
            }
            return caption;
        }

        if let Some(reason) = &self.failure {
            // The reason travels with the count: a grid usually fails for one
            // cause, and a bar reporting only "84 failed" is unactionable.
            return format!("{done} of {total} run(s), {failed} failed — {reason}");
        }
        if self.stopping {
            return format!("Stopping — {done} of {total} done");
        }
        format!("Sweep: {done} of {total} run(s), {}%", self.percent())
    }

    /// The caption, with the reason the first failure gave.
    pub fn caption_with_reason(&self) -> String {
        match &self.failure {
            Some(reason) => format!("{} — {reason}", self.caption()),
            None => self.caption(),
        }
    }
}

/// What the General part of the niche section has to say before a sweep can be
/// built on it.
///
/// # Why this is checked before the screen opens
///
/// Every run of a grid reads the same General settings — which cohort, which
/// columns, which processing method — so one of them missing fails every run of
/// the grid in the same instant. Eighty-four runs can fail that way before a
/// user has read the first message, and the grid they spent minutes choosing is
/// gone with them.
///
/// The parameters the sweep *varies* are not checked here: they are what the
/// screen is for, and it validates them as they are typed.
pub fn general_problems(config: &RawConfig) -> Vec<String> {
    let mut problems = Vec::new();
    let Ok(section) = config.section(NICHE) else {
        return vec!["the configuration has no `Niche Analysis` section".to_string()];
    };

    let filled = |key: &str| {
        section
            .get(key)
            .is_some_and(|value| !matches!(value, serde_yaml::Value::Null))
    };

    for key in [
        "Patient column name",
        "Extension",
        "Processing method",
        "Niches method",
        "Column to aggregate",
    ] {
        if !filled(key) {
            problems.push(format!("`{key}` is empty"));
        }
    }

    // The composition figures are what a sweep is read through, and without a
    // phenotype column none of them are drawn.
    if !filled("Phenotype column") {
        problems
            .push("`Phenotype column` is empty, so no niche composition will be drawn".to_string());
    }
    problems
}

/// The General settings a sweep will run against, for the dialog that confirms
/// them.
///
/// # Why these six
///
/// They are the ones that decide what a run *is* rather than how it is
/// computed, and the ones a user is least likely to have looked at recently:
/// chosen once, on another screen, possibly in another session. Every run of
/// the grid shares them, so one of them wrong is a grid that fails — or worse,
/// succeeds and answers a question nobody asked.
pub const CONFIRMED_SETTINGS: &[&str] = &[
    "Network directory",
    "Phenotype column",
    "Column to aggregate",
    "X coordinates column for niches",
    "Y coordinates column for niches",
    "Plot Network",
];

/// One setting, as the dialog has to show it.
///
/// A list is kept as a list rather than joined into a line: `Column to
/// aggregate` on a real cohort is thirty-four phenotypes, and a dialog that
/// spells them all out stops being a summary — the six settings it exists to
/// put in front of a reader are pushed off the screen by one of them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingValue {
    /// Nothing is set.
    Missing,
    Text(String),
    List(Vec<String>),
}

impl SettingValue {
    /// The line the dialog shows, which for a long list is a count.
    ///
    /// The same rule the main interface's column picker follows, so the two
    /// read alike: up to three are named, and past that the number stands in
    /// for them and the names are one click away.
    pub fn caption(&self) -> String {
        match self {
            SettingValue::Missing => "—".to_string(),
            SettingValue::Text(text) => text.clone(),
            SettingValue::List(items) => match items.len() {
                0 => "—".to_string(),
                n if n <= 3 => items.join(", "),
                n => format!("{n} columns"),
            },
        }
    }

    /// Whether the caption stands in for names the reader may want to see.
    pub fn is_abbreviated(&self) -> bool {
        matches!(self, SettingValue::List(items) if items.len() > 3)
    }

    /// The names behind the caption.
    pub fn items(&self) -> &[String] {
        match self {
            SettingValue::List(items) => items,
            _ => &[],
        }
    }

    /// Whether anything is set at all.
    pub fn is_missing(&self) -> bool {
        match self {
            SettingValue::Missing => true,
            SettingValue::Text(text) => text.trim().is_empty(),
            SettingValue::List(items) => items.is_empty(),
        }
    }
}

/// Those settings, as `(name, value)`, for the dialog to lay out in a column.
pub fn general_summary(config: &RawConfig) -> Vec<(String, SettingValue)> {
    let Ok(section) = config.section(NICHE) else {
        return Vec::new();
    };
    CONFIRMED_SETTINGS
        .iter()
        .map(|key| {
            let value = match section.get(key) {
                None | Some(serde_yaml::Value::Null) => SettingValue::Missing,
                Some(serde_yaml::Value::String(text)) => SettingValue::Text(text.clone()),
                Some(serde_yaml::Value::Sequence(items)) => SettingValue::List(
                    items
                        .iter()
                        .filter_map(serde_yaml::Value::as_str)
                        .map(str::to_string)
                        .collect(),
                ),
                Some(other) => SettingValue::Text(
                    serde_yaml::to_string(other)
                        .unwrap_or_default()
                        .trim()
                        .to_string(),
                ),
            };
            (key.to_string(), value)
        })
        .collect()
}

/// The clusterers a sweep may offer, which are the ones anything implements.
pub fn available_clusterers() -> &'static [&'static str] {
    IMPLEMENTED_CLUSTERERS
}

#[cfg(test)]
mod tests {
    use super::*;

    fn interval(key: &str, from: f64, to: f64, count: usize, scale: Scale, integer: bool) -> Axis {
        Axis {
            key: key.to_string(),
            stage: stage_of(key),
            values: Values::Interval {
                from,
                to,
                count,
                scale,
                integer,
            },
        }
    }

    fn choices(key: &str, chosen: &[&str]) -> Axis {
        Axis {
            key: key.to_string(),
            stage: stage_of(key),
            values: Values::Choices {
                chosen: chosen.iter().map(|c| c.to_string()).collect(),
            },
        }
    }

    /// Point an edit at an interval, the way the screen's fields would.
    fn set(axis: &mut AxisEdit, from: &str, to: &str, count: &str) {
        axis.from = from.into();
        axis.to = to.into();
        axis.count = count.into();
        axis.refresh_candidates();
    }

    /// Tick these values in a whole-number menu, the way the drop-down would.
    fn tick(axis: &mut AxisEdit, wanted: &[&str]) {
        for (name, picked) in &mut axis.choices {
            *picked = wanted.contains(&name.as_str());
        }
    }

    fn labels(points: &[Point]) -> Vec<String> {
        points.iter().map(Point::label).collect()
    }

    // -----------------------------------------------------------------------
    // Sampling an interval
    // -----------------------------------------------------------------------

    /// The example from the specification: `resolution` between 0.005 and 0.05,
    /// spread over a decade rather than bunched in its top half.
    #[test]
    fn a_log_interval_covers_its_decade_evenly() {
        let points = sample(0.005, 0.05, 4, Scale::Log, false);
        assert_eq!(
            labels(&points),
            vec!["0.005", "0.010772", "0.023208", "0.05"]
        );
    }

    #[test]
    fn a_linear_interval_steps_evenly() {
        let points = sample(0.005, 0.05, 4, Scale::Linear, false);
        assert_eq!(labels(&points), vec!["0.005", "0.02", "0.035", "0.05"]);
    }

    /// Both ends are always run: an interval is inclusive, so the bounds the
    /// user typed are among the values tried.
    #[test]
    fn both_ends_of_an_interval_are_sampled() {
        for scale in [Scale::Linear, Scale::Log] {
            let points = sample(0.005, 0.05, 5, scale, false);
            assert_eq!(points.first().unwrap().label(), "0.005");
            assert_eq!(points.last().unwrap().label(), "0.05");
        }
    }

    /// The specification's other example: two values of `resolution`. Two
    /// points of an interval are its ends.
    #[test]
    fn two_values_are_the_two_bounds() {
        let points = sample(0.05, 0.005, 2, Scale::Log, false);
        assert_eq!(labels(&points), vec!["0.005", "0.05"]);
    }

    /// Bounds given the wrong way round are read as an interval, not as an
    /// error: the user said "between these two".
    #[test]
    fn the_bounds_may_be_given_in_either_order() {
        assert_eq!(
            sample(0.05, 0.005, 3, Scale::Log, false),
            sample(0.005, 0.05, 3, Scale::Log, false)
        );
    }

    /// One value is the *first* field, not the lower of the two: the screen
    /// greys the second one out, so the bound the user can still see is the one
    /// that must be run.
    #[test]
    fn one_value_is_the_first_bound_whichever_is_lower() {
        assert_eq!(
            labels(&sample(4.0, 10.0, 1, Scale::Linear, true)),
            vec!["4"]
        );
        assert_eq!(
            labels(&sample(10.0, 4.0, 1, Scale::Linear, true)),
            vec!["10"]
        );
    }

    #[test]
    fn no_values_is_an_empty_axis() {
        assert!(sample(4.0, 10.0, 0, Scale::Linear, true).is_empty());
    }

    /// An integer interval narrower than the number of points asked for repeats
    /// itself. Each repeat would be a run computed twice and a duplicate row in
    /// the comparison table.
    #[test]
    fn an_integer_interval_collapses_its_repeats() {
        let points = sample(4.0, 6.0, 4, Scale::Linear, true);
        assert_eq!(labels(&points), vec!["4", "5", "6"]);
    }

    #[test]
    fn integers_come_back_whole() {
        let points = sample(4.0, 10.0, 4, Scale::Linear, true);
        assert_eq!(labels(&points), vec!["4", "6", "8", "10"]);
        assert!(matches!(points[0], Point::Integer(4)));
    }

    /// `min_dist` starts at zero, where a ratio is not defined. Falling back to
    /// linear is what "spread these points out" has to mean there.
    #[test]
    fn a_log_interval_through_zero_falls_back_to_linear() {
        assert_eq!(
            sample(0.0, 0.5, 3, Scale::Log, false),
            sample(0.0, 0.5, 3, Scale::Linear, false)
        );
    }

    /// A degenerate interval is one value, not a division by zero.
    #[test]
    fn an_interval_of_no_width_is_one_point() {
        assert_eq!(
            labels(&sample(0.05, 0.05, 4, Scale::Log, false)),
            vec!["0.05"]
        );
    }

    // -----------------------------------------------------------------------
    // Categorical axes
    // -----------------------------------------------------------------------

    /// A categorical parameter is a set, so several may be tried at once.
    #[test]
    fn a_categorical_axis_offers_every_choice_picked() {
        let axis = choices("metric", &["euclidean", "cosine"]);
        assert_eq!(labels(&axis.points()), vec!["euclidean", "cosine"]);
    }

    #[test]
    fn a_categorical_axis_collapses_a_repeated_choice() {
        let axis = choices("metric", &["cosine", "cosine", "euclidean"]);
        assert_eq!(labels(&axis.points()), vec!["cosine", "euclidean"]);
    }

    /// An axis nobody swept contributes nothing, and the configuration's own
    /// value stands.
    #[test]
    fn a_fixed_axis_contributes_nothing() {
        assert!(Axis::fixed("order", Stage::Aggregation).points().is_empty());
    }

    // -----------------------------------------------------------------------
    // The plan
    // -----------------------------------------------------------------------

    #[test]
    fn a_plan_is_the_product_of_its_axes() {
        let shared = vec![interval("n_neighbors", 10.0, 20.0, 2, Scale::Linear, true)];
        let leiden = ClustererSweep {
            clusterer: "leiden".into(),
            axes: vec![interval("resolution", 0.005, 0.05, 3, Scale::Log, false)],
        };

        let plan = plan(&shared, &[leiden]);
        assert_eq!(plan.len(), 6);
        assert_eq!(plan.requested, 6);
    }

    /// The whole reason the order is fixed: the aggregation varies slowest, so
    /// every run after the first in a block reads its features and its
    /// projection from the cache instead of recomputing them.
    #[test]
    fn the_aggregation_varies_slowest_and_the_clustering_fastest() {
        let shared = vec![
            interval("order", 1.0, 2.0, 2, Scale::Linear, true),
            interval("n_neighbors", 10.0, 20.0, 2, Scale::Linear, true),
        ];
        let leiden = ClustererSweep {
            clusterer: "leiden".into(),
            axes: vec![interval("resolution", 0.01, 0.02, 2, Scale::Linear, false)],
        };

        let plan = plan(&shared, &[leiden]);
        let orders: Vec<String> = plan
            .combinations
            .iter()
            .map(|c| c.get("order").unwrap().label())
            .collect();
        assert_eq!(
            orders,
            vec!["1", "1", "1", "1", "2", "2", "2", "2"],
            "the aggregation did not vary slowest"
        );

        let resolutions: Vec<String> = plan
            .combinations
            .iter()
            .map(|c| c.get("resolution").unwrap().label())
            .collect();
        assert_eq!(
            resolutions,
            vec!["0.01", "0.02", "0.01", "0.02", "0.01", "0.02", "0.01", "0.02"],
            "the clustering did not vary fastest"
        );
    }

    /// What the interface shows before anything starts: how many runs are
    /// expensive, and how many are nearly free.
    #[test]
    fn a_plan_says_how_much_of_it_is_expensive() {
        let shared = vec![
            interval("order", 1.0, 3.0, 3, Scale::Linear, true),
            interval("n_neighbors", 10.0, 20.0, 2, Scale::Linear, true),
        ];
        let leiden = ClustererSweep {
            clusterer: "leiden".into(),
            axes: vec![interval("resolution", 0.01, 0.05, 4, Scale::Log, false)],
        };

        let plan = plan(&shared, &[leiden]);
        assert_eq!(plan.len(), 24, "3 x 2 x 4");
        assert_eq!(plan.aggregations(), 3, "one per order");
        assert_eq!(plan.reductions(), 6, "one per order and neighbourhood");
    }

    /// Several clusterers, each varying what only it reads.
    #[test]
    fn each_clusterer_varies_its_own_parameters() {
        let shared = vec![interval("n_neighbors", 10.0, 20.0, 2, Scale::Linear, true)];
        let leiden = ClustererSweep {
            clusterer: "leiden".into(),
            axes: vec![interval("resolution", 0.01, 0.05, 3, Scale::Log, false)],
        };
        let gmm = ClustererSweep {
            clusterer: "gmm".into(),
            axes: vec![interval("n_clusters", 4.0, 8.0, 2, Scale::Linear, true)],
        };

        let plan = plan(&shared, &[leiden, gmm]);
        // Two neighbourhoods, times (three resolutions + two cluster counts).
        assert_eq!(plan.len(), 2 * (3 + 2));

        let leiden_runs: Vec<_> = plan
            .combinations
            .iter()
            .filter(|c| c.clusterer == "leiden")
            .collect();
        assert_eq!(leiden_runs.len(), 6);
        assert!(
            leiden_runs.iter().all(|c| c.get("n_clusters").is_none()),
            "leiden was given a parameter it does not read"
        );

        let gmm_runs: Vec<_> = plan
            .combinations
            .iter()
            .filter(|c| c.clusterer == "gmm")
            .collect();
        assert_eq!(gmm_runs.len(), 4);
        assert!(gmm_runs.iter().all(|c| c.get("resolution").is_none()));
    }

    /// Every combination says which clusterer it is, because the clusterer is
    /// itself a setting the run has to be given.
    #[test]
    fn every_combination_carries_its_clusterer() {
        let leiden = ClustererSweep {
            clusterer: "leiden".into(),
            axes: vec![interval("resolution", 0.01, 0.05, 2, Scale::Log, false)],
        };
        let plan = plan(&[], &[leiden]);
        for combination in &plan.combinations {
            assert_eq!(combination.get("clusterer_type").unwrap().label(), "leiden");
        }
    }

    /// A clusterer with nothing varying is still one run: the user asked to try
    /// that clusterer.
    #[test]
    fn a_clusterer_with_no_swept_parameter_is_one_run() {
        let plan = plan(&[], &[ClustererSweep::new("gmm")]);
        assert_eq!(plan.len(), 1);
        assert_eq!(plan.combinations[0].clusterer, "gmm");
    }

    /// And no clusterer at all is no sweep: there is nothing to run.
    #[test]
    fn a_plan_with_no_clusterer_is_empty() {
        let shared = vec![interval("n_neighbors", 10.0, 20.0, 2, Scale::Linear, true)];
        assert!(plan(&shared, &[]).is_empty());
    }

    /// The count before duplicates were collapsed, so the interface can say
    /// what it dropped rather than quietly running fewer points than asked.
    #[test]
    fn a_plan_reports_what_was_asked_for_as_well_as_what_is_left() {
        let gmm = ClustererSweep {
            clusterer: "gmm".into(),
            // Four points over an integer range of three. The screen no longer
            // offers this shape for a whole number, but a plan built by hand —
            // or read back from a saved sweep — still has to collapse it.
            axes: vec![interval("n_clusters", 4.0, 6.0, 4, Scale::Linear, true)],
        };
        let plan = plan(&[], &[gmm]);
        assert_eq!(plan.requested, 4);
        assert_eq!(plan.len(), 3, "the repeated point was collapsed");
    }

    /// The columns of the comparison table, in the order the stages run.
    #[test]
    fn the_columns_follow_the_stages() {
        let shared = vec![
            interval("n_neighbors", 10.0, 20.0, 2, Scale::Linear, true),
            interval("order", 1.0, 2.0, 2, Scale::Linear, true),
        ];
        let leiden = ClustererSweep {
            clusterer: "leiden".into(),
            axes: vec![interval("resolution", 0.01, 0.05, 2, Scale::Log, false)],
        };
        let plan = plan(&shared, &[leiden]);
        assert_eq!(
            plan.columns(),
            vec!["order", "n_neighbors", "clusterer_type", "resolution"]
        );
    }

    // -----------------------------------------------------------------------
    // Which parameters belong where
    // -----------------------------------------------------------------------

    /// The stage of a key decides both the order of the product and what the
    /// cost estimate counts, so the two cannot disagree.
    #[test]
    fn every_parameter_is_assigned_to_the_stage_that_recomputes_it() {
        assert_eq!(stage_of("order"), Stage::Aggregation);
        assert_eq!(stage_of("n_neighbors"), Stage::Reduction);
        assert_eq!(stage_of("dim_clust"), Stage::Reduction);
        assert_eq!(stage_of("min_dist"), Stage::Reduction);
        assert_eq!(stage_of("metric"), Stage::Reduction);
        assert_eq!(stage_of("resolution"), Stage::Clustering);
        assert_eq!(stage_of("n_clusters"), Stage::Clustering);
    }

    /// A clusterer is only offered the parameters it reads. Offering leiden a
    /// `n_clusters` axis would produce runs the register records as identical,
    /// which is a sweep that silently does nothing.
    #[test]
    fn a_clusterer_is_offered_only_what_it_reads() {
        assert_eq!(
            ClustererSweep::parameters_of("leiden"),
            &["resolution", "k_cluster"]
        );
        assert_eq!(ClustererSweep::parameters_of("gmm"), &["n_clusters"]);
        assert_eq!(ClustererSweep::parameters_of("spectral"), &["n_clusters"]);
    }

    /// And every clusterer the sweep offers is one the pipeline implements.
    #[test]
    fn the_sweep_offers_only_clusterers_that_exist() {
        assert_eq!(available_clusterers(), IMPLEMENTED_CLUSTERERS);
        for clusterer in available_clusterers() {
            assert!(
                !ClustererSweep::parameters_of(clusterer).is_empty(),
                "`{clusterer}` is offered with nothing to vary"
            );
        }
    }

    // -----------------------------------------------------------------------
    // How a value reaches the configuration
    // -----------------------------------------------------------------------

    /// `order` is the one integer the configuration spells as a string, and the
    /// validation accepts either — so a plan may write it as a number.
    #[test]
    fn a_point_becomes_the_yaml_value_it_stands_for() {
        assert_eq!(
            Point::Integer(2).to_yaml(),
            serde_yaml::Value::Number(2.into())
        );
        assert_eq!(
            Point::Text("leiden".into()).to_yaml(),
            serde_yaml::Value::String("leiden".into())
        );
        match Point::Number(0.05).to_yaml() {
            serde_yaml::Value::Number(n) => assert!((n.as_f64().unwrap() - 0.05).abs() < 1e-12),
            other => panic!("a decimal became {other:?}"),
        }
    }

    // -----------------------------------------------------------------------
    // From a combination to a configuration
    // -----------------------------------------------------------------------

    fn configuration() -> RawConfig {
        RawConfig::from_yaml_str(
            "\
Niche Analysis:
  Processing method: Aggregated nodes
  Phenotype column: Cluster
  Aggregated nodes:
    reducer_type: umap
    clusterer_type: gmm
    n_clusters: 6
    resolution: 0.05
    n_neighbors: 20
    order: '1'
  Per sample:
    clusterer_type: gmm
    n_clusters: 5
",
        )
        .unwrap()
    }

    /// A swept run is an ordinary run: the keys the sweep varies are replaced
    /// and everything else is left exactly as the main interface has it.
    #[test]
    fn a_combination_replaces_only_the_keys_it_varies() {
        let leiden = ClustererSweep {
            clusterer: "leiden".into(),
            axes: vec![interval("resolution", 0.01, 0.01, 1, Scale::Log, false)],
        };
        let plan = plan(&[], &[leiden]);
        let updated = apply(&configuration(), &plan.combinations[0], AGGREGATED);

        let sub = updated.get(NICHE, AGGREGATED).unwrap();
        assert_eq!(sub.get("clusterer_type").unwrap().as_str(), Some("leiden"));
        assert_eq!(sub.get("resolution").unwrap().as_f64(), Some(0.01));
        // Untouched.
        assert_eq!(sub.get("n_neighbors").unwrap().as_i64(), Some(20));
        assert_eq!(sub.get("order").unwrap().as_str(), Some("1"));
        assert_eq!(
            updated.get(NICHE, "Phenotype column").unwrap().as_str(),
            Some("Cluster")
        );
    }

    /// The other sub-section is not touched, so a sweep of the aggregated mode
    /// cannot disturb the per-sample settings.
    #[test]
    fn the_other_sub_section_is_left_alone() {
        let gmm = ClustererSweep {
            clusterer: "gmm".into(),
            axes: vec![interval("n_clusters", 9.0, 9.0, 1, Scale::Linear, true)],
        };
        let plan = plan(&[], &[gmm]);
        let updated = apply(&configuration(), &plan.combinations[0], AGGREGATED);

        assert_eq!(
            updated
                .get(NICHE, PER_SAMPLE)
                .unwrap()
                .get("n_clusters")
                .unwrap()
                .as_i64(),
            Some(5),
            "the per-sample sub-section was modified"
        );
    }

    /// And the configuration that comes out is one `mosna` accepts: the
    /// validation is the same one the main interface's runs go through.
    #[test]
    fn the_configuration_a_combination_produces_is_valid() {
        let shared = vec![
            interval("n_neighbors", 10.0, 20.0, 2, Scale::Linear, true),
            choices("metric", &["euclidean", "cosine"]),
        ];
        let leiden = ClustererSweep {
            clusterer: "leiden".into(),
            axes: vec![interval("resolution", 0.005, 0.05, 3, Scale::Log, false)],
        };
        let base = RawConfig::from_yaml_str(include_str!("../../tests/sweep_config.yaml")).unwrap();

        for combination in plan(&shared, &[leiden]).combinations {
            let updated = apply(&base, &combination, AGGREGATED);
            mosna_config::validate::assert_params::assert_params(
                mosna_config::validate::assert_params::Analysis::NicheAnalysis,
                updated.section(NICHE).unwrap(),
            )
            .unwrap_or_else(|e| panic!("{:?} produced an invalid configuration: {e}", combination));
        }
    }

    /// The sweep follows the configuration's own processing method: showing one
    /// mode and sweeping the other would put the runs where nobody looks.
    #[test]
    fn the_sweep_follows_the_configured_processing_method() {
        assert_eq!(swept_subsection(&configuration()), AGGREGATED);

        let mut per_sample = configuration();
        per_sample.set(
            NICHE,
            "Processing method",
            serde_yaml::Value::String("Per sample".into()),
        );
        assert_eq!(swept_subsection(&per_sample), PER_SAMPLE);
    }

    // -----------------------------------------------------------------------
    // The comparison table
    // -----------------------------------------------------------------------

    fn finished(run: &str, niches: usize, largest: usize, components: Option<usize>) -> Row {
        let leiden = ClustererSweep {
            clusterer: "leiden".into(),
            axes: vec![interval("resolution", 0.01, 0.01, 1, Scale::Log, false)],
        };
        let mut row = Row::pending(plan(&[], &[leiden]).combinations.remove(0));
        row.run = Some(run.to_string());
        row.state = RunState::Done;
        row.niches = Some(niches);
        row.largest = Some(largest);
        row.graph_components = components;
        row
    }

    #[test]
    fn the_table_is_one_line_per_run() {
        let columns = vec!["clusterer_type".to_string(), "resolution".to_string()];
        let csv = to_csv(&columns, &[finished("1-1-1", 418, 5719, Some(418))]);

        let lines: Vec<&str> = csv.lines().collect();
        assert_eq!(
            lines[0],
            "run,status,clusterer_type,resolution,niches,largest_niche,graph_components"
        );
        assert_eq!(lines[1], "1-1-1,done,leiden,0.01,418,5719,418");
    }

    /// A run that has not happened yet still has a line: the table is the plan
    /// as much as it is the result, and a sweep interrupted half-way says which
    /// points it never reached.
    #[test]
    fn a_pending_run_is_still_a_line() {
        let leiden = ClustererSweep {
            clusterer: "leiden".into(),
            axes: vec![interval("resolution", 0.01, 0.01, 1, Scale::Log, false)],
        };
        let row = Row::pending(plan(&[], &[leiden]).combinations.remove(0));
        let csv = to_csv(&["resolution".to_string()], &[row]);

        assert_eq!(csv.lines().nth(1).unwrap(), ",pending,0.01,,,");
    }

    /// A clusterer that builds no graph reports no components, and the column
    /// is empty rather than zero — zero components would be a claim.
    #[test]
    fn a_missing_measurement_is_an_empty_field_and_not_a_zero() {
        let csv = to_csv(&[], &[finished("1-1-1", 6, 15330, None)]);
        assert_eq!(csv.lines().nth(1).unwrap(), "1-1-1,done,6,15330,");
    }

    /// Column names come from the configuration and values from the user, so a
    /// comma in either must not shift every column after it.
    #[test]
    fn a_field_holding_a_comma_is_quoted() {
        let mut row = finished("1-1-1", 6, 100, None);
        row.combination.settings.push((
            "stat_funcs".to_string(),
            Point::Text("np.mean,np.std".into()),
        ));
        let csv = to_csv(&["stat_funcs".to_string()], &[row]);
        assert!(
            csv.lines().nth(1).unwrap().contains("\"np.mean,np.std\""),
            "{csv}"
        );
    }

    /// What a finished run recorded about itself, read straight out of its
    /// `run.json` — nothing is recomputed to fill the table.
    #[test]
    fn a_row_absorbs_what_the_run_recorded() {
        let leiden = ClustererSweep {
            clusterer: "leiden".into(),
            axes: vec![interval("resolution", 0.01, 0.01, 1, Scale::Log, false)],
        };
        let mut row = Row::pending(plan(&[], &[leiden]).combinations.remove(0));
        row.absorb(&serde_json::json!({
            "result": { "niches": 418, "sizes": [5719, 12, 3], "graph_components": 418 }
        }));

        assert_eq!(row.niches, Some(418));
        assert_eq!(row.largest, Some(5719));
        assert_eq!(row.graph_components, Some(418));
    }

    /// A record this version does not understand leaves the outcome empty
    /// rather than stopping the ninety-nine runs after it.
    #[test]
    fn an_unreadable_record_leaves_the_outcome_empty() {
        let leiden = ClustererSweep {
            clusterer: "leiden".into(),
            axes: vec![interval("resolution", 0.01, 0.01, 1, Scale::Log, false)],
        };
        let mut row = Row::pending(plan(&[], &[leiden]).combinations.remove(0));
        row.absorb(&serde_json::json!({ "something": "else" }));

        assert_eq!(row.niches, None);
        assert_eq!(row.largest, None);
        assert_eq!(row.graph_components, None);
    }

    // -----------------------------------------------------------------------
    // The form the screen edits
    // -----------------------------------------------------------------------

    /// A fresh form is the configuration the main interface is already
    /// showing: every count at one, so the grid is the single run that would
    /// have been started by hand. Raising a count is what opens it out.
    #[test]
    fn a_fresh_form_is_one_run_once_a_clusterer_is_chosen() {
        let mut form = SweepForm::from_config(&configuration());
        assert_eq!(form.problems(), vec!["choose at least one clusterer"]);

        form.clusterers
            .iter_mut()
            .find(|c| c.clusterer == "gmm")
            .unwrap()
            .selected = true;

        assert!(form.is_runnable());
        assert_eq!(form.plan().len(), 1, "every count is one");
    }

    /// And the fields are seeded from the configuration, so that single run is
    /// the one the main interface would have produced rather than a screen of
    /// blanks complaining that they are not numbers.
    #[test]
    fn a_fresh_form_is_seeded_from_the_configuration() {
        let form = SweepForm::from_config(&configuration());

        let neighbours = form.shared.iter().find(|a| a.key == "n_neighbors").unwrap();
        assert_eq!(neighbours.from, "20");
        assert_eq!(neighbours.problem(), None);

        let gmm = form
            .clusterers
            .iter()
            .find(|c| c.clusterer == "gmm")
            .unwrap();
        let clusters = gmm.axes.iter().find(|a| a.key == "n_clusters").unwrap();
        assert_eq!(clusters.from, "6");

        // A categorical opens on the value in force.
        let order = form.shared.iter().find(|a| a.key == "order").unwrap();
        assert_eq!(
            order
                .choices
                .iter()
                .filter(|(_, picked)| *picked)
                .map(|(name, _)| name.as_str())
                .collect::<Vec<_>>(),
            vec!["1"]
        );
        assert_eq!(form.normalize, "total");
    }

    /// The example from the specification: resolution between 0.05 and 0.005,
    /// two values, with leiden.
    #[test]
    fn the_specifications_example_produces_two_runs() {
        let mut form = SweepForm::from_config(&configuration());
        let leiden = form
            .clusterers
            .iter_mut()
            .find(|c| c.clusterer == "leiden")
            .unwrap();
        leiden.selected = true;
        let resolution = leiden
            .axes
            .iter_mut()
            .find(|a| a.key == "resolution")
            .unwrap();
        set(resolution, "0.05", "0.005", "2");

        assert!(form.is_runnable());
        let plan = form.plan();
        assert_eq!(plan.len(), 2);
        let values: Vec<String> = plan
            .combinations
            .iter()
            .map(|c| c.get("resolution").unwrap().label())
            .collect();
        assert_eq!(values, vec!["0.005", "0.05"]);
    }

    /// `resolution` is offered in log by default, because that is the scale it
    /// acts on; an integer count is offered linearly.
    #[test]
    fn each_parameter_opens_on_the_scale_that_suits_it() {
        assert_eq!(Sampling::of("resolution"), Sampling::Decimal(Scale::Log));
        assert_eq!(Sampling::of("n_clusters"), Sampling::Integer);
        assert_eq!(Sampling::of("min_dist"), Sampling::Decimal(Scale::Linear));
        assert!(matches!(Sampling::of("metric"), Sampling::Choice(_)));

        let axis = AxisEdit::new("resolution", Stage::Clustering, Sampling::of("resolution"));
        assert_eq!(axis.scale, Scale::Log, "and the field opens on it");
        assert_eq!(axis.count, "1", "and holds the parameter still until asked");
    }

    /// A categorical parameter offers every value the pipeline accepts, and
    /// several may be picked at once.
    #[test]
    fn a_categorical_axis_offers_what_the_pipeline_accepts() {
        let axis = AxisEdit::new("metric", Stage::Reduction, Sampling::of("metric"));
        let offered: Vec<&str> = axis.choices.iter().map(|(name, _)| name.as_str()).collect();
        assert_eq!(offered, vec!["euclidean", "manhattan", "cosine"]);

        // Exactly one is ticked to begin with — the value the pipeline would
        // have used — so the row opens on one value rather than on none.
        let picked: Vec<&str> = axis
            .choices
            .iter()
            .filter(|(_, picked)| *picked)
            .map(|(name, _)| name.as_str())
            .collect();
        assert_eq!(picked, vec!["euclidean"]);
        assert_eq!(axis.problem(), None);
    }

    /// A bound half-typed is not an error to raise: the plan simply has one
    /// fewer dimension until the field makes sense again.
    ///
    /// `"0."` is deliberately not among these — Rust parses it as zero, so a
    /// user typing `0.05` passes through a state that is already a number and
    /// the axis never blinks out from under them.
    #[test]
    fn a_half_typed_bound_leaves_the_axis_out_rather_than_failing() {
        for typed in ["", "-", "1e", "0,05"] {
            let mut axis =
                AxisEdit::new("resolution", Stage::Clustering, Sampling::of("resolution"));
            set(&mut axis, typed, "0.05", "3");

            assert!(
                matches!(axis.to_axis().values, Values::Fixed),
                "`{typed}` was read as a bound"
            );
            assert_eq!(axis.problem().unwrap(), "`from` is not a number");
        }
    }

    /// And a bound that is already a number mid-way through being typed keeps
    /// the axis alive, so the count beside it does not flicker.
    #[test]
    fn a_bound_that_parses_keeps_the_axis() {
        let mut axis = AxisEdit::new("resolution", Stage::Clustering, Sampling::of("resolution"));
        set(&mut axis, "0.", "0.05", "3");

        assert!(matches!(axis.to_axis().values, Values::Interval { .. }));
        assert_eq!(axis.problem(), None);
    }

    /// A count of zero is a question, not a sweep. Decimals are the ones that
    /// still carry a count; whole numbers are ticked from a menu.
    #[test]
    fn a_count_of_zero_is_reported() {
        let mut axis = AxisEdit::new("resolution", Stage::Clustering, Sampling::of("resolution"));
        set(&mut axis, "0.005", "0.05", "0");
        assert_eq!(axis.problem().unwrap(), "how many values?");
    }

    /// A categorical axis the user has emptied says so: unticking everything
    /// asks for a parameter with no values, which is not a sweep.
    #[test]
    fn a_categorical_axis_with_nothing_picked_is_reported() {
        let mut axis = AxisEdit::new("metric", Stage::Reduction, Sampling::of("metric"));
        for (_, picked) in &mut axis.choices {
            *picked = false;
        }
        assert_eq!(axis.problem().unwrap(), "pick at least one value");
    }

    /// `order` is a pair of choices rather than an interval: one and two are
    /// what the aggregation implements, and an interval would invite a third.
    #[test]
    fn the_neighbourhood_order_offers_the_two_values_that_exist() {
        let axis = AxisEdit::new("order", Stage::Aggregation, Sampling::of("order"));
        let offered: Vec<&str> = axis.choices.iter().map(|(name, _)| name.as_str()).collect();
        assert_eq!(offered, vec!["1", "2"]);
    }

    /// One value greys the second bound out, so nothing typed there can reach
    /// the grid — and a stale second bound cannot keep the row out of it.
    #[test]
    fn a_single_value_ignores_the_second_bound() {
        let mut axis = AxisEdit::new("resolution", Stage::Clustering, Sampling::of("resolution"));
        set(&mut axis, "0.05", "nonsense", "1");

        assert!(!axis.spans_an_interval());
        assert_eq!(axis.problem(), None, "the second field is not in play");
        assert_eq!(labels(&axis.to_axis().points()), vec!["0.05"]);

        set(&mut axis, "0.05", "nonsense", "3");
        assert!(axis.spans_an_interval());
        assert_eq!(axis.problem().unwrap(), "`to` is not a number");
    }

    /// Two clusterers, each varying only what it reads — the case the
    /// specification asks for.
    #[test]
    fn two_clusterers_each_vary_their_own_parameters() {
        let mut form = SweepForm::from_config(&configuration());
        for edit in &mut form.clusterers {
            if edit.clusterer == "leiden" {
                edit.selected = true;
                set(
                    edit.axes
                        .iter_mut()
                        .find(|a| a.key == "resolution")
                        .unwrap(),
                    "0.005",
                    "0.05",
                    "3",
                );
            }
            if edit.clusterer == "gmm" {
                edit.selected = true;
                let axis = edit
                    .axes
                    .iter_mut()
                    .find(|a| a.key == "n_clusters")
                    .unwrap();
                set(axis, "4", "8", "1");
                tick(axis, &["4", "6", "8"]);
            }
        }

        let plan = form.plan();
        assert_eq!(plan.len(), 6, "three resolutions and three cluster counts");
        assert!(form.is_runnable());
    }

    /// A shared axis multiplies every clusterer's grid, and the aggregation
    /// still varies slowest so the cache pays off.
    #[test]
    fn a_shared_axis_multiplies_every_clusterers_grid() {
        let mut form = SweepForm::from_config(&configuration());
        let order = form.shared.iter_mut().find(|a| a.key == "order").unwrap();
        for (_, picked) in &mut order.choices {
            *picked = true;
        }

        let gmm = form
            .clusterers
            .iter_mut()
            .find(|c| c.clusterer == "gmm")
            .unwrap();
        gmm.selected = true;
        let axis = gmm.axes.iter_mut().find(|a| a.key == "n_clusters").unwrap();
        set(axis, "4", "6", "1");
        tick(axis, &["4", "5", "6"]);

        let plan = form.plan();
        assert_eq!(plan.len(), 6);
        assert_eq!(plan.aggregations(), 2, "one per order");
        let orders: Vec<String> = plan
            .combinations
            .iter()
            .map(|c| c.get("order").unwrap().label())
            .collect();
        assert_eq!(orders, vec!["1", "1", "1", "2", "2", "2"]);
    }

    /// A parameter that changes no partition is not offered: sweeping it would
    /// produce a grid of points the register collapses onto each other.
    #[test]
    fn the_rendering_parameters_are_not_sweepable() {
        let offered: Vec<&str> = Stage::all()
            .into_iter()
            .flat_map(sweepable)
            .map(|(key, _)| *key)
            .collect();
        for absent in ["normalize", "Phenotype column", "min_cluster_size", "CPU"] {
            assert!(
                !offered.contains(&absent),
                "`{absent}` is offered but changes no partition"
            );
        }
    }

    /// And every parameter that is offered is one the pipeline reads.
    #[test]
    fn every_offered_parameter_reaches_the_pipeline() {
        let base = RawConfig::from_yaml_str(include_str!("../../tests/sweep_config.yaml")).unwrap();
        for (key, _) in Stage::all().into_iter().flat_map(sweepable) {
            let present = base
                .get(NICHE, AGGREGATED)
                .and_then(|sub| sub.get(key))
                .is_some();
            assert!(present, "`{key}` is offered but no sub-section holds it");
        }
    }

    // -----------------------------------------------------------------------
    // A sweep under way
    // -----------------------------------------------------------------------

    fn three_runs() -> Sweep {
        let gmm = ClustererSweep {
            clusterer: "gmm".into(),
            axes: vec![interval("n_clusters", 4.0, 6.0, 3, Scale::Linear, true)],
        };
        Sweep::new(&plan(&[], &[gmm]))
    }

    #[test]
    fn a_sweep_starts_with_everything_pending() {
        let sweep = three_runs();
        assert_eq!(sweep.total(), 3);
        assert_eq!(sweep.finished(), 0);
        assert_eq!(sweep.percent(), 0);
        assert_eq!(sweep.next_pending(), Some(0));
        assert!(!sweep.is_over());
    }

    /// The bar the specification asks for: a percentage of the runs the chosen
    /// parameters imply.
    #[test]
    fn the_bar_counts_the_runs_the_parameters_imply() {
        let mut sweep = three_runs();
        sweep.begin(0);
        sweep.end(true, Some("1-1-1".into()), None);
        assert_eq!(sweep.percent(), 33);

        sweep.begin(1);
        sweep.end(true, Some("1-1-2".into()), None);
        assert_eq!(sweep.percent(), 67);

        sweep.begin(2);
        sweep.end(true, Some("1-1-3".into()), None);
        assert_eq!(sweep.percent(), 100);
        assert!(sweep.is_over());
    }

    /// A run that failed still counts as done for the bar — the sweep has got
    /// past it — but is counted apart in the caption.
    #[test]
    fn a_failed_run_advances_the_bar_and_is_reported() {
        let mut sweep = three_runs();
        sweep.begin(0);
        sweep.end(false, None, None);

        assert_eq!(sweep.finished(), 1);
        assert_eq!(sweep.failed(), 1);
        assert_eq!(sweep.rows[0].state, RunState::Failed);
        assert_eq!(sweep.next_pending(), Some(1), "the sweep carries on");
    }

    /// Stopping lets the run in flight finish: killing it would leave a
    /// directory the register calls `running` and half a set of figures.
    #[test]
    fn stopping_hands_out_no_more_runs_but_finishes_the_one_in_flight() {
        let mut sweep = three_runs();
        sweep.begin(0);
        sweep.stopping = true;

        assert_eq!(sweep.next_pending(), None);
        assert!(!sweep.is_over(), "a run is still in flight");

        sweep.end(true, Some("1-1-1".into()), None);
        assert!(sweep.is_over());
        assert_eq!(sweep.rows[1].state, RunState::Pending, "never started");
    }

    /// The run directory and the outcome land on the row that was running, so
    /// the table can be read while the sweep is still going.
    #[test]
    fn a_finished_run_fills_in_its_own_row() {
        let mut sweep = three_runs();
        sweep.begin(0);
        sweep.end(
            true,
            Some("1-1-7".into()),
            Some(&serde_json::json!({
                "result": { "niches": 5, "sizes": [10, 20, 30], "graph_components": null }
            })),
        );

        let row = &sweep.rows[0];
        assert_eq!(row.run.as_deref(), Some("1-1-7"));
        assert_eq!(row.niches, Some(5));
        assert_eq!(row.largest, Some(30));
        assert_eq!(row.graph_components, None);
    }

    /// An empty sweep is over before it starts rather than dividing by zero.
    #[test]
    fn an_empty_sweep_is_over_and_shows_no_progress() {
        let sweep = Sweep::new(&plan(&[], &[]));
        assert_eq!(sweep.fraction(), 0.0);
        assert!(sweep.is_over());
    }

    #[test]
    fn the_caption_says_where_the_sweep_has_got_to() {
        let mut sweep = three_runs();
        assert!(sweep.caption().contains("0 of 3"));

        sweep.begin(0);
        sweep.end(false, None, None);
        sweep.begin(1);
        sweep.end(true, None, None);
        sweep.begin(2);
        sweep.end(true, None, None);

        let caption = sweep.caption();
        assert!(caption.contains("finished"), "{caption}");
        assert!(caption.contains("1 failed"), "{caption}");
    }

    /// Ticking `log` must change the values, and the row under the field is
    /// where that shows.
    #[test]
    fn the_log_box_changes_the_values_it_produces() {
        let mut axis = AxisEdit::new("resolution", Stage::Clustering, Sampling::of("resolution"));
        set(&mut axis, "0.005", "0.05", "4");

        axis.scale = Scale::Log;
        let log = labels(&axis.to_axis().points());
        axis.scale = Scale::Linear;
        let linear = labels(&axis.to_axis().points());

        assert_ne!(log, linear, "the scale made no difference");
        assert_eq!(log, vec!["0.005", "0.010772", "0.023208", "0.05"]);
        assert_eq!(linear, vec!["0.005", "0.02", "0.035", "0.05"]);
    }

    /// At two values the two scales agree, because two points of an interval
    /// are its ends whichever way they are spaced. The box is offered from
    /// three values up, where it starts meaning something.
    #[test]
    fn at_two_values_the_scale_cannot_show() {
        let mut axis = AxisEdit::new("resolution", Stage::Clustering, Sampling::of("resolution"));
        set(&mut axis, "0.005", "0.05", "2");

        axis.scale = Scale::Log;
        let log = labels(&axis.to_axis().points());
        axis.scale = Scale::Linear;
        assert_eq!(log, labels(&axis.to_axis().points()));
        assert!(!axis.scale_matters(), "the box would change nothing here");

        set(&mut axis, "0.005", "0.05", "3");
        assert!(axis.scale_matters());
    }

    // -----------------------------------------------------------------------
    // Whole numbers are ticked from a menu, not sampled from an interval
    // -----------------------------------------------------------------------

    /// The case that prompted this: `dim_clust` is two, or three, or whatever
    /// the data allows — a handful of values each meaning something, not four
    /// points of an interval to be rounded.
    #[test]
    fn a_whole_number_offers_every_value_between_its_bounds() {
        let mut axis = AxisEdit::new("dim_clust", Stage::Reduction, Sampling::of("dim_clust"));
        axis.from = "2".into();
        axis.to = "5".into();
        axis.refresh_candidates();

        assert_eq!(axis.candidates(), vec!["2", "3", "4", "5"]);
        assert!(axis.is_a_list());
    }

    /// A fresh row is one value — the one in force — with the rest of the menu
    /// waiting to be ticked.
    #[test]
    fn a_whole_number_row_opens_on_a_single_value() {
        let axis = AxisEdit::new("dim_clust", Stage::Reduction, Sampling::of("dim_clust"));
        let picked: Vec<String> = axis
            .choices
            .iter()
            .filter(|(_, picked)| *picked)
            .map(|(name, _)| name.clone())
            .collect();

        assert_eq!(picked, vec!["2"], "one value, and it is the default");
        assert!(axis.choices.len() > 1, "and the menu offers more");
        assert_eq!(axis.problem(), None);
    }

    /// Ticking several is what opens the sweep out.
    #[test]
    fn ticking_several_whole_numbers_produces_several_runs() {
        let mut axis = AxisEdit::new("n_clusters", Stage::Clustering, Sampling::of("n_clusters"));
        axis.from = "4".into();
        axis.to = "8".into();
        axis.refresh_candidates();
        for (name, picked) in &mut axis.choices {
            *picked = matches!(name.as_str(), "4" | "6" | "8");
        }

        assert_eq!(labels(&axis.to_axis().points()), vec!["4", "6", "8"]);
    }

    /// Narrowing the bounds drops the values they no longer offer: a menu that
    /// kept them would run numbers it had stopped showing.
    #[test]
    fn narrowing_the_bounds_drops_what_they_no_longer_offer() {
        let mut axis = AxisEdit::new("n_clusters", Stage::Clustering, Sampling::of("n_clusters"));
        axis.from = "4".into();
        axis.to = "10".into();
        axis.refresh_candidates();
        for (name, picked) in &mut axis.choices {
            *picked = matches!(name.as_str(), "5" | "9");
        }

        axis.to = "6".into();
        axis.refresh_candidates();

        assert_eq!(axis.candidates(), vec!["4", "5", "6"]);
        assert_eq!(labels(&axis.to_axis().points()), vec!["5"], "9 is gone");
    }

    /// And a range that drops everything the user had chosen falls back to its
    /// first value rather than leaving a row that produces no run.
    #[test]
    fn a_range_that_drops_every_choice_keeps_one() {
        let mut axis = AxisEdit::new("n_clusters", Stage::Clustering, Sampling::of("n_clusters"));
        axis.from = "4".into();
        axis.to = "10".into();
        axis.refresh_candidates();
        for (name, picked) in &mut axis.choices {
            *picked = name == "9";
        }

        axis.from = "2".into();
        axis.to = "3".into();
        axis.refresh_candidates();

        assert_eq!(labels(&axis.to_axis().points()), vec!["2"]);
        assert_eq!(axis.problem(), None);
    }

    /// A menu is a menu, not an interval in disguise: bounds a thousand apart
    /// are capped rather than listed.
    #[test]
    fn an_unusable_menu_is_capped() {
        let mut axis = AxisEdit::new("n_neighbors", Stage::Reduction, Sampling::of("n_neighbors"));
        axis.from = "2".into();
        axis.to = "100000".into();
        assert!(axis.candidates().len() <= 64);
    }

    /// Bounds that are not whole numbers say so rather than offering nothing.
    #[test]
    fn bounds_that_are_not_whole_numbers_are_reported() {
        let mut axis = AxisEdit::new("dim_clust", Stage::Reduction, Sampling::of("dim_clust"));
        axis.from = "two".into();
        axis.refresh_candidates();
        assert_eq!(axis.problem().unwrap(), "the bounds are not whole numbers");
    }

    /// The four parameters the menu applies to, and the two it does not.
    #[test]
    fn the_whole_numbers_are_the_ones_chosen_from_a_menu() {
        for key in ["dim_clust", "n_clusters", "k_cluster", "n_neighbors"] {
            assert_eq!(Sampling::of(key), Sampling::Integer, "`{key}`");
        }
        for key in ["resolution", "min_dist"] {
            assert!(
                matches!(Sampling::of(key), Sampling::Decimal(_)),
                "`{key}` is a decimal and keeps its interval"
            );
        }
    }

    // -----------------------------------------------------------------------
    // A reduction nobody runs
    // -----------------------------------------------------------------------

    /// Choosing `none` and nothing else means no projection is ever computed,
    /// so the parameters only the reducer reads decide nothing. Sweeping them
    /// then builds a dimension that collapses the moment it runs: the register
    /// records the runs as identical, because `reduction_parameters` is absent
    /// for a run that does not reduce.
    #[test]
    fn a_reduction_nobody_runs_retires_the_parameters_only_it_reads() {
        let mut form = SweepForm::from_config(&configuration());
        pick(&mut form, "reducer_type", &["none"]);

        assert!(form.reduction_is_inert());
        for key in ["dim_clust", "min_dist", "metric"] {
            assert!(form.is_inert(key), "`{key}` still counts without a reducer");
        }
    }

    /// `n_neighbors` is not the reducer's alone: it caps leiden's graph degree
    /// through `effective_k_cluster`, so it still decides something with no
    /// reduction at all — and greying it would be a control that changes the
    /// answer while looking as though it cannot.
    #[test]
    fn the_neighbourhood_survives_a_run_without_a_reduction() {
        let mut form = SweepForm::from_config(&configuration());
        pick(&mut form, "reducer_type", &["none"]);

        assert!(!form.is_inert("n_neighbors"));
    }

    /// Both reducers chosen is a sweep that does reduce, for half its runs, so
    /// nothing is retired.
    #[test]
    fn keeping_umap_among_the_choices_keeps_its_parameters() {
        let mut form = SweepForm::from_config(&configuration());
        pick(&mut form, "reducer_type", &["umap", "none"]);

        assert!(!form.reduction_is_inert());
        for key in ["dim_clust", "min_dist", "metric"] {
            assert!(!form.is_inert(key));
        }
    }

    /// And a retired parameter is dropped from the grid, not merely greyed: a
    /// sweep of three `dim_clust` values with no reducer would otherwise run
    /// three times and record one run.
    #[test]
    fn a_retired_parameter_does_not_multiply_the_grid() {
        let mut form = SweepForm::from_config(&configuration());
        form.clusterers
            .iter_mut()
            .find(|c| c.clusterer == "gmm")
            .unwrap()
            .selected = true;

        let dim = form
            .shared
            .iter_mut()
            .find(|a| a.key == "dim_clust")
            .unwrap();
        set(dim, "2", "5", "1");
        tick(dim, &["2", "3", "4"]);
        assert_eq!(form.plan().len(), 3, "with umap the three are three runs");

        pick(&mut form, "reducer_type", &["none"]);
        assert_eq!(
            form.plan().len(),
            1,
            "without a reducer the three collapse into one"
        );
    }

    /// Tick exactly these choices on a shared axis.
    fn pick(form: &mut SweepForm, key: &str, wanted: &[&str]) {
        let axis = form.shared.iter_mut().find(|a| a.key == key).unwrap();
        for (name, picked) in &mut axis.choices {
            *picked = wanted.contains(&name.as_str());
        }
    }

    // -----------------------------------------------------------------------
    // The General settings a grid is built on
    // -----------------------------------------------------------------------

    /// One General setting missing fails every run of the grid in the same
    /// instant, so it is checked before the screen opens rather than
    /// eighty-four times afterwards.
    #[test]
    fn an_empty_general_setting_is_reported_before_the_screen_opens() {
        let mut config = configuration();
        config.set(NICHE, "Column to aggregate", serde_yaml::Value::Null);

        let problems = general_problems(&config);
        assert!(
            problems.iter().any(|p| p.contains("Column to aggregate")),
            "{problems:?}"
        );
    }

    /// The shipped configuration, whose columns are still empty, is exactly the
    /// case this exists for.
    #[test]
    fn the_shipped_configuration_is_reported_as_incomplete() {
        let config = RawConfig::from_yaml_str(
            "Niche Analysis:\n  Phenotype column: null\n  Column to aggregate: null\n",
        )
        .unwrap();
        let problems = general_problems(&config);
        assert!(problems.len() >= 2, "{problems:?}");
    }

    /// A section that is filled in raises nothing.
    #[test]
    fn a_complete_general_section_raises_nothing() {
        let config =
            RawConfig::from_yaml_str(include_str!("../../tests/sweep_config.yaml")).unwrap();
        assert_eq!(general_problems(&config), Vec::<String>::new());
    }

    /// The parameters the sweep varies are not checked here: they are what the
    /// screen is for.
    #[test]
    fn the_swept_parameters_are_not_part_of_the_check() {
        let mut config =
            RawConfig::from_yaml_str(include_str!("../../tests/sweep_config.yaml")).unwrap();
        config.set(NICHE, AGGREGATED, serde_yaml::Value::Null);
        assert_eq!(general_problems(&config), Vec::<String>::new());
    }

    /// What the confirmation dialog shows: the settings every run will share.
    #[test]
    fn the_summary_names_the_settings_every_run_shares() {
        let config =
            RawConfig::from_yaml_str(include_str!("../../tests/sweep_config.yaml")).unwrap();
        let summary = general_summary(&config);

        let shown: Vec<&str> = summary.iter().map(|(name, _)| name.as_str()).collect();
        assert_eq!(shown, CONFIRMED_SETTINGS);

        let get = |key: &str| {
            summary
                .iter()
                .find(|(name, _)| name == key)
                .map(|(_, value)| value.caption())
        };
        assert_eq!(get("Column to aggregate").as_deref(), Some("Cluster"));
        assert_eq!(get("Network directory").as_deref(), Some("Default"));
        assert_eq!(
            get("X coordinates column for niches").as_deref(),
            Some("X_position")
        );
        assert_eq!(get("Plot Network").as_deref(), Some("false"));
    }

    /// An empty setting reads as a dash rather than as the word `null`.
    #[test]
    fn the_summary_shows_an_empty_setting_as_a_dash() {
        let mut config =
            RawConfig::from_yaml_str(include_str!("../../tests/sweep_config.yaml")).unwrap();
        config.set(NICHE, "Phenotype column", serde_yaml::Value::Null);

        let summary = general_summary(&config);
        let value = summary
            .iter()
            .find(|(name, _)| name == "Phenotype column")
            .map(|(_, value)| value.caption());
        assert_eq!(value.as_deref(), Some("—"));
    }

    // -----------------------------------------------------------------------
    // Stopping
    // -----------------------------------------------------------------------

    /// A sweep that has finished is not a sweep in flight: the screen must
    /// offer to start another rather than leaving a Stop button that can never
    /// do anything again.
    #[test]
    fn a_finished_sweep_is_no_longer_running() {
        let gmm = ClustererSweep {
            clusterer: "gmm".into(),
            axes: vec![interval("n_clusters", 4.0, 5.0, 2, Scale::Linear, true)],
        };
        let mut sweep = Sweep::new(&plan(&[], &[gmm]));
        assert!(sweep.is_running());

        sweep.begin(0);
        sweep.end(true, None, None);
        assert!(sweep.is_running(), "one run is still pending");

        sweep.begin(1);
        sweep.end(true, None, None);
        assert!(!sweep.is_running());
        assert!(sweep.is_over());
    }

    /// And one that was stopped is finished too, once the run in flight ends.
    #[test]
    fn a_stopped_sweep_is_finished_once_its_run_ends() {
        let gmm = ClustererSweep {
            clusterer: "gmm".into(),
            axes: vec![interval("n_clusters", 4.0, 6.0, 3, Scale::Linear, true)],
        };
        let mut sweep = Sweep::new(&plan(&[], &[gmm]));
        sweep.begin(0);
        sweep.stopping = true;
        assert!(sweep.is_running(), "a run is still in flight");

        sweep.end(false, None, None);
        assert!(!sweep.is_running());
        assert!(sweep.caption().contains("stopped"), "{}", sweep.caption());
    }

    /// Every configuration the *screen* produces has to be one `mosna` accepts.
    ///
    /// The earlier version of this built its axes by hand, which walked the
    /// interval path and never the menu — so whole numbers going out as the
    /// strings the menu stores them as went unnoticed until a sweep of
    /// fifty-four runs failed on every one of them with `n_neighbors must be
    /// int`.
    #[test]
    fn every_configuration_the_screen_produces_is_valid() {
        let base = RawConfig::from_yaml_str(include_str!("../../tests/sweep_config.yaml")).unwrap();
        let mut form = SweepForm::from_config(&base);

        for (_, picked) in &mut form
            .shared
            .iter_mut()
            .find(|a| a.key == "order")
            .unwrap()
            .choices
        {
            *picked = true;
        }
        for key in ["n_neighbors", "dim_clust"] {
            let axis = form.shared.iter_mut().find(|a| a.key == key).unwrap();
            set(axis, "2", "6", "1");
            for (_, picked) in &mut axis.choices {
                *picked = true;
            }
        }
        for clusterer in &mut form.clusterers {
            clusterer.selected = true;
            for axis in &mut clusterer.axes {
                if matches!(axis.sampling, Sampling::Integer) {
                    set(axis, "2", "5", "1");
                    for (_, picked) in &mut axis.choices {
                        *picked = true;
                    }
                } else {
                    set(axis, "0.005", "0.5", "3");
                }
            }
        }

        let plan = form.plan();
        assert!(plan.len() > 50, "only {} runs", plan.len());
        for combination in &plan.combinations {
            let updated = apply(&base, combination, AGGREGATED);
            mosna_config::validate::assert_params::assert_params(
                mosna_config::validate::assert_params::Analysis::NicheAnalysis,
                updated.section(NICHE).unwrap(),
            )
            .unwrap_or_else(|e| {
                panic!(
                    "{:?} produced an invalid configuration: {e}",
                    combination.settings
                )
            });
        }
    }

    /// A whole number reaches the configuration as a number. It used to reach
    /// it as the string the menu stores, which every integer check refuses.
    #[test]
    fn a_whole_number_reaches_the_configuration_as_a_number() {
        let mut axis = AxisEdit::new("n_neighbors", Stage::Reduction, Sampling::of("n_neighbors"));
        set(&mut axis, "10", "12", "1");
        tick(&mut axis, &["10", "12"]);

        let points = axis.to_axis().points();
        assert_eq!(points.len(), 2);
        for point in &points {
            assert!(
                matches!(point, Point::Integer(_)),
                "`n_neighbors` went out as {point:?}"
            );
            assert!(
                point.to_yaml().is_number(),
                "`n_neighbors` went out as {:?}",
                point.to_yaml()
            );
        }
    }

    /// `order` is the exception the configuration itself makes: the interface
    /// has always written it as a string, and the validation accepts either.
    #[test]
    fn the_neighbourhood_order_stays_the_string_the_interface_writes() {
        let axis = AxisEdit::new("order", Stage::Aggregation, Sampling::of("order"));
        assert!(matches!(axis.to_axis().points()[0], Point::Text(_)));
    }

    /// `Column to aggregate` on a real cohort is thirty-four phenotypes. Spelt
    /// out, it pushes the five other settings the dialog exists to show off the
    /// screen — so past three the count stands in for them and the names are
    /// one click away, exactly as the main interface's column picker does it.
    #[test]
    fn a_long_list_of_columns_is_shown_as_a_count() {
        let many: Vec<String> = (1..=34).map(|i| format!("pheno_{i}")).collect();
        let value = SettingValue::List(many.clone());

        assert_eq!(value.caption(), "34 columns");
        assert!(value.is_abbreviated(), "the names are behind the count");
        assert_eq!(value.items(), many.as_slice());
    }

    /// Three or fewer are named outright: a count of two says less than the two
    /// names it replaces.
    #[test]
    fn a_short_list_of_columns_is_named_outright() {
        for names in [vec!["a"], vec!["a", "b"], vec!["a", "b", "c"]] {
            let value = SettingValue::List(names.iter().map(|s| s.to_string()).collect());
            assert_eq!(value.caption(), names.join(", "));
            assert!(
                !value.is_abbreviated(),
                "{names:?} was hidden behind a count"
            );
        }
    }

    /// A single column written as a scalar — which is how the configuration
    /// spells one — is not a list and is never abbreviated.
    #[test]
    fn a_single_column_is_not_a_list() {
        let value = SettingValue::Text("Cluster".into());
        assert_eq!(value.caption(), "Cluster");
        assert!(!value.is_abbreviated());
        assert!(value.items().is_empty());
    }

    /// Nothing set reads as a dash rather than as the word `null` or an empty
    /// line the reader cannot tell from a missing row.
    #[test]
    fn nothing_set_reads_as_a_dash() {
        assert_eq!(SettingValue::Missing.caption(), "—");
        assert_eq!(SettingValue::List(Vec::new()).caption(), "—");
        assert!(SettingValue::Missing.is_missing());
        assert!(SettingValue::List(Vec::new()).is_missing());
        assert!(SettingValue::Text("  ".into()).is_missing());
        assert!(!SettingValue::Text("Cluster".into()).is_missing());
    }

    /// And the whole thing end to end: a configuration aggregating many columns
    /// summarises to a count, with the names available behind it.
    #[test]
    fn a_configuration_with_many_columns_summarises_to_a_count() {
        let mut config =
            RawConfig::from_yaml_str(include_str!("../../tests/sweep_config.yaml")).unwrap();
        let many: Vec<serde_yaml::Value> = (1..=34)
            .map(|i| serde_yaml::Value::String(format!("pheno_{i}")))
            .collect();
        config.set(
            NICHE,
            "Column to aggregate",
            serde_yaml::Value::Sequence(many),
        );

        let summary = general_summary(&config);
        let (_, value) = summary
            .iter()
            .find(|(name, _)| name == "Column to aggregate")
            .unwrap();

        assert_eq!(value.caption(), "34 columns");
        assert_eq!(value.items().len(), 34);
        assert_eq!(value.items()[0], "pheno_1");
    }
}
