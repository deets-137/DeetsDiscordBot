//! Lil Deets' text formats and Components V2 card builders.
//!
//! Pure functions only: no I/O, no clock, no network. The caller passes in everything a
//! format needs (the queue, the now-playing state, the time). The Python bot's `bot/dm.py`
//! is the specification, and `tests/parity/` is the test (rust-port-plan.md §4).
//!
//! - [`model`]: the bridge replies the formats read (`Track`, `NowPlaying`, `Queue`).
//! - [`text`]: every text format.
//! - [`cards`]: the Components V2 cards (np, queue in layout C, picker), twilight-model 0.18.

pub mod cards;
pub mod model;
pub mod text;
