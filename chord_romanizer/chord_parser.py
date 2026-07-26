from dataclasses import dataclass
from typing import Optional

# Pitch-class canonical names (all sharps)
NOTE_NAMES = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"]
NATURAL_PITCH_CLASS = {"C": 0, "D": 2, "E": 4, "F": 5, "G": 7, "A": 9, "B": 11}

# Aliases for normalization (flats, H, etc.)
NOTE_ALIASES = {
    "CB": "B",
    "B#": "C",
    "DB": "C#",
    "EB": "D#",
    "E#": "F",
    "FB": "E",
    "GB": "F#",
    "AB": "G#",
    "BB": "A#",
    "HB": "B",
    "H": "B",
}


def normalize_note_pc(note: str) -> Optional[str]:
    """Normalize pitch-class spelling to canonical sharp-based representation."""
    if not note:
        return None

    up = note.strip().upper()
    if not up:
        return None

    # Preserve legacy aliases/synonyms first (e.g., H, Hb behavior).
    if up in NOTE_ALIASES:
        return NOTE_ALIASES[up]
    if up in NOTE_NAMES:
        return up

    letter = up[0]
    if letter == "H":
        base_pc = NATURAL_PITCH_CLASS["B"]
    elif letter in NATURAL_PITCH_CLASS:
        base_pc = NATURAL_PITCH_CLASS[letter]
    else:
        return None

    accidental_value = 0
    accidental_part = up[1:]
    for char in accidental_part:
        if char == "#":
            accidental_value += 1
        elif char == "B":
            accidental_value -= 1
        elif char == "X":
            accidental_value += 2
        else:
            return None

    return NOTE_NAMES[(base_pc + accidental_value) % 12]


def normalize_spelling(token: str) -> str:
    """Keep user spelling but capitalize first letter (Db -> Db, c# -> C#)."""
    token = token.strip()
    if not token:
        return token
    return token[0].upper() + token[1:]


@dataclass
class ParsedChord:
    symbol: str
    root: str  # user spelling (e.g., Db / C#)
    quality: str
    bass: Optional[str] = None  # user spelling (e.g., F#, Gb)


class ChordParser:
    @staticmethod
    def parse(symbol: str) -> Optional[ParsedChord]:
        """Parse chord symbol like 'C#m7/G#' into components."""
        if not symbol:
            return None

        text = symbol.strip()

        # Allow no-chord markers
        normalized_nc = text.replace(".", "").replace(" ", "").upper()
        if normalized_nc in {"NC", "NOCHORD"}:
            return ParsedChord(symbol=text, root="NC", quality="", bass=None)

        # Split slash bass if present
        if "/" in text:
            body, bass = text.split("/", 1)
            bass_token = bass.strip()
            if normalize_note_pc(bass_token) is None:
                return None
        else:
            body, bass_token = text, None

        body = body.strip()
        if not body:
            return None

        # Root token: first note letter + contiguous accidentals (#, b, x).
        # Supports repeated accidentals (e.g., F##, Bbb, C###).
        root_len = 1
        while root_len < len(body) and body[root_len] in ("#", "b", "B", "x", "X"):
            root_len += 1
        
        root_token = body[:root_len]
        rest = body[root_len:]

        # Validate root
        if normalize_note_pc(root_token) is None:
            return None

        root = normalize_spelling(root_token)
        quality = rest or ""

        bass: Optional[str] = None
        if bass_token:
            bass = normalize_spelling(bass_token)

        return ParsedChord(symbol=text, root=root, quality=quality, bass=bass)
