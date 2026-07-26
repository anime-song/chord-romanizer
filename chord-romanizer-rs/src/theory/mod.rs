//! Deterministic music-theory primitives.
//!
//! `speller` converts between pitch classes, scale degrees, and written note
//! names. `structure` turns a parsed quality into a single chord formula used
//! consistently by inversion checks, tone spelling, and functional tests.

pub(crate) mod speller;
pub(crate) mod structure;
