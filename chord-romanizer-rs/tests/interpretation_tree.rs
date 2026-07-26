use chord_romanizer::{
    CandidateConstraint, GlobalKeyRequest, InterpretationTreeNode, InterpretationTreeOptions,
    KeyAnalysisOptions, ProgressionItem, Romanizer, SpelledNote, TonalKey, TonalMode,
    TreeCondition, parse_chord,
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
fn folds_shared_prefixes_and_exposes_ui_metadata() {
    let analyzer = Romanizer::new("C").unwrap();
    let global_key = key("B", TonalMode::Major);
    let tree = analyzer.analyze_interpretation_tree(
        &progression(&["Daug/C", "B"]),
        InterpretationTreeOptions {
            key_analysis: KeyAnalysisOptions {
                global_key: GlobalKeyRequest::Fixed(global_key),
            },
            condition: None,
        },
        5,
    );

    assert_eq!(tree.requested_k, 5);
    assert!(tree.returned_path_count >= 2);
    assert_eq!(tree.roots.len(), 1);
    let root = &tree.roots[0];
    assert_eq!(root.global_key, global_key);
    assert_eq!(root.top_k_support_count, tree.returned_path_count);
    assert!(root.is_top_k_consensus);
    assert!(root.children.len() >= 2);
    assert_eq!(
        tree.consensus_node_ids.as_slice(),
        std::slice::from_ref(&root.node_id)
    );

    let first = &root.children[0];
    assert_eq!(first.event_index, 0);
    assert_eq!(first.chord_index, 0);
    assert_eq!(first.input_symbol, "Daug/C");
    assert_eq!(first.condition.prefix.len(), 1);
    assert_eq!(
        first.condition.prefix[0].candidate_id,
        first.selection.selection.candidate_id
    );
    assert_eq!(
        first.selection.selection.step_score,
        first.selection.selection.emission_score + first.selection.selection.transition_score
    );
    assert!(!first.selection.selection.evidence.is_empty());
}

#[test]
fn node_condition_recomputes_descendants_from_the_full_lattice() {
    let analyzer = Romanizer::new("C").unwrap();
    let items = progression(&["Daug/C", "B"]);
    let initial = analyzer.analyze_interpretation_tree(
        &items,
        InterpretationTreeOptions {
            key_analysis: KeyAnalysisOptions {
                global_key: GlobalKeyRequest::Fixed(key("B", TonalMode::Major)),
            },
            condition: None,
        },
        5,
    );
    let selected_condition = initial.roots[0].children[1].condition.clone();

    let conditioned = analyzer.analyze_interpretation_tree(
        &items,
        InterpretationTreeOptions {
            key_analysis: KeyAnalysisOptions::default(),
            condition: Some(selected_condition.clone()),
        },
        5,
    );

    assert!(conditioned.condition_applied);
    assert!(conditioned.condition_satisfied);
    assert_eq!(conditioned.condition, Some(selected_condition));
    assert_eq!(conditioned.roots.len(), 1);
    assert_eq!(conditioned.roots[0].children.len(), 1);
    assert_eq!(
        conditioned.roots[0].children[0]
            .selection
            .selection
            .candidate_id,
        initial.roots[0].children[1]
            .selection
            .selection
            .candidate_id
    );
    assert!(conditioned.roots[0].children[0].is_top_k_consensus);

    let changed_input = analyzer.analyze_interpretation_tree(
        &progression(&["Daug/C", "C"]),
        InterpretationTreeOptions {
            key_analysis: KeyAnalysisOptions::default(),
            condition: conditioned.condition,
        },
        5,
    );
    assert!(changed_input.condition_applied);
    assert!(!changed_input.condition_satisfied);
}

#[test]
fn stale_condition_is_reported_without_panicking() {
    let analyzer = Romanizer::new("C").unwrap();
    let items = progression(&["C", "G7", "C"]);
    let initial =
        analyzer.analyze_interpretation_tree(&items, InterpretationTreeOptions::default(), 1);
    let condition = TreeCondition {
        rule_set_version: initial.rule_set_version,
        progression_fingerprint: initial.progression_fingerprint,
        global_key: key("C", TonalMode::Major),
        prefix: vec![CandidateConstraint {
            event_index: 0,
            candidate_id: "event-0:candidate-does-not-exist".to_owned(),
        }],
    };
    let tree = analyzer.analyze_interpretation_tree(
        &items,
        InterpretationTreeOptions {
            key_analysis: KeyAnalysisOptions::default(),
            condition: Some(condition),
        },
        5,
    );

    assert!(tree.condition_applied);
    assert!(!tree.condition_satisfied);
    assert_eq!(tree.returned_path_count, 0);
    assert!(tree.roots.is_empty());
}

#[test]
fn original_event_indexes_survive_transparent_no_chord() {
    let analyzer = Romanizer::new("C").unwrap();
    let tree = analyzer.analyze_interpretation_tree(
        &progression(&["Dm7", "N.C.", "G7", "C"]),
        InterpretationTreeOptions {
            key_analysis: KeyAnalysisOptions {
                global_key: GlobalKeyRequest::Fixed(key("C", TonalMode::Major)),
            },
            condition: None,
        },
        1,
    );
    let first = &tree.roots[0].children[0];
    let second = &first.children[0];
    assert_eq!((first.event_index, first.chord_index), (0, 0));
    assert_eq!((second.event_index, second.chord_index), (2, 1));
}

fn find_node<'a>(
    nodes: &'a [InterpretationTreeNode],
    predicate: &impl Fn(&InterpretationTreeNode) -> bool,
) -> Option<&'a InterpretationTreeNode> {
    for node in nodes {
        if predicate(node) {
            return Some(node);
        }
        if let Some(found) = find_node(&node.children, predicate) {
            return Some(found);
        }
    }
    None
}

#[test]
fn multi_stage_key_branch_remains_conditionable() {
    let analyzer = Romanizer::new("C").unwrap();
    let items = progression(&["C", "Am7", "D7", "G", "Em7", "A7", "D", "G", "A7", "D"]);
    let initial = analyzer.analyze_interpretation_tree(
        &items,
        InterpretationTreeOptions {
            key_analysis: KeyAnalysisOptions {
                global_key: GlobalKeyRequest::Fixed(key("C", TonalMode::Major)),
            },
            condition: None,
        },
        20,
    );
    let second_pivot = find_node(&initial.roots[0].children, &|node| {
        node.selection.is_pivot
            && node.selection.active_key == key("D", TonalMode::Major)
            && node.selection.selection.candidate_id.contains("@mod:")
    })
    .expect("the C -> G -> D branch should expose its second pivot");

    let conditioned = analyzer.analyze_interpretation_tree(
        &items,
        InterpretationTreeOptions {
            key_analysis: KeyAnalysisOptions::default(),
            condition: Some(second_pivot.condition.clone()),
        },
        5,
    );
    assert!(conditioned.condition_satisfied);
    assert!(
        find_node(&conditioned.roots[0].children, &|node| {
            node.selection.selection.candidate_id == second_pivot.selection.selection.candidate_id
        })
        .is_some()
    );
}
