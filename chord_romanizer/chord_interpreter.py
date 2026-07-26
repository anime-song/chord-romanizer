from dataclasses import dataclass
from enum import Enum
from typing import List, Optional, Tuple

from .chord_parser import ParsedChord
from .note_speller import NoteSpeller
from .chord_structure import ChordStructure


class HybridKind(str, Enum):
    """ハイブリッドコードの分類ID"""

    NONE = "none"
    BLACKADDER = "blackadder"
    SEC_DOM_3_IN_BASS = "sec_dom_3inbass"
    HALFDIM_9 = "halfdim9"
    SUS4_9 = "9sus4"
    SUS4_7_B9 = "7sus4(b9)"


@dataclass
class HybridAnalysis:
    """ハイブリッド/スラッシュコードの解析結果"""

    is_hybrid: bool = False
    alter: Optional[str] = None  # "Gb7(9,#11)" など
    bass_preference: Optional[bool] = None  # Bassの#b優先度
    root_override: Optional[str] = None  # Blackadder時のアンカーなど
    kind: HybridKind = HybridKind.NONE  # "blackadder", "halfdim9" など


class ChordInterpreter:
    """
    複雑なコード（AugSlash, Blackadderなど）の機能的な解釈を行うクラス。
    Romanizerから複雑な条件分岐を隠蔽する。
    """

    # --- Scoring Configuration ---
    SCORE_SEMITONE_RESOLUTION = 3.0  # 半音解決
    SCORE_BACKDOOR_RESOLUTION = 5.0  # 裏コード/Backdoor解決
    SCORE_STRONG_RESOLUTION = 6.0  # 強進行 (V->I)
    SCORE_WEAK_RESOLUTION = 2.0  # 弱進行 (Dominantへの解決など)
    SCORE_BASE_BIAS = 0.5  # 同点時の下駄

    # -----------------------------
    # public
    # -----------------------------
    def analyze_slash_chord(
        self,
        chord: ParsedChord,
        next_chord: Optional[ParsedChord],
    ) -> HybridAnalysis:
        if not chord.bass:
            return HybridAnalysis(is_hybrid=False)

        # 1. 転回形 (Structureに委譲)
        if ChordStructure.is_inversion(chord.root, chord.bass, chord.quality):
            return HybridAnalysis(is_hybrid=False)

        # 2. Aug slash (文脈解釈)
        if ChordStructure.is_aug_quality(chord.quality):
            aug_result = self._infer_aug_slash(chord, next_chord)
            if aug_result:
                return aug_result

        # 3. 通常の hybrid (簡易定義に基づく)
        alter, kind = self._infer_normal_hybrid(chord)
        if alter:
            return HybridAnalysis(is_hybrid=True, alter=alter, kind=kind)

        return HybridAnalysis(is_hybrid=True, kind=HybridKind.NONE)

    # --- Internal Logic ---

    def _infer_aug_slash(
        self, chord: ParsedChord, next_chord: Optional[ParsedChord]
    ) -> Optional[HybridAnalysis]:
        candidates: List[Tuple[float, HybridAnalysis]] = []

        blackadder = self._check_blackadder(chord, next_chord)
        if blackadder:
            candidates.append(blackadder)

        halfdim = self._check_halfdim(chord, next_chord)
        if halfdim:
            candidates.append(halfdim)

        if not candidates:
            return None

        # スコア最大を採用
        return max(candidates, key=lambda x: x[0])[1]

    def _check_blackadder(
        self, chord: ParsedChord, next_chord: Optional[ParsedChord]
    ) -> Optional[Tuple[float, HybridAnalysis]]:
        bass_pitch_class = NoteSpeller.pitch_class_of(chord.bass)
        triad_pitch_classes = ChordStructure.get_aug_triad_pitch_classes(chord.root)

        if bass_pitch_class is None or triad_pitch_classes is None:
            return None

        # Anchor check: bass + 6 semitones が構成音に含まれるか
        anchor_pitch_class = (bass_pitch_class + 6) % 12
        if anchor_pitch_class not in triad_pitch_classes:
            return None

        # --- Context Analysis ---
        score = 0.0
        bass_to_next = (
            NoteSpeller.semitone_distance(next_chord.root, chord.bass)
            if next_chord
            else None
        )

        # Determine Bass Preference (Sharp or Flat)
        bass_preference = self._bass_pref_from_resolution(chord.bass, next_chord)
        bass_fixed = NoteSpeller.name_of_pitch_class(bass_pitch_class, bass_preference)
        anchor_name = NoteSpeller.name_of_pitch_class(
            anchor_pitch_class, bass_preference
        )  # Canonical

        # Default Interpretation
        alter = f"{bass_fixed}7(9,#11)"
        kind = HybridKind.BLACKADDER

        # Rule 1: 半音解決 (Tritone substitution behavior)
        if bass_to_next in (1, 11):
            score += self.SCORE_SEMITONE_RESOLUTION

        # Rule 2: Backdoor (bVII -> I)
        if (
            next_chord
            and bass_to_next == 2
            and ChordStructure.is_tonic_quality(next_chord.quality)
        ):
            score += self.SCORE_BACKDOOR_RESOLUTION

        # Rule 3: 3rd in Bass Secondary Dominant (e.g., Faug/B -> C)
        # Anchor(F) -> Dominant(G) -> Target(C)
        anchor_parsed = NoteSpeller.parse_note(anchor_name)
        if anchor_parsed and next_chord:
            dominant_letter = NoteSpeller.shift_letter(anchor_parsed[0], 1)
            dominant_pc = (anchor_pitch_class + 2) % 12
            dominant_name = NoteSpeller.spell_pitch_class(dominant_letter, dominant_pc)

            dominant_to_next = NoteSpeller.semitone_distance(
                next_chord.root, dominant_name
            )
            # Dominant resolution check (V->I)
            if dominant_to_next in (5, 7) and ChordStructure.is_tonic_quality(
                next_chord.quality
            ):
                alter = f"{dominant_name}7(9,#11)/{bass_fixed}"
                kind = HybridKind.SEC_DOM_3_IN_BASS
                score += self.SCORE_STRONG_RESOLUTION

        # Overriding Root:
        # If the detected anchor is the same pitch class as the original root,
        # we should NOT force a restart of the spelling.
        # This prevents "C#aug/G" (Anchor=C#) from being rewritten as "Dbaug/G"
        # just because the bass is descending (prefer_sharps=False).
        final_root_override = anchor_name
        original_root_pc = NoteSpeller.pitch_class_of(chord.root)
        if original_root_pc == anchor_pitch_class:
            final_root_override = None

        return score, HybridAnalysis(
            is_hybrid=True,
            kind=kind,
            alter=alter,
            bass_preference=bass_preference,
            root_override=final_root_override,
        )

    def _check_halfdim(
        self, chord: ParsedChord, next_chord: Optional[ParsedChord]
    ) -> Optional[Tuple[float, HybridAnalysis]]:
        bass_pitch_class = NoteSpeller.pitch_class_of(chord.bass)
        triad_pitch_classes = ChordStructure.get_aug_triad_pitch_classes(chord.root)
        if bass_pitch_class is None or triad_pitch_classes is None:
            return None

        # Half-dim check: relative intervals {2, 6, 10}
        relative_intervals = {(pc - bass_pitch_class) % 12 for pc in triad_pitch_classes}
        if not {2, 6, 10}.issubset(relative_intervals):
            return None

        score = self.SCORE_BASE_BIAS
        bass_preference = None
        bass_fixed = NoteSpeller.name_of_pitch_class(bass_pitch_class, bass_preference)

        if next_chord:
            bass_to_next = NoteSpeller.semitone_distance(next_chord.root, chord.bass)
            next_is_dominant = ChordStructure.is_dominant_quality(next_chord.quality)

            # iiø -> V (Strong)
            if bass_to_next in (5, 7) and next_is_dominant:
                score += self.SCORE_STRONG_RESOLUTION
            # Any resolution to Dominant (Weak)
            elif next_is_dominant:
                score += self.SCORE_WEAK_RESOLUTION

        return score, HybridAnalysis(
            is_hybrid=True,
            kind=HybridKind.HALFDIM_9,
            alter=f"{bass_fixed}m7-5(9)",
            bass_preference=bass_preference,
        )

    def _infer_normal_hybrid(
        self, chord: ParsedChord
    ) -> Tuple[Optional[str], HybridKind]:
        dist = NoteSpeller.semitone_distance(chord.root, chord.bass)
        if dist is None:
            return None, HybridKind.NONE

        intervals = ChordStructure.get_intervals(chord.quality)
        relative_intervals = {(i + dist) % 12 for i in intervals}

        has_third = (3 in relative_intervals) or (4 in relative_intervals)
        if not has_third:
            if {2, 5, 10}.issubset(relative_intervals):
                return f"{chord.bass}9sus4", HybridKind.SUS4_9
            if {1, 5, 10}.issubset(relative_intervals):
                return f"{chord.bass}7sus4(b9)", HybridKind.SUS4_7_B9
        return None, HybridKind.NONE

    # Helper: Bass preference logic
    def _bass_pref_from_resolution(
        self, bass: str, next_chord: Optional[ParsedChord]
    ) -> Optional[bool]:
        if not next_chord:
            return None
        bass_to_next = NoteSpeller.semitone_distance(next_chord.root, bass)
        if bass_to_next == 1:
            return True  # Ascending -> Sharp
        if bass_to_next == 11:
            return False  # Descending -> Flat
        return None
