"""Generate the Python 0.1.9 compatibility fixtures used by Rust tests.

Run from the repository root:

    python tools/generate_python_golden.py

The case manifest is deliberately dependency-free and easy for a Rust test to
parse. The JSON Lines file is the canonical Python result for every case.
"""

from __future__ import annotations

import json
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Sequence, Tuple, Union

REPOSITORY_ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(REPOSITORY_ROOT))

from chord_romanizer import ChordParser, Romanizer


Item = Union[str, Tuple[str, str]]


@dataclass(frozen=True)
class Case:
    name: str
    default_tonic: str
    symbols: Sequence[Item]
    simplify_accidentals: bool = False


# These cases correspond to every Romanizer scenario in the 60-test Python
# suite. Some pytest functions contain multiple independent progressions, so
# those are represented as separate golden cases.
CASES = [
    # tests/test_issues.py
    Case("issue_enharmonic_bass_minor", "Gb", ["D#m/F#"]),
    Case("issue_tritone_diminished_spelling", "Gb", ["F#/C#", "Cdim", "B"]),
    Case("issue_non_diatonic_bass_spelling", "Ab", ["A#m7/D#"]),
    Case("issue_ascending_7th_flat_eb", "C", ["Eb7", "Em"]),
    Case("issue_ascending_7th_flat_ab", "C", ["Ab7", "Am7"]),
    Case("issue_enharmonic_spelling_gb_aug", "Gb", ["Gbaug/C", "BM7"]),
    Case("issue_emajor_em7_spelling", "E", ["F#m7", "B7", "EM7", "AM7"]),
    Case("issue_gbm7_over_ab", "G", ["GbM7/Ab"], True),
    Case("issue_caug_gb", "E", ["Caug/Gb"], True),
    # tests/test_theory.py
    Case("diatonic_c", "C", ["C", "Dm7", "Em7", "F", "G7", "Am7", "Bm7-5"]),
    Case("major_seventh_notation", "C", ["Cmaj7", "Fmaj7"]),
    Case("basic_progression_c", "C", ["F", "G7", "Em7"]),
    Case("neapolitan_to_tonic", "C", ["Db7", "C"]),
    Case("leading_tone_to_five", "C", ["F#dim", "G"]),
    Case("borrowed_degrees_and_tritone", "C", ["Db", "Eb", "Gb", "Ab", "Bb", "F#"]),
    Case("slash_root_roman", "C", ["E/G#"]),
    Case("tritone_downward", "C", ["F#m7-5", "FM7"]),
    Case("tritone_upward", "C", ["F#7", "G"]),
    Case("local_tonicization_flat_spelling", "C", ["Dbm7", "Gb7", "Bm7"]),
    Case("local_tonicization_b", "C", ["C#m7", "F#7", "Bm7"]),
    Case("local_tonicization_alternates", "C", ["C#m7", "F#7", "Bm7"]),
    Case("local_tonicization_not_triggered", "C", ["C#m7", "F7", "Bm7"]),
    Case("g_major_basic", "G", ["C", "D7", "G"]),
    Case("g_major_slash", "G", ["C/E", "D/F#", "G/B"]),
    Case("alternate_c_sharp", "C", ["C#"]),
    Case("alternate_d_flat", "C", ["Db"]),
    Case("alternate_f_sharp", "C", ["F#"]),
    Case("up_chromatic_db_d", "C", ["Db", "D"]),
    Case("up_chromatic_dsharp_e", "C", ["D#", "E"]),
    Case("up_chromatic_gsharp_a", "C", ["G#", "A"]),
    Case("up_chromatic_asharp_b", "C", ["A#", "B"]),
    Case("up_dim_csharp_d", "C", ["C#dim", "D"]),
    Case("up_dim_dsharp_e", "C", ["D#dim", "E"]),
    Case("up_dim_gsharp_a", "C", ["G#dim", "A"]),
    Case("up_dim_asharp_b", "C", ["A#dim", "B"]),
    Case("down_chromatic_csharp_c", "C", ["C#", "C"]),
    Case("down_chromatic_dsharp_d", "C", ["D#", "D"]),
    Case("down_chromatic_gsharp_g", "C", ["G#", "G"]),
    Case("down_chromatic_asharp_a", "C", ["A#", "A"]),
    Case("primary_alternate_c_sharp", "C", ["C#"]),
    Case("primary_alternate_d_flat", "C", ["Db"]),
    Case("primary_alternate_f_sharp", "C", ["F#"]),
    Case("inversion_and_hybrid", "C", ["E/G#", "F/G"]),
    Case("hybrid_e_major", "E", ["A/B"]),
    Case("minor_degrees_major_context", "C", ["Eb", "Fm", "Bb"]),
    Case("extreme_f_sharp", "F#", ["F#", "B", "C#7"]),
    Case("extreme_g_flat", "Gb", ["Gb", "Cb", "Db7"]),
    Case("double_sharp_input", "G#", ["Fx"]),
    Case("simplify_g_sharp_off", "G#", ["Gdim"]),
    Case("simplify_g_sharp_on", "G#", ["Gdim"], True),
    Case("simplify_f_flat_off", "Fb", ["AM7"]),
    Case("simplify_f_flat_on", "Fb", ["AM7"], True),
    # tests/test_theory_ii_v.py
    Case("standard_ii_v_i", "C", ["Dm7", "G7", "Cmaj7"]),
    Case("hybrid_ii_v", "C", ["Em7", "G/A", "Dmaj7"]),
    Case("tritone_sub_resolution", "C", ["Dm7", "Db7", "Cmaj7"]),
    Case("flat_key_ii_v", "C", ["Fm7", "Bb7", "Ebmaj7"]),
    # Public per-item key API, documented in README but not covered by pytest.
    Case(
        "per_item_tonic",
        "C",
        [("C", "C"), ("F", "C"), ("Dm7", "F"), ("G7", "F"), ("C", "F")],
    ),
]


def encode_result(result) -> dict:
    return {
        "alter": result.alter,
        "alternate_labels": result.alternate_labels,
        "degree_bass": result.degree_bass,
        "degree_root": result.degree_root,
        "is_hybrid": result.is_hybrid,
        "is_ii_v_start": result.is_ii_v_start,
        "is_resolution_target": result.is_resolution_target,
        "resolution_type": result.resolution_type,
        "roman": result.roman,
        "roman_root_bass": result.roman_root_bass,
        "symbol_fixed": result.symbol_fixed,
    }


def encode_manifest_item(item: Item) -> str:
    if isinstance(item, tuple):
        symbol, tonic = item
        return f"{symbol}~{tonic}"
    return item


def main() -> None:
    root = REPOSITORY_ROOT
    fixtures = root / "chord-romanizer-rs" / "tests" / "fixtures"
    fixtures.mkdir(parents=True, exist_ok=True)

    manifest_lines = []
    golden_lines = []
    for case in CASES:
        manifest_lines.append(
            "|".join(
                [
                    case.name,
                    case.default_tonic,
                    "1" if case.simplify_accidentals else "0",
                    ";".join(encode_manifest_item(item) for item in case.symbols),
                ]
            )
        )

        progression = []
        for item in case.symbols:
            if isinstance(item, tuple):
                symbol, tonic = item
                parsed = ChordParser.parse(symbol)
                assert parsed is not None, (case.name, symbol)
                progression.append((parsed, tonic))
            else:
                parsed = ChordParser.parse(item)
                assert parsed is not None, (case.name, item)
                progression.append(parsed)

        romanizer = Romanizer(
            default_tonic=case.default_tonic,
            simplify_accidentals=case.simplify_accidentals,
        )
        results = romanizer.annotate_progression(progression)
        golden_lines.append(
            json.dumps(
                {"case": case.name, "results": [encode_result(result) for result in results]},
                ensure_ascii=False,
                sort_keys=True,
                separators=(",", ":"),
            )
        )

    (fixtures / "compat_cases.txt").write_text(
        "\n".join(manifest_lines) + "\n", encoding="utf-8"
    )
    (fixtures / "python_golden.jsonl").write_text(
        "\n".join(golden_lines) + "\n", encoding="utf-8"
    )
    print(f"generated {len(CASES)} compatibility cases in {fixtures}")


if __name__ == "__main__":
    main()
