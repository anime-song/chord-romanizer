//! Conversion between pitch classes, written notes, and Roman scale degrees.
//!
//! Pitch class answers "which sounding pitch?" while `SpelledNote` answers
//! "which diatonic letter and accidental express it?" Keeping both avoids
//! losing distinctions such as F# versus Gb during harmonic analysis.

use crate::domain::{Degree, NoteLetter, PitchClass, RomanDegree, SpelledNote};

pub const MAJOR_SCALE_STEPS: [u8; 7] = [0, 2, 4, 5, 7, 9, 11];

pub fn semitone_distance(target: SpelledNote, reference: SpelledNote) -> u8 {
    target.pitch_class().distance_from(reference.pitch_class())
}

pub fn name_of_pitch_class(pitch_class: PitchClass, prefer_sharps: Option<bool>) -> SpelledNote {
    const SHARP_NAMES: [(NoteLetter, i8); 12] = [
        (NoteLetter::C, 0),
        (NoteLetter::C, 1),
        (NoteLetter::D, 0),
        (NoteLetter::D, 1),
        (NoteLetter::E, 0),
        (NoteLetter::F, 0),
        (NoteLetter::F, 1),
        (NoteLetter::G, 0),
        (NoteLetter::G, 1),
        (NoteLetter::A, 0),
        (NoteLetter::A, 1),
        (NoteLetter::B, 0),
    ];
    const FLAT_NAMES: [(NoteLetter, i8); 12] = [
        (NoteLetter::C, 0),
        (NoteLetter::D, -1),
        (NoteLetter::D, 0),
        (NoteLetter::E, -1),
        (NoteLetter::E, 0),
        (NoteLetter::F, 0),
        (NoteLetter::G, -1),
        (NoteLetter::G, 0),
        (NoteLetter::A, -1),
        (NoteLetter::A, 0),
        (NoteLetter::B, -1),
        (NoteLetter::B, 0),
    ];

    let (letter, accidental) = if prefer_sharps == Some(false) {
        FLAT_NAMES[pitch_class.value() as usize]
    } else {
        SHARP_NAMES[pitch_class.value() as usize]
    };
    SpelledNote::new(letter, accidental)
}

pub fn spell_pitch_class(base_letter: NoteLetter, target: PitchClass) -> SpelledNote {
    let mut diff = i16::from(target.distance_from(base_letter.natural_pitch_class()));
    if diff > 6 {
        diff -= 12;
    }
    SpelledNote::new(base_letter, diff as i8)
}

pub fn calc_degree_base(distance: u8, prefer_sharps: Option<bool>) -> Degree {
    // Compare every major-scale degree by the smallest signed chromatic
    // displacement. Ties are notation choices, resolved by the requested
    // sharp/flat preference rather than by pitch-class arithmetic.
    let prefer = prefer_sharps.unwrap_or(false);
    let mut best_score = i16::MAX;
    let mut best_index = 0;
    let mut best_alteration = 0;

    for (index, step) in MAJOR_SCALE_STEPS.iter().copied().enumerate() {
        let delta = (i16::from(distance) - i16::from(step)).rem_euclid(12);
        let alteration = if delta <= 6 { delta } else { delta - 12 };
        let score = alteration.abs();
        if score < best_score {
            best_score = score;
            best_index = index;
            best_alteration = alteration;
        } else if score == best_score
            && ((prefer && alteration > best_alteration)
                || (!prefer && alteration < best_alteration))
        {
            best_index = index;
            best_alteration = alteration;
        }
    }

    Degree::new(
        best_alteration as i8,
        RomanDegree::from_index(best_index).expect("major scale always has seven degrees"),
    )
}

pub fn degree_from_spelling(note: SpelledNote, tonic: SpelledNote) -> Degree {
    // The written letter fixes the Roman degree. The accidental is then the
    // chromatic difference between that scale-degree spelling and the note's
    // actual pitch class. This preserves theoretical spellings exactly.
    let degree_index = (note.letter.index() + 7 - tonic.letter.index()) % 7;
    let expected = tonic
        .pitch_class()
        .offset(i16::from(MAJOR_SCALE_STEPS[degree_index]));
    let mut diff = i16::from(note.pitch_class().distance_from(expected));
    if diff > 6 {
        diff -= 12;
    }
    Degree::new(
        diff as i8,
        RomanDegree::from_index(degree_index).expect("letter distance is in range"),
    )
}

pub fn spell_degree_note(degree: Degree, tonic: SpelledNote) -> SpelledNote {
    // Reverse of `degree_from_spelling`: first choose the required diatonic
    // letter, then add enough accidentals to reach the desired pitch class.
    let degree_index = degree.degree.index();
    let target = tonic
        .pitch_class()
        .offset(i16::from(MAJOR_SCALE_STEPS[degree_index]) + i16::from(degree.accidental));
    spell_pitch_class(tonic.letter.shift(degree_index), target)
}

pub fn simplify_spelling(note: SpelledNote) -> SpelledNote {
    // Simplification is display-only. Callers retain the theoretical symbol
    // separately, so replacing a double accidental cannot corrupt analysis.
    if note.accidental.unsigned_abs() < 2 {
        return note;
    }
    name_of_pitch_class(note.pitch_class(), Some(note.accidental > 0))
}

pub fn target_accidental_preference(root: SpelledNote) -> Option<bool> {
    if root.accidental < 0 {
        return Some(false);
    }
    if root.accidental > 0 {
        return Some(true);
    }
    match root.letter {
        NoteLetter::F => Some(false),
        NoteLetter::G | NoteLetter::D | NoteLetter::A | NoteLetter::E | NoteLetter::B => Some(true),
        NoteLetter::C => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(value: &str) -> SpelledNote {
        SpelledNote::parse(value).unwrap()
    }

    #[test]
    fn default_degree_ties_prefer_flats() {
        assert_eq!(calc_degree_base(1, None).to_string(), "bII");
        assert_eq!(calc_degree_base(3, None).to_string(), "bIII");
        assert_eq!(calc_degree_base(8, None).to_string(), "bVI");
    }

    #[test]
    fn spelling_round_trip_preserves_degree() {
        for tonic in [note("C"), note("Gb"), note("G#"), note("Fb")] {
            for degree in [
                Degree::new(0, RomanDegree::I),
                Degree::new(-1, RomanDegree::Iii),
                Degree::new(1, RomanDegree::Iv),
                Degree::new(0, RomanDegree::Vii),
            ] {
                let spelled = spell_degree_note(degree, tonic);
                assert_eq!(degree_from_spelling(spelled, tonic), degree);
            }
        }
    }

    #[test]
    fn simplifies_double_accidentals_without_changing_pitch() {
        for value in ["F##", "Bbb", "C###", "Abb"] {
            let original = note(value);
            let simplified = simplify_spelling(original);
            assert_eq!(original.pitch_class(), simplified.pitch_class());
            assert!(simplified.accidental.unsigned_abs() < 2);
        }
    }
}
