use chord_romanizer::{
    GlobalKeyRequest, HarmonicRole, KeyAnalysisOptions, ProgressionItem, Romanizer, SpelledNote,
    TonalKey, TonalMode, TonalScope, parse_chord,
};

fn progression(symbols: &[&str]) -> Vec<ProgressionItem> {
    symbols
        .iter()
        .map(|symbol| ProgressionItem::new(parse_chord(symbol).unwrap()))
        .collect()
}

fn key(tonic: &str, mode: TonalMode) -> TonalKey {
    TonalKey::new(SpelledNote::parse(tonic).unwrap(), mode)
}

#[test]
fn infers_major_key_and_projects_plain_diatonic_functions() {
    let analyzer = Romanizer::new("C").unwrap();
    let paths = analyzer.analyze_keys_and_functions(
        &progression(&["Cmaj7", "Dm7", "G7", "Cmaj7"]),
        KeyAnalysisOptions::default(),
        3,
    );

    assert_eq!(paths[0].global_key, key("C", TonalMode::Major));
    assert_eq!(
        paths[0]
            .selections
            .iter()
            .map(|selection| selection.role)
            .collect::<Vec<_>>(),
        [
            Some(HarmonicRole::Tonic),
            Some(HarmonicRole::Predominant),
            Some(HarmonicRole::Dominant),
            Some(HarmonicRole::Tonic),
        ]
    );
    assert!(paths[0].key_score > 0.0);
    assert_eq!(
        paths[0].total_score,
        paths[0].key_score + paths[0].function_score
    );
}

#[test]
fn mode_is_inferred_before_minor_key_functions_are_generated() {
    let analyzer = Romanizer::new("C").unwrap();
    let paths = analyzer.analyze_keys_and_functions(
        &progression(&["Am", "Dm7", "E7", "Am"]),
        KeyAnalysisOptions::default(),
        3,
    );

    assert_eq!(paths[0].global_key, key("A", TonalMode::Minor));
    assert_eq!(paths[0].selections[0].role, Some(HarmonicRole::Tonic));
    // The tonic minor chord is not a parallel-minor borrowing when the global
    // hypothesis itself is minor.
    assert!(
        paths[0].selections[0]
            .selection
            .harmonic_classifications
            .iter()
            .all(|classification| classification.families.is_empty())
    );
}

#[test]
fn integrated_path_carries_applied_local_key_without_replacing_global_key() {
    let analyzer = Romanizer::new("C").unwrap();
    let paths = analyzer.analyze_keys_and_functions(
        &progression(&["Em7", "A7", "Dm7", "G7", "Cmaj7"]),
        KeyAnalysisOptions::default(),
        5,
    );
    assert_eq!(paths[0].global_key, key("C", TonalMode::Major));
    assert_eq!(paths[0].selections[0].local_key, key("D", TonalMode::Minor));
    assert_eq!(paths[0].selections[1].local_key, key("D", TonalMode::Minor));
    assert!(
        paths[0].selections[..2]
            .iter()
            .all(|selection| selection.scope == TonalScope::Tonicization)
    );
    // Dm is both the realized local tonic and global ii.  The best complete
    // path treats it as a pivot back to C because the following G7-C cadence
    // supplies stronger future evidence; the applied-target reading remains
    // available in a lower k-best path.
    assert!(
        paths[0].selections[2..]
            .iter()
            .all(|selection| selection.local_key == key("C", TonalMode::Major))
    );
    assert!(
        paths
            .iter()
            .any(|path| { path.selections[2].local_key == key("D", TonalMode::Minor) })
    );
}

#[test]
fn key_hint_is_a_prior_while_fixed_key_is_a_constraint() {
    let analyzer = Romanizer::new("C").unwrap();
    let ambiguous = progression(&["Am7", "Fmaj7", "Cmaj7", "G"]);
    let hinted = analyzer.analyze_keys_and_functions(
        &ambiguous,
        KeyAnalysisOptions {
            global_key: GlobalKeyRequest::Hint(key("A", TonalMode::Minor)),
        },
        3,
    );
    assert!(hinted.iter().any(|path| {
        path.evidence
            .iter()
            .any(|evidence| evidence.rule_id == "builtin.key.caller_hint")
    }));

    let fixed_key = key("E", TonalMode::Major);
    let fixed = analyzer.analyze_keys_and_functions(
        &ambiguous,
        KeyAnalysisOptions {
            global_key: GlobalKeyRequest::Fixed(fixed_key),
        },
        5,
    );
    assert!(fixed.iter().all(|path| path.global_key == fixed_key));
}

#[test]
fn no_chord_only_input_has_no_key_claim() {
    let analyzer = Romanizer::new("C").unwrap();
    let items = progression(&["N.C."]);
    assert!(
        analyzer
            .analyze_keys_and_functions(&items, KeyAnalysisOptions::default(), 5)
            .is_empty()
    );
}

#[test]
fn representative_sharp_flat_and_minor_cadences_choose_the_expected_key() {
    let analyzer = Romanizer::new("C").unwrap();
    let cases = [
        (&["G", "Am7", "D7", "G"][..], key("G", TonalMode::Major)),
        (&["F", "Bb", "C7", "F"][..], key("F", TonalMode::Major)),
        (&["Eb", "Fm7", "Bb7", "Eb"][..], key("Eb", TonalMode::Major)),
        (
            &["F#", "G#m7", "C#7", "F#"][..],
            key("F#", TonalMode::Major),
        ),
        (&["Dm", "Gm7", "A7", "Dm"][..], key("D", TonalMode::Minor)),
        (&["Em", "Am7", "B7", "Em"][..], key("E", TonalMode::Minor)),
    ];

    for (symbols, expected) in cases {
        let paths = analyzer.analyze_keys_and_functions(
            &progression(symbols),
            KeyAnalysisOptions::default(),
            3,
        );
        assert_eq!(
            paths[0].global_key, expected,
            "unexpected global key for {symbols:?}"
        );
    }
}

#[test]
fn chromatic_function_is_local_while_closing_cadence_keeps_global_key() {
    let analyzer = Romanizer::new("C").unwrap();
    let paths = analyzer.analyze_keys_and_functions(
        &progression(&["C", "E7", "F", "Dm7", "G7", "C"]),
        KeyAnalysisOptions::default(),
        5,
    );

    assert_eq!(paths[0].global_key, key("C", TonalMode::Major));
    assert_eq!(paths[0].selections[1].local_key, key("A", TonalMode::Minor));
    assert_eq!(paths[0].selections[2].local_key, key("A", TonalMode::Minor));
}
