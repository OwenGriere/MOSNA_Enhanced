//! How the report is arranged.
//!
//! Three analyses, three tabs, and inside each one the cohort first and then a
//! patient at a time. That is the order the results are read in: what happened
//! overall, then who it happened to.
//!
//! Separated from the page itself so the arrangement can be stated as a test
//! rather than found by reading HTML.

use std::path::{Path, PathBuf};

use crate::report::subject::{self, Subject};
use crate::report::tree::Output;

/// One figure, ready to be placed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Card {
    pub stem: String,
    pub image: Option<PathBuf>,
    pub chart: Option<PathBuf>,
    /// Where it came from, shown under the caption so a figure can be found on
    /// disk.
    pub directory: PathBuf,
}

/// The figures of one subject, or of the cohort.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Group {
    /// `None` for the figures that describe every sample at once.
    pub subject: Option<Subject>,
    pub cards: Vec<Card>,
}

impl Group {
    pub fn heading(&self) -> String {
        match &self.subject {
            Some(subject) => subject.label(),
            None => "The cohort".to_string(),
        }
    }

    /// What the search box matches this group against.
    pub fn search_key(&self) -> String {
        match &self.subject {
            Some(subject) => subject.search_key(),
            // The cohort figures are about everyone, so they answer to no
            // patient in particular — and a search for a patient should not
            // leave them on screen pretending to be his.
            None => String::new(),
        }
    }
}

/// One tab of the report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tab {
    pub id: &'static str,
    pub name: &'static str,
    pub groups: Vec<Group>,
}

impl Tab {
    pub fn figures(&self) -> usize {
        self.groups.iter().map(|group| group.cards.len()).sum()
    }
}

/// The tabs, in the order the workflow produces them.
///
/// A directory nothing recognises still gets a tab: a report that quietly drops
/// figures because they were written somewhere unexpected is worse than one
/// with an untidy heading.
const TABS: [(&str, &str, &str); 4] = [
    ("networks", "Networks", "Tysserand_Network"),
    ("assortativity", "Assortativity", "Assortativity"),
    ("niches", "Niches", "Niche_Analysis"),
    ("other", "Other figures", ""),
];

/// Arrange everything the scan found.
pub fn tabs(output: &Output) -> Vec<Tab> {
    let mut tabs: Vec<Tab> = TABS
        .iter()
        .map(|(id, name, _)| Tab {
            id,
            name,
            groups: Vec::new(),
        })
        .collect();

    for gallery in &output.galleries {
        let id = tab_of(&gallery.directory);
        let Some(tab) = tabs.iter_mut().find(|tab| tab.id == id) else {
            continue;
        };

        for figure in &gallery.figures {
            let subject = subject_of(&gallery.directory, &figure.stem);
            let card = Card {
                stem: figure.stem.clone(),
                image: figure.image.clone(),
                chart: figure.chart.clone(),
                directory: gallery.directory.clone(),
            };

            match tab.groups.iter_mut().find(|group| group.subject == subject) {
                Some(group) => group.cards.push(card),
                None => tab.groups.push(Group {
                    subject,
                    cards: vec![card],
                }),
            }
        }
    }

    for tab in &mut tabs {
        // The cohort first — what happened overall, before who it happened to —
        // then the patients in an order a reader can follow.
        tab.groups.sort_by(|a, b| match (&a.subject, &b.subject) {
            (None, None) => std::cmp::Ordering::Equal,
            (None, Some(_)) => std::cmp::Ordering::Less,
            (Some(_), None) => std::cmp::Ordering::Greater,
            (Some(a), Some(b)) => {
                natural(&a.patient, &b.patient).then_with(|| match (&a.sample, &b.sample) {
                    (Some(a), Some(b)) => natural(a, b),
                    (left, right) => left.cmp(right),
                })
            }
        });
    }

    tabs.retain(|tab| tab.figures() > 0);
    tabs
}

/// Order two identifiers the way a person would.
///
/// `10` after `9`, not between `1` and `2`: a cohort of twelve patients listed
/// lexicographically reads as a mistake, because it is one. Identifiers that
/// are not numbers fall back to the alphabet, which is the only order left.
///
/// Two spellings of the same number — `01` and `1` — are still two different
/// patients. They sort next to each other, which is right, but the tie has to
/// be broken or their samples interleave and one patient's figures end up
/// scattered through the other's.
fn natural(left: &str, right: &str) -> std::cmp::Ordering {
    match (left.parse::<u64>(), right.parse::<u64>()) {
        (Ok(a), Ok(b)) => a.cmp(&b).then_with(|| left.cmp(right)),
        _ => left.cmp(right),
    }
}

/// Which tab a directory belongs to, by its first component.
fn tab_of(directory: &Path) -> &'static str {
    let first = directory
        .components()
        .next()
        .map(|component| component.as_os_str().to_string_lossy().into_owned())
        .unwrap_or_default();

    TABS.iter()
        .find(|(_, _, root)| !root.is_empty() && *root == first)
        .map(|(id, _, _)| *id)
        .unwrap_or("other")
}

/// Who a figure is about: the file name first, then the directory it sits in.
///
/// The order matters. Step 3 writes one directory per sample holding files
/// named exactly as the cohort's are, so the directory has to be consulted —
/// but a file that names its own sample says so more precisely than the
/// directory ever could.
fn subject_of(directory: &Path, stem: &str) -> Option<Subject> {
    subject::from_stem(stem).or_else(|| {
        directory
            .file_name()
            .and_then(|name| name.to_str())
            .and_then(subject::from_directory)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::tree::{Figure, Gallery};

    fn gallery(directory: &str, stems: &[&str]) -> Gallery {
        Gallery {
            directory: PathBuf::from(directory),
            figures: stems
                .iter()
                .map(|stem| Figure {
                    stem: (*stem).to_string(),
                    image: Some(PathBuf::from(format!("{directory}/{stem}.png"))),
                    chart: Some(PathBuf::from(format!("{directory}/{stem}.html"))),
                })
                .collect(),
        }
    }

    fn output(galleries: Vec<Gallery>) -> Output {
        Output {
            galleries,
            ..Default::default()
        }
    }

    fn full() -> Output {
        output(vec![
            gallery("Tysserand_Network", &["net_1-8", "net_1-11", "net_2-6"]),
            gallery(
                "Assortativity",
                &["abundance", "Assortativity_heatmap_with_dendrogram"],
            ),
            gallery(
                "Assortativity/assort_files",
                &["heatmap_zscore_1-8", "heatmap_zscore_2-6"],
            ),
            gallery(
                "Niche_Analysis/Aggregation/niche_cluster",
                &["Niches_Histogram", "cluster_labels"],
            ),
            gallery(
                "Niche_Analysis/Per_sample/niche_cluster/patient-1_chunk-8",
                &["Niches_Histogram"],
            ),
        ])
    }

    fn tab<'a>(tabs: &'a [Tab], id: &str) -> &'a Tab {
        tabs.iter().find(|tab| tab.id == id).unwrap_or_else(|| {
            panic!(
                "no {id} tab in {:?}",
                tabs.iter().map(|t| t.id).collect::<Vec<_>>()
            )
        })
    }

    #[test]
    fn each_analysis_gets_its_own_tab() {
        let tabs = tabs(&full());
        let ids: Vec<&str> = tabs.iter().map(|tab| tab.id).collect();

        assert_eq!(ids, vec!["networks", "assortativity", "niches"]);
    }

    /// A tab with nothing in it is not shown: a run that stopped after step 1
    /// should not offer two empty tabs.
    #[test]
    fn a_tab_with_no_figures_is_not_offered() {
        let tabs = tabs(&output(vec![gallery("Tysserand_Network", &["net_1-8"])]));
        assert_eq!(tabs.len(), 1);
        assert_eq!(tabs[0].id, "networks");
    }

    /// The cohort comes first, because it is what is read first.
    #[test]
    fn the_cohort_comes_before_the_patients() {
        let tabs = tabs(&full());
        let assortativity = tab(&tabs, "assortativity");

        assert_eq!(assortativity.groups[0].subject, None);
        assert_eq!(
            assortativity.groups[0].cards.len(),
            2,
            "abundance and the heatmap"
        );
        assert!(assortativity.groups[1].subject.is_some());
    }

    #[test]
    fn a_tab_with_no_cohort_figures_starts_at_its_first_patient() {
        let tabs = tabs(&output(vec![gallery("Tysserand_Network", &["net_1-8"])]));
        assert!(
            tabs[0].groups[0].subject.is_some(),
            "an empty cohort group was kept"
        );
    }

    /// Grouped by sample, and in an order a reader can follow.
    #[test]
    fn the_patients_are_grouped_and_ordered() {
        let tabs = tabs(&full());
        let networks = tab(&tabs, "networks");

        let headings: Vec<String> = networks
            .groups
            .iter()
            .map(|group| group.heading())
            .collect();
        assert_eq!(
            headings,
            vec!["Patient 1 · 8", "Patient 1 · 11", "Patient 2 · 6"],
            "the samples are not in order"
        );
    }

    /// `10` after `9`, not between `1` and `2`. A cohort of twelve patients
    /// listed lexicographically reads as a mistake, because it is one.
    #[test]
    fn identifiers_that_are_numbers_are_ordered_as_numbers() {
        let tabs = tabs(&output(vec![gallery(
            "Tysserand_Network",
            &["net_10-1", "net_9-1", "net_2-1"],
        )]));
        let headings: Vec<String> = tabs[0].groups.iter().map(|g| g.heading()).collect();

        assert_eq!(
            headings,
            vec!["Patient 2 · 1", "Patient 9 · 1", "Patient 10 · 1"]
        );
    }

    /// `01` and `1` are two different patients that happen to be the same
    /// number. Without a tie-break their samples interleave — patient 01's
    /// first sample, then patient 1's second, then patient 01's third — and a
    /// reader scrolling the page sees one patient's figures scattered through
    /// another's.
    #[test]
    fn two_patients_that_are_the_same_number_do_not_interleave() {
        let tabs = tabs(&output(vec![gallery(
            "Tysserand_Network",
            &["net_01-1", "net_1-2", "net_01-3", "net_1-4"],
        )]));
        let headings: Vec<String> = tabs[0].groups.iter().map(|g| g.heading()).collect();

        assert_eq!(
            headings,
            vec![
                "Patient 01 · 1",
                "Patient 01 · 3",
                "Patient 1 · 2",
                "Patient 1 · 4"
            ]
        );
    }

    /// A cohort named by letters still sorts sensibly.
    #[test]
    fn identifiers_that_are_not_numbers_are_ordered_as_words() {
        let tabs = tabs(&output(vec![gallery(
            "Tysserand_Network",
            &["net_B-1", "net_A-1"],
        )]));
        let headings: Vec<String> = tabs[0].groups.iter().map(|g| g.heading()).collect();
        assert_eq!(headings, vec!["Patient A · 1", "Patient B · 1"]);
    }

    /// Step 3 writes one directory per sample, and the file inside is named
    /// like the cohort's. Without reading the directory those figures would all
    /// pile into the cohort group, under one heading, indistinguishable.
    #[test]
    fn a_figure_named_after_no_one_takes_the_subject_of_its_directory() {
        let tabs = tabs(&full());
        let niches = tab(&tabs, "niches");

        let headings: Vec<String> = niches.groups.iter().map(|g| g.heading()).collect();
        assert_eq!(headings, vec!["The cohort", "Patient 1 · 8"]);
        assert_eq!(niches.groups[1].cards[0].stem, "Niches_Histogram");
    }

    /// Every figure the scan found ends up in exactly one place. A report that
    /// loses one is worse than no report.
    #[test]
    fn no_figure_is_lost_or_duplicated() {
        let output = full();
        let counted: usize = output
            .galleries
            .iter()
            .map(|gallery| gallery.figures.len())
            .sum();
        let placed: usize = tabs(&output).iter().map(Tab::figures).sum();

        assert_eq!(placed, counted);
    }

    /// A directory nobody planned for still shows its figures.
    #[test]
    fn an_unexpected_directory_lands_in_its_own_tab() {
        let tabs = tabs(&output(vec![gallery("Scratch", &["something"])]));
        assert_eq!(tabs.len(), 1);
        assert_eq!(tabs[0].id, "other");
        assert_eq!(tabs[0].figures(), 1);
    }

    /// The card says where its figure lives, so it can be found on disk.
    #[test]
    fn a_card_remembers_the_directory_it_came_from() {
        let tabs = tabs(&full());
        let assortativity = tab(&tabs, "assortativity");
        let patient = &assortativity.groups[1].cards[0];

        assert_eq!(patient.directory, Path::new("Assortativity/assort_files"));
    }

    /// The cohort answers to no patient: a search for `2` must not leave the
    /// cohort figures on screen as though they were his.
    #[test]
    fn the_cohort_group_matches_no_patient_search() {
        let tabs = tabs(&full());
        assert_eq!(tab(&tabs, "assortativity").groups[0].search_key(), "");
    }
}
