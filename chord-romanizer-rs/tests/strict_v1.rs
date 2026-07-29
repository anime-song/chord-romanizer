use chord_romanizer::structure::{formula, spelled_tones_for};
use chord_romanizer::{
    AlternateKind, AnnotatedEvent, BehaviorProfile, BlackadderFunction, BlackadderObservationKind,
    BlackadderOrigin, BlackadderStructure, ChordDegree, ChordInterpreter, DominantRelation,
    HarmonicRole, HarmonicSource, HybridKind, InterpretationFamily, InterpretationKind,
    KeyBoundaryPolicy, ParsedChord, ParsedSymbol, ProgressionItem, ResolutionType, Romanizer,
    RomanizerOptions, SlashClassification, SpelledNote, TonalMode, TonalScope, parse_chord,
};

fn chord(symbol: &str) -> ParsedChord {
    match parse_chord(symbol).unwrap() {
        ParsedSymbol::Chord(chord) => chord,
        ParsedSymbol::NoChord { .. } | ParsedSymbol::Boundary { .. } => {
            panic!("expected chord: {symbol}")
        }
    }
}

fn item(symbol: &str) -> ProgressionItem {
    ProgressionItem::new(parse_chord(symbol).unwrap())
}

#[test]
fn display_api_formats_the_selected_functional_path() {
    let progression = [
        item("Bm7"),
        item("Eaug/A#"),
        item("AM7"),
        item("G#aug/D"),
        item("C#m7"),
        item("Am7"),
        item("Baug/F"),
        item("A/B"),
        item("E/G#"),
    ];
    let display = Romanizer::new("E")
        .unwrap()
        .display_progression(&progression);

    assert_eq!(
        display
            .iter()
            .map(|item| item.combined_label.as_str())
            .collect::<Vec<_>>(),
        [
            "Bm7 [ii7/IV|PD]",
            "Eaug/Bb [bV7(9,#11)|subV/IV]",
            "AM7 [IVM7|I/IV]",
            "G#aug/D [bVII7(9,#11)|subV/vi]",
            "C#m7 [vi7|i/vi]",
            "Am7 [iv7|SDm]",
            "Baug/F [bII7(9,#11)|subV/I]",
            "A/B [V9sus4|D]",
            "E/G# [I6|T]",
        ]
    );
    assert_eq!(display[1].function_label.as_deref(), Some("subV/IV"));
    assert_eq!(display[1].role_label.as_deref(), Some("D"));
    assert_eq!(display[2].local_label.as_deref(), Some("I/IV"));
}

#[test]
fn altered_tensions_create_only_the_written_pitch_classes() {
    let parsed = chord("C7(b9,#11)");
    let formula = formula(&parsed, BehaviorProfile::StrictV1).unwrap();
    let mut semitones: Vec<_> = formula
        .tones
        .iter()
        .map(|tone| tone.semitones.rem_euclid(12))
        .collect();
    semitones.sort_unstable();
    assert_eq!(semitones, [0, 1, 4, 6, 7, 10]);
    assert!(
        !formula
            .tones
            .iter()
            .any(|tone| { tone.degree == ChordDegree::Ninth && tone.alteration == 0 })
    );
}

#[test]
fn augmented_and_diminished_formulas_drive_inversion_and_spelling() {
    let interpreter = ChordInterpreter::new(BehaviorProfile::StrictV1);
    let augmented = chord("Caug/G#");
    let analysis = interpreter.analyze_slash_chord(&augmented, None);
    assert_eq!(
        analysis.slash_classification,
        SlashClassification::Inversion
    );

    let diminished = chord("Cdim/Gb");
    let tones = spelled_tones_for(
        &diminished,
        SpelledNote::parse("C").unwrap(),
        BehaviorProfile::StrictV1,
    );
    assert_eq!(
        tones[&SpelledNote::parse("Gb").unwrap().pitch_class()].to_string(),
        "Gb"
    );

    let diminished_seventh = chord("Cdim7");
    let mut intervals: Vec<_> = formula(&diminished_seventh, BehaviorProfile::StrictV1)
        .unwrap()
        .tones
        .iter()
        .map(|tone| tone.semitones.rem_euclid(12))
        .collect();
    intervals.sort_unstable();
    assert_eq!(intervals, [0, 3, 6, 9]);
}

#[test]
fn unknown_quality_does_not_fall_back_to_major_triad() {
    let unknown = chord("Cfoobar/E");
    assert!(formula(&unknown, BehaviorProfile::StrictV1).is_none());
    let analysis =
        ChordInterpreter::new(BehaviorProfile::StrictV1).analyze_slash_chord(&unknown, None);
    assert_eq!(
        analysis.slash_classification,
        SlashClassification::Indeterminate
    );
}

#[test]
fn midi_marker_colon_quality_preserves_inversion_spelling() {
    let result = Romanizer::new("G")
        .unwrap()
        .annotate_progression(&[item("B:7/D#")]);

    assert_eq!(
        result[0].slash_classification,
        SlashClassification::Inversion
    );
    assert_eq!(result[0].degree_bass.unwrap().to_string(), "#V");
    assert_eq!(result[0].symbol_fixed, "B:7/D#");
    assert_eq!(result[0].theoretical_symbol, "B:7/D#");
}

#[test]
fn unresolved_midi_marker_two_five_does_not_respell_borrowed_minor() {
    let result = Romanizer::new("E").unwrap().annotate_progression(&[
        item("C:m7(9)"),
        item("C:m7/F"),
        item("D:sus4"),
    ]);

    assert!(result[0].is_ii_v_start);
    assert!(!result[2].is_resolution_target);
    assert_eq!(result[0].degree_root.to_string(), "bVI");
    assert_eq!(result[0].symbol_fixed, "C:m7(9)");
}

#[test]
fn normalized_symbol_is_rendered_from_the_ast() {
    let romanizer = Romanizer::new("C").unwrap();
    let lowercase = romanizer.annotate_progression(&[item("c#/g#")]);
    assert_eq!(lowercase[0].normalized_symbol, "Db/Ab");

    let redundant = romanizer.annotate_progression(&[item("C/B#")]);
    assert_eq!(redundant[0].chord.original_symbol, "C/B#");
    assert_eq!(redundant[0].normalized_symbol, "C");
    assert_eq!(redundant[0].degree_bass, None);
}

#[test]
fn flat_root_and_slash_bass_are_spelled_as_a_pair() {
    let progression = [item("F#/G#")];
    let romanizer = Romanizer::new("G").unwrap();
    let result = romanizer.annotate_progression(&progression);

    assert_eq!(result[0].degree_root.to_string(), "bI");
    assert_eq!(result[0].degree_bass.unwrap().to_string(), "bII");
    assert_eq!(result[0].normalized_symbol, "Gb/Ab");
    assert_eq!(result[0].theoretical_symbol, "Gb/Ab");

    let display = romanizer.display_progression(&progression);
    assert_eq!(display[0].combined_label, "Gb/Ab [bII9sus4|S]");
}

#[test]
fn no_chord_is_aligned_and_transparent_but_boundary_is_not() {
    let romanizer = Romanizer::new("C").unwrap();
    let transparent = [item("Dm7"), item("N.C."), item("G7")];
    let events = romanizer.annotate_events(&transparent);
    assert_eq!(events.len(), 3);
    assert!(matches!(events[1], AnnotatedEvent::NoChord { .. }));
    assert!(romanizer.annotate_progression(&transparent)[0].is_ii_v_start);

    let split = [
        item("Dm7"),
        ProgressionItem::boundary("long silence"),
        item("G7"),
    ];
    assert!(!romanizer.annotate_progression(&split)[0].is_ii_v_start);
}

#[test]
fn key_boundary_policy_is_explicit() {
    let c = SpelledNote::parse("C").unwrap();
    let g = SpelledNote::parse("G").unwrap();
    let progression = [
        ProgressionItem::in_key(parse_chord("Dm7").unwrap(), c),
        ProgressionItem::in_key(parse_chord("G7").unwrap(), g),
    ];

    let strict = Romanizer::new("C").unwrap();
    assert!(!strict.annotate_progression(&progression)[0].is_ii_v_start);

    let mut options = RomanizerOptions::new("C").unwrap();
    options.key_boundary_policy = KeyBoundaryPolicy::Continue;
    let continuous = Romanizer::with_options(options).unwrap();
    assert!(continuous.annotate_progression(&progression)[0].is_ii_v_start);
}

#[test]
fn contextual_augmented_candidates_are_not_collapsed_early() {
    let interpreter = ChordInterpreter::new(BehaviorProfile::StrictV1);
    let candidates = interpreter.analyze_slash_candidates(&chord("Eaug/D"), None);
    assert!(candidates.len() >= 7);
    assert!(
        candidates
            .iter()
            .any(|candidate| candidate.analysis.kind == HybridKind::Blackadder)
    );
    assert!(
        candidates
            .iter()
            .any(|candidate| candidate.analysis.kind == HybridKind::HalfDiminishedNine)
    );

    let romanizer = Romanizer::new("C").unwrap();
    let result = romanizer.annotate_progression(&[item("Eaug/D")]);
    assert!(result[0].functional_interpretations.len() >= 7);
    assert!(
        romanizer.build_lattice(&[item("Eaug/D")]).layers[0]
            .candidates
            .len()
            > candidates.len()
    );
}

#[test]
fn blackadder_k_best_retains_function_in_each_path() {
    // C@ -> B is the characteristic semitone-down target of a tritone
    // substitute.  Other structurally valid readings remain in lower paths.
    let progression = [item("Daug/C"), item("B")];
    let romanizer = Romanizer::new("B").unwrap();
    // The newly ranked augmented-sixth resolution is a stronger contextual
    // candidate than the algebraic aug7 inversion, so inspect a slightly
    // wider structural list here. User-facing semantic top-k has a separate
    // test that keeps the augmented-sixth reading inside the first five.
    let paths = romanizer.analyze_top_k(&progression, 8);

    let top_blackadder = paths[0].selections[0].blackadder.as_ref().unwrap();
    assert_eq!(
        top_blackadder.function,
        Some(BlackadderFunction::TritoneSubstitute)
    );
    assert!(paths.iter().any(|path| {
        path.selections[0]
            .blackadder
            .as_ref()
            .is_some_and(|reading| {
                reading.structure == BlackadderStructure::AugmentedSeventhThirdInversion
            })
    }));
    assert!(paths[0].evidence.iter().any(|evidence| {
        evidence.rule_id == "builtin.blackadder.transition.tritone_substitute"
    }));
}

#[test]
fn blackadder_lookahead_uses_next_slash_chords_effective_root() {
    let progression = [
        item("G#:aug/D"),
        item("B/C#"),
        item("Bb/C"),
        item("E:aug/A#"),
        item("A/B"),
        item("E"),
    ];
    let result = Romanizer::new("E")
        .unwrap()
        .annotate_progression(&progression);

    assert_eq!(result[0].symbol_fixed, "G#:aug/D");
    assert_eq!(result[0].hybrid_kind, Some(HybridKind::Blackadder));
    assert_eq!(result[0].alter.as_deref(), Some("bVII7(9,#11)"));
    let first = result[0]
        .functional_interpretations
        .iter()
        .filter_map(|interpretation| interpretation.blackadder.as_ref())
        .find(|reading| reading.function == Some(BlackadderFunction::TritoneSubstitute))
        .unwrap();
    assert_eq!(
        first
            .classification
            .perspective
            .as_ref()
            .unwrap()
            .local_tonic,
        SpelledNote::parse("C#").unwrap()
    );

    assert_eq!(result[3].symbol_fixed, "E:aug/A#");
    assert_eq!(
        result[3].hybrid_kind,
        Some(HybridKind::SecondaryDominantThirdInBass)
    );
    assert_eq!(result[3].alter.as_deref(), Some("II7(9,#11)/#IV"));
    let second = result[3]
        .functional_interpretations
        .iter()
        .filter_map(|interpretation| interpretation.blackadder.as_ref())
        .find(|reading| reading.function == Some(BlackadderFunction::SecondaryDominant))
        .unwrap();
    assert_eq!(
        second
            .classification
            .perspective
            .as_ref()
            .unwrap()
            .local_tonic,
        SpelledNote::parse("B").unwrap()
    );
}

#[test]
fn blackadder_spelling_uses_the_canonical_bass() {
    let progression = [item("Bm7"), item("Eaug/A#"), item("AM7")];
    let result = Romanizer::new("E")
        .unwrap()
        .annotate_progression(&progression);

    assert!(result[0].is_ii_v_start);
    assert_eq!(result[1].symbol_fixed, "Eaug/Bb");
    assert_eq!(result[1].alter.as_deref(), Some("bV7(9,#11)"));
    assert_eq!(result[1].hybrid_kind, Some(HybridKind::Blackadder));
    assert!(
        result[1]
            .functional_interpretations
            .iter()
            .any(|candidate| {
                candidate.blackadder.as_ref().is_some_and(|reading| {
                    reading.function == Some(BlackadderFunction::TritoneSubstitute)
                        && reading.target_root == Some(SpelledNote::parse("A").unwrap())
                })
            })
    );
    assert!(result[2].is_resolution_target);
}

#[test]
fn blackadder_looks_through_one_prolonging_dominant() {
    let progression = [item("Am7"), item("Baug/F"), item("A/B"), item("E/G#")];
    let romanizer = Romanizer::new("E").unwrap();
    let result = romanizer.annotate_progression(&progression);

    assert_eq!(result[1].symbol_fixed, "Baug/F");
    assert_eq!(result[1].alter.as_deref(), Some("bII7(9,#11)"));
    assert_eq!(result[1].hybrid_kind, Some(HybridKind::Blackadder));
    assert_eq!(result[2].alter.as_deref(), Some("V9sus4"));
    assert_eq!(result[3].roman, "I/III");

    let paths = romanizer.analyze_top_k(&progression, 1);
    assert_eq!(
        paths[0].selections[1]
            .blackadder
            .as_ref()
            .and_then(|reading| reading.function),
        Some(BlackadderFunction::TritoneSubstitute)
    );
    assert!(
        paths[0]
            .evidence
            .iter()
            .any(|evidence| { evidence.rule_id == "builtin.progression.dominant_prolongation" })
    );

    // A genuine half-diminished predominant still resolves to the immediate
    // dominant; the bounded look-ahead must not skip it.
    let predominant =
        Romanizer::new("C")
            .unwrap()
            .annotate_progression(&[item("Daug/C"), item("G7"), item("C")]);
    assert_eq!(
        predominant[0].hybrid_kind,
        Some(HybridKind::HalfDiminishedNine)
    );
    assert_eq!(predominant[0].alter.as_deref(), Some("Im7-5(9)"));
}

#[test]
fn blackadder_rotation_is_not_a_separate_interpretation_axis() {
    let romanizer = Romanizer::new("G").unwrap();
    let progression = [item("G#aug/F#"), item("GM7/D")];
    let result = romanizer.annotate_progression(&progression);

    // Preserve the caller's spelling, but orient the public augmented shape at
    // the member a tritone above the bass: Caug/F#.
    assert_eq!(result[0].chord.original_symbol, "G#aug/F#");
    assert_eq!(result[0].normalized_symbol, "Caug/F#");
    for reading in result[0]
        .functional_interpretations
        .iter()
        .filter_map(|interpretation| interpretation.blackadder.as_ref())
    {
        assert_eq!(
            reading.written_upper_root,
            SpelledNote::parse("G#").unwrap()
        );
        assert_eq!(
            reading.canonical_upper_root,
            SpelledNote::parse("C").unwrap()
        );
        assert_eq!(reading.canonical_bass, SpelledNote::parse("F#").unwrap());
    }
}

#[test]
fn notation_only_alternates_do_not_consume_k_best_states() {
    // F# in C has both #IV and bV degree spellings, making the enharmonic
    // notation metadata observable in this test.
    let romanizer = Romanizer::new("C").unwrap();
    let progression = [item("F#aug/C"), item("CM7")];
    let annotations = romanizer.annotate_progression(&progression);

    // StrictV1 never suggests dropping an explicitly written bass.
    assert!(
        !annotations[0]
            .alternates
            .iter()
            .any(|alternate| alternate.kind == AlternateKind::WithoutBass)
    );
    let enharmonic: Vec<_> = annotations[0]
        .alternates
        .iter()
        .filter(|alternate| alternate.kind == AlternateKind::Enharmonic)
        .collect();
    assert!(!enharmonic.is_empty());
    assert!(
        enharmonic
            .iter()
            .all(|alternate| alternate.label.contains('/'))
    );

    // The historical label remains isolated behind the explicit compatibility
    // profile instead of leaking into new analysis output.
    let legacy = Romanizer::with_options(RomanizerOptions::python_019("C").unwrap()).unwrap();
    assert!(
        legacy.annotate_progression(&progression)[0]
            .alternates
            .iter()
            .any(|alternate| alternate.kind == AlternateKind::WithoutBass)
    );

    // It and enharmonic spellings are no longer harmonic/Viterbi states.
    let lattice = romanizer.build_lattice(&progression);
    assert!(lattice.layers.iter().all(|layer| {
        layer.candidates.iter().all(|candidate| {
            !matches!(
                candidate.kind,
                InterpretationKind::EnharmonicDegree | InterpretationKind::RootWithoutBass
            )
        })
    }));

    let semantic = romanizer.analyze_top_k_interpretations(&progression, 5);
    assert_eq!(semantic, lattice.decode_top_k_interpretations(5));
}

#[test]
fn backdoor_and_subdominant_minor_are_distinct_candidates() {
    // In C, Bb@ contains Ab (b6) and resolves by whole tone to I.  Therefore
    // backdoor-dominant and SDm readings are both justified, not aliases.
    let progression = [item("Abaug/Bb"), item("C")];
    let romanizer = Romanizer::new("C").unwrap();
    let lattice = romanizer.build_lattice(&progression);
    let functions: Vec<_> = lattice.layers[0]
        .candidates
        .iter()
        .filter_map(|candidate| {
            candidate
                .blackadder
                .as_ref()
                .and_then(|reading| reading.function)
        })
        .collect();
    assert!(functions.contains(&BlackadderFunction::BackdoorDominant));
    assert!(functions.contains(&BlackadderFunction::SubdominantMinor));

    let backdoor = lattice.layers[0]
        .candidates
        .iter()
        .filter_map(|candidate| candidate.blackadder.as_ref())
        .find(|reading| reading.function == Some(BlackadderFunction::BackdoorDominant))
        .unwrap();
    assert_eq!(backdoor.classification.role, Some(HarmonicRole::Dominant));
    assert_eq!(
        backdoor.classification.dominant_relation,
        Some(DominantRelation::Backdoor)
    );
    assert!(
        backdoor
            .classification
            .families
            .contains(&InterpretationFamily::Backdoor)
    );
    assert!(
        backdoor
            .classification
            .sources
            .contains(&HarmonicSource::SubdominantMinor)
    );
    assert!(
        backdoor
            .classification
            .families
            .contains(&InterpretationFamily::SubdominantMinor)
    );
}

#[test]
fn augmented_sixth_is_a_structure_family_and_predominant_role() {
    let result = Romanizer::new("C")
        .unwrap()
        .annotate_progression(&[item("Daug/C")]);
    let reading = result[0]
        .functional_interpretations
        .iter()
        .filter_map(|interpretation| interpretation.blackadder.as_ref())
        .find(|reading| reading.structure == BlackadderStructure::AugmentedSixth)
        .unwrap();
    assert_eq!(reading.classification.role, Some(HarmonicRole::Predominant));
    assert!(
        reading
            .classification
            .families
            .contains(&InterpretationFamily::AugmentedSixth)
    );
}

#[test]
fn applied_minor_two_five_one_keeps_global_and_local_keys() {
    // In global C, IIIø7-VI7-IIm7 is iiø-V-i when heard from temporary D
    // minor. The local view supplements rather than overwrites the global
    // Roman numerals.
    let progression = [item("Em7-5"), item("A7"), item("Dm7")];
    let romanizer = Romanizer::new("C").unwrap();
    let result = romanizer.annotate_progression(&progression);
    assert_eq!(
        result
            .iter()
            .map(|chord| chord.roman.as_str())
            .collect::<Vec<_>>(),
        ["IIIm7-5", "VI7", "IIm7"]
    );

    let predominant = result[0]
        .harmonic_classifications
        .iter()
        .find(|classification| classification.role == Some(HarmonicRole::Predominant))
        .unwrap();
    let perspective = predominant.perspective.as_ref().unwrap();
    assert_eq!(perspective.global_tonic, SpelledNote::parse("C").unwrap());
    assert_eq!(perspective.local_tonic, SpelledNote::parse("D").unwrap());
    assert_eq!(perspective.local_tonic_degree.to_string(), "II");
    assert_eq!(perspective.scope, TonalScope::Tonicization);
    assert_eq!(perspective.mode, TonalMode::Minor);
    assert!(
        predominant
            .families
            .contains(&InterpretationFamily::AppliedCadence)
    );

    let dominant = result[1]
        .harmonic_classifications
        .iter()
        .find(|classification| classification.role == Some(HarmonicRole::Dominant))
        .unwrap();
    assert_eq!(
        dominant.dominant_relation,
        Some(DominantRelation::FifthRelated)
    );
    assert_eq!(
        dominant.perspective.as_ref().unwrap().local_tonic,
        SpelledNote::parse("D").unwrap()
    );

    let tonic = result[2]
        .harmonic_classifications
        .iter()
        .find(|classification| classification.role == Some(HarmonicRole::Tonic))
        .unwrap();
    assert!(
        tonic
            .families
            .contains(&InterpretationFamily::AppliedCadence)
    );

    // The same common classification is copied into high-level path output.
    let path = romanizer.analyze_top_k_interpretations(&progression, 1);
    assert_eq!(
        path[0].selections[1].harmonic_classifications,
        result[1].harmonic_classifications
    );
}

#[test]
fn tritone_substitute_can_target_a_non_global_local_tonic() {
    // Eb7 is subV of A7 and resolves to Dm. Together with Em7b5 this is a
    // local iiø-subV-i in D minor while the caller's global key stays C.
    let result = Romanizer::new("C").unwrap().annotate_progression(&[
        item("Em7-5"),
        item("Eb7"),
        item("Dm7"),
    ]);
    let substitute = result[1]
        .harmonic_classifications
        .iter()
        .find(|classification| {
            classification.dominant_relation == Some(DominantRelation::TritoneSubstitute)
        })
        .unwrap();
    assert!(
        substitute
            .families
            .contains(&InterpretationFamily::TritoneSubstitute)
    );
    assert!(
        substitute
            .families
            .contains(&InterpretationFamily::AppliedCadence)
    );
    let perspective = substitute.perspective.as_ref().unwrap();
    assert_eq!(perspective.global_tonic, SpelledNote::parse("C").unwrap());
    assert_eq!(perspective.local_tonic, SpelledNote::parse("D").unwrap());
    assert_eq!(perspective.local_tonic_degree.to_string(), "II");
    assert_eq!(perspective.scope, TonalScope::Tonicization);
}

#[test]
fn ordinary_backdoor_dominant_uses_the_common_relation_axis() {
    let result = Romanizer::new("C")
        .unwrap()
        .annotate_progression(&[item("Bb7"), item("CM7")]);
    let backdoor = result[0]
        .harmonic_classifications
        .iter()
        .find(|classification| classification.dominant_relation == Some(DominantRelation::Backdoor))
        .unwrap();
    assert_eq!(backdoor.role, Some(HarmonicRole::Dominant));
    assert_eq!(
        backdoor.perspective.as_ref().unwrap().scope,
        TonalScope::Global
    );
    assert_eq!(result[1].resolution_type, Some(ResolutionType::Backdoor));
}

#[test]
fn text_only_origins_expose_future_observation_requirements() {
    let candidates = ChordInterpreter::new(BehaviorProfile::StrictV1)
        .analyze_slash_candidates(&chord("Daug/C"), None);
    let split = candidates
        .iter()
        .filter_map(|candidate| candidate.analysis.blackadder.as_ref())
        .find(|reading| reading.origin == Some(BlackadderOrigin::SplitVoiceLeading))
        .unwrap();
    assert!(
        split
            .unresolved_observations
            .contains(&BlackadderObservationKind::VoiceLeading)
    );

    let incidental = candidates
        .iter()
        .filter_map(|candidate| candidate.analysis.blackadder.as_ref())
        .find(|reading| reading.origin == Some(BlackadderOrigin::Incidental))
        .unwrap();
    assert!(
        incidental
            .unresolved_observations
            .contains(&BlackadderObservationKind::Timing)
    );
    assert!(
        incidental
            .unresolved_observations
            .contains(&BlackadderObservationKind::PartSeparation)
    );
}

#[test]
fn unresolved_fallback_families_reach_semantic_top_five() {
    // No following chord supports a dominant/predominant function, and the
    // spelling does not create an augmented-sixth candidate. The top five
    // should therefore expose all three observation-dependent explanations
    // instead of filling every slot with unsupported algebraic identities.
    let romanizer = Romanizer::new("C").unwrap();
    let paths = romanizer.analyze_top_k_interpretations(&[item("Bbaug/C")], 5);
    assert_eq!(paths.len(), 5);

    let readings: Vec<_> = paths
        .iter()
        .filter_map(|path| path.selections[0].blackadder.as_ref())
        .collect();
    assert!(
        readings
            .iter()
            .any(|reading| { reading.structure == BlackadderStructure::WholeToneSubset })
    );
    assert!(
        readings
            .iter()
            .any(|reading| { reading.origin == Some(BlackadderOrigin::SplitVoiceLeading) })
    );
    assert!(
        readings
            .iter()
            .any(|reading| { reading.origin == Some(BlackadderOrigin::Incidental) })
    );
}

#[test]
fn final_hybrid_choice_drives_progression_metadata() {
    let romanizer = Romanizer::new("C").unwrap();
    let results = romanizer.annotate_progression(&[item("Faug/B"), item("C")]);
    assert_eq!(
        results[0].hybrid_kind,
        Some(HybridKind::SecondaryDominantThirdInBass)
    );
    assert_eq!(results[1].resolution_type, Some(ResolutionType::Perfect));
}

#[test]
fn only_plain_h_is_accepted_as_a_legacy_b_alias() {
    let parsed = chord("H7");
    assert_eq!(parsed.root.to_string(), "B");
    assert!(parse_chord("Hb7").is_err());
    assert!(parse_chord("H#7").is_err());
}

#[test]
fn reproduce_issue_progression_is_preserved_as_an_ambiguous_analysis() {
    let progression: Vec<_> = ["Cm7-5", "C#m7-5/G", "C#aug/G", "F#m7"]
        .into_iter()
        .map(item)
        .collect();
    let romanizer = Romanizer::new("A").unwrap();
    let results = romanizer.annotate_progression(&progression);

    assert_eq!(results.len(), progression.len());
    for result in &results {
        assert!(parse_chord(&result.normalized_symbol).is_ok());
    }

    let paths = romanizer.build_lattice(&progression).decode_top_k(3);
    assert!(!paths.is_empty());
    assert!(paths.iter().all(|path| path.selections.len() == 4));
}

#[test]
fn ordinary_related_two_five_is_a_ranked_local_key_path() {
    // In global C, F#m7-B7-Em7 is heard as ii-V-i/III even though the
    // displayed degrees remain #IVm7-VII7-iiim7.  The major-mode ii quality
    // does not prevent a minor target from being represented in k-best.
    let progression = [item("F#m7"), item("B7"), item("Em7")];
    let romanizer = Romanizer::new("C").unwrap();
    let paths = romanizer.analyze_top_k_interpretations(&progression, 5);
    assert_eq!(paths.len(), 5);

    let best = &paths[0];
    let predominant = &best.selections[0].harmonic_classifications[0];
    assert_eq!(predominant.role, Some(HarmonicRole::Predominant));
    assert_eq!(predominant.local_degree.unwrap().to_string(), "II");
    assert!(
        predominant
            .families
            .contains(&InterpretationFamily::AppliedCadence)
    );
    let perspective = predominant.perspective.as_ref().unwrap();
    assert_eq!(perspective.local_tonic, SpelledNote::parse("E").unwrap());
    assert_eq!(perspective.local_tonic_degree.to_string(), "III");
    assert_eq!(perspective.mode, TonalMode::Minor);

    assert_eq!(
        best.selections[1].harmonic_classifications[0].role,
        Some(HarmonicRole::Dominant)
    );
    assert_eq!(
        best.selections[2].harmonic_classifications[0].role,
        Some(HarmonicRole::Tonic)
    );
}

#[test]
fn flat_two_major_seventh_keeps_neapolitan_and_modal_candidates() {
    let progression = [item("DbM7"), item("G7"), item("CM7")];
    let romanizer = Romanizer::new("C").unwrap();
    let lattice = romanizer.build_lattice(&progression);
    let first = &lattice.layers[0].candidates;

    assert!(first.iter().any(|candidate| {
        candidate
            .harmonic_classifications
            .iter()
            .any(|classification| {
                classification
                    .families
                    .contains(&InterpretationFamily::Neapolitan)
            })
    }));
    assert!(first.iter().any(|candidate| {
        candidate
            .harmonic_classifications
            .iter()
            .any(|classification| {
                classification
                    .families
                    .contains(&InterpretationFamily::ModalInterchange)
                    && classification.sources.contains(&HarmonicSource::Phrygian)
            })
    }));

    let best = romanizer.analyze_top_k_interpretations(&progression, 1);
    assert!(
        best[0].selections[0].harmonic_classifications[0]
            .families
            .contains(&InterpretationFamily::Neapolitan)
    );
}

#[test]
fn flat_mediant_major_seventh_has_distinct_top_k_meanings() {
    let progression = [item("CM7"), item("EbM7"), item("CM7")];
    let romanizer = Romanizer::new("C").unwrap();
    let lattice = romanizer.build_lattice(&progression);
    let middle = &lattice.layers[1].candidates;

    assert!(middle.iter().any(|candidate| {
        candidate
            .harmonic_classifications
            .iter()
            .any(|classification| {
                classification
                    .families
                    .contains(&InterpretationFamily::ModalInterchange)
                    && classification
                        .sources
                        .contains(&HarmonicSource::ParallelMinor)
            })
    }));
    assert!(middle.iter().any(|candidate| {
        candidate
            .harmonic_classifications
            .iter()
            .any(|classification| {
                classification
                    .families
                    .contains(&InterpretationFamily::ChromaticMediant)
            })
    }));

    let paths = romanizer.analyze_top_k_interpretations(&progression, 3);
    assert_eq!(paths.len(), 3);
    assert!(paths.iter().any(|path| {
        path.selections[1]
            .harmonic_classifications
            .iter()
            .any(|classification| {
                classification
                    .families
                    .contains(&InterpretationFamily::ChromaticMediant)
            })
    }));
}

#[test]
fn chromatic_minor_seventh_line_is_not_forced_into_a_borrowed_key() {
    let romanizer = Romanizer::new("C").unwrap();
    let result = romanizer.annotate_progression(&[item("F#m7"), item("Fm7"), item("Em7")]);

    assert!(
        result[0]
            .harmonic_interpretations
            .iter()
            .any(|interpretation| {
                interpretation
                    .classification
                    .families
                    .contains(&InterpretationFamily::ChromaticApproach)
            })
    );
    assert!(
        result[1]
            .harmonic_interpretations
            .iter()
            .any(|interpretation| {
                interpretation
                    .classification
                    .families
                    .contains(&InterpretationFamily::ChromaticApproach)
            })
    );
}

#[test]
fn applied_leading_tone_is_separate_from_root_position_dominant() {
    let romanizer = Romanizer::new("C").unwrap();
    let result = romanizer.annotate_progression(&[item("F#m7-5"), item("GM7")]);
    let leading_tone = result[0]
        .harmonic_interpretations
        .iter()
        .find(|interpretation| {
            interpretation.classification.dominant_relation == Some(DominantRelation::LeadingTone)
        })
        .unwrap();

    assert_eq!(
        leading_tone
            .classification
            .local_degree
            .unwrap()
            .to_string(),
        "VII"
    );
    assert!(
        leading_tone
            .classification
            .families
            .contains(&InterpretationFamily::AppliedLeadingTone)
    );
    assert_eq!(result[1].resolution_type, Some(ResolutionType::LeadingTone));
}

#[test]
fn fully_diminished_inversion_keeps_leading_tone_and_rootless_dominant_readings() {
    let romanizer = Romanizer::new("C").unwrap();
    let result = romanizer.annotate_progression(&[item("Ebdim7"), item("G")]);
    let interpretations = &result[0].harmonic_interpretations;

    // E-flat diminished contains the same pitch classes as F-sharp
    // diminished.  The written bIII root must therefore not prevent the
    // analyzer from hearing vii°7/V.
    let leading_tone = interpretations
        .iter()
        .find(|interpretation| {
            interpretation.classification.dominant_relation == Some(DominantRelation::LeadingTone)
        })
        .unwrap();
    assert_eq!(
        leading_tone
            .classification
            .local_degree
            .unwrap()
            .to_string(),
        "VII"
    );
    assert_eq!(
        leading_tone
            .classification
            .perspective
            .as_ref()
            .unwrap()
            .local_tonic
            .to_string(),
        "G"
    );

    // The same notes also admit the complementary D7(b9)-without-D reading.
    // These are distinct semantic states even though their sounding pitch set
    // and displayed Roman symbol are identical.
    let rootless = interpretations
        .iter()
        .find(|interpretation| {
            interpretation
                .classification
                .families
                .contains(&InterpretationFamily::RootlessDominantNinth)
        })
        .unwrap();
    assert_eq!(
        rootless.classification.dominant_relation,
        Some(DominantRelation::FifthRelated)
    );
    assert_eq!(
        rootless.classification.local_degree.unwrap().to_string(),
        "V"
    );
    assert_eq!(result[1].resolution_type, Some(ResolutionType::LeadingTone));
}

#[test]
fn diminished_before_two_minor_retains_three_competing_meanings_in_top_k() {
    let romanizer = Romanizer::new("C").unwrap();
    let progression = [item("C"), item("Cdim7"), item("Dm7")];
    let result = romanizer.annotate_progression(&progression);
    let middle = &result[1].harmonic_interpretations;

    for family in [
        InterpretationFamily::RootlessDominantNinth,
        InterpretationFamily::PassingDiminished,
        InterpretationFamily::TonicSubstitute,
    ] {
        assert!(
            middle
                .iter()
                .any(|interpretation| { interpretation.classification.families.contains(&family) }),
            "missing diminished interpretation family {family:?}"
        );
    }

    // Top-k is the user-facing ambiguity contract.  The candidates must not
    // merely exist deep in the lattice; all three musically different
    // readings should be reachable in a small result set.
    let paths = romanizer.analyze_top_k_interpretations(&progression, 5);
    for family in [
        InterpretationFamily::RootlessDominantNinth,
        InterpretationFamily::PassingDiminished,
        InterpretationFamily::TonicSubstitute,
    ] {
        assert!(
            paths.iter().any(|path| {
                path.selections[1]
                    .harmonic_classifications
                    .iter()
                    .any(|classification| classification.families.contains(&family))
            }),
            "top-5 omitted diminished interpretation family {family:?}"
        );
    }
}

#[test]
fn chromatic_and_common_tone_diminished_patterns_are_ranked_by_context() {
    let romanizer = Romanizer::new("C").unwrap();

    let passing =
        romanizer.analyze_top_k_interpretations(&[item("Em7"), item("Ebdim7"), item("Dm7")], 1);
    assert!(
        passing[0].selections[1].harmonic_classifications[0]
            .families
            .contains(&InterpretationFamily::PassingDiminished)
    );

    let neighbor =
        romanizer.analyze_top_k_interpretations(&[item("C"), item("Cdim7"), item("C")], 1);
    assert!(
        neighbor[0].selections[1].harmonic_classifications[0]
            .families
            .contains(&InterpretationFamily::CommonToneDiminished)
    );
    assert!(
        neighbor[0].selections[1].harmonic_classifications[0]
            .families
            .contains(&InterpretationFamily::AuxiliaryDiminished)
    );
}

#[test]
fn diminished_symmetry_rules_do_not_apply_to_incomplete_diminished_chords() {
    let romanizer = Romanizer::new("C").unwrap();
    for symbol in ["Ebdim", "Ebm7-5"] {
        let result = romanizer.annotate_progression(&[item(symbol), item("G")]);
        assert!(
            result[0]
                .harmonic_interpretations
                .iter()
                .all(|interpretation| {
                    !interpretation
                        .classification
                        .families
                        .contains(&InterpretationFamily::RootlessDominantNinth)
                }),
            "{symbol} was incorrectly treated as a symmetric dim7"
        );
    }
}

#[test]
fn diminished_chord_is_not_accepted_as_a_tonic_resolution_target() {
    let romanizer = Romanizer::new("C").unwrap();

    for progression in [[item("G7"), item("Cdim")], [item("Bdim7"), item("Cdim")]] {
        let result = romanizer.annotate_progression(&progression);
        assert!(!result[1].is_resolution_target);
        assert_eq!(result[1].resolution_type, None);
        assert!(result[1].harmonic_interpretations.iter().all(
            |interpretation| interpretation.rule_id != "builtin.ordinary.tonicized_target"
        ));
    }
}

#[test]
fn tritone_substitute_related_two_forms_a_coherent_top_ranked_path() {
    // Abm7 is the related ii of Db7 (subV7 of C), not ii of C itself. The
    // global label/local degree must therefore remain bVI while the whole
    // candidate path shares C as its tonal target.
    let progression = [item("Abm7"), item("Db7"), item("C")];
    let romanizer = Romanizer::new("C").unwrap();
    let result = romanizer.annotate_progression(&progression);
    let related_two = result[0]
        .harmonic_interpretations
        .iter()
        .find(|interpretation| {
            interpretation
                .classification
                .families
                .contains(&InterpretationFamily::TritoneSubstituteRelatedTwo)
        })
        .unwrap();

    assert_eq!(
        related_two.classification.role,
        Some(HarmonicRole::Predominant)
    );
    assert_eq!(
        related_two.classification.local_degree.unwrap().to_string(),
        "bVI"
    );
    assert_eq!(
        related_two
            .classification
            .perspective
            .as_ref()
            .unwrap()
            .local_tonic
            .to_string(),
        "C"
    );

    let best = romanizer.analyze_top_k_interpretations(&progression, 1);
    assert!(
        best[0].selections[0].harmonic_classifications[0]
            .families
            .contains(&InterpretationFamily::TritoneSubstituteRelatedTwo)
    );
    assert_eq!(
        best[0].selections[1].harmonic_classifications[0].dominant_relation,
        Some(DominantRelation::TritoneSubstitute)
    );
}

#[test]
fn flat_seven_keeps_subdominant_minor_separate_from_backdoor_dominant() {
    let progression = [item("Bb7"), item("C")];
    let romanizer = Romanizer::new("C").unwrap();
    let result = romanizer.annotate_progression(&progression);
    let interpretations = &result[0].harmonic_interpretations;

    assert!(interpretations.iter().any(|interpretation| {
        interpretation.classification.role == Some(HarmonicRole::Subdominant)
            && interpretation
                .classification
                .sources
                .contains(&HarmonicSource::SubdominantMinor)
            && interpretation
                .classification
                .families
                .contains(&InterpretationFamily::SubdominantMinor)
            && interpretation.classification.dominant_relation.is_none()
    }));
    assert!(interpretations.iter().any(|interpretation| {
        interpretation.classification.dominant_relation == Some(DominantRelation::Backdoor)
    }));

    let paths = romanizer.analyze_top_k_interpretations(&progression, 4);
    assert!(paths.iter().any(|path| {
        path.selections[0]
            .harmonic_classifications
            .iter()
            .any(|classification| {
                classification
                    .sources
                    .contains(&HarmonicSource::SubdominantMinor)
                    && classification.dominant_relation.is_none()
            })
    }));
}

#[test]
fn flat_six_to_five_and_flat_two_to_one_prefer_specific_meanings() {
    let romanizer = Romanizer::new("C").unwrap();

    let flat_six = romanizer.analyze_top_k_interpretations(&[item("Ab"), item("G")], 1);
    assert!(
        flat_six[0].selections[0].harmonic_classifications[0]
            .families
            .contains(&InterpretationFamily::SubdominantMinor)
    );
    assert!(
        flat_six[0]
            .evidence
            .iter()
            .any(|evidence| { evidence.rule_id == "builtin.progression.flat_six_to_dominant" })
    );

    let flat_two = romanizer.analyze_top_k_interpretations(&[item("Db"), item("C")], 1);
    assert!(
        flat_two[0].selections[0].harmonic_classifications[0]
            .families
            .contains(&InterpretationFamily::Neapolitan)
    );
    assert!(flat_two[0].evidence.iter().any(|evidence| {
        evidence.rule_id == "builtin.progression.flat_two_neapolitan_to_tonic"
    }));
}

#[test]
fn secondary_dominant_deceptive_resolution_keeps_its_implied_local_key() {
    // In global C, E7 normally targets A. F major is bVI in that temporary
    // A-minor frame, so III7-IV is a secondary V7-bVI deceptive resolution.
    let progression = [item("E7"), item("FM7")];
    let romanizer = Romanizer::new("C").unwrap();
    let annotated = romanizer.annotate_progression(&progression);

    assert!(annotated[1].is_resolution_target);
    assert_eq!(
        annotated[1].resolution_type,
        Some(ResolutionType::Deceptive)
    );

    let best = romanizer.analyze_top_k_interpretations(&progression, 1);
    for selection in &best[0].selections {
        let classification = &selection.harmonic_classifications[0];
        assert!(
            classification
                .families
                .contains(&InterpretationFamily::SecondaryDominantDeceptive)
        );
        assert_eq!(
            classification
                .perspective
                .as_ref()
                .unwrap()
                .local_tonic
                .to_string(),
            "A"
        );
    }
}

#[test]
fn alternate_key_pair_is_a_real_k_best_tonal_state() {
    // The global labels remain bVIIM7-VIm7 in C, while the selected semantic
    // path may hear the pair as IVM7-IIIm7 in temporary F.
    let progression = [item("BbM7"), item("Am7")];
    let best = Romanizer::new("C")
        .unwrap()
        .analyze_top_k_interpretations(&progression, 1);

    for selection in &best[0].selections {
        let classification = &selection.harmonic_classifications[0];
        assert!(
            classification
                .families
                .contains(&InterpretationFamily::AlternateKeySequence)
        );
        let perspective = classification.perspective.as_ref().unwrap();
        assert_eq!(perspective.local_tonic.to_string(), "F");
        assert_eq!(perspective.scope, TonalScope::Tonicization);
    }
    assert!(
        best[0].evidence.iter().any(|evidence| {
            evidence.rule_id == "builtin.progression.continue_local_tonal_state"
        })
    );
}

#[test]
fn k_best_can_preserve_several_temporary_key_spans() {
    // Abstract regression for a progression that successively supports
    // global ii-iii, temporary-Eb ii-iii and iv-V, then temporary-A V-bVI.
    // The exact local-state path need not be top-1 because global modal
    // readings remain valid, but it must be available in the compact top-3.
    let progression = [
        item("Dm7"),
        item("Em7"),
        item("Fm7"),
        item("Gm7"),
        item("Abm7"),
        item("Bb7"),
        item("E7"),
        item("FM7"),
    ];
    let paths = Romanizer::new("C")
        .unwrap()
        .analyze_top_k_interpretations(&progression, 3);

    assert!(paths.iter().any(|path| {
        let local_tonic = |event_index: usize| {
            path.selections[event_index].harmonic_classifications[0]
                .perspective
                .as_ref()
                .map(|perspective| perspective.local_tonic.to_string())
        };
        local_tonic(2).as_deref() == Some("Eb")
            && local_tonic(3).as_deref() == Some("Eb")
            && local_tonic(4).as_deref() == Some("Eb")
            && local_tonic(5).as_deref() == Some("Eb")
            && local_tonic(6).as_deref() == Some("A")
            && local_tonic(7).as_deref() == Some("A")
    }));
}

#[test]
fn suspended_dominant_is_ranked_as_a_semantic_candidate() {
    let progression = [item("Dm7/G"), item("G7"), item("C")];
    let best = Romanizer::new("C")
        .unwrap()
        .analyze_top_k_interpretations(&progression, 1);

    assert_eq!(
        best[0].selections[0].hybrid_kind,
        Some(HybridKind::SusFourNine)
    );
    assert!(
        best[0].selections[0].harmonic_classifications[0]
            .families
            .contains(&InterpretationFamily::SuspendedDominant)
    );
    assert!(
        best[0]
            .evidence
            .iter()
            .any(|evidence| { evidence.rule_id == "builtin.progression.suspension_to_dominant" })
    );
}

#[test]
fn augmented_sixth_candidate_survives_semantic_top_five() {
    let progression = [item("Daug/C"), item("B")];
    let paths = Romanizer::new("C")
        .unwrap()
        .analyze_top_k_interpretations(&progression, 5);

    assert!(paths.iter().any(|path| {
        path.selections[0]
            .blackadder
            .as_ref()
            .is_some_and(|reading| {
                reading.structure == BlackadderStructure::AugmentedSixth
                    && reading
                        .classification
                        .families
                        .contains(&InterpretationFamily::AugmentedSixth)
            })
    }));
}

#[test]
fn retained_augmented_upper_structure_gates_bass_only_function() {
    let progression = [item("C"), item("Caug"), item("Caug/F#"), item("FM7")];
    let paths = Romanizer::new("C")
        .unwrap()
        .analyze_top_k_interpretations(&progression, 5);

    assert!(
        paths[0].selections[2]
            .blackadder
            .as_ref()
            .is_some_and(|reading| {
                reading.origin == Some(BlackadderOrigin::SplitVoiceLeading)
                    && reading.function.is_none()
            })
    );
    assert!(paths.iter().any(|path| {
        path.selections[2]
            .harmonic_classifications
            .iter()
            .any(|classification| {
                classification
                    .families
                    .contains(&InterpretationFamily::VoiceLeadingRequired)
            })
    }));
}
