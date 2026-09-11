//! The generator nearest-neighbour descent draws from.

use rand::Rng;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;

use crate::reduction::umap::Scalar;

/// pynndescent's `tau_rand_int`, a combined Tausworthe generator on three
/// words.
///
/// # Why this generator and not the crate's usual one
///
/// The rest of the reduction draws from `ChaCha8Rng`, which is a better
/// generator by every statistical measure. It is the wrong one here. This
/// generator is not a source of noise the algorithm happens to need — it *is*
/// part of the algorithm: it picks the pair of points each hyperplane is drawn
/// between, it decides which side of a hyperplane a point on the knife edge
/// falls, and it assigns the priorities that decide which candidates survive
/// the `max_candidates` cap. A different stream gives a different forest and a
/// different set of candidates, which is a different approximation. Since the
/// point of this module is to approximate the neighbours the way umap-learn
/// does, the stream is part of the specification.
///
/// The arithmetic is reproduced as numba executes it: three `int64` words with
/// wrapping shifts, masked to 32 bits, and a result truncated to `int32`. The
/// words can go negative, and the shifts are arithmetic — `>>` on a negative
/// value keeps its sign, as it does in numba.
#[derive(Clone, Debug)]
pub struct TauRand {
    state: [i64; 3],
}

impl TauRand {
    /// Seed the three words the way `NNDescent` does.
    ///
    /// pynndescent takes them from `random_state.randint(INT32_MIN, INT32_MAX,
    /// 3)`, a half-open range. The source of those three draws is not part of
    /// the algorithm — only their being an arbitrary starting point is — so
    /// this uses the crate's generator, seeded from the run's seed, which makes
    /// the whole search reproducible where umap-learn's is not.
    pub fn seeded(seed: u64) -> Self {
        let mut source = ChaCha8Rng::seed_from_u64(seed);
        Self::draw(&mut source)
    }

    /// One more state from an already-running source, as the forest draws one
    /// per tree.
    pub fn draw(source: &mut ChaCha8Rng) -> Self {
        let mut state = [0i64; 3];
        for word in &mut state {
            *word = source.gen_range(i32::MIN..i32::MAX) as i64;
        }
        Self { state }
    }

    /// The stream a parallel block works from: `rng_state + n`.
    ///
    /// pynndescent adds the block number to every word, doing arithmetic on the
    /// state rather than seeding from it. The streams are therefore neither
    /// independent nor reliably distinct: each word is masked at its low bits
    /// before being shifted up, so whether a small offset survives at all
    /// depends on the bits it happens to carry into. Two adjacent blocks can
    /// draw the identical sequence — seed 3 does exactly that for blocks 0 and
    /// 1, which the tests below pin down.
    ///
    /// That is less damaging than it sounds: a block draws only for the pairs
    /// it owns, so two blocks sharing a sequence still spend it on different
    /// pairs. It is reproduced as is rather than repaired, because the
    /// priorities it hands out are what the `max_candidates` cap selects on.
    pub fn stream(&self, block: i64) -> Self {
        Self {
            state: [
                self.state[0] + block,
                self.state[1] + block,
                self.state[2] + block,
            ],
        }
    }

    /// The next value, as an `int32`.
    pub fn next_int(&mut self) -> i32 {
        let s = &mut self.state;
        s[0] = (((s[0] & 4_294_967_294) << 12) & 0xFFFF_FFFF)
            ^ ((((s[0] << 13) & 0xFFFF_FFFF) ^ s[0]) >> 19);
        s[1] = (((s[1] & 4_294_967_288) << 4) & 0xFFFF_FFFF)
            ^ ((((s[1] << 2) & 0xFFFF_FFFF) ^ s[1]) >> 25);
        s[2] = (((s[2] & 4_294_967_280) << 17) & 0xFFFF_FFFF)
            ^ ((((s[2] << 3) & 0xFFFF_FFFF) ^ s[2]) >> 11);
        (s[0] ^ s[1] ^ s[2]) as i32
    }

    /// `tau_rand`: the next value mapped into `[0, 1]`.
    ///
    /// Divided in double precision and narrowed at the end, as numba's
    /// `abs(float(integer) / 0x7FFFFFFF)` does.
    pub fn next_unit(&mut self) -> Scalar {
        let value = self.next_int() as f64 / i32::MAX as f64;
        value.abs() as Scalar
    }

    /// A coin flip: `abs(tau_rand_int(state)) % 2`.
    pub fn next_side(&mut self) -> u8 {
        (self.next_int().wrapping_abs() % 2) as u8
    }
}

/// Index `len` elements with a value that numpy would have wrapped.
///
/// pynndescent takes `tau_rand_int(rng_state) % indices.shape[0]` without an
/// absolute value, so the result is negative whenever the draw is — and numba,
/// like numpy, reads a negative index from the end of the array. Reproduced
/// rather than corrected: the two points a hyperplane is drawn between are
/// picked this way, and picking them differently is a different forest.
pub fn wrap_index(value: i32, len: usize) -> usize {
    let len = len as i32;
    let at = value % len;
    if at < 0 {
        (at + len) as usize
    } else {
        at as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The generator's first values, taken from pynndescent itself.
    ///
    /// Produced by `from pynndescent.utils import tau_rand_int; state =
    /// np.array([...], dtype=np.int64); [tau_rand_int(state) for _ in
    /// range(6)]`. The second state starts with negative words, which is what
    /// `randint(INT32_MIN, INT32_MAX)` hands out half the time and what makes
    /// the arithmetic shifts observable.
    #[test]
    fn matches_pynndescent_from_a_known_state() {
        let mut rng = TauRand {
            state: [123_456_789, 362_436_069, 521_288_629],
        };
        let drawn: Vec<i32> = (0..6).map(|_| rng.next_int()).collect();
        assert_eq!(
            drawn,
            vec![
                -1_844_911_742,
                1_850_835_924,
                1_554_551_309,
                1_039_485_522,
                100_536_870,
                -2_037_467_962
            ]
        );

        let mut negative = TauRand {
            state: [-1_834_591_634, 1_057_145_231, -204_124_832],
        };
        let drawn: Vec<i32> = (0..6).map(|_| negative.next_int()).collect();
        assert_eq!(
            drawn,
            vec![
                257_473_592,
                -844_877_467,
                -12_234_743,
                19_961_602,
                -1_884_128_566,
                355_822_192
            ]
        );
    }

    /// The unit draw, likewise.
    #[test]
    fn the_unit_draw_matches_pynndescent() {
        let mut rng = TauRand {
            state: [123_456_789, 362_436_069, 521_288_629],
        };
        let drawn: Vec<Scalar> = (0..3).map(|_| rng.next_unit()).collect();
        assert_eq!(drawn, vec![0.859_104, 0.861_862_66, 0.723_894_36]);
    }

    #[test]
    fn the_unit_draw_stays_in_range() {
        let mut rng = TauRand::seeded(7);
        for _ in 0..10_000 {
            let value = rng.next_unit();
            assert!((0.0..=1.0).contains(&value), "{value} out of range");
        }
    }

    #[test]
    fn a_seed_fixes_the_stream() {
        let mut first = TauRand::seeded(11);
        let mut second = TauRand::seeded(11);
        let mut other = TauRand::seeded(12);
        let a: Vec<i32> = (0..20).map(|_| first.next_int()).collect();
        let b: Vec<i32> = (0..20).map(|_| second.next_int()).collect();
        let c: Vec<i32> = (0..20).map(|_| other.next_int()).collect();
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    /// The block offsets are applied as pynndescent applies them.
    ///
    /// Reference values from `state = np.array([123456789, 362436069,
    /// 521288629]) + n; [tau_rand_int(state) for _ in range(5)]`.
    #[test]
    fn block_streams_match_pynndescents_offsets() {
        let base = TauRand {
            state: [123_456_789, 362_436_069, 521_288_629],
        };
        let drawn = |block| {
            let mut rng = base.stream(block);
            (0..5).map(|_| rng.next_int()).collect::<Vec<i32>>()
        };

        assert_eq!(
            drawn(0),
            vec![
                -1_844_911_742,
                1_850_835_924,
                1_554_551_309,
                1_039_485_522,
                100_536_870
            ]
        );
        assert_eq!(
            drawn(1),
            vec![
                -1_844_919_934,
                1_817_281_364,
                1_554_027_087,
                -1_108_252_078,
                1_207_833_254
            ]
        );
    }

    /// Offsetting the state is not seeding it: adjacent blocks may collide
    /// outright. Pinned so the quirk is not mistaken for a porting slip later.
    #[test]
    fn adjacent_block_streams_can_coincide() {
        let base = TauRand::seeded(3);
        let drawn = |block| {
            let mut rng = base.stream(block);
            (0..20).map(|_| rng.next_int()).collect::<Vec<i32>>()
        };
        assert_eq!(drawn(0), drawn(1));
        assert_ne!(drawn(0), drawn(8));
    }

    #[test]
    fn negative_draws_index_from_the_end() {
        assert_eq!(wrap_index(-1, 10), 9);
        assert_eq!(wrap_index(-10, 10), 0);
        assert_eq!(wrap_index(3, 10), 3);
        assert_eq!(wrap_index(13, 10), 3);
    }
}
