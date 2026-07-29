//! Multi-axis interpretation of Blackadder sonorities.
//!
//! A Blackadder sonority is the pitch-class set `{0, 2, 6, 10}` measured from
//! its bass.  Those four notes do not determine one chord name or function:
//! the same sound can be heard as a dominant tension chord, a half-diminished
//! chord without its third, an augmented-seventh inversion, a whole-tone
//! fragment, or the temporary result of independent voices.
//!
//! Consequently this module never uses one large, mutually-exclusive enum.
//! Every candidate has three independent axes:
//!
//! - [`BlackadderStructure`]: which parent chord/scale explains the notes;
//! - [`BlackadderFunction`]: what the chord does in the current key/progression;
//! - [`BlackadderOrigin`]: how voice leading or orchestration produced it.
//!
//! Chord symbols can establish the pitch set and some root-motion functions.
//! Timing, voicing, and melody-dependent interpretations are emitted as weak
//! hypotheses with explicit [`BlackadderObservationKind`] requirements.  A
//! future MIDI/audio front end can satisfy those requirements without changing
//! either the public candidate schema or the k-best decoder.

use crate::analysis::{
    DominantRelation, HarmonicClassification, HarmonicRole, HarmonicSource, InterpretationFamily,
    ScoreEvidence, TonalMode, TonalPerspective, TonalScope,
};
use crate::domain::{NoteLetter, ParsedChord, PitchClass, QualityClass, SpelledNote};
use crate::profile::BehaviorProfile;
use crate::speller::{
    degree_from_spelling, name_of_pitch_class, semitone_distance, spell_pitch_class,
    target_accidental_preference,
};
use crate::structure;

/// The conventional spelling of the tritone in the bass-rooted dominant view.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum TritoneSpelling {
    SharpEleventh,
    FlatFifth,
    Ambiguous,
}

/// Pitch-class parity identifies one of the two whole-tone collections.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum WholeToneCollection {
    /// C-D-E-F#-G#-Bb and enharmonic equivalents.
    EvenPitchClasses,
    /// Db-Eb-F-G-A-B and enharmonic equivalents.
    OddPitchClasses,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
/// A chord-scale suggested by one structural reading.
pub enum BlackadderScale {
    LydianDominant,
    LocrianNaturalTwo,
    WholeTone(WholeToneCollection),
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
/// A pitch-set or parent-chord explanation, independent of harmonic function.
pub enum BlackadderStructure {
    /// The written augmented triad retained above an independent slash bass.
    AugmentedTriadOverBass,
    /// `7(9,#11)` or `7(9,b5)` with the third and fifth omitted.
    DominantNinthOmitThirdAndFifth { tritone_spelling: TritoneSpelling },
    /// `m7b5(add9)` with the minor third omitted.
    HalfDiminishedAddNineOmitThird,
    /// An `aug7` chord whose seventh is in the bass.
    AugmentedSeventhThirdInversion,
    /// An augmented-sixth spelling of the ten-semitone outer interval.
    AugmentedSixth,
    /// The four notes regarded as a fragment of a whole-tone collection.
    WholeToneSubset,
    /// Existing compatibility interpretation: a different dominant root is
    /// inferred and the written bass is that dominant's third.
    RootlessDominantThirdInBass,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
/// Progression-level role.  Several roles may compete for the same sonority.
pub enum BlackadderFunction {
    Dominant,
    SecondaryDominant,
    TritoneSubstitute,
    BackdoorDominant,
    SubdominantMinor,
    Predominant,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
/// A generative/voice-leading account, separate from chord function.
pub enum BlackadderOrigin {
    UpperStructureWithIndependentBass,
    SplitVoiceLeading,
    Incidental,
    ChordScaleSonority,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
/// Missing observations that a symbolic chord sequence cannot establish.
pub enum BlackadderObservationKind {
    VoiceLeading,
    Timing,
    MeterPosition,
    PartSeparation,
    MelodicScaleContext,
    AugmentedSixthResolution,
}

#[derive(Clone, Debug, Default, PartialEq)]
/// Normalized facts that a future MIDI/audio analyzer can provide.
///
/// These are derived observations rather than raw MIDI events.  This keeps the
/// harmony engine independent from one input format: a MIDI, MusicXML, or audio
/// adapter may all populate the same fields.
pub struct BlackadderObservations {
    pub split_voice_leading: Option<bool>,
    pub incidental_formation: Option<bool>,
    pub whole_tone_context: Option<bool>,
    pub augmented_sixth_resolution: Option<bool>,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
/// One factorized Blackadder hypothesis retained by local and k-best APIs.
pub struct BlackadderInterpretation {
    /// Canonical root of the `{0,2,6,10}` representation: always the bass.
    pub canonical_bass: SpelledNote,
    /// Root written before `aug` in the caller's original slash symbol.
    ///
    /// Augmented triads are symmetric, so this is observed notation rather
    /// than a separate harmonic hypothesis.
    pub written_upper_root: SpelledNote,
    /// Deterministic display orientation of the augmented upper structure.
    ///
    /// The member a tritone above the bass is used.  For example, every
    /// rotation of `{F#, G#/Ab, C, E}` is displayed canonically as `Caug/F#`
    /// while `written_upper_root` still preserves `G#`, `E`, or `C`.
    pub canonical_upper_root: SpelledNote,
    pub structure: BlackadderStructure,
    pub function: Option<BlackadderFunction>,
    pub origin: Option<BlackadderOrigin>,
    /// Root used when measuring functional motion.  This is normally the bass,
    /// but differs for aug7 and rootless-dominant readings.
    pub effective_root: Option<SpelledNote>,
    /// The following chord root when a textual progression establishes one.
    pub target_root: Option<SpelledNote>,
    pub scale: Option<BlackadderScale>,
    /// Common multi-axis view shared with ordinary (non-Blackadder)
    /// progression analysis. `function` remains above as a compatibility
    /// projection of these more precise fields.
    pub classification: HarmonicClassification,
    /// Empty means the available input supports the candidate.  A non-empty
    /// list means the interpretation remains possible but needs richer input.
    pub unresolved_observations: Vec<BlackadderObservationKind>,
}

#[derive(Clone, Copy, Debug, Default)]
/// Optional observations and symbolic context supplied to local analysis.
pub struct BlackadderContext<'a> {
    pub tonic: Option<SpelledNote>,
    pub previous_chord: Option<&'a ParsedChord>,
    pub next_chord: Option<&'a ParsedChord>,
    pub observations: Option<&'a BlackadderObservations>,
}

#[derive(Clone, Debug)]
pub(crate) struct ScoredBlackadder {
    pub interpretation: BlackadderInterpretation,
    pub alter: String,
    pub root_override: Option<SpelledNote>,
    pub bass_preference: Option<bool>,
    pub intrinsic_score: f64,
    pub evidence: Vec<ScoreEvidence>,
}

/// Return every supported reading of an augmented-over-bass Blackadder shape.
pub(crate) fn analyze_blackadder(
    chord: &ParsedChord,
    context: BlackadderContext<'_>,
    behavior: BehaviorProfile,
) -> Vec<ScoredBlackadder> {
    let Some(bass) = chord.bass else {
        return Vec::new();
    };
    if !has_exact_blackadder_shape(chord, bass) {
        return Vec::new();
    }

    let bass_preference = bass_preference_from_resolution(bass, context.next_chord);
    let canonical_bass = name_of_pitch_class(bass.pitch_class(), bass_preference);
    // The written augmented root is evidence, not a separate interpretation:
    // all three rotations have the same pitch classes.  Use the member a
    // tritone above the bass as the deterministic public shape.  Key spelling
    // controls only the note name chosen for that pitch class.
    let written_upper_root = chord.root;
    let canonical_upper_root = name_of_pitch_class(
        canonical_bass.pitch_class().offset(6),
        context.tonic.and_then(target_accidental_preference),
    );
    // The aug7-in-third-inversion identity is more specific: its root must be
    // the member a major second above the bass.  Keep that theoretical root
    // separate from the canonical augmented-triad display orientation.
    let augmented_seventh_root = spell_pitch_class(
        canonical_bass.letter.shift(1),
        canonical_bass.pitch_class().offset(2),
    );
    let root_override = (chord.root != canonical_upper_root).then_some(canonical_upper_root);
    // The public label uses the contextually respelled bass. Classify the
    // tritone against that same spelling so `Eaug/A# -> Eaug/Bb` also changes
    // `b5` to the theoretically consistent `#11`.
    let tritone_spelling = classify_tritone_spelling(chord, canonical_bass, behavior);
    let whole_tone = if canonical_bass.pitch_class().value() % 2 == 0 {
        WholeToneCollection::EvenPitchClasses
    } else {
        WholeToneCollection::OddPitchClasses
    };
    let repeated_augmented_upper = repeats_augmented_upper_structure(chord, context.previous_chord);
    let mut readings = Vec::new();

    // 1. Preserve the literal upper-structure view.  This is the least
    // committal interpretation and acts as a useful fallback when no function
    // is established by the following chord.
    readings.push(scored(
        interpretation(
            canonical_bass,
            written_upper_root,
            canonical_upper_root,
            BlackadderStructure::AugmentedTriadOverBass,
            None,
            Some(BlackadderOrigin::UpperStructureWithIndependentBass),
            Some(canonical_upper_root),
            None,
            None,
            Vec::new(),
        ),
        format!("{canonical_upper_root}aug/{canonical_bass}"),
        root_override,
        bass_preference,
        if repeated_augmented_upper { 1.25 } else { 0.25 },
        "builtin.blackadder.structure.upper_augmented",
        if repeated_augmented_upper {
            "Augmented upper structure is retained from the preceding augmented chord while the bass changes"
        } else {
            "Exact {0,2,6,10} sonority written as an augmented triad over an independent bass"
        },
    ));

    // 2. A bass-rooted dominant-tension structure is always algebraically
    // possible.  Its more specific function is inferred from root motion, but
    // the score for that motion is attached to a lattice edge, not here.
    let (dominant_function, target_root) =
        dominant_function_from_next(canonical_bass, context.tonic, context.next_chord, behavior);
    let tension = match tritone_spelling {
        TritoneSpelling::FlatFifth => "b5",
        TritoneSpelling::SharpEleventh | TritoneSpelling::Ambiguous => "#11",
    };
    readings.push(scored(
        interpretation(
            canonical_bass,
            written_upper_root,
            canonical_upper_root,
            BlackadderStructure::DominantNinthOmitThirdAndFifth { tritone_spelling },
            dominant_function,
            Some(BlackadderOrigin::ChordScaleSonority),
            Some(canonical_bass),
            target_root,
            Some(BlackadderScale::LydianDominant),
            Vec::new(),
        ),
        format!("{canonical_bass}7(9,{tension})"),
        root_override,
        bass_preference,
        0.0,
        "builtin.blackadder.structure.dominant_ninth",
        "Blackadder core read as a bass-rooted dominant ninth with omitted third and fifth",
    ));

    // 3. Replacing the absent minor third yields the half-diminished reading.
    // Its predominant role is only asserted when the next written chord is a
    // dominant; otherwise the structural possibility remains functionless.
    let halfdim_target = context
        .next_chord
        .filter(|next| structure::is_dominant_for(next, behavior))
        .map(|next| next.root);
    readings.push(scored(
        interpretation(
            canonical_bass,
            written_upper_root,
            canonical_upper_root,
            BlackadderStructure::HalfDiminishedAddNineOmitThird,
            halfdim_target.map(|_| BlackadderFunction::Predominant),
            None,
            Some(canonical_bass),
            halfdim_target,
            Some(BlackadderScale::LocrianNaturalTwo),
            Vec::new(),
        ),
        format!("{canonical_bass}m7-5(9)"),
        root_override,
        bass_preference,
        0.5,
        "builtin.blackadder.structure.half_diminished",
        "Blackadder core read as m7b5(add9) with the minor third omitted",
    ));

    // 4. For every Blackadder set, the note a major second over the bass can
    // be the root of aug7 and the bass can be its seventh.  Because this is a
    // mathematical identity rather than contextual evidence, its base score
    // is deliberately neutral.
    readings.push(scored(
        interpretation(
            canonical_bass,
            written_upper_root,
            canonical_upper_root,
            BlackadderStructure::AugmentedSeventhThirdInversion,
            None,
            None,
            Some(augmented_seventh_root),
            None,
            None,
            Vec::new(),
        ),
        format!("{augmented_seventh_root}aug7/{canonical_bass}"),
        root_override,
        bass_preference,
        0.0,
        "builtin.blackadder.structure.aug7_inversion",
        "Blackadder core read as an augmented-seventh chord in third inversion",
    ));

    // 5. Every exact set belongs to one whole-tone collection, so membership
    // alone is weak evidence.  Melody or neighboring voicings can later turn
    // this into a strong chord-scale interpretation.
    let mut whole_tone_requirements = vec![BlackadderObservationKind::MelodicScaleContext];
    let whole_tone_score = observation_score(
        context
            .observations
            .and_then(|observations| observations.whole_tone_context),
        &mut whole_tone_requirements,
        BlackadderObservationKind::MelodicScaleContext,
        -0.5,
        3.0,
        -2.0,
    );
    readings.push(scored(
        interpretation(
            canonical_bass,
            written_upper_root,
            canonical_upper_root,
            BlackadderStructure::WholeToneSubset,
            None,
            Some(BlackadderOrigin::ChordScaleSonority),
            None,
            None,
            Some(BlackadderScale::WholeTone(whole_tone)),
            whole_tone_requirements,
        ),
        format!("{canonical_bass}7(9,{tension})"),
        root_override,
        bass_preference,
        whole_tone_score,
        "builtin.blackadder.structure.whole_tone",
        "Blackadder core is a subset of one whole-tone collection",
    ));

    // 6. SDm is key-relative rather than a local chord shape.  A pitch-class
    // match to b6 creates a candidate; exact scale-degree spelling contributes
    // a small extra amount because it is stronger evidence than enharmonic
    // equivalence alone.
    if let Some(tonic) = context.tonic {
        if blackadder_pitch_classes(canonical_bass).contains(&tonic.pitch_class().offset(8)) {
            let target = context
                .next_chord
                .filter(|next| next.root.pitch_class() == tonic.pitch_class())
                .map(|next| next.root);
            let exact_flat_six = contains_spelled_flat_six(chord, bass, tonic, behavior);
            let score = if exact_flat_six { 1.0 } else { 0.35 };
            readings.push(scored(
                interpretation(
                    canonical_bass,
                    written_upper_root,
                    canonical_upper_root,
                    BlackadderStructure::AugmentedTriadOverBass,
                    Some(BlackadderFunction::SubdominantMinor),
                    Some(BlackadderOrigin::ChordScaleSonority),
                    Some(canonical_bass),
                    target,
                    None,
                    Vec::new(),
                ),
                format!("{canonical_bass}7(9,{tension})"),
                root_override,
                bass_preference,
                score,
                if exact_flat_six {
                    "builtin.blackadder.function.sdm.spelled_flat_six"
                } else {
                    "builtin.blackadder.function.sdm.enharmonic_flat_six"
                },
                "The sonority contains the current key's flat-six scale degree and supports an SDm reading",
            ));
        }
    }

    // 7. An augmented-sixth interpretation requires the ten-semitone member
    // to be written as a diatonic sixth above the bass.  Resolution behavior is
    // intentionally deferred to MIDI/voice-leading observations.
    if contains_spelled_augmented_sixth(chord, bass, behavior) {
        let mut requirements = vec![BlackadderObservationKind::AugmentedSixthResolution];
        let score = observation_score(
            context
                .observations
                .and_then(|observations| observations.augmented_sixth_resolution),
            &mut requirements,
            BlackadderObservationKind::AugmentedSixthResolution,
            0.0,
            3.5,
            -2.0,
        );
        readings.push(scored(
            interpretation(
                canonical_bass,
                written_upper_root,
                canonical_upper_root,
                BlackadderStructure::AugmentedSixth,
                Some(BlackadderFunction::Predominant),
                None,
                Some(canonical_bass),
                context.next_chord.map(|next| next.root),
                None,
                requirements,
            ),
            format!("{canonical_bass}(+6,9,#11)"),
            root_override,
            bass_preference,
            score,
            "builtin.blackadder.structure.augmented_sixth",
            "The ten-semitone member is spelled as an augmented sixth above the bass",
        ));
    }

    // 8/9. Text alone cannot distinguish independent voice leading from an
    // incidental verticality.  Keep both hypotheses at low rank and expose
    // exactly which observations would allow a future MIDI analyzer to decide.
    let split_flag = context
        .observations
        .and_then(|observations| observations.split_voice_leading);
    let mut split_requirements = vec![BlackadderObservationKind::VoiceLeading];
    let mut split_score = observation_score(
        split_flag,
        &mut split_requirements,
        BlackadderObservationKind::VoiceLeading,
        -1.0,
        4.0,
        -3.0,
    );
    if repeated_augmented_upper && split_flag.is_none() {
        // Caug -> Caug/F# is concrete symbolic evidence that the augmented
        // upper voices may have been retained while only the bass moved. It
        // cannot prove the actual voicing, but it should lift the linear
        // account above generic unresolved fallbacks.
        split_score = 3.0;
    }
    readings.push(scored(
        interpretation(
            canonical_bass,
            written_upper_root,
            canonical_upper_root,
            BlackadderStructure::AugmentedTriadOverBass,
            None,
            Some(BlackadderOrigin::SplitVoiceLeading),
            Some(canonical_upper_root),
            None,
            None,
            split_requirements,
        ),
        format!("{canonical_upper_root}aug/{canonical_bass}"),
        root_override,
        bass_preference,
        split_score,
        "builtin.blackadder.origin.split_voice_leading",
        "Upper structure and bass may be independent melodic lines",
    ));

    let incidental_flag = context
        .observations
        .and_then(|observations| observations.incidental_formation);
    let mut incidental_requirements = vec![
        BlackadderObservationKind::Timing,
        BlackadderObservationKind::MeterPosition,
        BlackadderObservationKind::PartSeparation,
    ];
    let incidental_score = match incidental_flag {
        Some(true) => {
            incidental_requirements.clear();
            4.0
        }
        Some(false) => {
            incidental_requirements.clear();
            -3.5
        }
        None => -1.5,
    };
    readings.push(scored(
        interpretation(
            canonical_bass,
            written_upper_root,
            canonical_upper_root,
            BlackadderStructure::AugmentedTriadOverBass,
            None,
            Some(BlackadderOrigin::Incidental),
            None,
            None,
            None,
            incidental_requirements,
        ),
        format!("{canonical_upper_root}aug/{canonical_bass}"),
        root_override,
        bass_preference,
        incidental_score,
        "builtin.blackadder.origin.incidental",
        "A short weak-beat sonority assembled across parts may be incidental",
    ));

    // 10. Preserve the pre-existing rootless dominant interpretation as an
    // additional candidate, but never let it replace the canonical bass-rooted
    // Blackadder analyses.  It is generated only when its inferred root really
    // resolves to the following tonic-like chord.
    if let Some(next) = context.next_chord {
        let anchor_pc = canonical_bass.pitch_class().offset(6);
        let anchor = name_of_pitch_class(anchor_pc, bass_preference);
        let dominant_pc = anchor_pc.offset(2);
        let dominant = spell_pitch_class(anchor.letter.shift(1), dominant_pc);
        let distance = semitone_distance(next.root, dominant);
        if matches!(distance, 5 | 7) && structure::is_tonic_for(next, behavior) {
            readings.push(scored(
                interpretation(
                    canonical_bass,
                    written_upper_root,
                    canonical_upper_root,
                    BlackadderStructure::RootlessDominantThirdInBass,
                    Some(BlackadderFunction::SecondaryDominant),
                    None,
                    Some(dominant),
                    Some(next.root),
                    Some(BlackadderScale::LydianDominant),
                    Vec::new(),
                ),
                format!("{dominant}7(9,#11)/{canonical_bass}"),
                root_override,
                bass_preference,
                0.0,
                "builtin.blackadder.structure.rootless_dominant_third_in_bass",
                "The same notes support a rootless dominant whose third is the written bass",
            ));
        }
    }

    classify_readings(&mut readings, context);
    if repeated_augmented_upper {
        for reading in &mut readings {
            if matches!(
                reading.interpretation.function,
                Some(
                    BlackadderFunction::Dominant
                        | BlackadderFunction::SecondaryDominant
                        | BlackadderFunction::TritoneSubstitute
                        | BlackadderFunction::BackdoorDominant
                )
            ) {
                // The functional reading remains available, but callers can
                // see that chord symbols alone do not settle whether the bass
                // or the retained augmented upper structure governs hearing.
                reading
                    .interpretation
                    .classification
                    .add_family(InterpretationFamily::VoiceLeadingRequired);
            }
        }
    }

    // A negative prior must not turn an observation-dependent explanation
    // into a practical impossibility.  When chord-symbol context establishes
    // no conventional function at all, let the three unresolved fallback
    // families rise just above unsupported neutral labels.  Their scores stay
    // tiny: this changes relative rank, not confidence.  Positive/negative
    // MIDI observations and any supported functional reading bypass this
    // fallback and retain their ordinary weights.
    rank_unresolved_fallbacks(&mut readings);

    readings
}

/// Candidate-specific transition evidence used by both 1-best metadata and the
/// k-best lattice.  Keeping it here prevents display code from reimplementing
/// music-theory conditions.
pub(crate) fn transition_evidence(
    interpretation: &BlackadderInterpretation,
    next: &ParsedChord,
    tonic: SpelledNote,
    behavior: BehaviorProfile,
) -> Vec<ScoreEvidence> {
    if interpretation
        .target_root
        .is_some_and(|target| target.pitch_class() != next.root.pitch_class())
    {
        return Vec::new();
    }

    let (rule_id, score, explanation) = match interpretation.function {
        Some(BlackadderFunction::Predominant)
            if interpretation.structure == BlackadderStructure::AugmentedSixth
                && semitone_distance(next.root, interpretation.canonical_bass) == 11
                && structure::is_tonic_for(next, behavior) =>
        {
            (
                "builtin.blackadder.transition.augmented_sixth",
                5.7,
                "Spelled augmented-sixth outer notes converge by semitone on the following harmony",
            )
        }
        Some(BlackadderFunction::TritoneSubstitute)
            if semitone_distance(next.root, interpretation.canonical_bass) == 11
                && structure::is_tonic_for(next, behavior) =>
        {
            (
                "builtin.blackadder.transition.tritone_substitute",
                6.0,
                "Bass-rooted altered dominant resolves to a target one semitone below",
            )
        }
        Some(BlackadderFunction::Dominant | BlackadderFunction::SecondaryDominant)
            if interpretation
                .effective_root
                .is_some_and(|root| semitone_distance(next.root, root) == 5)
                && structure::is_tonic_for(next, behavior) =>
        {
            (
                "builtin.blackadder.transition.dominant",
                6.0,
                "Dominant interpretation resolves upward by perfect fourth",
            )
        }
        Some(BlackadderFunction::BackdoorDominant)
            if semitone_distance(next.root, interpretation.canonical_bass) == 2
                && structure::is_tonic_for(next, behavior) =>
        {
            (
                "builtin.blackadder.transition.backdoor",
                5.0,
                "Backdoor dominant interpretation resolves upward by whole tone",
            )
        }
        Some(BlackadderFunction::Predominant)
            if interpretation.structure == BlackadderStructure::HalfDiminishedAddNineOmitThird
                && structure::is_dominant_for(next, behavior) =>
        {
            (
                "builtin.blackadder.transition.halfdim_to_dominant",
                5.5,
                "Half-diminished interpretation precedes a dominant chord",
            )
        }
        Some(BlackadderFunction::SubdominantMinor)
            if next.root.pitch_class() == tonic.pitch_class() =>
        {
            (
                "builtin.blackadder.transition.sdm_to_tonic",
                4.5,
                "Subdominant-minor interpretation resolves to the current tonic",
            )
        }
        _ => return Vec::new(),
    };
    vec![ScoreEvidence::new(rule_id, score, explanation)]
}

pub(crate) fn transition_score(
    interpretation: &BlackadderInterpretation,
    next: Option<&ParsedChord>,
    tonic: Option<SpelledNote>,
    behavior: BehaviorProfile,
) -> f64 {
    let Some(next) = next else {
        return 0.0;
    };
    transition_evidence(
        interpretation,
        next,
        tonic.unwrap_or(interpretation.canonical_bass),
        behavior,
    )
    .into_iter()
    .map(|evidence| evidence.contribution)
    .sum()
}

pub(crate) fn has_exact_blackadder_shape(chord: &ParsedChord, bass: SpelledNote) -> bool {
    let relative: std::collections::HashSet<u8> = structure::aug_triad_pitch_classes(chord.root)
        .into_iter()
        .map(|pitch| pitch.distance_from(bass.pitch_class()))
        .collect();
    relative == std::collections::HashSet::from([2, 6, 10])
}

fn repeats_augmented_upper_structure(
    chord: &ParsedChord,
    previous_chord: Option<&ParsedChord>,
) -> bool {
    let Some(previous) = previous_chord else {
        return false;
    };
    if previous.quality_parsed.class != QualityClass::Augmented {
        return false;
    }
    structure::aug_triad_pitch_classes(previous.root)
        == structure::aug_triad_pitch_classes(chord.root)
}

fn blackadder_pitch_classes(bass: SpelledNote) -> [PitchClass; 4] {
    let root = bass.pitch_class();
    [root, root.offset(2), root.offset(6), root.offset(10)]
}

fn dominant_function_from_next(
    bass: SpelledNote,
    tonic: Option<SpelledNote>,
    next: Option<&ParsedChord>,
    behavior: BehaviorProfile,
) -> (Option<BlackadderFunction>, Option<SpelledNote>) {
    let Some(next) = next else {
        return (None, None);
    };
    let distance = semitone_distance(next.root, bass);
    let function = match distance {
        11 if structure::is_tonic_for(next, behavior) => {
            Some(BlackadderFunction::TritoneSubstitute)
        }
        5 if structure::is_tonic_for(next, behavior) => {
            if tonic.is_some_and(|tonic| tonic.pitch_class() == next.root.pitch_class()) {
                Some(BlackadderFunction::Dominant)
            } else {
                Some(BlackadderFunction::SecondaryDominant)
            }
        }
        2 if structure::is_tonic_for(next, behavior) => Some(BlackadderFunction::BackdoorDominant),
        _ => None,
    };
    (function, function.map(|_| next.root))
}

fn classify_tritone_spelling(
    chord: &ParsedChord,
    bass: SpelledNote,
    behavior: BehaviorProfile,
) -> TritoneSpelling {
    let Some(note) = spelled_upper_tones(chord, behavior)
        .into_iter()
        .find(|note| semitone_distance(*note, bass) == 6)
    else {
        return TritoneSpelling::Ambiguous;
    };
    match diatonic_distance(note.letter, bass.letter) {
        3 => TritoneSpelling::SharpEleventh,
        4 => TritoneSpelling::FlatFifth,
        _ => TritoneSpelling::Ambiguous,
    }
}

fn contains_spelled_augmented_sixth(
    chord: &ParsedChord,
    bass: SpelledNote,
    behavior: BehaviorProfile,
) -> bool {
    spelled_upper_tones(chord, behavior)
        .into_iter()
        .any(|note| {
            semitone_distance(note, bass) == 10 && diatonic_distance(note.letter, bass.letter) == 5
        })
}

fn contains_spelled_flat_six(
    chord: &ParsedChord,
    bass: SpelledNote,
    tonic: SpelledNote,
    behavior: BehaviorProfile,
) -> bool {
    let flat_six_pc = tonic.pitch_class().offset(8);
    let flat_six_letter = tonic.letter.shift(5);
    std::iter::once(bass)
        .chain(spelled_upper_tones(chord, behavior))
        .any(|note| note.pitch_class() == flat_six_pc && note.letter == flat_six_letter)
}

fn spelled_upper_tones(chord: &ParsedChord, behavior: BehaviorProfile) -> Vec<SpelledNote> {
    structure::spelled_tones_for(chord, chord.root, behavior)
        .into_values()
        .collect()
}

fn diatonic_distance(note: NoteLetter, reference: NoteLetter) -> usize {
    (note.index() + NoteLetter::ALL.len() - reference.index()) % NoteLetter::ALL.len()
}

fn bass_preference_from_resolution(
    bass: SpelledNote,
    next_chord: Option<&ParsedChord>,
) -> Option<bool> {
    match semitone_distance(next_chord?.root, bass) {
        1 => Some(true),
        11 => Some(false),
        _ => None,
    }
}

#[allow(clippy::too_many_arguments)]
fn interpretation(
    canonical_bass: SpelledNote,
    written_upper_root: SpelledNote,
    canonical_upper_root: SpelledNote,
    structure: BlackadderStructure,
    function: Option<BlackadderFunction>,
    origin: Option<BlackadderOrigin>,
    effective_root: Option<SpelledNote>,
    target_root: Option<SpelledNote>,
    scale: Option<BlackadderScale>,
    unresolved_observations: Vec<BlackadderObservationKind>,
) -> BlackadderInterpretation {
    BlackadderInterpretation {
        canonical_bass,
        written_upper_root,
        canonical_upper_root,
        structure,
        function,
        origin,
        effective_root,
        target_root,
        scale,
        classification: HarmonicClassification::default(),
        unresolved_observations,
    }
}

fn classify_readings(readings: &mut [ScoredBlackadder], context: BlackadderContext<'_>) {
    for reading in readings {
        let interpretation = &mut reading.interpretation;
        let mut classification = match interpretation.function {
            Some(
                BlackadderFunction::Dominant
                | BlackadderFunction::SecondaryDominant
                | BlackadderFunction::TritoneSubstitute
                | BlackadderFunction::BackdoorDominant,
            ) => HarmonicClassification::with_role(HarmonicRole::Dominant),
            Some(BlackadderFunction::Predominant) => {
                HarmonicClassification::with_role(HarmonicRole::Predominant)
            }
            Some(BlackadderFunction::SubdominantMinor) => {
                HarmonicClassification::with_role(HarmonicRole::Subdominant)
            }
            None if matches!(
                interpretation.origin,
                Some(BlackadderOrigin::SplitVoiceLeading | BlackadderOrigin::Incidental)
            ) || interpretation.structure == BlackadderStructure::WholeToneSubset =>
            {
                HarmonicClassification::with_role(HarmonicRole::NonFunctional)
            }
            None => HarmonicClassification::default(),
        };

        match interpretation.function {
            Some(BlackadderFunction::Dominant | BlackadderFunction::SecondaryDominant) => {
                classification.dominant_relation = Some(DominantRelation::FifthRelated);
            }
            Some(BlackadderFunction::TritoneSubstitute) => {
                classification.dominant_relation = Some(DominantRelation::TritoneSubstitute);
                classification.add_family(InterpretationFamily::TritoneSubstitute);
            }
            Some(BlackadderFunction::BackdoorDominant) => {
                classification.dominant_relation = Some(DominantRelation::Backdoor);
                classification.add_family(InterpretationFamily::Backdoor);
                // Backdoor motion and SDm borrowing answer different
                // questions. When the same pitch set contains the current
                // key's flat sixth, retain both facts on one candidate while
                // still keeping the alternative SDm-role candidate.
                if context.tonic.is_some_and(|tonic| {
                    blackadder_pitch_classes(interpretation.canonical_bass)
                        .contains(&tonic.pitch_class().offset(8))
                }) {
                    classification.add_source(HarmonicSource::SubdominantMinor);
                    classification.add_family(InterpretationFamily::SubdominantMinor);
                }
            }
            Some(BlackadderFunction::SubdominantMinor) => {
                classification.add_source(HarmonicSource::SubdominantMinor);
                classification.add_family(InterpretationFamily::SubdominantMinor);
            }
            Some(BlackadderFunction::Predominant) | None => {}
        }

        match interpretation.scale {
            Some(BlackadderScale::LydianDominant) => {
                classification.add_source(HarmonicSource::LydianDominant);
            }
            Some(BlackadderScale::LocrianNaturalTwo) => {
                classification.add_source(HarmonicSource::LocrianNaturalTwo);
            }
            Some(BlackadderScale::WholeTone(_)) => {
                classification.add_source(HarmonicSource::WholeTone);
                classification.add_family(InterpretationFamily::WholeTone);
            }
            None => {}
        }
        if interpretation.structure == BlackadderStructure::AugmentedSixth {
            classification.add_family(InterpretationFamily::AugmentedSixth);
        }
        match interpretation.origin {
            Some(BlackadderOrigin::SplitVoiceLeading) => {
                classification.add_family(InterpretationFamily::SplitVoiceLeading);
            }
            Some(BlackadderOrigin::Incidental) => {
                classification.add_family(InterpretationFamily::Incidental);
            }
            Some(
                BlackadderOrigin::UpperStructureWithIndependentBass
                | BlackadderOrigin::ChordScaleSonority,
            )
            | None => {}
        }

        // Dominants are heard from the key of the chord they target. Other
        // key-relative roles (SDm and predominant) use the caller's current
        // tonic because their immediate target may be V rather than I.
        if let Some(global_tonic) = context.tonic {
            let local_tonic = match classification.role {
                Some(HarmonicRole::Dominant) => interpretation.target_root,
                Some(HarmonicRole::Predominant | HarmonicRole::Subdominant) => Some(global_tonic),
                Some(HarmonicRole::Tonic | HarmonicRole::NonFunctional) | None => None,
            };
            if let Some(local_tonic) = local_tonic {
                let target_chord = context
                    .next_chord
                    .filter(|next| next.root.pitch_class() == local_tonic.pitch_class());
                classification.perspective = Some(tonal_perspective(
                    global_tonic,
                    local_tonic,
                    target_chord.map_or(TonalMode::Unknown, tonal_mode),
                ));
            }
        }
        interpretation.classification = classification;
    }
}

fn tonal_perspective(
    global_tonic: SpelledNote,
    local_tonic: SpelledNote,
    mode: TonalMode,
) -> TonalPerspective {
    TonalPerspective {
        global_tonic,
        local_tonic,
        local_tonic_degree: degree_from_spelling(local_tonic, global_tonic),
        scope: if local_tonic.pitch_class() == global_tonic.pitch_class() {
            TonalScope::Global
        } else {
            TonalScope::Tonicization
        },
        mode,
    }
}

fn tonal_mode(chord: &ParsedChord) -> TonalMode {
    match chord.quality_parsed.class {
        QualityClass::Major => TonalMode::Major,
        QualityClass::Minor | QualityClass::Diminished | QualityClass::HalfDiminished => {
            TonalMode::Minor
        }
        QualityClass::Augmented
        | QualityClass::Suspended2
        | QualityClass::Suspended4
        | QualityClass::Power
        | QualityClass::Unknown => TonalMode::Unknown,
    }
}

#[allow(clippy::too_many_arguments)]
fn scored(
    interpretation: BlackadderInterpretation,
    alter: String,
    root_override: Option<SpelledNote>,
    bass_preference: Option<bool>,
    intrinsic_score: f64,
    rule_id: &str,
    explanation: &str,
) -> ScoredBlackadder {
    ScoredBlackadder {
        interpretation,
        alter,
        root_override,
        bass_preference,
        intrinsic_score,
        evidence: vec![ScoreEvidence::new(rule_id, intrinsic_score, explanation)],
    }
}

fn observation_score(
    flag: Option<bool>,
    requirements: &mut Vec<BlackadderObservationKind>,
    requirement: BlackadderObservationKind,
    unknown_score: f64,
    true_score: f64,
    false_score: f64,
) -> f64 {
    match flag {
        Some(true) => {
            requirements.retain(|candidate| *candidate != requirement);
            true_score
        }
        Some(false) => {
            // A negative observation resolves the question too: the candidate
            // remains visible for auditability, but is contradicted rather
            // than awaiting more data.
            requirements.retain(|candidate| *candidate != requirement);
            false_score
        }
        None => unknown_score,
    }
}

/// Raise unresolved observation-dependent hypotheses only when the symbolic
/// progression provides no conventional functional explanation.
///
/// These are deliberately small *ranking floors*, not evidence that whole-tone
/// context, split voice leading, or an incidental verticality was observed.
/// The unresolved-observation list remains intact so API users can distinguish
/// "worth showing" from "confirmed".  Conversely, an explicit `false`
/// observation clears that list and therefore can never receive this floor.
fn rank_unresolved_fallbacks(readings: &mut [ScoredBlackadder]) {
    let has_supported_function = readings
        .iter()
        .any(|reading| reading.interpretation.function.is_some());
    if has_supported_function {
        return;
    }

    for reading in readings {
        let interpretation = &reading.interpretation;
        let fallback_score = if interpretation.structure == BlackadderStructure::WholeToneSubset
            && interpretation
                .unresolved_observations
                .contains(&BlackadderObservationKind::MelodicScaleContext)
        {
            Some(0.20)
        } else if interpretation.origin == Some(BlackadderOrigin::SplitVoiceLeading)
            && interpretation
                .unresolved_observations
                .contains(&BlackadderObservationKind::VoiceLeading)
        {
            Some(0.10)
        } else if interpretation.origin == Some(BlackadderOrigin::Incidental)
            && !interpretation.unresolved_observations.is_empty()
        {
            Some(0.05)
        } else {
            None
        };

        let Some(fallback_score) = fallback_score else {
            continue;
        };
        reading.intrinsic_score = fallback_score;
        if let Some(evidence) = reading.evidence.first_mut() {
            // `ScoreEvidence` explains the final emission contribution.  Keep
            // the stable family rule id, but make clear that this is a
            // low-confidence fallback caused by the absence of a better
            // chord-symbol explanation, not a positive observation.
            evidence.contribution = fallback_score;
            evidence.explanation.push_str(
                "; retained as a low-confidence fallback because no conventional function is established",
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ParsedSymbol;
    use crate::parser::parse_chord;

    fn chord(symbol: &str) -> ParsedChord {
        match parse_chord(symbol).unwrap() {
            ParsedSymbol::Chord(chord) => chord,
            ParsedSymbol::NoChord { .. } | ParsedSymbol::Boundary { .. } => {
                panic!("expected chord")
            }
        }
    }

    #[test]
    fn exact_shape_produces_independent_structural_readings() {
        let input = chord("Daug/C");
        let readings = analyze_blackadder(
            &input,
            BlackadderContext::default(),
            BehaviorProfile::StrictV1,
        );
        assert!(readings.iter().any(|reading| {
            matches!(
                reading.interpretation.structure,
                BlackadderStructure::DominantNinthOmitThirdAndFifth { .. }
            )
        }));
        assert!(readings.iter().any(|reading| {
            reading.interpretation.structure == BlackadderStructure::HalfDiminishedAddNineOmitThird
        }));
        assert!(readings.iter().any(|reading| {
            reading.interpretation.structure == BlackadderStructure::AugmentedSeventhThirdInversion
        }));
        assert!(readings.iter().any(|reading| {
            reading.interpretation.structure == BlackadderStructure::WholeToneSubset
        }));
    }

    #[test]
    fn midi_style_observation_satisfies_deferred_requirement() {
        let input = chord("Daug/C");
        let observations = BlackadderObservations {
            split_voice_leading: Some(true),
            ..BlackadderObservations::default()
        };
        let readings = analyze_blackadder(
            &input,
            BlackadderContext {
                observations: Some(&observations),
                ..BlackadderContext::default()
            },
            BehaviorProfile::StrictV1,
        );
        let split = readings
            .iter()
            .find(|reading| {
                reading.interpretation.origin == Some(BlackadderOrigin::SplitVoiceLeading)
            })
            .unwrap();
        assert!(split.interpretation.unresolved_observations.is_empty());
        assert_eq!(split.intrinsic_score, 4.0);
    }

    #[test]
    fn unresolved_families_rise_only_when_no_function_is_supported() {
        // Bbaug/C has the exact Blackadder pitch set but, unlike Daug/C, does
        // not spell the ten-semitone member as an augmented sixth. With no
        // following chord or key-relative function, the three weak families
        // should be visible near the top while remaining low-confidence.
        let input = chord("Bbaug/C");
        let readings = analyze_blackadder(
            &input,
            BlackadderContext::default(),
            BehaviorProfile::StrictV1,
        );
        let score_for = |predicate: &dyn Fn(&BlackadderInterpretation) -> bool| {
            readings
                .iter()
                .find(|reading| predicate(&reading.interpretation))
                .unwrap()
                .intrinsic_score
        };

        assert_eq!(
            score_for(&|reading| reading.structure == BlackadderStructure::WholeToneSubset),
            0.20
        );
        assert_eq!(
            score_for(&|reading| reading.origin == Some(BlackadderOrigin::SplitVoiceLeading)),
            0.10
        );
        assert_eq!(
            score_for(&|reading| reading.origin == Some(BlackadderOrigin::Incidental)),
            0.05
        );

        // Once the following chord establishes a tritone-substitute function,
        // unknown observations return to their deliberately weak priors.
        let target = chord("B");
        let functional = analyze_blackadder(
            &input,
            BlackadderContext {
                tonic: Some(SpelledNote::parse("B").unwrap()),
                next_chord: Some(&target),
                ..BlackadderContext::default()
            },
            BehaviorProfile::StrictV1,
        );
        assert!(functional.iter().any(|reading| {
            reading.interpretation.function == Some(BlackadderFunction::TritoneSubstitute)
        }));
        assert!(functional.iter().any(|reading| {
            reading.interpretation.structure == BlackadderStructure::WholeToneSubset
                && reading.intrinsic_score == -0.5
        }));
        assert!(functional.iter().any(|reading| {
            reading.interpretation.origin == Some(BlackadderOrigin::SplitVoiceLeading)
                && reading.intrinsic_score == -1.0
        }));
        assert!(functional.iter().any(|reading| {
            reading.interpretation.origin == Some(BlackadderOrigin::Incidental)
                && reading.intrinsic_score == -1.5
        }));
    }
}
