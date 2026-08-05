//! The four MOSNA analyses.
//!
//! One module per Python entry point:
//!
//! | Rust | Python |
//! |---|---|
//! | [`fn@tysserand_network`] | `package/tysserand_network.py` |
//! | [`fn@assortativity`] | `package/assortativity.py` |
//! | [`fn@niche_analysis`] | `package/niche_analysis.py` |
//! | [`fn@clear_temporary`] | `package/clear_temporary.py` |
//!
//! Each takes the parsed configuration, the working directory chosen in the
//! GUI, a progress reporter and a figure sink. Splitting the figures out behind
//! a trait keeps the data pipeline runnable — and testable — while the plotting
//! crate is still being written.

pub mod assortativity;
pub mod clear_temporary;
pub mod error;
pub mod figures;
pub mod niche_analysis;
pub mod progress;
pub mod tysserand_network;
pub mod verif_cpu;

pub use assortativity::assortativity;
pub use clear_temporary::clear_temporary;
pub use error::{PipelineError, Result};
pub use figures::{FigureSink, NoFigures};
pub use niche_analysis::niche_analysis;
pub use progress::{Progress, SilentProgress, StdoutProgress};
pub use tysserand_network::tysserand_network;
pub use verif_cpu::verif_cpu;
