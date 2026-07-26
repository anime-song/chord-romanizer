//! Harmonic interpretation and progression-level inference.
//!
//! The analysis pipeline is deliberately split into three layers:
//!
//! 1. [`interpreter`] produces every plausible local slash/hybrid reading.
//! 2. `context` derives neighboring-chord hints without discarding those
//!    local candidates.
//! 3. `lattice` exposes the candidates as a graph and decodes ranked paths.
//!
//! Keeping these layers separate is important: a locally weaker reading can
//! become the best interpretation after a later resolution is observed.

pub mod blackadder;
pub(crate) mod context;
mod evidence;
mod harmony;
pub mod interpreter;
mod key;
mod lattice;
mod memory;
mod modulation;
mod ordinary;
mod tree;

pub use blackadder::*;
pub use evidence::*;
pub use harmony::*;
pub use key::*;
pub use lattice::*;
pub use memory::*;
pub use modulation::*;
pub use tree::*;
