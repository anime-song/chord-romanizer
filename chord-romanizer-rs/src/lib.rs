//! A Rust port of the Python `chord_romanizer` package.
//!
//! The compatibility profile intentionally preserves the Python 0.1.9
//! analysis rules while replacing stringly-typed notes and degrees with
//! explicit domain types.
//!
//! ```
//! use chord_romanizer::{ProgressionItem, Romanizer, parse_chord};
//!
//! let progression: Vec<_> = ["Dm7", "G7", "Cmaj7"]
//!     .into_iter()
//!     .map(|symbol| ProgressionItem::new(parse_chord(symbol).unwrap()))
//!     .collect();
//! let results = Romanizer::new("C")
//!     .unwrap()
//!     .annotate_progression(&progression);
//!
//! assert_eq!(
//!     results.iter().map(|result| result.roman.as_str()).collect::<Vec<_>>(),
//!     ["IIm7", "V7", "IM7"],
//! );
//! assert!(results[0].is_ii_v_start);
//! assert!(results[2].is_resolution_target);
//! ```

pub mod analysis;
mod display;
pub mod domain;
pub mod error;
mod notation;
mod profile;
pub mod romanizer;
mod theory;

// These compatibility facades preserve the public module paths from the
// initial port while the implementation lives in responsibility-based
// folders. Existing callers can keep using `chord_romanizer::parser`, etc.
pub mod interpreter {
    pub use crate::analysis::interpreter::*;
}

pub mod parser {
    pub use crate::notation::parser::*;
}

pub mod speller {
    pub use crate::theory::speller::*;
}

pub mod structure {
    pub use crate::theory::structure::*;
}

pub use analysis::{
    AnalysisLattice, AnalysisPath, BlackadderContext, BlackadderFunction, BlackadderInterpretation,
    BlackadderObservationKind, BlackadderObservations, BlackadderOrigin, BlackadderScale,
    BlackadderStructure, CadentialSpan, CandidateConstraint, CandidateLayer, CandidateTransition,
    DominantRelation, GlobalKeyRequest, HarmonicClassification, HarmonicInterpretation,
    HarmonicResolution, HarmonicResolutionKind, HarmonicRole, HarmonicSource,
    InterpretationCandidate, InterpretationFamily, InterpretationKind, InterpretationTree,
    InterpretationTreeNode, InterpretationTreeOptions, KeyAnalysisOptions, KeyTreeRoot,
    KeyedAnalysisPath, KeyedPathSelection, ModulationCadence, ModulationMechanism, ModulationSpan,
    PathSelection, PendingPredominant, PendingResolution, PivotChord, PivotKind, ScoreEvidence,
    TonalKey, TonalMode, TonalPerspective, TonalScope, TreeCondition, TritoneSpelling,
    WholeToneCollection,
};
pub use display::AnalysisDisplay;
pub use domain::{
    ChordDegree, ChordQuality, Degree, DegreeModifier, ModifierKind, NoteLetter, ParsedChord,
    ParsedSymbol, PitchClass, ProgressionItem, QualityClass, RomanDegree, SeventhQuality,
    SpelledNote,
};
pub use error::{AnalysisError, ParseError};
pub use interpreter::{
    ChordInterpreter, FunctionalRole, HybridAnalysis, HybridCandidate, HybridKind,
    SlashClassification,
};
pub use parser::parse_chord;
pub use profile::{BehaviorProfile, KeyBoundaryPolicy, NoChordPolicy};
pub use romanizer::{
    AlternateKind, AlternateLabel, AnnotatedEvent, FunctionalInterpretation, ResolutionType,
    RomanizedChord, Romanizer, RomanizerOptions,
};
