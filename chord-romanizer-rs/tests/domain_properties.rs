use chord_romanizer::speller::{degree_from_spelling, spell_degree_note};
use chord_romanizer::{
    Degree, NoteLetter, ParsedSymbol, ProgressionItem, RomanDegree, Romanizer, SpelledNote,
    parse_chord,
};

#[test]
fn degree_spelling_round_trips_exhaustively_in_supported_range() {
    for tonic_letter in NoteLetter::ALL {
        for tonic_accidental in -2..=2 {
            let tonic = SpelledNote::new(tonic_letter, tonic_accidental);
            for degree in RomanDegree::ALL {
                for degree_accidental in -2..=2 {
                    let expected = Degree::new(degree_accidental, degree);
                    let note = spell_degree_note(expected, tonic);
                    assert_eq!(
                        degree_from_spelling(note, tonic),
                        expected,
                        "round trip failed: tonic={tonic}, degree={expected}, note={note}"
                    );
                }
            }
        }
    }
}

#[test]
fn parser_handles_a_deterministic_fuzz_corpus_without_panics() {
    let accidentals = ["", "b", "#", "bb", "##", "x"];
    let qualities = ["", "m", "7", "M7", "maj7", "m7", "m7-5", "dim", "+"];
    let basses = [None, Some("C"), Some("F#"), Some("Bbb")];

    for letter in NoteLetter::ALL {
        for accidental in accidentals {
            for quality in qualities {
                for bass in basses {
                    let slash = bass.map_or_else(String::new, |bass| format!("/{bass}"));
                    let symbol = format!("{}{accidental}{quality}{slash}", letter.as_char());
                    let parsed = parse_chord(&symbol)
                        .unwrap_or_else(|error| panic!("failed to parse {symbol}: {error}"));
                    assert!(matches!(parsed, ParsedSymbol::Chord(_)));
                }
            }
        }
    }

    for invalid in ["", "/", "Q7", "C/", "C/Huh", "C/G/Db", "♭C"] {
        assert!(
            parse_chord(invalid).is_err(),
            "accepted invalid input {invalid}"
        );
    }
}

#[test]
fn every_pitch_class_can_be_analyzed_in_representative_keys() {
    let roots = [
        "C", "C#", "D", "Eb", "E", "F", "F#", "G", "Ab", "A", "Bb", "B",
    ];
    for tonic in ["C", "F#", "Gb", "G#", "Fb"] {
        let romanizer = Romanizer::new(tonic).unwrap();
        for root in roots {
            let result =
                romanizer.annotate_progression(&[ProgressionItem::new(parse_chord(root).unwrap())]);
            assert_eq!(result.len(), 1, "tonic={tonic}, root={root}");
            assert!(!result[0].roman.is_empty());
        }
    }
}
