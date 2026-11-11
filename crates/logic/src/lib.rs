//! Lil Deets' logic process (rust-port-plan.md §3.1): every interaction, the cards, the
//! permissions and the watchers. It becomes a binary in P5–P7, supervised by the host.
//!
//! Built so far (no Discord connection needed):
//! - [`links`]: `/links`, the port of `journal/linkscan.py`'s `extract_from_message` and
//!   `write_outputs` (P6), checked byte for byte against `tests/parity/expected/links/`.

pub mod links;
