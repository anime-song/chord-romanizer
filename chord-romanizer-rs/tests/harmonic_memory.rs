use chord_romanizer::{
    GlobalKeyRequest, HarmonicResolutionKind, InterpretationFamily, KeyAnalysisOptions,
    ProgressionItem, Romanizer, SpelledNote, TonalKey, TonalMode, parse_chord,
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

fn fixed_c() -> KeyAnalysisOptions {
    KeyAnalysisOptions {
        global_key: GlobalKeyRequest::Fixed(key("C", TonalMode::Major)),
    }
}

#[test]
fn dominant_target_survives_one_decorative_chord() {
    let paths = Romanizer::new("C").unwrap().analyze_keys_and_functions(
        &progression(&["C", "D7", "Am7", "G", "C"]),
        fixed_c(),
        8,
    );
    let path = paths
        .iter()
        .find(|path| {
            path.harmonic_resolutions.iter().any(|resolution| {
                resolution.source_event_index == 1
                    && resolution.resolution_event_index == 3
                    && resolution.intervening_chords == 1
            })
        })
        .expect("D7's G target should survive across Am7");
    let resolution = path
        .harmonic_resolutions
        .iter()
        .find(|resolution| resolution.source_event_index == 1)
        .unwrap();

    assert_eq!(resolution.kind, HarmonicResolutionKind::TonicArrival);
    assert!(resolution.score > 0.0);
    assert!(path.memory_score > 0.0);
    assert_eq!(path.selections[1].pending_resolutions.len(), 1);
    assert_eq!(
        path.selections[2].pending_resolutions[0].intervening_chords,
        1
    );
    assert_eq!(path.selections[3].resolved_resolution_sources, [1]);
}

#[test]
fn bounded_stack_retains_outer_goal_while_nested_goal_resolves() {
    let paths = Romanizer::new("C").unwrap().analyze_keys_and_functions(
        &progression(&["C", "D7", "A7", "D7", "G7", "C"]),
        fixed_c(),
        12,
    );
    let path = paths
        .iter()
        .find(|path| {
            path.harmonic_resolutions.iter().any(|resolution| {
                resolution.source_event_index == 2
                    && resolution.resolution_event_index == 3
                    && resolution.depth == 2
            }) && path.harmonic_resolutions.iter().any(|resolution| {
                resolution.source_event_index == 1 && resolution.resolution_event_index == 4
            })
        })
        .expect("nested A7->D7 must not discard the older D7->G goal");

    assert_eq!(path.selections[2].pending_resolutions.len(), 2);
    assert_eq!(path.selections[2].pending_resolutions[1].depth, 2);
    assert_eq!(path.selections[3].resolved_resolution_sources, [2]);
    assert_eq!(path.selections[4].resolved_resolution_sources, [1]);
    assert!(path.harmonic_resolutions.iter().any(|resolution| {
        resolution.source_event_index == 1
            && resolution.kind == HarmonicResolutionKind::DominantChainLink
    }));
}

#[test]
fn explicit_boundary_clears_pending_goal_but_not_key_age() {
    let mut items = progression(&["C", "D7"]);
    items.push(ProgressionItem::boundary("new phrase"));
    items.extend(progression(&["Am7", "G", "C"]));

    let paths = Romanizer::new("C")
        .unwrap()
        .analyze_keys_and_functions(&items, fixed_c(), 5);
    let path = &paths[0];

    assert!(
        path.harmonic_resolutions
            .iter()
            .all(|resolution| resolution.source_event_index != 1)
    );
    let after_boundary = path
        .selections
        .iter()
        .find(|selection| selection.selection.event_index == 3)
        .unwrap();
    assert!(after_boundary.pending_resolutions.is_empty());
    assert_eq!(after_boundary.key_region_age_chords, 3);
    assert!(
        path.evidence
            .iter()
            .any(|evidence| { evidence.rule_id == "builtin.memory.boundary_clears_pending" })
    );
}

#[test]
fn transparent_no_chord_does_not_cancel_harmonic_memory() {
    let paths = Romanizer::new("C").unwrap().analyze_keys_and_functions(
        &progression(&["C", "D7", "N.C.", "G", "C"]),
        fixed_c(),
        5,
    );

    assert!(paths.iter().any(|path| {
        path.harmonic_resolutions.iter().any(|resolution| {
            resolution.source_event_index == 1
                && resolution.resolution_event_index == 3
                && resolution.intervening_chords == 0
        })
    }));
}

#[test]
fn immediate_cadence_is_recorded_without_double_scoring() {
    let path = &Romanizer::new("C").unwrap().analyze_keys_and_functions(
        &progression(&["Dm7", "G7", "C"]),
        fixed_c(),
        1,
    )[0];
    let resolution = path
        .harmonic_resolutions
        .iter()
        .find(|resolution| {
            resolution.source_event_index == 1 && resolution.resolution_event_index == 2
        })
        .expect("ordinary V-I is still exposed to the UI");

    assert_eq!(resolution.intervening_chords, 0);
    assert_eq!(resolution.score, 0.0);
    assert_eq!(path.memory_score, 0.0);
    assert!(path.cadential_spans.iter().any(|cadence| {
        cadence.predominant_event_index == 0
            && cadence.dominant_event_index == 1
            && cadence.resolution_event_index == 2
            && cadence.score == 0.0
    }));
}

#[test]
fn predominant_can_survive_a_decorative_chord_before_the_dominant() {
    let paths = Romanizer::new("C").unwrap().analyze_keys_and_functions(
        &progression(&["Dm7", "Em7", "G7", "C"]),
        fixed_c(),
        8,
    );
    let path = paths
        .iter()
        .find(|path| {
            path.cadential_spans.iter().any(|cadence| {
                cadence.predominant_event_index == 0
                    && cadence.dominant_event_index == 2
                    && cadence.resolution_event_index == 3
                    && cadence.intervening_before_dominant == 1
            })
        })
        .expect("Dm should remain a C-major preparation across Em");
    let cadence = path
        .cadential_spans
        .iter()
        .find(|cadence| cadence.predominant_event_index == 0)
        .unwrap();

    assert!(cadence.score > 0.0);
    assert_eq!(
        path.selections[0]
            .pending_predominant
            .as_ref()
            .unwrap()
            .source_event_index,
        0
    );
    assert_eq!(
        path.selections[1]
            .pending_predominant
            .as_ref()
            .unwrap()
            .intervening_chords,
        1
    );
    assert_eq!(path.selections[3].resolved_cadence_predominant_sources, [0]);
}

#[test]
fn selected_secondary_deceptive_candidate_discharges_the_goal() {
    let paths = Romanizer::new("C").unwrap().analyze_keys_and_functions(
        &progression(&["C", "E7", "FM7", "C"]),
        fixed_c(),
        12,
    );
    let path = paths
        .iter()
        .find(|path| {
            path.selections[1]
                .selection
                .harmonic_classifications
                .iter()
                .any(|classification| {
                    classification
                        .families
                        .contains(&InterpretationFamily::SecondaryDominantDeceptive)
                })
                && path.harmonic_resolutions.iter().any(|resolution| {
                    resolution.source_event_index == 1
                        && resolution.resolution_event_index == 2
                        && resolution.kind == HarmonicResolutionKind::DeceptiveArrival
                })
        })
        .expect("the explicit secondary-deceptive path should discharge E7's A-minor goal");

    assert!(path.selections[2].pending_resolutions.is_empty());
    assert!(path.evidence.iter().all(|evidence| {
        evidence.rule_id != "builtin.memory.expired_dominant_target"
            || !evidence.explanation.contains("event 1")
    }));
}

#[test]
fn boundary_prevents_a_predominant_from_claiming_the_next_phrase_cadence() {
    let mut items = progression(&["Dm7"]);
    items.push(ProgressionItem::boundary("new phrase"));
    items.extend(progression(&["Em7", "G7", "C"]));
    let paths = Romanizer::new("C")
        .unwrap()
        .analyze_keys_and_functions(&items, fixed_c(), 5);

    assert!(paths.iter().all(|path| {
        path.cadential_spans
            .iter()
            .all(|cadence| cadence.predominant_event_index != 0)
    }));
}
