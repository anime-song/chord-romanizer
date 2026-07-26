use std::error::Error;
use std::fmt;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ParseError {
    EmptyInput,
    InvalidRoot(String),
    InvalidBass(String),
    InvalidNote(String),
    AccidentalOutOfRange(String),
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyInput => write!(f, "the chord symbol is empty"),
            Self::InvalidRoot(value) => write!(f, "invalid chord root: {value}"),
            Self::InvalidBass(value) => write!(f, "invalid slash bass: {value}"),
            Self::InvalidNote(value) => write!(f, "invalid note: {value}"),
            Self::AccidentalOutOfRange(value) => {
                write!(f, "too many accidentals in note: {value}")
            }
        }
    }
}

impl Error for ParseError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AnalysisError {
    InvalidTonic(String),
}

impl fmt::Display for AnalysisError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidTonic(value) => write!(f, "invalid tonic: {value}"),
        }
    }
}

impl Error for AnalysisError {}
