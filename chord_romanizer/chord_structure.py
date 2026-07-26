from typing import Dict, Optional, Set, Tuple

from .chord_parser import NOTE_NAMES
from .note_speller import NATURAL_PITCH_CLASS, NoteSpeller


class ChordStructure:
    """
    コードの「静的な定義」のみを扱う。
    「文脈によってこう解釈する」というロジックは Interpreter へ移動。
    """

    @staticmethod
    def get_intervals(quality: str) -> Set[int]:
        if "M7" in quality:
            return {0, 4, 7, 11}

        quality_lower = (quality or "").lower()
        if ("m7-5" in quality_lower) or ("m7b5" in quality_lower):
            return {0, 3, 6, 10}
        if "dim" in quality_lower or "o" in quality_lower:
            return {0, 3, 6}
        if ("maj7" in quality_lower) or ("ma7" in quality_lower):
            return {0, 4, 7, 11}
        if "m7" in quality_lower:
            return {0, 3, 7, 10}
        if "7" in quality_lower:
            return {0, 4, 7, 10}
        if "m" in quality_lower:
            return {0, 3, 7}
        return {0, 4, 7}

    @staticmethod
    def is_inversion(root: str, bass: str, quality: str) -> bool:
        dist = NoteSpeller.semitone_distance(bass, root)
        if dist is None:
            return False
        return dist in ChordStructure.get_intervals(quality)

    @staticmethod
    def get_spelled_tones(root: str, quality: str) -> Dict[int, str]:
        root_parsed = NoteSpeller.parse_note(root)
        if root_parsed is None:
            return {}
        root_letter, root_accidental = root_parsed
        root_pitch_class = (NATURAL_PITCH_CLASS[root_letter] + root_accidental) % 12

        quality_lower = quality.lower()
        if "M7" in quality:
             definition = ([(0, 0), (4, 2), (7, 4)], (11, 6))
        elif "m7" in quality_lower:
            definition = ([(0, 0), (3, 2), (7, 4)], (10, 6))
        elif "maj7" in quality_lower:
            definition = ([(0, 0), (4, 2), (7, 4)], (11, 6))
        elif "7" in quality_lower:
            definition = ([(0, 0), (4, 2), (7, 4)], (10, 6))
        elif "m" in quality_lower:
             definition = ([(0, 0), (3, 2), (7, 4)], None)
        else:
             definition = ([(0, 0), (4, 2), (7, 4)], None)


        triad, seventh = definition
        tones = {}
        for semitone, step in triad:
            target_letter = NoteSpeller.shift_letter(root_letter, step)
            current_pitch_class = (root_pitch_class + semitone) % 12
            tones[current_pitch_class] = NoteSpeller.spell_pitch_class(
                target_letter, current_pitch_class
            )
            
        if seventh:
            semitone, step = seventh
            target_letter = NoteSpeller.shift_letter(root_letter, step)
            current_pitch_class = (root_pitch_class + semitone) % 12
            tones[current_pitch_class] = NoteSpeller.spell_pitch_class(
                target_letter, current_pitch_class
            )
        return tones

    @staticmethod
    def is_aug_quality(quality: str) -> bool:
        quality_lower = (quality or "").lower()
        return ("aug" in quality_lower) or ("+" in (quality or ""))

    @staticmethod
    def get_aug_triad_pitch_classes(root: str) -> Optional[Set[int]]:
        root_pc = NoteSpeller.pitch_class_of(root)
        if root_pc is None:
            return None
        return {(root_pc + 0) % 12, (root_pc + 4) % 12, (root_pc + 8) % 12}

    @staticmethod
    def is_dominant_quality(quality: str) -> bool:
        """機能的にドミナントになり得るQualityか判定"""
        # "M7" は Major 7th なのでドミナントではない (case-sensitive check)
        if "M7" in quality:
            return False
            
        quality_lower = (quality or "").lower()
        # maj7 は除外、7を含むものをドミナント候補とする
        if ("maj7" in quality_lower) or ("ma7" in quality_lower):
            return False
            
        # マイナー系 (m7 など) はドミナントではない (ただし dim7 はドミナント機能を持つことがあるので除外しない)
        if ("m" in quality_lower) and ("dim" not in quality_lower):
            return False
            
        return "7" in quality_lower

    @staticmethod
    def is_tonic_quality(quality: str) -> bool:
        """機能的にトニックになり得るQualityか判定"""
        if ChordStructure.is_dominant_quality(quality):
            return False
        # m, maj7, M7, 6, または無印(Triad)など
        return True

    @staticmethod
    def is_minor_quality(quality: str) -> bool:
        """機能的にマイナーになり得るQualityか判定（m, m7, m9など）"""
        # "M7" は Major 7th -> Not Minor
        if "M7" in quality:
            return False
            
        quality_lower = (quality or "").lower()
        if ("m" in quality_lower) and ("maj" not in quality_lower) and ("dim" not in quality_lower):
            return True
        return False
