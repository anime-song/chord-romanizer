//! Candidate generation for ordinary chromatic harmony.
//!
//! This module deliberately recognizes *explanations*, not exotic chord
//! spellings.  One observed chord may therefore produce several candidates:
//! E-flat major in C can be parallel-minor mixture, a chromatic mediant, or a
//! temporary E-flat tonic supported elsewhere in the progression.  The
//! lattice ranks those meanings later instead of collapsing them here.
//!
//! The rules are intentionally local and conservative.  They use the caller's
//! key plus immediate semantic neighbours, but do not claim a modulation from
//! one unusual chord.  A future key-state model can add longer-span evidence
//! without changing these candidate types.

use crate::analysis::{
    DominantRelation, HarmonicClassification, HarmonicInterpretation, HarmonicRole, HarmonicSource,
    InterpretationFamily, TonalMode, TonalPerspective, TonalScope,
};
use crate::domain::{QualityClass, SeventhQuality, SpelledNote};
use crate::speller::{degree_from_spelling, semitone_distance, spell_pitch_class};

/// Minimal, spelling-preserving information needed by the generic rules.
/// Keeping this independent of `context::AnalysisNode` prevents the reusable
/// theory pass from depending on deterministic display/spelling flags.
#[derive(Clone, Copy, Debug)]
pub(crate) struct HarmonyObservation {
    pub root: SpelledNote,
    pub tonic: SpelledNote,
    pub global_mode: TonalMode,
    pub quality: QualityClass,
    pub seventh: Option<SeventhQuality>,
    pub is_dominant: bool,
}

/// Produce zero or more mutually compatible/competing meanings per event.
/// Marker events remain `None` so the returned vector stays input-aligned.
pub(crate) fn infer_ordinary_interpretations(
    observations: &[Option<HarmonyObservation>],
    previous_chord: &[Option<usize>],
    next_chord: &[Option<usize>],
) -> Vec<Vec<HarmonicInterpretation>> {
    let mut output = vec![Vec::new(); observations.len()];

    for index in 0..observations.len() {
        let Some(current) = observations[index] else {
            continue;
        };
        let previous = previous_chord[index].and_then(|at| observations[at]);
        let next = next_chord[index].and_then(|at| observations[at]);

        // Fully diminished sevenths are symmetric: the same four pitch
        // classes can be named from four different roots.  These rules must
        // therefore run before spelling-oriented chromatic rules and compare
        // the complete diminished collection rather than only the written
        // root.
        add_diminished_interpretations(&mut output[index], current, previous, next);
        add_modal_interchange(&mut output[index], current, previous, next);
        add_flat_seven_subdominant_minor(&mut output[index], current, previous, next);
        add_neapolitan(&mut output[index], current, next);
        add_chromatic_mediant(&mut output[index], current, previous, next);
        add_half_diminished_common_tone_neighbor(&mut output[index], current, next);
        add_chromatic_approach(&mut output[index], current, next);
    }

    output
}

fn add_diminished_interpretations(
    output: &mut Vec<HarmonicInterpretation>,
    current: HarmonyObservation,
    previous: Option<HarmonyObservation>,
    next: Option<HarmonyObservation>,
) {
    // A diminished triad and a half-diminished seventh are not rotationally
    // symmetric.  Restrict the following equivalence rules to the complete
    // four-note diminished-seventh set {0, 3, 6, 9}.
    if !is_fully_diminished_seventh(current) {
        return;
    }

    if let Some(next) = next {
        add_rootless_dominant_readings(output, current, next);
        add_tonic_substitute_reading(output, current, next);
    }

    let (Some(previous), Some(next)) = (previous, next) else {
        return;
    };
    add_passing_diminished_reading(output, current, previous, next);
    add_common_tone_diminished_reading(output, current, previous, next);
}

fn add_rootless_dominant_readings(
    output: &mut Vec<HarmonicInterpretation>,
    current: HarmonyObservation,
    next: HarmonyObservation,
) {
    let next_distance = semitone_distance(next.root, current.root);

    // V7(b9) without its root leaves scale degrees 3, 5, b7 and b9: a fully
    // diminished seventh.  Because the remaining set is symmetric, the
    // written diminished root may be any of its four tones.  Consequently
    // bIII°7 -> V can still be the same sounding collection as #IV°7 -> V.
    if matches!(next_distance, 1 | 4 | 7 | 10) {
        let dominant_root = dominant_root_for_target(next.root);
        let mut classification = local_classification(
            current,
            HarmonicRole::Dominant,
            next.root,
            degree_from_spelling(dominant_root, next.root),
            observation_mode(next),
        );
        classification.dominant_relation = Some(DominantRelation::FifthRelated);
        classification.add_family(InterpretationFamily::RootlessDominantNinth);
        push_unique(
            output,
            HarmonicInterpretation::new(
                "builtin.ordinary.diminished.rootless_dominant_to_target",
                1.7,
                "Fully diminished seventh matches a rootless V7(b9) resolving to the next chord",
                classification,
            ),
        );
    }

    // Sometimes the omitted dominant root itself appears next instead of the
    // expected resolution target.  I°7 -> IIm in a major key is the familiar
    // example: I°7 is enharmonically the upper structure of II7(b9), while
    // the following IIm restores that absent II root with a changed quality.
    //
    // This is weaker than an actual V7(b9)-to-I resolution, but retaining it
    // as a separate candidate lets longer context (or future MIDI evidence)
    // decide whether a double-dominant hearing is convincing.
    if matches!(next_distance, 2 | 5 | 8 | 11) {
        let local_tonic = target_of_dominant(next.root);
        let mut classification = local_classification(
            current,
            HarmonicRole::Dominant,
            local_tonic,
            degree_from_spelling(next.root, local_tonic),
            TonalMode::Unknown,
        );
        classification.dominant_relation = Some(DominantRelation::FifthRelated);
        classification.add_family(InterpretationFamily::RootlessDominantNinth);
        push_unique(
            output,
            HarmonicInterpretation::new(
                "builtin.ordinary.diminished.rootless_dominant_root_restored",
                1.05,
                "Fully diminished seventh can be a rootless V7(b9) whose omitted root appears next",
                classification,
            ),
        );
    }
}

fn add_passing_diminished_reading(
    output: &mut Vec<HarmonicInterpretation>,
    current: HarmonyObservation,
    previous: HarmonyObservation,
    next: HarmonyObservation,
) {
    let into_current = semitone_distance(current.root, previous.root);
    let out_of_current = semitone_distance(next.root, current.root);
    let root_line_is_chromatic =
        (into_current == 1 && out_of_current == 1) || (into_current == 11 && out_of_current == 11);

    // A spelling such as I - I°7 - IIm does not expose the descending
    // chromatic line in the chord roots.  Nevertheless a fully diminished
    // seventh contains four possible roots, so it can encode a passing inner
    // voice (for example 3 -> b3 -> 2).  Keep that reading, but score it below
    // the case whose written roots themselves form the chromatic line.
    let stationary_written_root = previous.root.pitch_class() == current.root.pitch_class()
        && matches!(semitone_distance(next.root, previous.root), 2 | 10);
    if !root_line_is_chromatic && !stationary_written_root {
        return;
    }

    let mut classification = global_classification(current, HarmonicRole::NonFunctional);
    classification.add_source(HarmonicSource::Chromatic);
    classification.add_family(InterpretationFamily::PassingDiminished);
    if stationary_written_root {
        // This flag is intentionally an analytical caveat: chord symbols
        // propose the inner-voice explanation, but only voicing/MIDI can prove
        // that the expected chromatic strand is actually present.
        classification.add_family(InterpretationFamily::SplitVoiceLeading);
    }
    push_unique(
        output,
        HarmonicInterpretation::new(
            if root_line_is_chromatic {
                "builtin.ordinary.diminished.chromatic_passing"
            } else {
                "builtin.ordinary.diminished.inner_voice_passing"
            },
            if root_line_is_chromatic { 1.65 } else { 0.9 },
            if root_line_is_chromatic {
                "Diminished seventh connects two surrounding roots by chromatic passing motion"
            } else {
                "Diminished seventh may carry a chromatic passing inner voice between surrounding harmonies"
            },
            classification,
        ),
    );
}

fn add_common_tone_diminished_reading(
    output: &mut Vec<HarmonicInterpretation>,
    current: HarmonyObservation,
    previous: HarmonyObservation,
    next: HarmonyObservation,
) {
    let returns_to_same_harmony = previous.root.pitch_class() == next.root.pitch_class()
        && previous.quality == next.quality
        && previous.seventh == next.seventh;
    if !returns_to_same_harmony
        || !diminished_collection_contains(current, previous.root.pitch_class())
    {
        return;
    }

    let mut classification = global_classification(current, HarmonicRole::NonFunctional);
    classification.add_source(HarmonicSource::Chromatic);
    classification.add_family(InterpretationFamily::CommonToneDiminished);
    classification.add_family(InterpretationFamily::AuxiliaryDiminished);
    classification.add_family(InterpretationFamily::Incidental);
    push_unique(
        output,
        HarmonicInterpretation::new(
            "builtin.ordinary.diminished.common_tone_neighbor",
            1.7,
            "Diminished seventh decorates a repeated harmony while retaining a common tone",
            classification,
        ),
    );
}

fn add_tonic_substitute_reading(
    output: &mut Vec<HarmonicInterpretation>,
    current: HarmonyObservation,
    next: HarmonyObservation,
) {
    // A symmetric diminished set containing the global tonic can weakly stand
    // in for tonic before ii.  This deliberately receives a modest score:
    // the same event is often explained more concretely as passing motion or
    // a rootless dominant, but the tonic-surrogate hearing should remain in
    // top-k rather than disappear.
    let next_is_global_two_minor =
        semitone_distance(next.root, current.tonic) == 2 && next.quality == QualityClass::Minor;
    if !next_is_global_two_minor
        || !diminished_collection_contains(current, current.tonic.pitch_class())
    {
        return;
    }

    let mut classification = global_classification(current, HarmonicRole::Tonic);
    classification.add_source(HarmonicSource::Chromatic);
    classification.add_family(InterpretationFamily::TonicSubstitute);
    push_unique(
        output,
        HarmonicInterpretation::new(
            "builtin.ordinary.diminished.tonic_substitute",
            0.7,
            "Diminished seventh shares the global tonic and can act as a weak tonic substitute before ii",
            classification,
        ),
    );
}

fn add_modal_interchange(
    output: &mut Vec<HarmonicInterpretation>,
    current: HarmonyObservation,
    previous: Option<HarmonyObservation>,
    next: Option<HarmonyObservation>,
) {
    // The inventory below is specifically parallel-minor vocabulary heard
    // from a major global tonic.  Treating the tonic minor chord of an
    // inferred minor key as "borrowed from parallel minor" would reward a
    // false function and contaminate the joint key/function ranking.
    //
    // Minor-key modal borrowing will get its own explicit parallel-major and
    // melodic-minor rules; until then the neutral degree candidate is the
    // honest representation.
    if current.global_mode != TonalMode::Major {
        return;
    }

    let root_distance = semitone_distance(current.root, current.tonic);
    let plain_major =
        current.quality == QualityClass::Major && current.seventh != Some(SeventhQuality::Minor);

    // The first seven cases are the common parallel-natural-minor inventory
    // heard from a major tonic.  They are source descriptions; the assigned
    // role remains an independently inspectable analytical choice.
    let borrowed = match (root_distance, current.quality, current.seventh) {
        (0, QualityClass::Minor, _) => Some((
            HarmonicRole::Tonic,
            HarmonicSource::ParallelMinor,
            false,
            "Minor tonic borrowed from the parallel minor key",
        )),
        (2, QualityClass::HalfDiminished | QualityClass::Diminished, _) => Some((
            HarmonicRole::Predominant,
            HarmonicSource::ParallelMinor,
            true,
            "Minor-mode supertonic used as a borrowed predominant",
        )),
        (3, QualityClass::Major, seventh) if seventh != Some(SeventhQuality::Minor) => Some((
            HarmonicRole::Tonic,
            HarmonicSource::ParallelMinor,
            false,
            "Flat-mediant major chord borrowed from the parallel minor key",
        )),
        (5, QualityClass::Minor, _) => Some((
            HarmonicRole::Subdominant,
            HarmonicSource::ParallelMinor,
            true,
            "Minor subdominant borrowed from the parallel minor key",
        )),
        (7, QualityClass::Minor, _) => Some((
            HarmonicRole::Dominant,
            HarmonicSource::ParallelMinor,
            false,
            "Minor dominant borrowed from the parallel minor key",
        )),
        (8, QualityClass::Major, seventh) if seventh != Some(SeventhQuality::Minor) => Some((
            HarmonicRole::Subdominant,
            HarmonicSource::ParallelMinor,
            true,
            "Flat-submediant major chord borrowed from the parallel minor key",
        )),
        (10, QualityClass::Major, Some(SeventhQuality::Minor)) => Some((
            HarmonicRole::Subdominant,
            HarmonicSource::ParallelMinor,
            false,
            "Flat-seven dominant-quality chord from parallel-minor vocabulary",
        )),
        _ => None,
    };

    if let Some((role, source, subdominant_minor, explanation)) = borrowed {
        let mut classification = global_classification(current, role);
        classification.add_source(source);
        classification.add_family(InterpretationFamily::ModalInterchange);
        if subdominant_minor {
            classification.add_source(HarmonicSource::SubdominantMinor);
            classification.add_family(InterpretationFamily::SubdominantMinor);
        }

        // Resolution toward the global tonic is useful evidence for the role,
        // but it is deliberately only a bonus: a borrowed chord may occur in
        // a loop without an immediate cadence.
        let mut interpretation = HarmonicInterpretation::new(
            "builtin.ordinary.modal_interchange",
            0.95,
            explanation,
            classification,
        );
        if next.is_some_and(|event| {
            event.root.pitch_class() == current.tonic.pitch_class() && is_stable_tonic(event)
        }) {
            interpretation.add_evidence(
                "builtin.ordinary.modal_interchange.to_tonic",
                0.45,
                "Borrowed harmony returns directly to the global tonic",
            );
        } else if previous.is_some_and(|event| {
            event.root.pitch_class() == current.tonic.pitch_class() && is_stable_tonic(event)
        }) {
            interpretation.add_evidence(
                "builtin.ordinary.modal_interchange.from_tonic",
                0.2,
                "Borrowed harmony departs directly from the global tonic",
            );
        }
        push_unique(output, interpretation);
    }

    // These modal colours are not part of parallel Aeolian, so they receive
    // their own source instead of being folded into the cases above.
    let modal_colour = if root_distance == 1 && plain_major {
        Some((
            HarmonicRole::Subdominant,
            HarmonicSource::Phrygian,
            "Flat-two major harmony from parallel-Phrygian vocabulary",
        ))
    } else if root_distance == 10 && plain_major {
        Some((
            HarmonicRole::Subdominant,
            HarmonicSource::Mixolydian,
            "Major flat-seven harmony from Mixolydian/modal vocabulary",
        ))
    } else if root_distance == 6 && current.quality == QualityClass::HalfDiminished {
        Some((
            HarmonicRole::Predominant,
            HarmonicSource::Lydian,
            "Raised-four half-diminished harmony compatible with Lydian",
        ))
    } else {
        None
    };

    if let Some((role, source, explanation)) = modal_colour {
        let mut classification = global_classification(current, role);
        classification.add_source(source);
        classification.add_family(InterpretationFamily::ModalInterchange);
        push_unique(
            output,
            HarmonicInterpretation::new(
                "builtin.ordinary.modal_colour",
                0.75,
                explanation,
                classification,
            ),
        );
    }
}

fn add_flat_seven_subdominant_minor(
    output: &mut Vec<HarmonicInterpretation>,
    current: HarmonyObservation,
    previous: Option<HarmonyObservation>,
    next: Option<HarmonyObservation>,
) {
    let is_flat_seven_dominant_seventh = semitone_distance(current.root, current.tonic) == 10
        && current.quality == QualityClass::Major
        && current.seventh == Some(SeventhQuality::Minor);
    if !is_flat_seven_dominant_seventh {
        return;
    }

    // bVII7 has several viable origins.  A Mixolydian/parallel-minor colour,
    // an actual backdoor dominant, and a sonority derived from ivm6 must not
    // be collapsed into one state merely because all display as bVII7.
    let mut classification = global_classification(current, HarmonicRole::Subdominant);
    classification.add_source(HarmonicSource::SubdominantMinor);
    classification.add_family(InterpretationFamily::SubdominantMinor);
    let mut interpretation = HarmonicInterpretation::new(
        "builtin.ordinary.flat_seven_subdominant_minor",
        0.95,
        "Flat-seven dominant seventh can derive from subdominant-minor voice leading",
        classification,
    );
    if next.is_some_and(|event| {
        event.root.pitch_class() == current.tonic.pitch_class() && is_stable_tonic(event)
    }) {
        interpretation.add_evidence(
            "builtin.ordinary.flat_seven_subdominant_minor.to_tonic",
            0.45,
            "Subdominant-minor flat-seven harmony returns directly to tonic",
        );
    } else if previous.is_some_and(|event| {
        event.root.pitch_class() == current.tonic.pitch_class() && is_stable_tonic(event)
    }) {
        interpretation.add_evidence(
            "builtin.ordinary.flat_seven_subdominant_minor.from_tonic",
            0.15,
            "Subdominant-minor flat-seven harmony departs directly from tonic",
        );
    }
    push_unique(output, interpretation);
}

fn add_neapolitan(
    output: &mut Vec<HarmonicInterpretation>,
    current: HarmonyObservation,
    next: Option<HarmonyObservation>,
) {
    let is_flat_two_major = semitone_distance(current.root, current.tonic) == 1
        && current.quality == QualityClass::Major
        && current.seventh != Some(SeventhQuality::Minor);
    if !is_flat_two_major {
        return;
    }

    let next_is_global_dominant = next.is_some_and(|event| {
        semitone_distance(event.root, current.tonic) == 7 && event.is_dominant
    });
    let next_is_tonic = next.is_some_and(|event| {
        event.root.pitch_class() == current.tonic.pitch_class() && is_stable_tonic(event)
    });
    if !next_is_global_dominant && !next_is_tonic {
        return;
    }

    let mut classification = global_classification(current, HarmonicRole::Predominant);
    classification.add_source(HarmonicSource::Phrygian);
    classification.add_family(InterpretationFamily::Neapolitan);
    let (score, explanation) = if next_is_global_dominant {
        (
            2.15,
            "Flat-two major harmony prepares the global dominant as a Neapolitan",
        )
    } else {
        (
            1.25,
            "Flat-two major harmony resolves directly to tonic in a Neapolitan/Phrygian gesture",
        )
    };
    push_unique(
        output,
        HarmonicInterpretation::new(
            "builtin.ordinary.neapolitan",
            score,
            explanation,
            classification,
        ),
    );
}

fn add_chromatic_mediant(
    output: &mut Vec<HarmonicInterpretation>,
    current: HarmonyObservation,
    previous: Option<HarmonyObservation>,
    next: Option<HarmonyObservation>,
) {
    let root_distance = semitone_distance(current.root, current.tonic);
    let is_major_colour =
        current.quality == QualityClass::Major && current.seventh != Some(SeventhQuality::Minor);
    if !is_major_colour || !matches!(root_distance, 3 | 4 | 8 | 9) {
        return;
    }

    let touches_tonic = [previous, next].into_iter().flatten().any(|event| {
        event.root.pitch_class() == current.tonic.pitch_class() && is_stable_tonic(event)
    });
    let mut classification = global_classification(current, HarmonicRole::NonFunctional);
    classification.add_source(HarmonicSource::Chromatic);
    classification.add_family(InterpretationFamily::ChromaticMediant);
    push_unique(
        output,
        HarmonicInterpretation::new(
            "builtin.ordinary.chromatic_mediant",
            if touches_tonic { 1.35 } else { 0.55 },
            if touches_tonic {
                "Major harmony a third from the tonic forms a direct chromatic-mediant relation"
            } else {
                "Major harmony is a chromatic mediant of the global tonic"
            },
            classification,
        ),
    );
}

fn add_half_diminished_common_tone_neighbor(
    output: &mut Vec<HarmonicInterpretation>,
    current: HarmonyObservation,
    next: Option<HarmonyObservation>,
) {
    let Some(next) = next else {
        return;
    };

    // In C#m7b5 -> Cmaj7, the half-diminished third, fifth, and seventh
    // become the target major seventh's third, fifth, and seventh. Only the
    // written root moves down by semitone, so this is stronger evidence for a
    // common-tone decoration than for a predominant function.
    let is_tonic_major_seventh = next.root.pitch_class() == current.tonic.pitch_class()
        && next.quality == QualityClass::Major
        && next.seventh == Some(SeventhQuality::Major)
        && is_stable_tonic(next);
    if current.quality != QualityClass::HalfDiminished
        || semitone_distance(next.root, current.root) != 11
        || !is_tonic_major_seventh
    {
        return;
    }

    let mut classification = global_classification(current, HarmonicRole::NonFunctional);
    classification.add_source(HarmonicSource::Chromatic);
    classification.add_family(InterpretationFamily::CommonToneNeighbor);
    classification.add_family(InterpretationFamily::ChromaticApproach);
    push_unique(
        output,
        HarmonicInterpretation::new(
            "builtin.ordinary.half_diminished.common_tone_neighbor",
            1.9,
            "Half-diminished seventh retains three common tones while its root descends by semitone into tonic major seventh",
            classification,
        ),
    );
}
fn add_chromatic_approach(
    output: &mut Vec<HarmonicInterpretation>,
    current: HarmonyObservation,
    next: Option<HarmonyObservation>,
) {
    let Some(next) = next else {
        return;
    };
    let root_motion = semitone_distance(next.root, current.root);
    if !matches!(root_motion, 1 | 11) || is_diatonic_major(current) {
        return;
    }

    let same_structure = current.quality == next.quality && current.seventh == next.seventh;
    let approaches_tonic =
        next.root.pitch_class() == current.tonic.pitch_class() && is_stable_tonic(next);
    if !same_structure && !approaches_tonic {
        return;
    }

    let mut classification = global_classification(current, HarmonicRole::NonFunctional);
    classification.add_source(HarmonicSource::Chromatic);
    classification.add_family(InterpretationFamily::ChromaticApproach);
    push_unique(
        output,
        HarmonicInterpretation::new(
            "builtin.ordinary.chromatic_approach",
            if same_structure { 1.25 } else { 0.85 },
            if same_structure {
                "Non-diatonic chord approaches the next equal-quality chord by semitone"
            } else {
                "Non-diatonic chord approaches the global tonic by semitone"
            },
            classification,
        ),
    );
}

fn global_classification(
    observation: HarmonyObservation,
    role: HarmonicRole,
) -> HarmonicClassification {
    let mut classification = HarmonicClassification::with_role(role);
    classification.local_degree = Some(degree_from_spelling(observation.root, observation.tonic));
    classification.perspective = Some(TonalPerspective {
        global_tonic: observation.tonic,
        local_tonic: observation.tonic,
        local_tonic_degree: degree_from_spelling(observation.tonic, observation.tonic),
        scope: TonalScope::Global,
        mode: observation.global_mode,
    });
    classification
}

fn local_classification(
    observation: HarmonyObservation,
    role: HarmonicRole,
    local_tonic: SpelledNote,
    local_degree: crate::domain::Degree,
    mode: TonalMode,
) -> HarmonicClassification {
    let mut classification = HarmonicClassification::with_role(role);
    classification.local_degree = Some(local_degree);
    classification.perspective = Some(TonalPerspective {
        global_tonic: observation.tonic,
        local_tonic,
        local_tonic_degree: degree_from_spelling(local_tonic, observation.tonic),
        scope: if local_tonic.pitch_class() == observation.tonic.pitch_class() {
            TonalScope::Global
        } else {
            TonalScope::Tonicization
        },
        mode,
    });
    classification
}

fn is_fully_diminished_seventh(observation: HarmonyObservation) -> bool {
    observation.quality == QualityClass::Diminished
        && observation.seventh == Some(SeventhQuality::Diminished)
}

fn diminished_collection_contains(
    observation: HarmonyObservation,
    pitch_class: crate::domain::PitchClass,
) -> bool {
    matches!(
        pitch_class.distance_from(observation.root.pitch_class()),
        0 | 3 | 6 | 9
    )
}

fn dominant_root_for_target(target: SpelledNote) -> SpelledNote {
    // A dominant root is a fifth above its target and four staff letters
    // higher.  Spelling by letter first keeps D as V of G instead of choosing
    // an unrelated enharmonic name.
    spell_pitch_class(target.letter.shift(4), target.pitch_class().offset(7))
}

fn target_of_dominant(dominant: SpelledNote) -> SpelledNote {
    spell_pitch_class(dominant.letter.shift(3), dominant.pitch_class().offset(5))
}

fn observation_mode(observation: HarmonyObservation) -> TonalMode {
    match observation.quality {
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

fn is_diatonic_major(observation: HarmonyObservation) -> bool {
    matches!(
        (
            semitone_distance(observation.root, observation.tonic),
            observation.quality
        ),
        (0 | 5 | 7, QualityClass::Major)
            | (2 | 4 | 9, QualityClass::Minor)
            | (11, QualityClass::Diminished | QualityClass::HalfDiminished)
    )
}

fn is_stable_tonic(observation: HarmonyObservation) -> bool {
    matches!(
        observation.quality,
        QualityClass::Major | QualityClass::Minor
    ) && !observation.is_dominant
}

fn push_unique(output: &mut Vec<HarmonicInterpretation>, interpretation: HarmonicInterpretation) {
    if !output.iter().any(|existing| {
        existing.rule_id == interpretation.rule_id
            && existing.classification == interpretation.classification
    }) {
        output.push(interpretation);
    }
}
