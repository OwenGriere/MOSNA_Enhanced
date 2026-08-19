//! How many nodes fell into each niche.

use std::path::Path;

use mosna_core::colormap::make_cluster_cmap;

use crate::palette::hex;
use crate::spec::Spec;

pub const KIND: &str = "histogram";
pub const STEM: &str = "Niches_Histogram";

pub fn spec(niches: &[u32], save_dir: &Path) -> Spec {
    let mut counts: std::collections::BTreeMap<u32, usize> = Default::default();
    for niche in niches {
        *counts.entry(*niche).or_insert(0) += 1;
    }
    let bars: Vec<(u32, usize)> = counts.into_iter().collect();

    let categories: Vec<String> = bars.iter().map(|(niche, _)| niche.to_string()).collect();
    let values: Vec<usize> = bars.iter().map(|(_, count)| *count).collect();
    // The same palette the embedding and the composition heatmap use, so a
    // niche keeps one colour across the three figures that describe it.
    let colours: Vec<String> = make_cluster_cmap(bars.len()).into_iter().map(hex).collect();

    Spec::new(KIND, STEM, save_dir)
        .set("title", "Niches histogram")
        .set("categories", serde_json::json!(categories))
        .set("counts", serde_json::json!(values))
        .set("colours", serde_json::json!(colours))
        .set("width", 2000)
        .set("height", 800)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn niches_are_counted_and_ordered_by_their_identifier() {
        let spec = spec(&[2, 0, 2, 1, 2], Path::new("/out"));
        let json = spec.to_json();

        assert_eq!(json["categories"], serde_json::json!(["0", "1", "2"]));
        assert_eq!(json["counts"], serde_json::json!([1, 1, 3]));
        assert_eq!(spec.stem(), STEM);
    }

    /// A niche has to be the same colour here, in the embedding and in the
    /// composition heatmap, or the three figures cannot be read together.
    #[test]
    fn the_bars_carry_the_cluster_palette() {
        let json = spec(&[0, 1], Path::new("/out")).to_json();
        assert_eq!(json["colours"][0], hex(make_cluster_cmap(2)[0]));
        assert_eq!(json["colours"][1], hex(make_cluster_cmap(2)[1]));
    }

    #[test]
    fn no_niches_is_an_empty_figure_not_a_missing_one() {
        let json = spec(&[], Path::new("/out")).to_json();
        assert_eq!(json["categories"], serde_json::json!([]));
    }
}
