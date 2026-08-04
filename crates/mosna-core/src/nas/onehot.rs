//! One-hot encoding — port of the `pd.get_dummies` calls in `neighbors.py` and
//! `assortativity.py`.

/// Encode `labels` as indicator columns, one per entry of `categories`.
///
/// Returns a row-major `labels.len() * categories.len()` matrix.
///
/// The Python does
///
/// ```python
/// nodes = nodes.join(pd.get_dummies(nodes[attributes_col], prefix='', prefix_sep=''))
/// for col in set(use_attributes).difference(np.unique(nodes[attributes_col])):
///     nodes[col] = 0
/// X = nodes[use_attributes].astype(float).values
/// ```
///
/// so the column set is fixed by `use_attributes` — the phenotypes observed
/// across the *whole cohort* — and a phenotype missing from one sample
/// contributes an all-zero column there. That is what keeps every sample's
/// feature vector the same width and the same meaning, which the pooled
/// clustering depends on.
///
/// A label absent from `categories` contributes nothing, matching the Python
/// where such a column is simply not selected by `nodes[use_attributes]`.
pub fn one_hot(labels: &[Option<String>], categories: &[String]) -> Vec<f64> {
    let n_categories = categories.len();
    let mut matrix = vec![0.0f64; labels.len() * n_categories];

    // A hash lookup keeps this linear; the category list can hold hundreds of
    // phenotypes and is scanned once per cell.
    let index: std::collections::HashMap<&str, usize> = categories
        .iter()
        .enumerate()
        .map(|(i, c)| (c.as_str(), i))
        .collect();

    for (row, label) in labels.iter().enumerate() {
        if let Some(label) = label {
            if let Some(&col) = index.get(label.as_str()) {
                matrix[row * n_categories + col] = 1.0;
            }
        }
    }
    matrix
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels(items: &[&str]) -> Vec<Option<String>> {
        items.iter().map(|s| Some(s.to_string())).collect()
    }

    fn categories(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn encodes_one_indicator_per_category() {
        let matrix = one_hot(&labels(&["A", "B", "A"]), &categories(&["A", "B"]));
        assert_eq!(matrix, vec![1.0, 0.0, 0.0, 1.0, 1.0, 0.0]);
    }

    #[test]
    fn a_category_absent_from_this_sample_is_an_all_zero_column() {
        let matrix = one_hot(&labels(&["A", "A"]), &categories(&["A", "B", "C"]));
        // Column 1 and 2 are zero everywhere, but present.
        assert_eq!(matrix.len(), 6);
        assert_eq!(matrix[1], 0.0);
        assert_eq!(matrix[2], 0.0);
        assert_eq!(matrix[4], 0.0);
    }

    #[test]
    fn each_row_is_a_single_one() {
        let matrix = one_hot(&labels(&["B", "C"]), &categories(&["A", "B", "C"]));
        for row in matrix.chunks(3) {
            assert_eq!(row.iter().sum::<f64>(), 1.0);
        }
    }

    #[test]
    fn a_null_label_encodes_to_zeros() {
        let mut input = labels(&["A"]);
        input.push(None);
        let matrix = one_hot(&input, &categories(&["A", "B"]));
        assert_eq!(&matrix[2..4], &[0.0, 0.0]);
    }

    #[test]
    fn an_unknown_label_contributes_nothing() {
        let matrix = one_hot(&labels(&["Z"]), &categories(&["A", "B"]));
        assert_eq!(matrix, vec![0.0, 0.0]);
    }
}
