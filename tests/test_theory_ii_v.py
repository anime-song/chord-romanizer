
import pytest
from chord_romanizer.romanizer import Romanizer, RomanizedChord
from chord_romanizer.chord_parser import ParsedChord

@pytest.fixture
def romanizer():
    return Romanizer("C")

def make_chord(root, quality, bass=None):
    return ParsedChord(symbol=f"{root}{quality}", root=root, quality=quality, bass=bass)

def test_standard_ii_v_i(romanizer):
    # Dm7 -> G7 -> Cmaj7
    chords = [
        make_chord("D", "m7"),
        make_chord("G", "7"),
        make_chord("C", "maj7")
    ]
    results = romanizer.annotate_progression(chords)
    
    # Check Dm7 (II)
    assert results[0].degree_root == "II"
    assert results[0].is_ii_v_start == True
    
    # Check G7 (V) -> Resolution
    assert results[1].degree_root == "V"
    assert results[1].is_ii_v_start == False
    
    # Check Cmaj7 (I) -> Target
    assert results[2].degree_root == "I"
    assert results[2].is_resolution_target == True
    assert results[2].resolution_type == "perfect"

def test_hybrid_ii_v(romanizer):
    # Em7 -> G/A -> Dmaj7 (Key of D major)
    # We test it in Key of C first to see strict relative analysis, 
    # but let's change romanizer context to D for clarity if we want
    
    # Let's stick to C major context but check relative detection
    # Em7 = IIIm7
    # G/A = A9sus4 = VI dominant
    # Dmaj7 = IImaj7
    # So Em7 -> G/A is a local ii-V to D (II).
    
    chords = [
        make_chord("E", "m7"),
        make_chord("G", "maj", bass="A"), # G/A
        make_chord("D", "maj7")
    ]
    
    # Temporarily force next chord analysis logic to see G/A as Dominant using logic in note_speller/interpreter?
    # Wait, simple G/A is SUS4_9 logic.
    
    results = romanizer.annotate_progression(chords)
    
    # Em7 (II relative to D)
    # Root E to Root A (from G/A) is 5 semitones.
    # Em7 is Minor. G/A (Effective A) is Dominant?
    
    # Check G/A analysis
    # G (root) vs A (bass) -> dist 2 semitones.
    # Structure G triad: G, B, D.
    # A to G=10, A to B=2, A to D=5. Intervals {2, 5, 10} -> SUS4_9 -> True.
    # So G/A is treated as A dominant.
    
    # Em7 -> A dominant. E -> A is 5 semitones. 
    # So Em7 should be marked is_ii_v_start = True.
    assert results[0].is_ii_v_start == True
    
    # A dominant -> Dmaj7. A -> D is 5 semitones.
    # So Dmaj7 should be resolution target.
    assert results[2].is_resolution_target == True
    assert results[2].resolution_type == "perfect"

def test_tritone_sub(romanizer):
    # Dm7 -> Db7 -> Cmaj7
    chords = [
        make_chord("D", "m7"),
        make_chord("Db", "7"),
        make_chord("C", "maj7")
    ]
    results = romanizer.annotate_progression(chords)
    
    # Dm7 -> Db7. D->Db is 11 (down 1). Not 5. So NOT II-V bracket.
    assert results[0].is_ii_v_start == False
    
    # Db7 -> Cmaj7. Db->C is 11 (down 1).
    # Db7 is Dominant.
    # So Cmaj7 should be resolution target (semitone).
    assert results[2].is_resolution_target == True
    assert results[2].resolution_type == "semitone"

def test_flat_key_ii_v(romanizer):
    # Fm7 -> Bb7 -> Eb (Key of Eb major, analyzed in C context)
    # Eb is bIII. Fm is IVm. Bb is bVII.
    # Current logic checks Resolution Target (Eb).
    # Eb has 'b', so prefers flats.
    # Fm7 -> IVm (Natural). Bb7 -> bVII7 (Flat preferred). 
    # If logic forced sharps, Bb7 might become A#7 (#VI7).
    
    chords = [
        make_chord("F", "m7"),
        make_chord("Bb", "7"),
        make_chord("Eb", "maj7")
    ]
    results = romanizer.annotate_progression(chords)
    
    # Check Fm7 (II)
    # F is natural, so II vs #I etc doesn't matter much unless key is weird.
    # But let's check basic flagging
    assert results[0].is_ii_v_start == True
    
    # Check Bb7 (V)
    # Should be "bVII7" (Bb), NOT "#VI7" (A#)
    assert results[1].roman.startswith("bVII") 
    assert "A#" not in results[1].roman
    
    # Check Ebmaj7 (I)
    assert results[2].is_resolution_target == True
