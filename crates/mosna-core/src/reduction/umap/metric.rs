//! Distance metrics UMAP can be run with.

/// The three metrics the configuration allows.
///
/// `assert_params` asserts `metric in ['manhattan', 'euclidean', 'cosine']`, so
/// there is nothing else to support.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Metric {
    Euclidean,
    Manhattan,
    Cosine,
}

impl Metric {
    /// Parse the spelling used in `configuration.yaml`.
    pub fn parse(name: &str) -> Self {
        match name {
            "manhattan" => Metric::Manhattan,
            "cosine" => Metric::Cosine,
            _ => Metric::Euclidean,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Metric::Euclidean => "euclidean",
            Metric::Manhattan => "manhattan",
            Metric::Cosine => "cosine",
        }
    }

    /// Distance between two feature vectors.
    ///
    /// Cosine distance of a zero vector is defined as 1: the vector has no
    /// direction, so it is maximally dissimilar from everything. numpy would
    /// produce `NaN` there, which would poison the neighbour search.
    #[inline]
    pub fn distance(self, a: &[f64], b: &[f64]) -> f64 {
        match self {
            Metric::Euclidean => self.squared_euclidean(a, b).sqrt(),
            Metric::Manhattan => a.iter().zip(b).map(|(x, y)| (x - y).abs()).sum(),
            Metric::Cosine => {
                let mut dot = 0.0;
                let mut norm_a = 0.0;
                let mut norm_b = 0.0;
                for (x, y) in a.iter().zip(b) {
                    dot += x * y;
                    norm_a += x * x;
                    norm_b += y * y;
                }
                if norm_a <= 0.0 || norm_b <= 0.0 {
                    1.0
                } else {
                    1.0 - dot / (norm_a.sqrt() * norm_b.sqrt())
                }
            }
        }
    }

    /// A monotone surrogate of [`Metric::distance`], cheaper to evaluate.
    ///
    /// Ranking by this is the same as ranking by the true distance, so the
    /// neighbour search can avoid the square root in its inner loop and take it
    /// once per retained neighbour.
    #[inline]
    pub fn rank_distance(self, a: &[f64], b: &[f64]) -> f64 {
        match self {
            Metric::Euclidean => self.squared_euclidean(a, b),
            other => other.distance(a, b),
        }
    }

    /// Convert a ranking value back to a true distance.
    #[inline]
    pub fn from_rank(self, rank: f64) -> f64 {
        match self {
            Metric::Euclidean => rank.sqrt(),
            _ => rank,
        }
    }

    #[inline]
    fn squared_euclidean(self, a: &[f64], b: &[f64]) -> f64 {
        a.iter()
            .zip(b)
            .map(|(x, y)| {
                let d = x - y;
                d * d
            })
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn euclidean_is_the_straight_line_distance() {
        assert_eq!(Metric::Euclidean.distance(&[0.0, 0.0], &[3.0, 4.0]), 5.0);
    }

    #[test]
    fn manhattan_sums_the_axis_offsets() {
        assert_eq!(Metric::Manhattan.distance(&[0.0, 0.0], &[3.0, 4.0]), 7.0);
    }

    #[test]
    fn cosine_ignores_magnitude() {
        // Same direction, different lengths.
        let d = Metric::Cosine.distance(&[1.0, 0.0], &[7.0, 0.0]);
        assert!(d.abs() < 1e-12, "parallel vectors are at distance {d}");

        // Orthogonal.
        let d = Metric::Cosine.distance(&[1.0, 0.0], &[0.0, 1.0]);
        assert!((d - 1.0).abs() < 1e-12);

        // Opposite.
        let d = Metric::Cosine.distance(&[1.0, 0.0], &[-1.0, 0.0]);
        assert!((d - 2.0).abs() < 1e-12);
    }

    #[test]
    fn cosine_of_a_zero_vector_is_maximally_distant() {
        assert_eq!(Metric::Cosine.distance(&[0.0, 0.0], &[1.0, 1.0]), 1.0);
        assert_eq!(Metric::Cosine.distance(&[0.0, 0.0], &[0.0, 0.0]), 1.0);
    }

    #[test]
    fn rank_distance_orders_the_same_way_as_distance() {
        let origin = [0.0, 0.0];
        let near = [1.0, 0.0];
        let far = [5.0, 0.0];

        for metric in [Metric::Euclidean, Metric::Manhattan, Metric::Cosine] {
            let (rn, rf) = (
                metric.rank_distance(&origin, &near),
                metric.rank_distance(&origin, &far),
            );
            let (dn, df) = (
                metric.distance(&origin, &near),
                metric.distance(&origin, &far),
            );
            assert_eq!(
                rn <= rf,
                dn <= df,
                "{metric:?} ranks differently from its distance"
            );
        }
    }

    #[test]
    fn from_rank_inverts_rank_distance() {
        let a = [1.0, 2.0];
        let b = [4.0, 6.0];
        for metric in [Metric::Euclidean, Metric::Manhattan, Metric::Cosine] {
            let recovered = metric.from_rank(metric.rank_distance(&a, &b));
            assert!((recovered - metric.distance(&a, &b)).abs() < 1e-12);
        }
    }

    #[test]
    fn parsing_matches_the_configuration_spelling() {
        assert_eq!(Metric::parse("manhattan"), Metric::Manhattan);
        assert_eq!(Metric::parse("cosine"), Metric::Cosine);
        assert_eq!(Metric::parse("euclidean"), Metric::Euclidean);
        assert_eq!(Metric::parse("anything else"), Metric::Euclidean);
    }
}
