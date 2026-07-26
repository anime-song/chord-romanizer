"""Legacy diagnostic progression retained for manual comparison.

Run this file from any working directory; the repository root is inserted into
``sys.path`` so the local Python package is used without installation.
"""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from chord_romanizer.romanizer import Romanizer
from chord_romanizer.chord_parser import ChordParser

romanizer = Romanizer(default_tonic="A")
# Progression: Cm7-5, C#m7-5/G, C#aug/G, F#m7
symbols = ["Cm7-5", "C#m7-5/G", "C#aug/G", "F#m7"]
chords = [ChordParser.parse(s) for s in symbols]
print(f"Parsed: {[c.root + '/' + (c.bass or '') for c in chords]}")

results = romanizer.annotate_progression(chords)

print("\nResults:")
for r in results:
    print(f"{r.chord.symbol} -> {r.roman} (Fixed Symbol: {r.symbol_fixed})")
