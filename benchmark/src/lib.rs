//! Benchmark and reproducibility harness for MOSNA.
//!
//! Three levels, because the pipeline is not uniformly deterministic and
//! pretending otherwise would produce a benchmark that either fails at random
//! or proves nothing:
//!
//! | Level | What it covers | What it claims |
//! |---|---|---|
//! | 1 — golden | the deterministic core: geometry, NAS, mixing matrices, assortativity | the numbers are the ones recorded in `golden/`, to 1e-9 relative |
//! | 2 — reproducibility | the seeded stochastic parts: permutation null, k-means, GMM, Leiden, UMAP | two runs agree exactly, whatever the thread count |
//! | 3 — recovery | the irreducibly stochastic layer | the niches found match the niches planted, by adjusted Rand index |
//!
//! Plus timings and peak memory across cohort sizes, which is what the word
//! benchmark usually means.
//!
//! See `benchmark/README.md` for the protocol and `cargo run -p mosna-bench --
//! --help` for the commands.

pub mod agreement;
pub mod cohort;
pub mod fingerprint;
pub mod levels;
pub mod report;
pub mod timing;
