//! Port of `neighbors.py::make_features_NAS`.

use rayon::prelude::*;

use crate::nas::adjacency::Adjacency;
use crate::nas::bfs::BfsScratch;
use crate::Pair;

/// The aggregated feature table: `n_obs` rows by `n_var * n_stats` columns,
/// stored row-major, with the column names.
#[derive(Debug, Clone, PartialEq)]
pub struct NasFeatures {
    pub column_names: Vec<String>,
    /// Row-major values, `n_rows * column_names.len()` long.
    pub values: Vec<f64>,
    pub n_rows: usize,
}

impl NasFeatures {
    /// Number of feature columns.
    pub fn n_columns(&self) -> usize {
        self.column_names.len()
    }

    /// Borrow one row.
    pub fn row(&self, i: usize) -> &[f64] {
        let width = self.n_columns();
        &self.values[i * width..(i + 1) * width]
    }
}

/// Aggregate `x` over the `order`-hop neighbourhood of every node.
///
/// `x` is row-major, `n_obs` rows by `var_names.len()` columns.
///
/// Only the default `stat_funcs` — mean and population standard deviation — is
/// implemented, because that is the only combination the configuration can
/// select: the GUI offers `np.mean,np.std` and `np.mean`, and `assert_params`
/// accepts nothing else. `stat_names` still controls the column suffixes, so a
/// configuration renaming the statistics keeps working.
///
/// Column layout matches the Python exactly: all variables for the first
/// statistic, then all variables for the second, each named `{var}{sep}{stat}`.
pub fn make_features_nas(
    x: &[f64],
    n_obs: usize,
    pairs: &[Pair],
    order: usize,
    var_names: &[String],
    stat_names: &[String],
    var_sep: &str,
) -> NasFeatures {
    let n_var = var_names.len();
    debug_assert_eq!(x.len(), n_obs * n_var, "x must be n_obs by n_var");

    // The Python default is `['mean', 'std']`; a single name means the caller
    // asked for the mean alone.
    let want_std = stat_names.len() > 1;
    let n_stats = if want_std { 2 } else { 1 };

    let mut column_names = Vec::with_capacity(n_var * n_stats);
    for stat in stat_names.iter().take(n_stats) {
        for var in var_names {
            column_names.push(format!("{var}{var_sep}{stat}"));
        }
    }

    let width = n_var * n_stats;
    let mut values = vec![0.0f64; n_obs * width];

    if n_obs == 0 || n_var == 0 {
        return NasFeatures {
            column_names,
            values,
            n_rows: n_obs,
        };
    }

    let adj = Adjacency::from_pairs(pairs, n_obs);

    // One node per work item; each writes only its own row, so the rows can be
    // filled in parallel without synchronisation.
    values.par_chunks_mut(width).enumerate().for_each_init(
        || BfsScratch::new(n_obs),
        |scratch, (node, row)| {
            let neighbourhood = scratch.neighbourhood(&adj, node, order);
            let count = neighbourhood.len() as f64;

            for v in 0..n_var {
                let mut sum = 0.0;
                for &neighbour in neighbourhood {
                    sum += x[neighbour as usize * n_var + v];
                }
                let mean = sum / count;
                row[v] = mean;

                if want_std {
                    // Two-pass, like `np.std`: subtracting the mean inside
                    // the loop avoids the catastrophic cancellation the
                    // E[X^2] - E[X]^2 identity suffers when the variance is
                    // tiny relative to the mean, which is the common case
                    // for one-hot phenotype indicators.
                    let mut sum_sq = 0.0;
                    for &neighbour in neighbourhood {
                        let delta = x[neighbour as usize * n_var + v] - mean;
                        sum_sq += delta * delta;
                    }
                    row[n_var + v] = (sum_sq / count).sqrt();
                }
            }
        },
    );

    NasFeatures {
        column_names,
        values,
        n_rows: n_obs,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn column_names_match_the_python_layout() {
        let features = make_features_nas(
            &[1.0, 0.0, 0.0, 1.0],
            2,
            &[(0, 1)],
            1,
            &names(&["A", "B"]),
            &names(&["mean", "std"]),
            " ",
        );
        assert_eq!(
            features.column_names,
            vec!["A mean", "B mean", "A std", "B std"]
        );
    }

    #[test]
    fn averages_over_the_neighbourhood_including_self() {
        // Path 0-1-2 with a single variable taking values 0, 3, 9.
        let x = vec![0.0, 3.0, 9.0];
        let features = make_features_nas(
            &x,
            3,
            &[(0, 1), (1, 2)],
            1,
            &names(&["v"]),
            &names(&["mean", "std"]),
            " ",
        );
        // Node 0 sees {0, 3}; node 1 sees {0, 3, 9}; node 2 sees {3, 9}.
        assert!((features.row(0)[0] - 1.5).abs() < 1e-12);
        assert!((features.row(1)[0] - 4.0).abs() < 1e-12);
        assert!((features.row(2)[0] - 6.0).abs() < 1e-12);
    }

    #[test]
    fn standard_deviation_is_the_population_one() {
        // Node 1 sees {0, 3, 9}: mean 4, population variance 14, std sqrt(14).
        let x = vec![0.0, 3.0, 9.0];
        let features = make_features_nas(
            &x,
            3,
            &[(0, 1), (1, 2)],
            1,
            &names(&["v"]),
            &names(&["mean", "std"]),
            " ",
        );
        let expected = 14.0f64.sqrt();
        assert!(
            (features.row(1)[1] - expected).abs() < 1e-12,
            "got {}",
            features.row(1)[1]
        );
    }

    #[test]
    fn a_single_stat_name_produces_only_the_mean() {
        let features = make_features_nas(
            &[1.0, 2.0],
            2,
            &[(0, 1)],
            1,
            &names(&["v"]),
            &names(&["mean"]),
            " ",
        );
        assert_eq!(features.column_names, vec!["v mean"]);
        assert_eq!(features.n_columns(), 1);
    }

    #[test]
    fn an_isolated_node_aggregates_only_itself() {
        let x = vec![5.0, 1.0];
        let features =
            make_features_nas(&x, 2, &[], 1, &names(&["v"]), &names(&["mean", "std"]), " ");
        assert_eq!(features.row(0)[0], 5.0);
        assert_eq!(features.row(0)[1], 0.0, "a lone node has zero dispersion");
    }

    #[test]
    fn higher_order_widens_the_neighbourhood() {
        let x = vec![0.0, 0.0, 12.0];
        let order_one = make_features_nas(
            &x,
            3,
            &[(0, 1), (1, 2)],
            1,
            &names(&["v"]),
            &names(&["mean"]),
            " ",
        );
        let order_two = make_features_nas(
            &x,
            3,
            &[(0, 1), (1, 2)],
            2,
            &names(&["v"]),
            &names(&["mean"]),
            " ",
        );
        // Node 0 cannot see node 2 at order 1, but does at order 2.
        assert_eq!(order_one.row(0)[0], 0.0);
        assert!((order_two.row(0)[0] - 4.0).abs() < 1e-12);
    }

    #[test]
    fn parallel_rows_are_independent() {
        // A larger network, checked against a straightforward serial reference.
        let n = 200;
        let x: Vec<f64> = (0..n).map(|i| (i as f64 * 0.37).sin()).collect();
        let pairs: Vec<Pair> = (0..n as u32 - 1).map(|i| (i, i + 1)).collect();
        let features = make_features_nas(&x, n, &pairs, 1, &names(&["v"]), &names(&["mean"]), " ");

        for node in 0..n {
            let mut members = vec![node];
            if node > 0 {
                members.push(node - 1);
            }
            if node + 1 < n {
                members.push(node + 1);
            }
            let expected: f64 = members.iter().map(|&m| x[m]).sum::<f64>() / members.len() as f64;
            assert!(
                (features.row(node)[0] - expected).abs() < 1e-12,
                "row {node} diverged"
            );
        }
    }

    #[test]
    fn an_empty_network_yields_an_empty_table() {
        let features = make_features_nas(&[], 0, &[], 1, &names(&["v"]), &names(&["mean"]), " ");
        assert_eq!(features.n_rows, 0);
        assert!(features.values.is_empty());
    }
}
