//! Port of `assortativity.py::sample_assort_mixmat`.

use crate::assortativity::attribute_ac::attribute_ac;
use crate::assortativity::mixing_matrix::mixing_matrix;
use crate::assortativity::mixmat_columns::{attributes_pairs, mixmat_to_columns};
use crate::assortativity::randomized_mixmat::randomized_mixmat;
use crate::assortativity::zscore::zscore;
use crate::Pair;

/// One row of `net_stat.csv`: the statistics of a single sample's network.
#[derive(Debug, Clone)]
pub struct SampleStats {
    /// Column names, in the order the Python builds them.
    pub column_names: Vec<String>,
    /// The sample identifier, i.e. the `id` column.
    pub id: String,
    /// Numeric values, aligned with `column_names` after the `id`.
    pub values: Vec<f64>,
}

/// Compute the z-scored assortativity and mixing matrix of one sample.
///
/// The column layout reproduces the Python exactly:
///
/// ```text
/// id, # total, % <attr>..., assort, assort MEAN, assort STD, assort Z,
/// <pair> RAW..., <pair> MEAN..., <pair> STD..., <pair> Z...
/// ```
///
/// `assignments[node]` gives the attribute index of each node, or `None` when
/// the node's phenotype is missing.
pub fn sample_assort_mixmat(
    assignments: &[Option<u32>],
    pairs: &[Pair],
    attributes: &[String],
    sample_id: &str,
    n_shuffle: usize,
) -> SampleStats {
    let n_attributes = attributes.len();
    let n_nodes = assignments.len();

    let mixmat = mixing_matrix(assignments, pairs, n_attributes, true, true);
    let assort = attribute_ac(&mixmat);

    let null = randomized_mixmat(assignments, pairs, n_attributes, n_shuffle);
    let (assort_mean, assort_std, assort_z) = zscore(assort, &null.assort);

    // Element-wise z-scores of the mixing matrix against its null.
    let mut mixmat_z = crate::assortativity::mixing_matrix::MixMat::zeros(n_attributes);
    for i in 0..n_attributes * n_attributes {
        let std = null.mixmat_std.values[i];
        mixmat_z.values[i] = (mixmat.values[i] - null.mixmat_mean.values[i]) / std;
    }

    // Abundance of each attribute, `nodes[col].sum() / nb_nodes`.
    let mut abundance = vec![0.0f64; n_attributes];
    for assignment in assignments.iter().flatten() {
        abundance[*assignment as usize] += 1.0;
    }
    let denominator = n_nodes.max(1) as f64;
    abundance.iter_mut().for_each(|v| *v /= denominator);

    let mut column_names = Vec::new();
    column_names.push("# total".to_string());
    column_names.extend(attributes.iter().map(|a| format!("% {a}")));
    column_names.extend(
        ["assort", "assort MEAN", "assort STD", "assort Z"]
            .iter()
            .map(|s| s.to_string()),
    );
    for suffix in [" RAW", " MEAN", " STD", " Z"] {
        column_names.extend(attributes_pairs(attributes, "", " - ", suffix));
    }

    let mut values = Vec::with_capacity(column_names.len());
    values.push(n_nodes as f64);
    values.extend(abundance);
    values.extend([assort, assort_mean, assort_std, assort_z]);
    values.extend(mixmat_to_columns(&mixmat));
    values.extend(mixmat_to_columns(&null.mixmat_mean));
    values.extend(mixmat_to_columns(&null.mixmat_std));
    values.extend(mixmat_to_columns(&mixmat_z));

    debug_assert_eq!(values.len(), column_names.len());

    SampleStats {
        column_names,
        id: sample_id.to_string(),
        values,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn attrs(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    /// A ring of 12 nodes alternating between two phenotypes.
    fn ring() -> (Vec<Option<u32>>, Vec<Pair>) {
        let n = 12u32;
        let assignments = (0..n).map(|i| Some(i % 2)).collect();
        let pairs = (0..n).map(|i| (i, (i + 1) % n)).collect();
        (assignments, pairs)
    }

    #[test]
    fn column_layout_matches_the_python() {
        let (assignments, pairs) = ring();
        let stats = sample_assort_mixmat(&assignments, &pairs, &attrs(&["A", "B"]), "s", 10);

        assert_eq!(
            &stats.column_names[..7],
            &[
                "# total",
                "% A",
                "% B",
                "assort",
                "assort MEAN",
                "assort STD",
                "assort Z"
            ]
        );
        // Four blocks of three pairs each for two attributes.
        assert_eq!(stats.column_names.len(), 7 + 4 * 3);
        assert_eq!(stats.values.len(), stats.column_names.len());
        // `B - A`, not `A - B`: the reference names the lower triangle with the
        // larger index first, and the values are flattened in that same order.
        assert!(stats.column_names.contains(&"B - A RAW".to_string()));
        assert!(stats.column_names.contains(&"B - A Z".to_string()));
    }

    #[test]
    fn reports_the_node_count_and_abundances() {
        let (assignments, pairs) = ring();
        let stats = sample_assort_mixmat(&assignments, &pairs, &attrs(&["A", "B"]), "s", 5);
        assert_eq!(stats.values[0], 12.0);
        assert!((stats.values[1] - 0.5).abs() < 1e-12);
        assert!((stats.values[2] - 0.5).abs() < 1e-12);
    }

    #[test]
    fn a_disassortative_network_scores_strongly_negative() {
        let (assignments, pairs) = ring();
        let stats = sample_assort_mixmat(&assignments, &pairs, &attrs(&["A", "B"]), "s", 200);
        let assort_index = stats
            .column_names
            .iter()
            .position(|c| c == "assort")
            .unwrap();
        let z_index = stats
            .column_names
            .iter()
            .position(|c| c == "assort Z")
            .unwrap();

        assert!(stats.values[assort_index] < -0.9);
        assert!(
            stats.values[z_index] < -2.0,
            "the structure must be far from the null, got {}",
            stats.values[z_index]
        );
    }

    #[test]
    fn an_assortative_network_scores_strongly_positive() {
        // Two cliques of one phenotype each, joined by a single edge.
        let assignments: Vec<Option<u32>> =
            (0..12).map(|i| Some(if i < 6 { 0 } else { 1 })).collect();
        let mut pairs: Vec<Pair> = Vec::new();
        for group in 0..2u32 {
            for i in 0..6u32 {
                for j in (i + 1)..6 {
                    pairs.push((group * 6 + i, group * 6 + j));
                }
            }
        }
        pairs.push((5, 6));

        let stats = sample_assort_mixmat(&assignments, &pairs, &attrs(&["A", "B"]), "s", 200);
        let z_index = stats
            .column_names
            .iter()
            .position(|c| c == "assort Z")
            .unwrap();
        assert!(
            stats.values[z_index] > 2.0,
            "clustered phenotypes must score positive, got {}",
            stats.values[z_index]
        );
    }

    #[test]
    fn results_are_reproducible() {
        let (assignments, pairs) = ring();
        let a = sample_assort_mixmat(&assignments, &pairs, &attrs(&["A", "B"]), "s", 40);
        let b = sample_assort_mixmat(&assignments, &pairs, &attrs(&["A", "B"]), "s", 40);
        assert_eq!(a.values, b.values);
    }

    #[test]
    fn the_id_is_carried_through() {
        let (assignments, pairs) = ring();
        let stats = sample_assort_mixmat(
            &assignments,
            &pairs,
            &attrs(&["A", "B"]),
            "patient-1_sample-2",
            2,
        );
        assert_eq!(stats.id, "patient-1_sample-2");
    }
}
