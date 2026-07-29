from chord_romanizer import (
    Boundary,
    CandidateConstraint,
    Romanizer,
    TonalKey,
    TreeCondition,
)
from chord_romanizer.chord_parser import ChordParser


def test_python_api_reports_abi3_native_backend():
    backend = Romanizer().native_backend
    assert backend["abi"] == "abi3-py38"


def test_display_progression_returns_ready_and_structured_labels():
    display = Romanizer.strict("E").display_progression(
        [
            "Bm7",
            "Eaug/A#",
            "AM7",
            "G#aug/D",
            "C#m7",
            "Am7",
            "Baug/F",
            "A/B",
            "E/G#",
        ]
    )

    assert [item.combined_label for item in display] == [
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
    assert display[1].symbol == "Eaug/Bb"
    assert display[1].theoretical_symbol == "Eaug/Bb"
    assert display[1].function_label == "subV/IV"


def test_display_progression_spells_flat_root_and_bass_consistently():
    item = Romanizer.strict("G").display_progression(["F#/G#"])[0]

    assert item.symbol == "Gb/Ab"
    assert item.theoretical_symbol == "Gb/Ab"
    assert item.global_label == "bII9sus4"
    assert item.combined_label == "Gb/Ab [bII9sus4|S]"


def test_default_profile_preserves_python_019_surface():
    romanizer = Romanizer("C")
    results = romanizer.romanize_progression(["Dm7", "G7", "Cmaj7"])
    assert [result.roman for result in results] == ["IIm7", "V7", "IM7"]
    assert results[0].is_ii_v_start is True
    assert results[2].resolution_type == "perfect"


def test_strict_k_best_exposes_blackadder_function():
    paths = Romanizer.strict("B").analyze_top_k(["Daug/C", "B"], k=6)
    assert paths
    first = paths[0].selections[0]
    assert first.blackadder is not None
    assert first.blackadder.function == "tritone_substitute"
    assert any(
        evidence.rule_id == "builtin.blackadder.transition.tritone_substitute"
        for evidence in paths[0].evidence
    )


def test_semantic_top_k_keeps_spelling_metadata_out_of_result_slots():
    romanizer = Romanizer.strict("G")
    paths = romanizer.analyze_top_k_interpretations(
        ["G#aug/F#", "GM7/D"], k=5
    )

    assert paths
    reading = paths[0].selections[0].blackadder
    assert reading is not None
    assert reading.written_upper_root == "G#"
    assert reading.canonical_upper_root == "C"
    assert reading.canonical_bass == "F#"
    assert reading.canonical_shape == "Caug/F#"

    annotated = romanizer.annotate_progression(["G#aug/F#", "GM7/D"])
    assert annotated[0].normalized_symbol == "Caug/F#"
    assert not any(
        alternate["kind"] == "without_bass"
        for alternate in annotated[0].alternates
    )
    ambiguous = Romanizer.strict("C").annotate_progression(["F#aug/C", "CM7"])
    enharmonic = [
        alternate
        for alternate in ambiguous[0].alternates
        if alternate["kind"] == "enharmonic"
    ]
    assert enharmonic
    assert all("/" in alternate["label"] for alternate in enharmonic)

    legacy = Romanizer("G").annotate_progression(["G#aug/F#", "GM7/D"])
    assert any(
        alternate["kind"] == "without_bass"
        for alternate in legacy[0].alternates
    )


def test_aligned_api_keeps_no_chord_and_explicit_boundary():
    no_chord = ChordParser.parse("N.C.")
    events = Romanizer.strict("C").annotate_events(
        ["Dm7", no_chord, Boundary("long silence"), "G7"]
    )
    assert len(events) == 4
    assert events[1]["kind"] == "no_chord"
    assert events[2]["kind"] == "boundary"

    display = Romanizer.strict("C").display_progression(
        ["Dm7", no_chord, Boundary("long silence"), "G7"]
    )
    assert [item.event_index for item in display] == [0, 3]


def test_applied_minor_cadence_exposes_local_tonal_perspective():
    results = Romanizer.strict("C").annotate_progression(
        ["Em7-5", "A7", "Dm7"]
    )

    predominant = next(
        item
        for item in results[0].harmonic_classifications
        if item.role == "predominant"
    )
    assert predominant.families == ["applied_cadence"]
    assert predominant.perspective.global_tonic == "C"
    assert predominant.perspective.local_tonic == "D"
    assert predominant.perspective.local_tonic_degree == "II"
    assert predominant.perspective.scope == "tonicization"
    assert predominant.perspective.mode == "minor"

    dominant = next(
        item
        for item in results[1].harmonic_classifications
        if item.role == "dominant"
    )
    assert dominant.dominant_relation == "fifth_related"
    assert "applied_cadence" in dominant.families


def test_secondary_tritone_substitute_keeps_global_and_local_keys():
    results = Romanizer.strict("C").annotate_progression(
        ["Em7-5", "Eb7", "Dm7"]
    )
    substitute = next(
        item
        for item in results[1].harmonic_classifications
        if item.dominant_relation == "tritone_substitute"
    )

    assert "tritone_substitute" in substitute.families
    assert "applied_cadence" in substitute.families
    assert substitute.perspective.global_tonic == "C"
    assert substitute.perspective.local_tonic == "D"
    assert substitute.perspective.local_tonic_degree == "II"
    assert substitute.perspective.scope == "tonicization"


def test_blackadder_common_axes_are_available_on_k_best_selection():
    paths = Romanizer.strict("B").analyze_top_k_interpretations(
        ["Daug/C", "B"], k=1
    )
    reading = paths[0].selections[0].blackadder

    assert reading.classification.role == "dominant"
    assert reading.classification.dominant_relation == "tritone_substitute"
    assert "tritone_substitute" in reading.classification.families
    assert reading.classification.perspective.local_tonic == "B"


def test_ordinary_top_k_exposes_local_degree_and_competing_meanings():
    romanizer = Romanizer.strict("C")
    paths = romanizer.analyze_top_k_interpretations(
        ["F#m7", "B7", "Em7"], k=5
    )

    assert len(paths) == 5
    predominant = paths[0].selections[0].harmonic_classifications[0]
    assert predominant.role == "predominant"
    assert predominant.local_degree == "II"
    assert "applied_cadence" in predominant.families
    assert predominant.perspective.local_tonic == "E"
    assert predominant.perspective.local_tonic_degree == "III"
    assert predominant.perspective.mode == "minor"

    borrowed = romanizer.annotate_progression(["CM7", "EbM7", "CM7"])[1]
    families = {
        family
        for interpretation in borrowed.harmonic_interpretations
        for family in interpretation.classification.families
    }
    assert "modal_interchange" in families
    assert "chromatic_mediant" in families


def test_flat_two_major_seventh_keeps_neapolitan_and_phrygian_candidates():
    result = Romanizer.strict("C").annotate_progression(
        ["DbM7", "G7", "CM7"]
    )[0]
    interpretations = result.harmonic_interpretations

    assert any(
        "neapolitan" in item.classification.families
        for item in interpretations
    )
    assert any(
        "modal_interchange" in item.classification.families
        and "phrygian" in item.classification.sources
        for item in interpretations
    )


def test_diminished_context_families_cross_the_native_boundary():
    romanizer = Romanizer.strict("C")
    result = romanizer.annotate_progression(
        ["C", "Cdim7", "Dm7"]
    )[1]
    families = {
        family
        for interpretation in result.harmonic_interpretations
        for family in interpretation.classification.families
    }

    assert {
        "rootless_dominant_ninth",
        "passing_diminished",
        "tonic_substitute",
    } <= families

    paths = romanizer.analyze_top_k_interpretations(
        ["C", "Cdim7", "Dm7"], k=5
    )
    selected_families = {
        family
        for path in paths
        for classification in path.selections[1].harmonic_classifications
        for family in classification.families
    }
    assert {
        "rootless_dominant_ninth",
        "passing_diminished",
        "tonic_substitute",
    } <= selected_families

    auxiliary = romanizer.annotate_progression(
        ["C", "Cdim7", "C"]
    )[1]
    assert any(
        "auxiliary_diminished" in item.classification.families
        for item in auxiliary.harmonic_interpretations
    )


def test_quality_related_two_and_modal_ranking_cross_native_boundary():
    romanizer = Romanizer.strict("C")

    invalid_target = romanizer.annotate_progression(["G7", "Cdim"])
    assert not invalid_target[1].is_resolution_target
    assert invalid_target[1].resolution_type is None

    related = romanizer.analyze_top_k_interpretations(
        ["Abm7", "Db7", "C"], k=1
    )[0]
    assert (
        "tritone_substitute_related_two"
        in related.selections[0].harmonic_classifications[0].families
    )
    assert (
        related.selections[1]
        .harmonic_classifications[0]
        .dominant_relation
        == "tritone_substitute"
    )

    flat_seven = romanizer.annotate_progression(["Bb7", "C"])[0]
    assert any(
        item.classification.role == "subdominant"
        and "subdominant_minor" in item.classification.sources
        and item.classification.dominant_relation is None
        for item in flat_seven.harmonic_interpretations
    )

    flat_six = romanizer.analyze_top_k_interpretations(
        ["Ab", "G"], k=1
    )[0]
    assert (
        "subdominant_minor"
        in flat_six.selections[0].harmonic_classifications[0].families
    )

    flat_two = romanizer.analyze_top_k_interpretations(
        ["Db", "C"], k=1
    )[0]
    assert (
        "neapolitan"
        in flat_two.selections[0].harmonic_classifications[0].families
    )


def test_tonal_state_and_quality_families_cross_native_boundary():
    romanizer = Romanizer.strict("C")

    deceptive = romanizer.analyze_top_k_interpretations(
        ["E7", "FM7"], k=1
    )[0]
    assert all(
        "secondary_dominant_deceptive"
        in selection.harmonic_classifications[0].families
        for selection in deceptive.selections
    )
    annotated = romanizer.annotate_progression(["E7", "FM7"])
    assert annotated[1].resolution_type == "deceptive"

    alternate = romanizer.analyze_top_k_interpretations(
        ["BbM7", "Am7"], k=1
    )[0]
    assert all(
        "alternate_key_sequence"
        in selection.harmonic_classifications[0].families
        and selection.harmonic_classifications[0].perspective.local_tonic == "F"
        for selection in alternate.selections
    )

    suspended = romanizer.analyze_top_k_interpretations(
        ["Dm7/G", "G7", "C"], k=1
    )[0]
    assert suspended.selections[0].hybrid_kind == "9sus4"
    assert (
        "suspended_dominant"
        in suspended.selections[0].harmonic_classifications[0].families
    )

    voice_led = romanizer.analyze_top_k_interpretations(
        ["C", "Caug", "Caug/F#", "FM7"], k=5
    )
    assert voice_led[0].selections[2].blackadder.origin == "split_voice_leading"
    assert any(
        "voice_leading_required" in classification.families
        for path in voice_led
        for classification in path.selections[2].harmonic_classifications
    )


def test_half_diminished_tonic_neighbor_is_common_tone_decoration():
    romanizer = Romanizer.strict("C")
    annotated = romanizer.annotate_progression(["C#m7-5", "CM7"])
    common_tone = next(
        classification
        for classification in annotated[0].harmonic_classifications
        if "common_tone_neighbor" in classification.families
    )

    assert annotated[0].normalized_symbol == "C#m7-5"
    assert common_tone.role == "non_functional"
    assert common_tone.sources == ["chromatic"]
    assert "chromatic_approach" in common_tone.families
    assert romanizer.display_progression(["C#m7-5", "CM7"])[0].combined_label == (
        "C#m7-5 [#im7-5|CT]"
    )

def test_joint_key_and_function_inference_crosses_native_boundary():
    paths = Romanizer.strict().analyze_keys_and_functions(
        ["Em7", "A7", "Dm7", "G7", "Cmaj7"], k=5
    )

    assert paths[0].global_key.tonic == "C"
    assert paths[0].global_key.mode == "major"
    assert paths[0].selections[0].local_key.tonic == "D"
    assert paths[0].selections[0].active_key.tonic == "C"
    assert paths[0].selections[0].local_key.mode == "minor"
    assert paths[0].selections[0].scope == "tonicization"
    assert paths[0].selections[0].role == "predominant"
    assert paths[0].selections[1].role == "dominant"
    assert paths[0].total_score == (
        paths[0].key_score
        + paths[0].function_score
        + paths[0].modulation_score
        + paths[0].memory_score
    )


def test_key_inference_supports_minor_hint_and_fixed_key():
    romanizer = Romanizer.strict()
    minor = romanizer.analyze_keys_and_functions(
        ["Am", "Dm7", "E7", "Am"], k=3
    )
    assert (minor[0].global_key.tonic, minor[0].global_key.mode) == (
        "A",
        "minor",
    )

    hinted = romanizer.analyze_keys_and_functions(
        ["Am7", "Fmaj7", "Cmaj7", "G"],
        k=3,
        global_key_hint="A",
        global_key_hint_mode="minor",
    )
    assert any(
        evidence.rule_id == "builtin.key.caller_hint"
        for path in hinted
        for evidence in path.evidence
    )

    fixed = romanizer.analyze_keys_and_functions(
        ["Am7", "Fmaj7", "Cmaj7", "G"],
        k=3,
        global_key="E",
        global_mode="major",
    )
    assert all(path.global_key.tonic == "E" for path in fixed)


def test_joint_key_api_rejects_conflicting_constraints():
    romanizer = Romanizer.strict()
    try:
        romanizer.analyze_keys_and_functions(
            ["C"], global_key="C", global_key_hint="G"
        )
    except ValueError as error:
        assert "mutually exclusive" in str(error)
    else:
        raise AssertionError("conflicting key constraints must fail")


def test_interpretation_tree_is_directly_renderable_and_conditionable():
    romanizer = Romanizer.strict()
    tree = romanizer.analyze_interpretation_tree(
        ["Daug/C", "B"],
        k=5,
        global_key="B",
        global_mode="major",
    )

    assert tree.returned_path_count >= 2
    assert len(tree.roots) == 1
    root = tree.roots[0]
    assert root.is_top_k_consensus
    assert root.top_k_support_count == tree.returned_path_count
    assert len(root.children) >= 2
    assert tree.consensus_node_ids == [root.node_id]

    node = root.children[1]
    assert node.input_symbol == "Daug/C"
    assert node.event_index == 0
    assert node.chord_index == 0
    assert node.condition.prefix[-1].candidate_id == node.candidate_id
    assert node.step_score == node.emission_score + node.transition_score
    assert node.evidence

    conditioned = romanizer.analyze_interpretation_tree(
        ["Daug/C", "B"],
        k=5,
        condition=node.condition,
    )
    assert conditioned.condition_applied
    assert conditioned.condition_satisfied
    assert len(conditioned.roots[0].children) == 1
    assert conditioned.roots[0].children[0].candidate_id == node.candidate_id
    assert conditioned.roots[0].children[0].is_top_k_consensus


def test_interpretation_tree_reports_stale_condition():
    romanizer = Romanizer.strict()
    initial = romanizer.analyze_interpretation_tree(["C", "G7", "C"], k=1)
    stale = TreeCondition(
        rule_set_version=initial.rule_set_version,
        progression_fingerprint=initial.progression_fingerprint,
        global_key=TonalKey("C", "major"),
        prefix=[
            CandidateConstraint(
                event_index=0,
                candidate_id="event-0:candidate-does-not-exist",
            )
        ],
    )
    tree = romanizer.analyze_interpretation_tree(
        ["C", "G7", "C"],
        condition=stale,
    )

    assert tree.condition_applied
    assert not tree.condition_satisfied
    assert tree.returned_path_count == 0
    assert tree.roots == []


def test_modulation_and_pivot_metadata_crosses_native_boundary():
    romanizer = Romanizer.strict()
    symbols = ["C", "Am7", "D7", "G", "C", "D7", "G"]
    paths = romanizer.analyze_keys_and_functions(
        symbols,
        k=8,
        global_key="C",
        global_mode="major",
    )

    modulated = next(
        path
        for path in paths
        if any(
            span.to_key.tonic == "G"
            and span.mechanism == "diatonic_pivot"
            for span in path.modulations
        )
    )
    span = modulated.modulations[0]
    assert span.pivot is not None
    assert span.pivot.chord_symbol == "Am7"
    assert span.pivot.old_degree == "VI"
    assert span.pivot.new_degree == "II"
    assert any(
        selection.is_pivot
        and selection.active_key == TonalKey("G", "major")
        and selection.scope == "modulation"
        for selection in modulated.selections
    )
    assert modulated.total_score == (
        modulated.function_score
        + modulated.key_score
        + modulated.modulation_score
        + modulated.memory_score
    )
    assert span.duration_chords >= 3
    assert any(
        evidence.rule_id == "builtin.modulation.key_region_duration"
        for evidence in span.evidence
    )

    brief = romanizer.analyze_keys_and_functions(
        ["C", "E7", "Am", "G7", "C"],
        k=8,
        global_key="C",
        global_mode="major",
    )
    assert brief[0].modulations == []
    assert any(
        any(span.to_key == TonalKey("A", "minor") for span in path.modulations)
        for path in brief
    )


def test_long_harmonic_memory_crosses_native_and_tree_boundaries():
    romanizer = Romanizer.strict()
    symbols = ["C", "D7", "Am7", "G", "C"]
    paths = romanizer.analyze_keys_and_functions(
        symbols,
        k=8,
        global_key="C",
        global_mode="major",
    )
    path = next(
        path
        for path in paths
        if any(
            resolution.source_event_index == 1
            and resolution.resolution_event_index == 3
            and resolution.intervening_chords == 1
            for resolution in path.harmonic_resolutions
        )
    )

    assert path.memory_score > 0
    assert path.selections[1].pending_resolutions[0].target_key.tonic == "G"
    assert path.selections[2].pending_resolutions[0].intervening_chords == 1
    assert path.selections[3].resolved_resolution_sources == [1]
    assert [selection.key_region_age_chords for selection in path.selections] == [
        1,
        2,
        3,
        4,
        5,
    ]

    tree = romanizer.analyze_interpretation_tree(
        symbols,
        k=8,
        global_key="C",
        global_mode="major",
    )

    def all_nodes(nodes):
        for node in nodes:
            yield node
            yield from all_nodes(node.children)

    nodes = list(all_nodes(tree.roots[0].children))
    assert any(
        node.event_index == 1
        and any(goal.target_key.tonic == "G" for goal in node.pending_resolutions)
        for node in nodes
    )
    assert any(
        node.event_index == 3 and 1 in node.resolved_resolution_sources
        for node in nodes
    )


def test_cadential_phase_and_deceptive_resolution_cross_native_boundary():
    romanizer = Romanizer.strict()
    paths = romanizer.analyze_keys_and_functions(
        ["Dm7", "Em7", "G7", "C"],
        k=8,
        global_key="C",
        global_mode="major",
    )
    path = next(
        path
        for path in paths
        if any(
            cadence.predominant_event_index == 0
            and cadence.dominant_event_index == 2
            and cadence.resolution_event_index == 3
            for cadence in path.cadential_spans
        )
    )
    cadence = path.cadential_spans[0]
    assert cadence.target_key == TonalKey("C", "major")
    assert cadence.intervening_before_dominant == 1
    assert cadence.score > 0
    assert path.selections[0].pending_predominant.source_event_index == 0
    assert path.selections[3].resolved_cadence_predominant_sources == [0]

    deceptive_paths = romanizer.analyze_keys_and_functions(
        ["C", "E7", "FM7", "C"],
        k=12,
        global_key="C",
        global_mode="major",
    )
    deceptive = next(
        resolution
        for candidate in deceptive_paths
        for resolution in candidate.harmonic_resolutions
        if resolution.source_event_index == 1
        and resolution.resolution_event_index == 2
        and resolution.kind == "deceptive_arrival"
    )
    assert deceptive.target_key == TonalKey("A", "minor")


def test_modulation_branch_in_tree_can_be_conditioned():
    romanizer = Romanizer.strict()
    symbols = ["C", "Am7", "D7", "G", "C", "D7", "G"]
    tree = romanizer.analyze_interpretation_tree(
        symbols,
        k=8,
        global_key="C",
        global_mode="major",
    )

    def all_nodes(nodes):
        for node in nodes:
            yield node
            yield from all_nodes(node.children)

    pivot_node = next(
        node
        for node in all_nodes(tree.roots[0].children)
        if node.is_pivot and "@mod:" in node.candidate_id
    )
    conditioned = romanizer.analyze_interpretation_tree(
        symbols,
        k=5,
        condition=pivot_node.condition,
    )
    assert conditioned.condition_satisfied
    assert any(
        node.candidate_id == pivot_node.candidate_id
        for node in all_nodes(conditioned.roots[0].children)
    )


def test_multi_stage_modulation_and_global_key_return_cross_native_boundary():
    romanizer = Romanizer.strict()
    outward = ["C", "Am7", "D7", "G", "Em7", "A7", "D", "G", "A7", "D"]
    paths = romanizer.analyze_keys_and_functions(
        outward,
        k=12,
        global_key="C",
        global_mode="major",
    )
    chained = next(
        path
        for path in paths
        if len(path.modulations) >= 2
        and path.modulations[0].from_key == TonalKey("C", "major")
        and path.modulations[0].to_key == TonalKey("G", "major")
        and path.modulations[1].from_key == TonalKey("G", "major")
        and path.modulations[1].to_key == TonalKey("D", "major")
    )
    assert chained.selections[3].active_key == TonalKey("G", "major")
    assert chained.selections[6].active_key == TonalKey("D", "major")
    assert chained.modulations[0].end_event_index == 3
    assert chained.modulations[1].start_event_index == 4

    returning = ["C", "Am7", "D7", "G", "C", "G7", "C", "F", "G7", "C"]
    return_paths = romanizer.analyze_keys_and_functions(
        returning,
        k=12,
        global_key="C",
        global_mode="major",
    )
    returned = next(
        path
        for path in return_paths
        if len(path.modulations) >= 2
        and path.modulations[0].to_key == TonalKey("G", "major")
        and path.modulations[1].from_key == TonalKey("G", "major")
        and path.modulations[1].to_key == TonalKey("C", "major")
    )
    assert returned.selections[4].active_key == TonalKey("C", "major")
    assert returned.selections[4].scope == "global"
    assert returned.selections[4].is_pivot


def test_second_modulation_branch_in_tree_can_be_conditioned():
    romanizer = Romanizer.strict()
    symbols = ["C", "Am7", "D7", "G", "Em7", "A7", "D", "G", "A7", "D"]
    tree = romanizer.analyze_interpretation_tree(
        symbols,
        k=20,
        global_key="C",
        global_mode="major",
    )

    def all_nodes(nodes):
        for node in nodes:
            yield node
            yield from all_nodes(node.children)

    second_pivot = next(
        node
        for node in all_nodes(tree.roots[0].children)
        if node.is_pivot
        and node.active_key == TonalKey("D", "major")
        and "@mod:" in node.candidate_id
    )
    conditioned = romanizer.analyze_interpretation_tree(
        symbols,
        k=5,
        condition=second_pivot.condition,
    )
    assert conditioned.condition_satisfied
    assert any(
        node.candidate_id == second_pivot.candidate_id
        for node in all_nodes(conditioned.roots[0].children)
    )
