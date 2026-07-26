from typing import Optional, Tuple
from .chord_parser import NOTE_NAMES, normalize_note_pc

NOTE_LETTERS = ["C", "D", "E", "F", "G", "A", "B"]
NATURAL_PITCH_CLASS = {"C": 0, "D": 2, "E": 4, "F": 5, "G": 7, "A": 9, "B": 11}
SEMITONE_MAP = {name: idx for idx, name in enumerate(NOTE_NAMES)}


class NoteSpeller:
    SHARP_TO_FLAT = {"C#": "Db", "D#": "Eb", "F#": "Gb", "G#": "Ab", "A#": "Bb"}

    @staticmethod
    def name_of_pitch_class(pitch_class: int, prefer_sharps: Optional[bool] = None) -> str:
        sharp_name = NOTE_NAMES[pitch_class % 12]
        if prefer_sharps is False:
            return NoteSpeller.SHARP_TO_FLAT.get(sharp_name, sharp_name)
        return sharp_name

    @staticmethod
    def parse_note(note: str) -> Optional[Tuple[str, int]]:
        if not note:
            return None
        letter = note[0].upper()
        if letter not in NATURAL_PITCH_CLASS:
            return None
        
        accidental_value = 0
        accidental_part = note[1:].replace("x", "##")
        
        for char in accidental_part:
            if char == "#":
                accidental_value += 1
            elif char.lower() == "b":
                accidental_value -= 1
        return letter, accidental_value

    @staticmethod
    def pitch_class_of(note: str) -> Optional[int]:
        parsed = NoteSpeller.parse_note(note)
        if not parsed:
            return None
        letter, accidental = parsed
        return (NATURAL_PITCH_CLASS[letter] + accidental) % 12

    @staticmethod
    def semitone_distance(target_note: str, reference_note: str) -> Optional[int]:
        target_pc = normalize_note_pc(target_note)
        reference_pc = normalize_note_pc(reference_note)
        
        if target_pc is None or reference_pc is None:
            return None
            
        target_index = SEMITONE_MAP[target_pc]
        reference_index = SEMITONE_MAP[reference_pc]
        return (target_index - reference_index) % 12

    @staticmethod
    def spell_pitch_class(base_letter: str, target_pitch_class: int) -> str:
        base_pc = NATURAL_PITCH_CLASS[base_letter]
        diff = (target_pitch_class - base_pc) % 12
        
        if diff > 6:
            diff -= 12
            
        if diff == 0:
            return base_letter
            
        if diff > 0:
            return base_letter + ("#" * diff)
        else:
            return base_letter + ("b" * -diff)

    @staticmethod
    def shift_letter(letter: str, steps: int) -> str:
        try:
            current_index = NOTE_LETTERS.index(letter)
            return NOTE_LETTERS[(current_index + steps) % 7]
        except ValueError:
            # Fallback or error handling if needed, though input should be valid
            return letter
