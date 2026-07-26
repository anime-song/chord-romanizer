//! Parsing and rendering of user-facing chord notation.
//!
//! This module owns syntax only. Pitch relationships and chord formulas live
//! in `theory`, so formatting decisions cannot silently change music theory.

pub(crate) mod formatter;
pub(crate) mod parser;
