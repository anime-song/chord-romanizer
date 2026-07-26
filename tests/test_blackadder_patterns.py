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
            ["Daug/C", "G7", "C"],
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
