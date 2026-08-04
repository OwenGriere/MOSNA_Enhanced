//! Port of `tysserand::remove_duplicate_pairs`.

use crate::Pair;

/// Sort each pair and drop duplicates.
///
/// Equivalent to `np.unique(np.sort(pairs, axis=1), axis=0)`, which also
/// returns the rows in lexicographic order — relied upon downstream, since the
/// edge list is written to disk and compared between runs.
pub fn remove_duplicate_pairs(pairs: impl IntoIterator<Item = Pair>) -> Vec<Pair> {
    let mut sorted: Vec<Pair> = pairs
        .into_iter()
        .map(|(a, b)| if a <= b { (a, b) } else { (b, a) })
        .collect();
    sorted.sort_unstable();
    sorted.dedup();
    sorted
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_the_docstring_example() {
        // >>> remove_duplicate_pairs([[4, 3], [1, 2], [3, 4], [2, 1]])
        // array([[1, 2], [3, 4]])
        let out = remove_duplicate_pairs([(4, 3), (1, 2), (3, 4), (2, 1)]);
        assert_eq!(out, vec![(1, 2), (3, 4)]);
    }

    #[test]
    fn output_is_lexicographically_sorted() {
        let out = remove_duplicate_pairs([(5, 1), (0, 9), (2, 2)]);
        assert_eq!(out, vec![(0, 9), (1, 5), (2, 2)]);
    }

    #[test]
    fn an_empty_input_yields_an_empty_output() {
        assert!(remove_duplicate_pairs([]).is_empty());
    }
}
