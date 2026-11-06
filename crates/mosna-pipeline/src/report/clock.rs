//! When the report was made.
//!
//! The stamp lives in `mosna-config`, so that the interface — which does not
//! link the pipeline — writes the same spelling into a sensitivity plan as the
//! pipeline writes into a report and a run register.

pub use mosna_config::clock::stamp;
