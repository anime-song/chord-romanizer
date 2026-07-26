//! Parsed chord and progression-event domain types.

use crate::domain::{ChordQuality, SpelledNote};

#[derive(Clone, Debug, Eq, PartialEq)]
/// Lossless chord AST with both raw and structured quality views.
pub struct ParsedChord {
    pub original_symbol: String,
    pub root: SpelledNote,
    pub root_lexeme: String,
    pub quality: String,
    pub quality_parsed: ChordQuality,
    pub bass: Option<SpelledNote>,
    pub bass_lexeme: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// A timeline symbol. Boundaries are explicit because N.C. alone does not
/// reveal whether a silence is long enough to break harmonic context.
pub enum ParsedSymbol {
    Chord(ParsedChord),
    NoChord { original_symbol: String },
    Boundary { label: Option<String> },
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// One symbol plus an optional per-event tonic override.
pub struct ProgressionItem {
    pub symbol: ParsedSymbol,
    pub tonic: Option<SpelledNote>,
}

impl ProgressionItem {
    pub fn new(symbol: ParsedSymbol) -> Self {
        Self {
            symbol,
            tonic: None,
        }
    }

    pub fn in_key(symbol: ParsedSymbol, tonic: SpelledNote) -> Self {
        Self {
            symbol,
            tonic: Some(tonic),
        }
    }

    /// Create an unconditional context boundary such as a section break or a
    /// caller-confirmed long silence.
    pub fn boundary(label: impl Into<String>) -> Self {
        Self {
            symbol: ParsedSymbol::Boundary {
                label: Some(label.into()),
            },
            tonic: None,
        }
    }

    pub fn chord(&self) -> Option<&ParsedChord> {
        match &self.symbol {
            ParsedSymbol::Chord(chord) => Some(chord),
            ParsedSymbol::NoChord { .. } | ParsedSymbol::Boundary { .. } => None,
        }
    }

    pub fn is_boundary(&self) -> bool {
        matches!(self.symbol, ParsedSymbol::Boundary { .. })
    }
}
