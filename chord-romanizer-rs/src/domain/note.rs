//! Core note representation.
//!
//! `PitchClass` deliberately forgets spelling; `SpelledNote` deliberately
//! preserves it. Analysis compares pitch classes but uses written letters to
//! derive theoretically meaningful Roman degrees and chord-tone spellings.

use std::fmt;

use crate::error::ParseError;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
/// A sounding pitch modulo octave, always normalized to `0..=11`.
pub struct PitchClass(u8);

impl PitchClass {
    pub const fn new(value: u8) -> Self {
        Self(value % 12)
    }

    pub const fn value(self) -> u8 {
        self.0
    }

    pub fn offset(self, semitones: i16) -> Self {
        Self::new((i16::from(self.0) + semitones).rem_euclid(12) as u8)
    }

    pub fn distance_from(self, reference: Self) -> u8 {
        (i16::from(self.0) - i16::from(reference.0)).rem_euclid(12) as u8
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
/// One of the seven diatonic note letters.
pub enum NoteLetter {
    C,
    D,
    E,
    F,
    G,
    A,
    B,
}

impl NoteLetter {
    pub const ALL: [Self; 7] = [
        Self::C,
        Self::D,
        Self::E,
        Self::F,
        Self::G,
        Self::A,
        Self::B,
    ];

    pub fn parse(value: char) -> Option<Self> {
        match value.to_ascii_uppercase() {
            'C' => Some(Self::C),
            'D' => Some(Self::D),
            'E' => Some(Self::E),
            'F' => Some(Self::F),
            'G' => Some(Self::G),
            'A' => Some(Self::A),
            'B' | 'H' => Some(Self::B),
            _ => None,
        }
    }

    pub const fn index(self) -> usize {
        match self {
            Self::C => 0,
            Self::D => 1,
            Self::E => 2,
            Self::F => 3,
            Self::G => 4,
            Self::A => 5,
            Self::B => 6,
        }
    }

    pub const fn natural_pitch_class(self) -> PitchClass {
        PitchClass::new(match self {
            Self::C => 0,
            Self::D => 2,
            Self::E => 4,
            Self::F => 5,
            Self::G => 7,
            Self::A => 9,
            Self::B => 11,
        })
    }

    pub fn shift(self, steps: usize) -> Self {
        Self::ALL[(self.index() + steps) % Self::ALL.len()]
    }

    pub const fn as_char(self) -> char {
        match self {
            Self::C => 'C',
            Self::D => 'D',
            Self::E => 'E',
            Self::F => 'F',
            Self::G => 'G',
            Self::A => 'A',
            Self::B => 'B',
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
/// Written note identity: diatonic letter plus an unbounded accidental count.
pub struct SpelledNote {
    pub letter: NoteLetter,
    pub accidental: i8,
}

impl SpelledNote {
    pub const fn new(letter: NoteLetter, accidental: i8) -> Self {
        Self { letter, accidental }
    }

    pub fn parse(token: &str) -> Result<Self, ParseError> {
        let token = token.trim();
        if token.is_empty() {
            return Err(ParseError::InvalidNote(token.to_owned()));
        }

        // Python 0.1.9 accepted H/Hb in its pitch-class normalizer but its
        // speller could not represent H. StrictV1 keeps only the unambiguous
        // entrance alias H = B and rejects accidental-bearing H spellings.
        // Normalizing here guarantees that no downstream type contains H.
        let upper = token.to_ascii_uppercase();
        if upper == "H" {
            return Ok(Self::new(NoteLetter::B, 0));
        }
        if upper.starts_with('H') {
            return Err(ParseError::InvalidNote(token.to_owned()));
        }

        let mut chars = token.chars();
        let first = chars
            .next()
            .ok_or_else(|| ParseError::InvalidNote(token.to_owned()))?;
        let letter =
            NoteLetter::parse(first).ok_or_else(|| ParseError::InvalidNote(token.to_owned()))?;

        let mut accidental = 0_i16;
        for ch in chars {
            match ch {
                '#' => accidental += 1,
                'b' | 'B' => accidental -= 1,
                'x' | 'X' => accidental += 2,
                _ => return Err(ParseError::InvalidNote(token.to_owned())),
            }
        }
        let accidental = i8::try_from(accidental)
            .map_err(|_| ParseError::AccidentalOutOfRange(token.to_owned()))?;
        Ok(Self::new(letter, accidental))
    }

    pub fn pitch_class(self) -> PitchClass {
        self.letter
            .natural_pitch_class()
            .offset(i16::from(self.accidental))
    }
}

impl fmt::Display for SpelledNote {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.letter.as_char())?;
        let (symbol, count) = if self.accidental >= 0 {
            ('#', self.accidental as usize)
        } else {
            ('b', self.accidental.unsigned_abs() as usize)
        };
        for _ in 0..count {
            write!(f, "{symbol}")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_repeated_accidentals_and_x() {
        assert_eq!(SpelledNote::parse("Bbb").unwrap().pitch_class().value(), 9);
        assert_eq!(SpelledNote::parse("F##").unwrap().pitch_class().value(), 7);
        assert_eq!(SpelledNote::parse("Fx").unwrap().to_string(), "F##");
    }

    #[test]
    fn pitch_class_distance_wraps() {
        assert_eq!(PitchClass::new(0).distance_from(PitchClass::new(1)), 11);
    }
}
