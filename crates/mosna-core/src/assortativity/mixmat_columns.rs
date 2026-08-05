//! Flattening a mixing matrix into named columns, and back.
//!
//! Ports `assortativity.py::{mixmat_to_columns, series_to_mixmat,
//! attributes_pairs}`.

use crate::assortativity::mixing_matrix::MixMat;

/// Flatten the lower triangle and the diagonal, row by row.
///
/// ```python
/// rows, cols = np.tril_indices(mixmat.shape[0])
/// return mixmat[rows, cols].tolist()
/// ```
///
/// `np.tril_indices` walks rows top to bottom and, within a row, columns left
/// to right up to the diagonal — so the order is
/// `(0,0), (1,0), (1,1), (2,0), ...`. The column names produced by
/// [`attributes_pairs`] must line up with that order, which is the one thing
/// that keeps `net_stat.csv` readable.
pub fn mixmat_to_columns(mixmat: &MixMat) -> Vec<f64> {
    let mut out = Vec::with_capacity(mixmat.n * (mixmat.n + 1) / 2);
    for i in 0..mixmat.n {
        for j in 0..=i {
            out.push(mixmat.get(i, j));
        }
    }
    out
}

/// Names of the flattened mixing-matrix elements.
///
/// ```python
/// N = len(attributes)
/// col = []
/// for i in range(N):
///     for j in range(i+1):
///         col.append(prefix + attributes[i] + medfix + attributes[j] + suffix)
/// ```
///
/// # These names must walk the matrix exactly as [`mixmat_to_columns`] does
///
/// Both walk the *lower* triangle, row by row: `(0,0), (1,0), (1,1), (2,0), …`
/// — so the name at position `k` describes the element at position `k`, and the
/// larger index comes first in the name (`fibroblast - cancer`, never
/// `cancer - fibroblast`).
///
/// This is the whole contract of this module, and it is not a detail. When the
/// two walks disagree, `net_stat.csv` reports one phenotype pair's value under
/// another pair's name — silently, for every dataset with three or more
/// phenotypes, across all four of the `RAW`, `MEAN`, `STD` and `Z` blocks, and
/// again in every figure that goes back through [`series_to_mixmat`]. The
/// symmetry of the matrix does not rescue it: the two positions that get
/// swapped are distinct pairs, not a pair and its transpose.
///
/// `crates/mosna-pipeline/tests/parity.rs` pins the alignment against the
/// reference implementation, which is how it should have been pinned from the
/// start — the two functions each had their own passing test while walking
/// opposite triangles.
pub fn attributes_pairs(
    attributes: &[String],
    prefix: &str,
    medfix: &str,
    suffix: &str,
) -> Vec<String> {
    let mut out = Vec::with_capacity(attributes.len() * (attributes.len() + 1) / 2);
    for (i, a) in attributes.iter().enumerate() {
        for b in &attributes[..=i] {
            out.push(format!("{prefix}{a}{medfix}{b}{suffix}"));
        }
    }
    out
}

/// Rebuild a symmetric matrix from `"{left} - {right}"`-named values.
///
/// Port of `series_to_mixmat`. The attribute order of the result is the order
/// in which names are first seen while scanning the pairs, matching the Python
/// `[*{*chain.from_iterable(zip(left, right))}]` in spirit; the Python version
/// goes through a `set` and so is *not* deterministic across runs, whereas this
/// is. That only reorders the rows and columns of a heatmap.
pub fn series_to_mixmat(
    names: &[String],
    values: &[f64],
    medfix: &str,
    discard: &str,
) -> (Vec<String>, MixMat) {
    let mut labels: Vec<String> = Vec::new();
    let mut index = std::collections::HashMap::new();
    let mut parsed: Vec<(usize, usize, f64)> = Vec::with_capacity(names.len());

    for (name, &value) in names.iter().zip(values) {
        let Some((left, right)) = name.split_once(medfix) else {
            continue;
        };
        let left = left.replace(discard, "");
        let right = right.replace(discard, "");

        let mut id_of = |label: String| -> usize {
            if let Some(&existing) = index.get(&label) {
                existing
            } else {
                let id = labels.len();
                index.insert(label.clone(), id);
                labels.push(label);
                id
            }
        };
        let i = id_of(left);
        let j = id_of(right);
        parsed.push((i, j, value));
    }

    let mut mixmat = MixMat::zeros(labels.len());
    // Cells never named by a pair stay NaN, which the figures render in grey
    // exactly as the Python `object` matrix full of `None` does.
    mixmat.values.iter_mut().for_each(|v| *v = f64::NAN);
    for (i, j, value) in parsed {
        mixmat.set(i, j, value);
        mixmat.set(j, i, value);
    }
    (labels, mixmat)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn attrs(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn flattening_walks_the_lower_triangle() {
        let mut m = MixMat::zeros(3);
        for i in 0..3 {
            for j in 0..3 {
                m.set(i, j, (i * 3 + j) as f64);
            }
        }
        // (0,0), (1,0), (1,1), (2,0), (2,1), (2,2)
        assert_eq!(mixmat_to_columns(&m), vec![0.0, 3.0, 4.0, 6.0, 7.0, 8.0]);
    }

    /// The names walk the lower triangle, larger index first — the same walk
    /// `mixmat_to_columns` makes over the values.
    ///
    /// This test previously asserted the upper triangle
    /// (`A - A, A - B, A - C, B - B, …`) under the name
    /// `pair_names_follow_combinations_with_replacement`. It passed, and so did
    /// the test above it, because each checked one function on its own — while
    /// the two walked opposite triangles and every value from the third column
    /// on was published under another pair's name.
    #[test]
    fn pair_names_walk_the_lower_triangle_like_the_values() {
        let names = attributes_pairs(&attrs(&["A", "B", "C"]), "", " - ", " Z");
        assert_eq!(
            names,
            vec!["A - A Z", "B - A Z", "B - B Z", "C - A Z", "C - B Z", "C - C Z"]
        );
    }

    /// The property the two functions exist to satisfy: the name at position
    /// `k` describes the element at position `k`. Checked on a deliberately
    /// asymmetric matrix, so that reading `(i, j)` instead of `(j, i)` is
    /// visible rather than hidden by symmetry.
    #[test]
    fn each_name_designates_the_element_whose_value_it_carries() {
        let labels = attrs(&["A", "B", "C", "D"]);
        let mut m = MixMat::zeros(4);
        for i in 0..4 {
            for j in 0..4 {
                m.set(i, j, (i * 10 + j) as f64);
            }
        }

        let names = attributes_pairs(&labels, "", " - ", "");
        let values = mixmat_to_columns(&m);
        assert_eq!(names.len(), values.len());

        for (name, value) in names.iter().zip(&values) {
            let (left, right) = name.split_once(" - ").expect("a well-formed pair name");
            let i = labels.iter().position(|l| l == left).expect("left label");
            let j = labels.iter().position(|l| l == right).expect("right label");
            assert_eq!(
                *value,
                m.get(i, j),
                "`{name}` carries {value}, but that cell holds {}",
                m.get(i, j)
            );
        }
    }

    #[test]
    fn there_are_as_many_names_as_values() {
        for n in 1..8 {
            let labels: Vec<String> = (0..n).map(|i| format!("p{i}")).collect();
            let names = attributes_pairs(&labels, "", " - ", "");
            let values = mixmat_to_columns(&MixMat::zeros(n));
            assert_eq!(names.len(), values.len(), "n = {n}");
        }
    }

    #[test]
    fn series_rebuilds_a_symmetric_matrix() {
        let names = attrs(&["A - A Z", "A - B Z", "B - B Z"]);
        let values = vec![1.0, 2.0, 3.0];
        let (labels, m) = series_to_mixmat(&names, &values, " - ", " Z");

        assert_eq!(labels, vec!["A", "B"]);
        assert_eq!(m.get(0, 0), 1.0);
        assert_eq!(m.get(0, 1), 2.0);
        assert_eq!(m.get(1, 0), 2.0);
        assert_eq!(m.get(1, 1), 3.0);
    }

    #[test]
    fn unnamed_cells_stay_missing() {
        // Only the A-B pair is given; the diagonal is unknown.
        let (labels, m) = series_to_mixmat(&attrs(&["A - B Z"]), &[5.0], " - ", " Z");
        assert_eq!(labels, vec!["A", "B"]);
        assert_eq!(m.get(0, 1), 5.0);
        assert!(m.get(0, 0).is_nan());
    }

    #[test]
    fn label_order_is_deterministic() {
        let names = attrs(&["B - C Z", "A - B Z"]);
        let (first, _) = series_to_mixmat(&names, &[1.0, 2.0], " - ", " Z");
        let (second, _) = series_to_mixmat(&names, &[1.0, 2.0], " - ", " Z");
        assert_eq!(first, second);
        assert_eq!(first, vec!["B", "C", "A"]);
    }
}
