from chord_romanizer.chord_parser import ChordParser
from chord_romanizer.romanizer import Romanizer
import chord_romanizer
print(f"DEBUG: chord_romanizer file: {chord_romanizer.__file__}")

def test_issue_enharmonic_bass_minor_chord():
    # Issue: D#m/F# in Gb was analyzed as VIm/#VII instead of VIm/I
    # Reason: D#m (Eb minor) tones were calculated as Major {Eb, G, Bb}, so bass F# (Gb) didn't match.
    # Fix: Added minor chord support in ChordStructure.get_spelled_tones.

    symbol = "D#m/F#"
    key = "Gb"
    
    chord = ChordParser.parse(symbol)
    rom = Romanizer(default_tonic=key)
    result = rom.annotate_progression([chord])[0]
    
    assert result.roman == "VIm/I"
    assert result.degree_bass == "I"

def test_issue_tritone_diminished_spelling():
    # Issue: Cdim in Gb was analyzed as bVdim (Dbb) instead of #IVdim (C).
    # Context: F#/C# -> Cdim -> B (descending to IV).
    # Fix: Updated Romanizer logic to prefer #IV (sharp) for dim chords on tritone degree.
    symbols = ["F#/C#", "Cdim", "B"]
    key = "Gb"
    
    chords = [ChordParser.parse(s) for s in symbols]
    rom = Romanizer(default_tonic=key)
    results = rom.annotate_progression(chords)
    
    # Check Cdim (index 1)
    cdim_result = results[1]
    
    # Expect #IVdim, NOT bVdim
    assert "#IV" in cdim_result.roman
    assert "bV" not in cdim_result.roman
    assert cdim_result.degree_root == "#IV"

def test_issue_non_diatonic_bass_spelling():
    """
    Issue: A#m7/D# in Ab was analyzed as IIm7/##IV
    Reason: Bass note D# is enharmonic to Eb (V degree)
    Fix: Added diatonic bass spelling enforcement for non-chord-tones
    """
    symbol = "A#m7/D#"
    key = "Ab"
    
    parsed = ChordParser.parse(symbol)
    rom = Romanizer(default_tonic=key)
    result = rom.annotate_progression([parsed])[0]
    
    assert result.roman == "IIm7/V"
    assert result.degree_bass == "V"

def test_issue_ascending_7th_flat_preference():
    """
    Issue: Eb7 -> Em (in C) was analyzed as #II7 -> IIIm
    Reason: Blanket rule enforced sharps for all ascending semitone steps.
    Fix: Only enforce sharps for diminished chords; prefer flats otherwise.
    """
    # Case 1: Eb7 -> Em (bIII7 -> IIIm)
    s1 = ["Eb7", "Em"]
    r1 = Romanizer(default_tonic="C")
    res1 = r1.annotate_progression([ChordParser.parse(c) for c in s1])
    assert res1[0].roman == "bIII7"
    assert res1[0].degree_root == "bIII"

    # Case 2: Ab7 -> Am7 (bVI7 -> VIm7)
    s2 = ["Ab7", "Am7"]
    r2 = Romanizer(default_tonic="C")
    res2 = r2.annotate_progression([ChordParser.parse(c) for c in s2])
    assert res2[0].roman == "bVI7"
    assert res2[0].degree_root == "bVI"

def test_issue_enharmonic_spelling_gb_aug():
    """
    Issue: Gbaug/C in Gb Major was analyzed as F#aug/C (Iaug/#IV).
    Reason: Next chord BM7 (flat key context CbM7) caused Interpreter to prefer F#->B (P5).
    Fix: Contextualize next chord spelling (Cb) and use bass preference for anchor spelling.
    """
    # Gbaug/C -> BM7(9)
    # in Gb: Iaug/#IV -> IVM7
    s = ["Gbaug/C", "BM7"] # Simplified next chord
    r = Romanizer(default_tonic="Gb")
    res = r.annotate_progression([ChordParser.parse(c) for c in s])
    
    # Check Gbaug/C
    chord_res = res[0]
    # Expect Root "Gb", not "F#"
    # "Gb" is I in Gb Major.
    assert chord_res.degree_root == "I"
    assert "F#" not in chord_res.symbol_fixed
    assert "Gb" in chord_res.symbol_fixed

def test_issue_emajor_em7_spelling():
    """
    Issue: EM7 in E Major was reported to become FbM7.
    Verification: Ensure it stays EM7 (IM7).
    """
    s = ["F#m7", "B7", "EM7", "AM7"]
    r = Romanizer(default_tonic="E")
    res = r.annotate_progression([ChordParser.parse(c) for c in s])
    
    # EM7 is index 2
    em_res = res[2]
    assert em_res.degree_root == "I"
    assert em_res.symbol_fixed == "EM7"


def test_issue_gbm7_over_ab_in_g_major():
    """
    Issue: GbM7/Ab in G Major was analyzed as F#M7/Ab (VIIM7/bII).
    Reason: Root F# (VII) was prioritized over Gb (bI), creating Aug 6th with Ab bass.
    Fix: Prioritize bI when bass is bII (dist=11, bass_dist=1).
    """
    symbol = "GbM7/Ab"
    key = "G"
    
    parsed = ChordParser.parse(symbol)
    rom = Romanizer(default_tonic=key, simplify_accidentals=True)
    result = rom.annotate_progression([parsed])[0]
    
    # Expect bIM7/bII -> GbM7/Ab
    assert result.degree_root == "bI"
    assert result.degree_bass == "bII"
    assert "Gb" in result.symbol_fixed
    assert "Ab" in result.symbol_fixed
    assert "F#" not in result.symbol_fixed


def test_issue_caug_gb_in_e_major():
    """
    Issue: Caug/Gb was analyzed as bVIaug/bbIII instead of bVIaug/II (Caug/F#).
    Reason: ChordInterpreter enforced "Gb" spelling (Flat) because input had 'b'.
    Fix: Changed ChordInterpreter to allow neutral preference, letting Romanizer apply diatonic F# (II).
    """
    symbol = "Caug/Gb"
    key = "E"
    
    parsed = ChordParser.parse(symbol)
    rom = Romanizer(default_tonic=key, simplify_accidentals=True)
    result = rom.annotate_progression([parsed])[0]
    
    # Expect Caug/F# -> bVIaug/II
    assert result.degree_root == "bVI"
    assert result.degree_bass == "II"
    assert "F#" in result.symbol_fixed
    assert "Gb" not in result.symbol_fixed


def test_issue_parser_double_accidentals():
    bbb7 = ChordParser.parse("Bbb7")
    assert bbb7 is not None
    assert bbb7.root == "Bbb"
    assert bbb7.quality == "7"

    f_sharp_sharp = ChordParser.parse("F##m7")
    assert f_sharp_sharp is not None
    assert f_sharp_sharp.root == "F##"
    assert f_sharp_sharp.quality == "m7"

    slash_double_flat = ChordParser.parse("C7/Bbb")
    assert slash_double_flat is not None
    assert slash_double_flat.root == "C"
    assert slash_double_flat.bass == "Bbb"

