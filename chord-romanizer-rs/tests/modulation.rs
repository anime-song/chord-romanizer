use chord_romanizer::{
    GlobalKeyRequest, KeyAnalysisOptions, ModulationMechanism, PivotKind, ProgressionItem,
    Romanizer, SpelledNote, TonalKey, TonalMode, TonalScope, parse_chord,
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

fn fixed(tonic: &str, mode: TonalMode) -> KeyAnalysisOptions {
    KeyAnalysisOptions {
        global_key: GlobalKeyRequest::Fixed(key(tonic, mode)),
    }
}

#[test]
fn common_chord_modulation_exposes_pivot_and_active_key() {
    let paths = Romanizer::new("C").unwrap().analyze_keys_and_functions(
        &progression(&["C", "Am7", "D7", "G", "C", "D7", "G"]),
        fixed("C", TonalMode::Major),
        8,
    );
    let modulated = paths
        .iter()
        .find(|path| {
            path.modulations.iter().any(|span| {
                span.to_key == key("G", TonalMode::Major)
                    && span.mechanism == ModulationMechanism::DiatonicPivot
            })
        })
        .expect("the confirmed G-major pivot reading should be in top-k");
    let span = &modulated.modulations[0];
    let pivot = span.pivot.as_ref().expect("common-chord pivot");
    assert_eq!(pivot.chord_symbol, "Am7");
    assert_eq!(pivot.kind, PivotKind::DiatonicCommonChord);
    assert_eq!(pivot.old_degree.to_string(), "VI");
    assert_eq!(pivot.new_degree.to_string(), "II");
    assert_eq!(
        span.duration_chords,
        modulated
            .selections
            .iter()
            .filter(|selection| {
                selection.selection.event_index >= span.start_event_index
                    && selection.selection.event_index <= span.end_event_index
            })
            .count()
    );
    assert!(
        span.evidence
            .iter()
            .any(|evidence| { evidence.rule_id == "builtin.modulation.key_region_duration" })
    );
    assert!(modulated.selections.iter().any(|selection| {
        selection.is_pivot
            && selection.active_key == key("G", TonalMode::Major)
            && selection.scope == TonalScope::Modulation
    }));
}

#[test]
fn old_dominant_to_new_dominant_is_a_bridge_not_a_pivot() {
    let paths = Romanizer::new("C").unwrap().analyze_keys_and_functions(
        &progression(&["C", "F", "G7", "D7", "G"]),
        fixed("C", TonalMode::Major),
        8,
    );
    let span = paths
        .iter()
        .flat_map(|path| path.modulations.iter())
        .find(|span| span.to_key == key("G", TonalMode::Major))
        .expect("G-major modulation candidate");
    assert_eq!(span.mechanism, ModulationMechanism::DominantBridge);
    assert!(span.pivot.is_none());
}

#[test]
fn fifth_related_dominants_are_classified_as_a_dominant_sequence() {
    let paths = Romanizer::new("C").unwrap().analyze_keys_and_functions(
        &progression(&["C", "F", "A7", "D7", "G"]),
        fixed("C", TonalMode::Major),
        8,
    );

    assert!(paths.iter().flat_map(|path| &path.modulations).any(|span| {
        span.to_key == key("G", TonalMode::Major)
            && span.mechanism == ModulationMechanism::DominantSequence
    }));
}

#[test]
fn borrowed_old_key_chord_can_become_a_chromatic_pivot() {
    let paths = Romanizer::new("C").unwrap().analyze_keys_and_functions(
        &progression(&["C", "F", "Fm7", "Bb7", "Eb"]),
        fixed("C", TonalMode::Major),
        12,
    );

    assert!(paths.iter().flat_map(|path| &path.modulations).any(|span| {
        span.mechanism == ModulationMechanism::ChromaticPivot
            && span
                .pivot
                .as_ref()
                .is_some_and(|pivot| pivot.kind == PivotKind::BorrowedCommonChord)
    }));
}

#[test]
fn one_applied_cadence_does_not_force_modulation_to_rank_first() {
    let paths = Romanizer::new("C").unwrap().analyze_keys_and_functions(
        &progression(&["C", "E7", "Am", "G7", "C"]),
        fixed("C", TonalMode::Major),
        8,
    );
    assert!(paths[0].modulations.is_empty());
    assert!(paths.iter().any(|path| {
        path.modulations
            .iter()
            .any(|span| span.to_key == key("A", TonalMode::Minor))
    }));
}

#[test]
fn segmental_key_path_can_modulate_from_c_to_g_and_then_from_g_to_d() {
    let paths = Romanizer::new("C").unwrap().analyze_keys_and_functions(
        &progression(&["C", "Am7", "D7", "G", "Em7", "A7", "D", "G", "A7", "D"]),
        fixed("C", TonalMode::Major),
        12,
    );

    let chained = paths
        .iter()
        .find(|path| {
            path.modulations.len() >= 2
                && path.modulations[0].from_key == key("C", TonalMode::Major)
                && path.modulations[0].to_key == key("G", TonalMode::Major)
                && path.modulations[1].from_key == key("G", TonalMode::Major)
                && path.modulations[1].to_key == key("D", TonalMode::Major)
        })
        .expect("C -> G -> D key-state path should survive top-k");

    assert_eq!(chained.modulations[0].end_event_index, 3);
    assert_eq!(chained.modulations[1].start_event_index, 4);
    assert_eq!(chained.selections[3].active_key, key("G", TonalMode::Major));
    assert_eq!(chained.selections[6].active_key, key("D", TonalMode::Major));
    let span_score = chained
        .modulations
        .iter()
        .map(|span| span.score)
        .sum::<f64>();
    assert!((span_score - chained.modulation_score).abs() < 1.0e-9);
    let key_evidence_score = chained
        .evidence
        .iter()
        .filter(|evidence| evidence.rule_id.starts_with("builtin.key."))
        .map(|evidence| evidence.contribution)
        .sum::<f64>();
    assert!((key_evidence_score - chained.key_score).abs() < 1.0e-9);
}

#[test]
fn segmental_key_path_can_return_to_the_global_key() {
    let paths = Romanizer::new("C").unwrap().analyze_keys_and_functions(
        &progression(&["C", "Am7", "D7", "G", "C", "G7", "C", "F", "G7", "C"]),
        fixed("C", TonalMode::Major),
        12,
    );

    let returned = paths
        .iter()
        .find(|path| {
            path.modulations.len() >= 2
                && path.modulations[0].to_key == key("G", TonalMode::Major)
                && path.modulations[1].from_key == key("G", TonalMode::Major)
                && path.modulations[1].to_key == key("C", TonalMode::Major)
        })
        .expect("C -> G -> C return path should survive top-k");

    let return_pivot = returned.modulations[1]
        .pivot
        .as_ref()
        .expect("C is IV in G and I in C");
    assert_eq!(return_pivot.event_index, 4);
    assert_eq!(
        returned.selections[4].active_key,
        key("C", TonalMode::Major)
    );
    assert_eq!(returned.selections[4].scope, TonalScope::Global);
    assert!(returned.selections[4].is_pivot);
}

#[test]
fn later_transition_can_use_borrowing_relative_to_the_selected_active_key() {
    let paths = Romanizer::new("C").unwrap().analyze_keys_and_functions(
        &progression(&["C", "Am7", "D7", "G", "Cm7", "F7", "Bb", "Eb", "F7", "Bb"]),
        fixed("C", TonalMode::Major),
        20,
    );

    let chained = paths
        .iter()
        .find(|path| {
            path.modulations.len() >= 2
                && path.modulations[0].to_key == key("G", TonalMode::Major)
                && path.modulations[1].from_key == key("G", TonalMode::Major)
                && path.modulations[1].to_key == key("Bb", TonalMode::Major)
        })
        .expect("C -> G -> Bb path should survive top-k");
    let pivot = chained.modulations[1]
        .pivot
        .as_ref()
        .expect("Cm7 is borrowed iv in G and diatonic ii in Bb");
    assert_eq!(pivot.chord_symbol, "Cm7");
    assert_eq!(pivot.kind, PivotKind::BorrowedCommonChord);
}

#[test]
fn boundary_breaks_pivot_search_but_preserves_the_selected_active_key() {
    let mut items = progression(&["C", "Am7", "D7", "G"]);
    items.push(ProgressionItem::boundary("new phrase"));
    items.extend(progression(&["Em7", "A7", "D", "G", "A7", "D"]));

    let paths = Romanizer::new("C").unwrap().analyze_keys_and_functions(
        &items,
        fixed("C", TonalMode::Major),
        20,
    );
    let chained = paths
        .iter()
        .find(|path| {
            path.modulations.len() >= 2
                && path.modulations[0].to_key == key("G", TonalMode::Major)
                && path.modulations[1].from_key == key("G", TonalMode::Major)
                && path.modulations[1].to_key == key("D", TonalMode::Major)
        })
        .expect("active G should survive the boundary and become from_key for D");

    assert_eq!(chained.modulations[0].end_event_index, 3);
    assert_eq!(chained.modulations[1].start_event_index, 5);
    assert_eq!(
        chained
            .selections
            .iter()
            .find(|selection| selection.selection.event_index == 5)
            .unwrap()
            .active_key,
        key("D", TonalMode::Major)
    );
}

#[test]
fn inferred_global_key_top_k_can_retain_a_modulating_home_key_path() {
    let paths = Romanizer::new("C").unwrap().analyze_keys_and_functions(
        &progression(&["C", "Am7", "D7", "G", "Em7", "A7", "D", "G", "A7", "D"]),
        KeyAnalysisOptions::default(),
        20,
    );
    assert!(paths.iter().any(|path| {
        path.global_key == key("C", TonalMode::Major)
            && path.modulations.len() >= 2
            && path.modulations[0].to_key == key("G", TonalMode::Major)
            && path.modulations[1].to_key == key("D", TonalMode::Major)
    }));
}
