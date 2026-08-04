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
/// [prefix + a + medfix + b + suffix
///  for a, b in combinations_with_replacement(attributes, 2)]
/// ```
///
/// # A mismatch inherited from Python
///
/// `combinations_with_replacement` yields `(a0,a0), (a0,a1), (a0,a2), ...` —
/// the *upper* triangle in row-major order — while [`mixmat_to_columns`] emits
/// the *lower* triangle. The two orders differ as soon as there are three or
/// more attributes, so the value stored under `X - Y` is the one at a different
/// position of the matrix.
///
/// Because the mixing matrix is symmetric, every value still appears exactly
/// once and `series_to_mixmat` rebuilds a correct symmetric matrix from the
/// pairs — the labels and the numbers are simply paired up differently than the
/// names suggest. Reproducing the Python order is what keeps `net_stat.csv`
/// column-for-column identical between the two implementations, so it is kept;
/// the figures, which go through `series_to_mixmat`, are unaffected either way.
pub fn attributes_pairs(
    attributes: &[String],
    prefix: &str,
    medfix: &str,
    suffix: &str,
) -> Vec<String> {
    let mut out = Vec::with_capacity(attributes.len() * (attributes.len() + 1) / 2);
    for (i, a) in attributes.iter().enumerate() {
        for b in &attributes[i..] {
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

    #[test]
    fn pair_names_follow_combinations_with_replacement() {
        let names = attributes_pairs(&attrs(&["A", "B", "C"]), "", " - ", " Z");
        assert_eq!(
            names,
            vec!["A - A Z", "A - B Z", "A - C Z", "B - B Z", "B - C Z", "C - C Z"]
        );
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
