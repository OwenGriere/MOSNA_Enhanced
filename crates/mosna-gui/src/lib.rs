//! Graphical interface of MOSNA.
//!
//! Reproduces `GUI_MOSNA.py` — the same three panels in the same splitter, the
//! same tabs, the same widget for every configuration key, the same buttons —
//! on `egui` instead of PySide6, and in a slate-and-teal palette.
//!
//! The crate is split so that everything except the drawing is testable:
//!
//! * [`model`] holds the logic — which widget a key gets, how its value reads
//!   back, how the form is laid out, how a log line is classified, how a
//!   progress line is parsed, which figures belong to which patient.
//! * [`panels`] draws it.
//! * [`docs`] is the manual, as data the interface draws itself.
//! * [`theme`] is the palette.
//!
//! The interface launches the `mosna` binary as a sub-process and parses its
//! `[QT_PROGRESS]` / `[QT_INFO]` output, exactly as the Python GUI launches
//! `python -m package.<module>`. That keeps the two implementations
//! interchangeable: either interface can drive either backend.

pub mod app;
pub mod docs;
pub mod icon;
pub mod model;
pub mod panels;
pub mod theme;

pub use app::MosnaApp;
