//! MCP server for MOSNA, over both of its implementations.
//!
//! MOSNA_Enhanced (Rust) and MOSNA_GUI (Python) run the same three steps on the
//! same `configuration.yaml` and the same nodes files. This server stands on
//! that shared contract rather than on either code base: it checks inputs
//! against what *both* will do with them, starts a step with either one, and
//! compares what they produced.

pub mod agreement;
pub mod assortativity;
pub mod backend;
pub mod check;
pub mod compare;
pub mod jobs;
pub mod protocol;
pub mod report;
pub mod server;

pub use server::Server;
