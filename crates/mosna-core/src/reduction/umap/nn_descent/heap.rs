//! The bounded heaps the descent keeps, one row per point.

use crate::reduction::umap::Scalar;

/// An empty slot.
pub const EMPTY: i32 = -1;

/// A max-heap of at most `k` candidates per point, flat and row-major.
///
/// The root of each row is its *worst* retained candidate, which is what makes
/// the descent cheap: a pair only has to beat that one value to be worth
/// considering, and the comparison is a single read.
///
/// Rows are laid out contiguously so that a block of points is a contiguous
/// slice of all three arrays. That is what lets the parallel phases hand each
/// block a disjoint `&mut` without any locking or interior mutability: a point
/// belongs to exactly one block, and only that block ever writes its row.
pub struct Heap {
    /// Candidates per point.
    pub k: usize,
    /// Neighbour ids, [`EMPTY`] where the row is not full.
    pub indices: Vec<i32>,
    /// Matching distances, `+inf` where the row is not full.
    pub priorities: Vec<Scalar>,
    /// Whether each entry is new — has yet to be joined against.
    pub flags: Vec<u8>,
}

impl Heap {
    /// An empty heap for `n_rows` points.
    pub fn new(n_rows: usize, k: usize) -> Self {
        Self {
            k,
            indices: vec![EMPTY; n_rows * k],
            priorities: vec![Scalar::INFINITY; n_rows * k],
            flags: vec![0; n_rows * k],
        }
    }

    /// Number of points.
    pub fn n_rows(&self) -> usize {
        self.indices.len() / self.k
    }

    /// The worst distance point `v` currently retains — its rejection
    /// threshold.
    pub fn threshold(&self, v: usize) -> Scalar {
        self.priorities[v * self.k]
    }

    /// Point `v`'s row: distances, ids.
    pub fn row(&self, v: usize) -> (&[Scalar], &[i32]) {
        let at = v * self.k;
        (
            &self.priorities[at..at + self.k],
            &self.indices[at..at + self.k],
        )
    }

    /// Offer `(d, n)` to point `v`. Returns whether it was retained.
    pub fn push(&mut self, v: usize, d: Scalar, n: i32, flag: u8) -> bool {
        let at = v * self.k;
        let end = at + self.k;
        checked_flagged_push(
            &mut self.priorities[at..end],
            &mut self.indices[at..end],
            &mut self.flags[at..end],
            d,
            n,
            flag,
        )
    }
}

/// pynndescent's `checked_flagged_heap_push`, on one row.
///
/// Returns whether the row changed, which is what the descent counts to decide
/// it has converged.
///
/// The duplicate check is a linear scan of the row rather than a lookup: `k` is
/// a few tens, and the scan runs only for a candidate that already beat the
/// threshold.
pub fn checked_flagged_push(
    priorities: &mut [Scalar],
    indices: &mut [i32],
    flags: &mut [u8],
    d: Scalar,
    n: i32,
    flag: u8,
) -> bool {
    push(priorities, indices, Some(flags), d, n, flag)
}

/// The same push without the flags, for the candidate heaps.
///
/// pynndescent keeps this as a second copy of the routine
/// (`checked_heap_push`); here it is the same one, told there are no flags to
/// carry. The `Option` is a constant at each call site, so the check costs
/// nothing once inlined.
pub fn checked_push(priorities: &mut [Scalar], indices: &mut [i32], d: Scalar, n: i32) -> bool {
    push(priorities, indices, None, d, n, 0)
}

/// Offer `(d, n)` to one row, keeping the row a max-heap.
///
/// A candidate that does not beat the root — the worst entry retained — is
/// refused outright, which is the comparison that makes the descent cheap.
/// Otherwise it takes the root's place and sinks to where it belongs.
fn push(
    priorities: &mut [Scalar],
    indices: &mut [i32],
    mut flags: Option<&mut [u8]>,
    d: Scalar,
    n: i32,
    flag: u8,
) -> bool {
    if d >= priorities[0] {
        return false;
    }
    if indices.contains(&n) {
        return false;
    }

    let size = priorities.len();
    let mut i = 0;
    loop {
        let child = 2 * i + 1;
        let sibling = child + 1;

        // The reference writes the two comparisons against `child` the other
        // way round from one another — `priorities[ic1] > p` and `p <
        // priorities[ic1]` — which is the same test twice, so the two branches
        // are one here.
        let swap = if child >= size {
            break;
        } else if sibling >= size || priorities[child] >= priorities[sibling] {
            if d < priorities[child] {
                child
            } else {
                break;
            }
        } else if d < priorities[sibling] {
            sibling
        } else {
            break;
        };

        priorities[i] = priorities[swap];
        indices[i] = indices[swap];
        if let Some(flags) = flags.as_deref_mut() {
            flags[i] = flags[swap];
        }
        i = swap;
    }

    priorities[i] = d;
    indices[i] = n;
    if let Some(flags) = flags {
        flags[i] = flag;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The heap's contents, worst first is not guaranteed — so read it sorted.
    fn sorted(heap: &Heap, v: usize) -> Vec<(Scalar, i32)> {
        let (priorities, indices) = heap.row(v);
        let mut entries: Vec<(Scalar, i32)> = priorities
            .iter()
            .zip(indices)
            .filter(|(_, &n)| n != EMPTY)
            .map(|(&d, &n)| (d, n))
            .collect();
        entries.sort_by(|a, b| a.partial_cmp(b).unwrap());
        entries
    }

    #[test]
    fn keeps_the_nearest_k() {
        let mut heap = Heap::new(1, 3);
        for (d, n) in [(5.0, 5), (1.0, 1), (4.0, 4), (2.0, 2), (3.0, 3)] {
            heap.push(0, d, n, 1);
        }
        assert_eq!(sorted(&heap, 0), vec![(1.0, 1), (2.0, 2), (3.0, 3)]);
    }

    #[test]
    fn the_root_is_the_worst_retained() {
        let mut heap = Heap::new(1, 3);
        for (d, n) in [(5.0, 5), (1.0, 1), (4.0, 4)] {
            heap.push(0, d, n, 1);
        }
        assert_eq!(heap.threshold(0), 5.0);
        heap.push(0, 2.0, 2, 1);
        assert_eq!(heap.threshold(0), 4.0);
    }

    #[test]
    fn an_empty_row_accepts_anything_and_holds_infinity() {
        let heap = Heap::new(2, 4);
        assert_eq!(heap.threshold(1), Scalar::INFINITY);
        assert_eq!(heap.row(1).1, &[EMPTY; 4]);
    }

    #[test]
    fn a_duplicate_is_refused() {
        let mut heap = Heap::new(1, 4);
        assert!(heap.push(0, 3.0, 7, 1));
        assert!(!heap.push(0, 1.0, 7, 1));
        assert_eq!(sorted(&heap, 0), vec![(3.0, 7)]);
    }

    #[test]
    fn a_candidate_at_the_threshold_is_refused() {
        let mut heap = Heap::new(1, 2);
        heap.push(0, 1.0, 1, 1);
        heap.push(0, 2.0, 2, 1);
        assert!(!heap.push(0, 2.0, 3, 1));
    }

    #[test]
    fn rows_are_independent() {
        let mut heap = Heap::new(3, 2);
        heap.push(0, 1.0, 10, 1);
        heap.push(2, 2.0, 20, 1);
        assert_eq!(sorted(&heap, 0), vec![(1.0, 10)]);
        assert!(sorted(&heap, 1).is_empty());
        assert_eq!(sorted(&heap, 2), vec![(2.0, 20)]);
    }

    #[test]
    fn the_flag_travels_with_its_entry() {
        let mut heap = Heap::new(1, 3);
        heap.push(0, 3.0, 3, 1);
        heap.push(0, 1.0, 1, 0);
        heap.push(0, 2.0, 2, 1);
        let (_, indices) = heap.row(0);
        let at = indices.iter().position(|&n| n == 1).unwrap();
        assert_eq!(heap.flags[at], 0);
    }

    /// The flagless variant must agree with the flagged one.
    #[test]
    fn both_pushes_keep_the_same_set() {
        let mut flagged = Heap::new(1, 4);
        let mut priorities = vec![Scalar::INFINITY; 4];
        let mut indices = vec![EMPTY; 4];

        for (d, n) in [(9.0, 9), (2.0, 2), (7.0, 7), (1.0, 1), (5.0, 5), (3.0, 3)] {
            let a = flagged.push(0, d, n, 1);
            let b = checked_push(&mut priorities, &mut indices, d, n);
            assert_eq!(a, b);
        }

        let mut left = flagged.row(0).1.to_vec();
        let mut right = indices.clone();
        left.sort_unstable();
        right.sort_unstable();
        assert_eq!(left, right);
    }
}
