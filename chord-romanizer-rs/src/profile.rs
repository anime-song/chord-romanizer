//! Explicit behavior switches separating compatibility from corrected rules.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BehaviorProfile {
    /// Reproduce the Python package's observable harmonic rules.
    Python019,
    /// Use parsed qualities, one chord formula, normalized rendering, and
    /// candidate-preserving analysis.
    StrictV1,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Whether a caller-supplied tonic change severs progression context.
pub enum KeyBoundaryPolicy {
    /// Analyze each known-key region independently.
    Break,
    /// Allow transition rules to cross tonic changes.
    Continue,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NoChordPolicy {
    /// A rest or short gap remains in the aligned output but harmonic context
    /// connects the surrounding chords.
    Transparent,
    /// No-chord events split harmonic context.
    Break,
}
