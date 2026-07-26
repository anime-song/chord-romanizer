//! Explainable score contributions shared by local and sequence analysis.
//!
//! Rules are deliberately identified by stable ids, but the runtime model does
//! not carry web provenance.  A consumer only needs to know which normalized
//! rule fired, how much it contributed, and why it applied to this input.

#[derive(Clone, Debug, PartialEq)]
/// One independently inspectable contribution to a candidate or transition.
pub struct ScoreEvidence {
    pub rule_id: String,
    pub contribution: f64,
    pub explanation: String,
}

impl ScoreEvidence {
    pub(crate) fn new(
        rule_id: impl Into<String>,
        contribution: f64,
        explanation: impl Into<String>,
    ) -> Self {
        Self {
            rule_id: rule_id.into(),
            contribution,
            explanation: explanation.into(),
        }
    }
}
