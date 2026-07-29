"""Regression tests built from small, synthetic harmonic patterns."""

import pytest

from chord_romanizer import Romanizer


@pytest.mark.parametrize(
    ("tonic", "progression", "axis", "expected"),
    [
        ("B", ["Daug/C", "B"], "function", "tritone_substitute"),
        ("C", ["Abaug/Bb", "C"], "function", "subdominant_minor"),
        (
            "G",
            ["G#aug/F#", "GM7/D"],
            "structure",
            "rootless_dominant_third_in_bass",
        ),
        (
            "C",
            ["Caug/F#", "B7"],
            "structure",
            "half_diminished_add_nine_omit_third",
        ),
    ],
)
def test_semantic_top_five_contains_abstract_functional_reading(
    tonic, progression, axis, expected
):
    paths = Romanizer.strict(tonic).analyze_top_k_interpretations(
        progression, k=5
    )

    assert any(
        selection.event_index == 0
        and selection.blackadder is not None
        and getattr(selection.blackadder, axis) == expected
        for path in paths
        for selection in path.selections
    )


def test_text_only_analysis_retains_observation_dependent_readings():
    result = Romanizer.strict("C").annotate_progression(["Daug/C"])[0]
    readings = [
        interpretation["blackadder"]
        for interpretation in result.functional_interpretations
        if interpretation["blackadder"] is not None
    ]

    assert any(reading["structure"] == "whole_tone_subset" for reading in readings)
    assert any(reading["origin"] == "split_voice_leading" for reading in readings)
    assert any(reading["origin"] == "incidental" for reading in readings)


def test_half_diminished_reading_requires_root_related_dominant_for_top_label():
    romanizer = Romanizer.strict("C")

    f_sharp_to_b = romanizer.display_progression(["Caug/F#", "B7"])
    b_to_e = romanizer.display_progression(["Faug/B", "E7"])
    unrelated = romanizer.display_progression(["Caug/F#", "G7"])
    isolated = romanizer.display_progression(["Caug/F#"])

    assert f_sharp_to_b[0].combined_label == "Caug/F# [#IVm7-5(9)|PD]"
    assert b_to_e[0].combined_label == "Faug/B [VIIm7-5(9)|PD]"
    assert unrelated[0].combined_label == "Caug/F# [II7(9,#11)/#IV|V/V]"
    assert "m7-5(9)" not in isolated[0].combined_label


def test_augmented_dominant_with_approach_bass_outranks_substitute():
    romanizer = Romanizer.strict("C")
    progression = ["C", "Caug/F#", "FM7"]
    display = romanizer.display_progression(progression)
    paths = romanizer.analyze_top_k_interpretations(progression, k=3)
    reading = paths[0].selections[1].blackadder

    assert display[1].combined_label == "Caug/F# [Iaug/#IV|V+/IV]"
    assert reading is not None
    assert reading.structure == "augmented_triad_over_bass"
    assert reading.function == "secondary_dominant"
    assert reading.origin == "upper_structure_with_independent_bass"
    assert reading.canonical_bass == "F#"
    assert reading.effective_root == "C"
    assert reading.target_root == "F"
    assert "chromatic_approach_bass" in reading.classification.families
    assert any(
        evidence.rule_id
        == "builtin.blackadder.transition.augmented_dominant_approach_bass"
        for evidence in paths[0].evidence
    )

def test_unresolved_fallback_families_reach_semantic_top_five():
    paths = Romanizer.strict("C").analyze_top_k_interpretations(
        ["Bbaug/C"], k=5
    )
    readings = [
        selection.blackadder
        for path in paths
        for selection in path.selections
        if selection.event_index == 0 and selection.blackadder is not None
    ]

    assert any(reading.structure == "whole_tone_subset" for reading in readings)
    assert any(reading.origin == "split_voice_leading" for reading in readings)
    assert any(reading.origin == "incidental" for reading in readings)
