//! Chord formulas and structural predicates.
//!
//! Every StrictV1 operation that needs chord tones comes through
//! [`ChordFormula`]. This single source prevents inversion checks, written tone
//! spelling, and dominant/minor classification from disagreeing about the same
//! symbol. Legacy helpers remain for the Python019 compatibility profile.

use std::collections::{HashMap, HashSet};

use crate::domain::{
    ChordDegree, ChordQuality, ModifierKind, ParsedChord, PitchClass, QualityClass, SeventhQuality,
    SpelledNote,
};
use crate::profile::BehaviorProfile;
use crate::speller::{semitone_distance, spell_pitch_class};

type FormulaTone = (u8, usize);
type TriadFormula = &'static [FormulaTone];
type SeventhFormula = Option<FormulaTone>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// One formula member with diatonic identity and chromatic distance.
pub struct StructuredTone {
    /// Chord-member identity (third, fifth, ninth, ...).
    pub degree: ChordDegree,
    /// Chromatic change relative to the natural major/perfect interval.
    pub alteration: i8,
    /// Semitone distance above the root, reduced only when a pitch class is
    /// requested.
    pub semitones: i8,
    /// Diatonic letter displacement used to spell the tone correctly.
    pub letter_steps: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// Authoritative set of tones implied or stated by a parsed quality.
pub struct ChordFormula {
    pub tones: Vec<StructuredTone>,
}

/// Select the formula rules belonging to the requested behavior profile.
///
/// Python019 preserves its permissive major-triad fallback. StrictV1 returns
/// `None` for unknown notation so callers can report indeterminate structure.
pub fn formula(chord: &ParsedChord, behavior: BehaviorProfile) -> Option<ChordFormula> {
    match behavior {
        BehaviorProfile::Python019 => Some(legacy_formula(&chord.quality)),
        BehaviorProfile::StrictV1 => formula_from_quality(&chord.quality_parsed),
    }
}

/// Build a StrictV1 formula from a structured quality.
pub fn formula_from_quality(quality: &ChordQuality) -> Option<ChordFormula> {
    if !quality.is_fully_recognized() {
        return None;
    }

    // Start with the base structure. Suspended chords replace the third;
    // augmented and diminished classes define their altered fifth directly.
    let mut tones = match quality.class {
        QualityClass::Major => vec![
            structured_tone(ChordDegree::Root, 0),
            structured_tone(ChordDegree::Third, 0),
            structured_tone(ChordDegree::Fifth, 0),
        ],
        QualityClass::Minor => vec![
            structured_tone(ChordDegree::Root, 0),
            structured_tone(ChordDegree::Third, -1),
            structured_tone(ChordDegree::Fifth, 0),
        ],
        QualityClass::Diminished | QualityClass::HalfDiminished => vec![
            structured_tone(ChordDegree::Root, 0),
            structured_tone(ChordDegree::Third, -1),
            structured_tone(ChordDegree::Fifth, -1),
        ],
        QualityClass::Augmented => vec![
            structured_tone(ChordDegree::Root, 0),
            structured_tone(ChordDegree::Third, 0),
            structured_tone(ChordDegree::Fifth, 1),
        ],
        QualityClass::Suspended2 => vec![
            structured_tone(ChordDegree::Root, 0),
            structured_tone(ChordDegree::Second, 0),
            structured_tone(ChordDegree::Fifth, 0),
        ],
        QualityClass::Suspended4 => vec![
            structured_tone(ChordDegree::Root, 0),
            structured_tone(ChordDegree::Fourth, 0),
            structured_tone(ChordDegree::Fifth, 0),
        ],
        QualityClass::Power => vec![
            structured_tone(ChordDegree::Root, 0),
            structured_tone(ChordDegree::Fifth, 0),
        ],
        QualityClass::Unknown => return None,
    };

    // Seventh quality is orthogonal to the triad (e.g. minor-major seventh).
    if let Some(seventh) = quality.seventh {
        let alteration = match seventh {
            SeventhQuality::Major => 0,
            SeventhQuality::Minor => -1,
            SeventhQuality::Diminished => -2,
        };
        tones.push(structured_tone(ChordDegree::Seventh, alteration));
    }

    // Explicit alterations replace the natural member of the same degree.
    // Thus C9(b9) does not retain both natural 9 and b9 by accident, while
    // C7(b9) never invents a natural ninth in the first place.
    for modifier in &quality.modifiers {
        if modifier.kind == ModifierKind::Altered {
            tones.retain(|existing| existing.degree != modifier.degree || existing.alteration != 0);
        }
        let candidate = structured_tone(modifier.degree, modifier.alteration);
        if !tones.iter().any(|existing| {
            existing.degree == candidate.degree && existing.alteration == candidate.alteration
        }) {
            tones.push(candidate);
        }
    }
    // Omissions run last so they also remove tones implied by 9/11/13 symbols.
    tones.retain(|tone| !quality.omissions.contains(&tone.degree));
    Some(ChordFormula { tones })
}

fn legacy_formula(quality: &str) -> ChordFormula {
    // Keep legacy substring/fallback behavior isolated from StrictV1. It exists
    // only so differential tests can reproduce Python 0.1.9 exactly.
    ChordFormula {
        tones: intervals(quality)
            .into_iter()
            .map(|semitones| StructuredTone {
                degree: ChordDegree::Root,
                alteration: 0,
                semitones: semitones as i8,
                letter_steps: 0,
            })
            .collect(),
    }
}

fn structured_tone(degree: ChordDegree, alteration: i8) -> StructuredTone {
    // Compound degrees share a pitch-class base with their simple equivalent,
    // but retain a diatonic displacement for correct note-letter spelling.
    let (base, letter_steps) = match degree {
        ChordDegree::Root => (0, 0),
        ChordDegree::Second | ChordDegree::Ninth => (2, 1),
        ChordDegree::Third => (4, 2),
        ChordDegree::Fourth | ChordDegree::Eleventh => (5, 3),
        ChordDegree::Fifth => (7, 4),
        ChordDegree::Sixth | ChordDegree::Thirteenth => (9, 5),
        ChordDegree::Seventh => (11, 6),
    };
    StructuredTone {
        degree,
        alteration,
        semitones: base + alteration,
        letter_steps,
    }
}

pub fn formula_intervals(chord: &ParsedChord, behavior: BehaviorProfile) -> Option<HashSet<u8>> {
    Some(
        formula(chord, behavior)?
            .tones
            .into_iter()
            .map(|tone| tone.semitones.rem_euclid(12) as u8)
            .collect(),
    )
}

pub fn is_inversion_for(chord: &ParsedChord, behavior: BehaviorProfile) -> bool {
    let Some(bass) = chord.bass else {
        return false;
    };
    // A chord symbol does not encode octave or voicing. Membership by pitch
    // class is therefore the strongest inversion test available here.
    formula_intervals(chord, behavior)
        .is_some_and(|tones| tones.contains(&semitone_distance(bass, chord.root)))
}

pub fn spelled_tones_for(
    chord: &ParsedChord,
    root: SpelledNote,
    behavior: BehaviorProfile,
) -> HashMap<PitchClass, SpelledNote> {
    if behavior == BehaviorProfile::Python019 {
        return spelled_tones(root, &chord.quality);
    }
    let Some(formula) = formula(chord, behavior) else {
        return HashMap::new();
    };
    // `letter_steps` is calculated from the chord degree rather than guessed
    // from pitch class, preserving spellings such as Gb instead of F# in Cdim.
    formula
        .tones
        .into_iter()
        .map(|tone| {
            let pc = root.pitch_class().offset(i16::from(tone.semitones));
            (
                pc,
                spell_pitch_class(root.letter.shift(tone.letter_steps), pc),
            )
        })
        .collect()
}

pub fn is_dominant_for(chord: &ParsedChord, behavior: BehaviorProfile) -> bool {
    if behavior == BehaviorProfile::Python019 {
        return is_dominant_quality(&chord.quality);
    }
    chord.quality_parsed.seventh == Some(SeventhQuality::Minor)
        && matches!(
            chord.quality_parsed.class,
            QualityClass::Major
                | QualityClass::Augmented
                | QualityClass::Suspended2
                | QualityClass::Suspended4
        )
}

pub fn is_tonic_for(chord: &ParsedChord, behavior: BehaviorProfile) -> bool {
    if behavior == BehaviorProfile::Python019 {
        // Compatibility mode preserves the historical broad definition.
        return !is_dominant_for(chord, behavior);
    }

    // "Not dominant" is not sufficient evidence that a sonority can be a
    // tonic-resolution target.  In particular, Cdim must not complete G7-C
    // or Bdim7-C merely because its written root is C.  StrictV1 therefore
    // admits only stable major/minor structures and excludes dominant-quality
    // sevenths even though their base triad is major.
    matches!(
        chord.quality_parsed.class,
        QualityClass::Major | QualityClass::Minor
    ) && !is_dominant_for(chord, behavior)
}

pub fn is_minor_for(chord: &ParsedChord, behavior: BehaviorProfile) -> bool {
    if behavior == BehaviorProfile::Python019 {
        return is_minor_quality(&chord.quality);
    }
    matches!(
        chord.quality_parsed.class,
        QualityClass::Minor | QualityClass::HalfDiminished
    )
}

pub fn is_diminished_for(chord: &ParsedChord, behavior: BehaviorProfile) -> bool {
    if behavior == BehaviorProfile::Python019 {
        let lower = chord.quality.to_ascii_lowercase();
        return lower.contains("dim") || lower.contains("m7-5") || lower.contains("m7b5");
    }
    matches!(
        chord.quality_parsed.class,
        QualityClass::Diminished | QualityClass::HalfDiminished
    )
}

pub fn intervals(quality: &str) -> HashSet<u8> {
    if quality.contains("M7") {
        return HashSet::from([0, 4, 7, 11]);
    }

    let lower = quality.to_ascii_lowercase();
    if lower.contains("m7-5") || lower.contains("m7b5") {
        HashSet::from([0, 3, 6, 10])
    } else if lower.contains("dim") || lower.contains('o') {
        HashSet::from([0, 3, 6])
    } else if lower.contains("maj7") || lower.contains("ma7") {
        HashSet::from([0, 4, 7, 11])
    } else if lower.contains("m7") {
        HashSet::from([0, 3, 7, 10])
    } else if lower.contains('7') {
        HashSet::from([0, 4, 7, 10])
    } else if lower.contains('m') {
        HashSet::from([0, 3, 7])
    } else {
        HashSet::from([0, 4, 7])
    }
}

pub fn is_inversion(root: SpelledNote, bass: SpelledNote, quality: &str) -> bool {
    intervals(quality).contains(&semitone_distance(bass, root))
}

pub fn spelled_tones(root: SpelledNote, quality: &str) -> HashMap<PitchClass, SpelledNote> {
    // This ordering intentionally mirrors Python 0.1.9. It is not merged with
    // `intervals` because doing so would silently change legacy slash behavior.
    let lower = quality.to_ascii_lowercase();
    let (triad, seventh): (TriadFormula, SeventhFormula) = if quality.contains("M7") {
        (&[(0, 0), (4, 2), (7, 4)], Some((11, 6)))
    } else if lower.contains("m7") {
        (&[(0, 0), (3, 2), (7, 4)], Some((10, 6)))
    } else if lower.contains("maj7") {
        (&[(0, 0), (4, 2), (7, 4)], Some((11, 6)))
    } else if lower.contains('7') {
        (&[(0, 0), (4, 2), (7, 4)], Some((10, 6)))
    } else if lower.contains('m') {
        (&[(0, 0), (3, 2), (7, 4)], None)
    } else {
        (&[(0, 0), (4, 2), (7, 4)], None)
    };

    let root_pc = root.pitch_class();
    let mut tones = HashMap::new();
    for (semitones, letter_steps) in triad.iter().copied() {
        let pc = root_pc.offset(i16::from(semitones));
        tones.insert(pc, spell_pitch_class(root.letter.shift(letter_steps), pc));
    }
    if let Some((semitones, letter_steps)) = seventh {
        let pc = root_pc.offset(i16::from(semitones));
        tones.insert(pc, spell_pitch_class(root.letter.shift(letter_steps), pc));
    }
    tones
}

pub fn is_aug_quality(quality: &str) -> bool {
    quality.to_ascii_lowercase().contains("aug") || quality.contains('+')
}

pub fn aug_triad_pitch_classes(root: SpelledNote) -> HashSet<PitchClass> {
    HashSet::from([
        root.pitch_class(),
        root.pitch_class().offset(4),
        root.pitch_class().offset(8),
    ])
}

pub fn is_dominant_quality(quality: &str) -> bool {
    if quality.contains("M7") {
        return false;
    }
    let lower = quality.to_ascii_lowercase();
    if lower.contains("maj7") || lower.contains("ma7") {
        return false;
    }
    if lower.contains('m') && !lower.contains("dim") {
        return false;
    }
    lower.contains('7')
}

pub fn is_tonic_quality(quality: &str) -> bool {
    !is_dominant_quality(quality)
}

pub fn is_minor_quality(quality: &str) -> bool {
    if quality.contains("M7") {
        return false;
    }
    let lower = quality.to_ascii_lowercase();
    lower.contains('m') && !lower.contains("maj") && !lower.contains("dim")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(value: &str) -> SpelledNote {
        SpelledNote::parse(value).unwrap()
    }

    #[test]
    fn recognizes_supported_formulas() {
        assert_eq!(intervals("m7-5"), HashSet::from([0, 3, 6, 10]));
        assert_eq!(intervals("M7"), HashSet::from([0, 4, 7, 11]));
        assert_eq!(intervals("7"), HashSet::from([0, 4, 7, 10]));
    }

    #[test]
    fn spells_minor_inversion_tones() {
        let tones = spelled_tones(note("D#"), "m");
        assert_eq!(tones[&note("F#").pitch_class()].to_string(), "F#");
    }
}
