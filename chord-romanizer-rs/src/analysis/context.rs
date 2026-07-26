//! Lightweight progression context used while producing `RomanizedChord`s.
//!
//! This is intentionally not the final probabilistic sequence model. Its job
//! is to provide deterministic, backwards-compatible hints to the romanizer:
//! semantic neighbors, effective roots, II-V markers, resolution markers, and
//! accidental preferences. The richer set of competing interpretations is
//! retained separately and later exposed through the analysis lattice.

use crate::analysis::ordinary::{HarmonyObservation, infer_ordinary_interpretations};
use crate::analysis::{
    BlackadderContext, DominantRelation, HarmonicClassification, HarmonicInterpretation,
    HarmonicRole, InterpretationFamily, TonalMode, TonalPerspective, TonalScope,
};
use crate::domain::{
    Degree, ParsedChord, ParsedSymbol, ProgressionItem, QualityClass, RomanDegree, SeventhQuality,
    SpelledNote,
};
use crate::interpreter::{ChordInterpreter, FunctionalRole, HybridCandidate, HybridKind};
use crate::profile::{BehaviorProfile, KeyBoundaryPolicy, NoChordPolicy};
use crate::speller::{
    degree_from_spelling, semitone_distance, spell_pitch_class, target_accidental_preference,
};
use crate::structure;

#[derive(Clone, Debug)]
pub(crate) struct AnalysisNode {
    /// Root used for progression motion. For a functional slash chord this can
    /// differ from the written upper-structure root and from the slash bass.
    pub effective_root: SpelledNote,
    /// Caller-supplied/global key for this event. A detected applied cadence
    /// keeps this value and places its temporary center in `perspective`.
    pub tonic: SpelledNote,
    /// Mode of the global key hypothesis currently being evaluated.  This is
    /// distinct from `tonal_mode`, which describes the observed chord/target.
    pub global_mode: TonalMode,
    /// Coarse functional flags used only by deterministic progression rules.
    pub is_dominant: bool,
    pub is_minor: bool,
    pub is_diminished: bool,
    pub is_tonic_quality: bool,
    pub quality: QualityClass,
    pub seventh: Option<SeventhQuality>,
    pub tonal_mode: TonalMode,
    /// All local slash/hybrid readings are retained even though the fields
    /// above summarize the current 1-best reading.
    pub hybrid_candidates: Vec<HybridCandidate>,
    /// Scored, mutually competing meanings for ordinary chords.  The union of
    /// their classifications remains available below for annotation clients,
    /// while the lattice materializes these entries as distinct states.
    pub harmonic_interpretations: Vec<HarmonicInterpretation>,
    pub harmonic_classifications: Vec<HarmonicClassification>,
    pub is_ii_v_start: bool,
    pub is_resolution_target: bool,
    pub resolution_type: Option<ResolutionKind>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ResolutionKind {
    Perfect,
    Semitone,
    Backdoor,
    LeadingTone,
    Deceptive,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct ContextHint {
    pub prefer_sharps: Option<bool>,
    pub node: Option<AnalysisNode>,
}

#[derive(Clone, Debug)]
pub(crate) struct ContextAnalysis {
    /// One entry per input event, including N.C. and explicit boundaries.
    pub hints: Vec<ContextHint>,
    /// Semantic chord neighbors. Transparent N.C. events have no node of their
    /// own but do not necessarily interrupt these links.
    pub previous_chord: Vec<Option<usize>>,
    pub next_chord: Vec<Option<usize>>,
}

/// Derive deterministic context without changing event alignment.
///
/// The function performs four small passes rather than one stateful loop. This
/// makes the data dependency explicit: neighbor links -> local nodes ->
/// progression markers -> spelling hints.
pub(crate) fn analyze_global_context(
    items: &[ProgressionItem],
    interpreter: &ChordInterpreter,
    default_tonic: SpelledNote,
    default_mode: TonalMode,
    key_boundary_policy: KeyBoundaryPolicy,
    no_chord_policy: NoChordPolicy,
) -> ContextAnalysis {
    // Phase 1: establish which chord is musically adjacent to which other
    // chord. Raw array adjacency is insufficient because N.C. can be
    // transparent and explicit boundaries must never be crossed.
    let (previous_chord, next_chord) =
        build_neighbors(items, default_tonic, key_boundary_policy, no_chord_policy);

    // Phase 2: produce local analyses. StrictV1 is allowed one-symbol
    // look-ahead so ambiguous augmented-over-bass readings can notice an
    // immediate resolution. Python019 deliberately preserves the old
    // context-free pre-analysis behavior.
    let mut nodes: Vec<Option<AnalysisNode>> = items
        .iter()
        .enumerate()
        .map(|(index, item)| {
            item.chord().map(|chord| {
                let contextual_previous = if interpreter.behavior() == BehaviorProfile::StrictV1 {
                    previous_chord[index].and_then(|previous| items[previous].chord())
                } else {
                    None
                };
                let contextual_next = if interpreter.behavior() == BehaviorProfile::StrictV1 {
                    next_chord[index].and_then(|next| items[next].chord())
                } else {
                    None
                };
                pre_analyze(
                    chord,
                    item_tonic(item, default_tonic),
                    default_mode,
                    contextual_previous,
                    contextual_next,
                    interpreter,
                )
            })
        })
        .collect();

    // Phase 3: annotate pairwise and three-chord patterns on the already
    // chosen effective roots. This avoids recomputing slash interpretation in
    // a second, potentially inconsistent pass.
    detect_ii_v_and_resolutions(&mut nodes, &next_chord, interpreter.behavior());

    // Phase 3b: generate competing meanings for ordinary chromatic chords.
    // This is separated from cadence detection because modal source, local
    // function, and linear voice-leading are independent analytical axes.
    if interpreter.behavior() == BehaviorProfile::StrictV1 {
        detect_ordinary_interpretations(&mut nodes, &previous_chord, &next_chord);
    }

    // Phase 4: derive a display-spelling preference. This affects whether a
    // chromatic pitch is rendered as, for example, #IV or bV; it never changes
    // the pitch class or removes alternate spellings.
    let hints = (0..nodes.len())
        .map(|index| {
            let Some(node) = nodes[index].clone() else {
                return ContextHint::default();
            };
            let mut prefer_sharps = None;

            if let Some(next) = next_node(index, &nodes, &next_chord) {
                match semitone_distance(next.effective_root, node.effective_root) {
                    1 => prefer_sharps = Some(node.is_diminished),
                    11 => prefer_sharps = Some(false),
                    _ => {}
                }
            }

            // A cadence target is stronger spelling evidence than a simple
            // semitone motion. For II-V-I, look through the dominant to I.
            let target = if node.is_ii_v_start {
                next_chord[index]
                    .and_then(|dominant| next_chord[dominant])
                    .and_then(|target| nodes[target].as_ref())
            } else if node.is_dominant {
                next_node(index, &nodes, &next_chord)
                    .filter(|next| semitone_distance(next.effective_root, node.effective_root) == 5)
            } else {
                None
            };
            if let Some(target) = target {
                if let Some(preference) = target_accidental_preference(target.effective_root) {
                    prefer_sharps = Some(preference);
                }
            }

            ContextHint {
                prefer_sharps,
                node: Some(node),
            }
        })
        .collect();

    ContextAnalysis {
        hints,
        previous_chord,
        next_chord,
    }
}

fn build_neighbors(
    items: &[ProgressionItem],
    default_tonic: SpelledNote,
    key_boundary_policy: KeyBoundaryPolicy,
    no_chord_policy: NoChordPolicy,
) -> (Vec<Option<usize>>, Vec<Option<usize>>) {
    let mut previous = vec![None; items.len()];
    let mut next = vec![None; items.len()];
    let mut last_chord = None;

    // One left-to-right scan is enough to fill both directions: when a new
    // chord arrives, it becomes the previous chord's `next` and remembers the
    // previous index itself.
    for (index, item) in items.iter().enumerate() {
        match &item.symbol {
            // Boundary is unconditional; it represents a section break or a
            // caller-confirmed long silence.
            ParsedSymbol::Boundary { .. } => last_chord = None,
            ParsedSymbol::NoChord { .. } => {
                // N.C. is commonly a rest inside one phrase, so StrictV1's
                // default policy leaves `last_chord` intact.
                if no_chord_policy == NoChordPolicy::Break {
                    last_chord = None;
                }
            }
            ParsedSymbol::Chord(_) => {
                if let Some(previous_index) = last_chord {
                    let key_changed = item_tonic(&items[previous_index], default_tonic)
                        != item_tonic(item, default_tonic);
                    if key_boundary_policy == KeyBoundaryPolicy::Break && key_changed {
                        last_chord = None;
                    }
                }
                if let Some(previous_index) = last_chord {
                    previous[index] = Some(previous_index);
                    next[previous_index] = Some(index);
                }
                last_chord = Some(index);
            }
        }
    }
    (previous, next)
}

fn item_tonic(item: &ProgressionItem, default_tonic: SpelledNote) -> SpelledNote {
    item.tonic.unwrap_or(default_tonic)
}

fn next_node<'a>(
    index: usize,
    nodes: &'a [Option<AnalysisNode>],
    next_chord: &[Option<usize>],
) -> Option<&'a AnalysisNode> {
    next_chord[index].and_then(|next| nodes[next].as_ref())
}

fn pre_analyze(
    chord: &ParsedChord,
    tonic: SpelledNote,
    global_mode: TonalMode,
    previous_chord: Option<&ParsedChord>,
    next_chord: Option<&ParsedChord>,
    interpreter: &ChordInterpreter,
) -> AnalysisNode {
    // Candidate generation is lossless: both Blackadder and half-diminished
    // readings, for example, remain available to the later lattice.
    let hybrid_candidates = if interpreter.behavior() == BehaviorProfile::StrictV1 {
        interpreter.analyze_slash_candidates_with_context(
            chord,
            BlackadderContext {
                tonic: Some(tonic),
                previous_chord,
                next_chord,
                observations: None,
            },
        )
    } else {
        interpreter.analyze_slash_candidates(chord, next_chord)
    };

    // Context metadata still needs one effective root. Select the highest
    // intrinsic score as a convenience view, but do not discard the vector.
    // A future sequence model may choose a different candidate globally.
    let mut analysis = hybrid_candidates
        .first()
        .map(|candidate| candidate.analysis.clone())
        .unwrap_or_default();
    let mut best_score = hybrid_candidates
        .first()
        .map_or(f64::NEG_INFINITY, |candidate| {
            interpreter.contextual_candidate_score(candidate, next_chord, Some(tonic))
        });
    for candidate in hybrid_candidates.iter().skip(1) {
        let score = interpreter.contextual_candidate_score(candidate, next_chord, Some(tonic));
        if score > best_score {
            analysis = candidate.analysis.clone();
            best_score = score;
        }
    }
    let mut effective_root = chord.root;

    if interpreter.behavior() == BehaviorProfile::StrictV1 {
        // Strict candidates state their functional root directly. In a
        // secondary-dominant-third-in-bass reading this is the dominant root,
        // not the written augmented root and not necessarily the bass.
        if let Some(root) = analysis.effective_root {
            effective_root = root;
        }
    } else if analysis.is_hybrid
        && matches!(
            analysis.kind,
            HybridKind::SusFourNine
                | HybridKind::SusFourSevenFlatNine
                | HybridKind::SecondaryDominantThirdInBass
        )
    {
        if let Some(bass) = chord.bass {
            effective_root = bass;
        }
    }

    let is_dominant = structure::is_dominant_for(chord, interpreter.behavior())
        || (interpreter.behavior() == BehaviorProfile::StrictV1
            && analysis.functional_role == Some(FunctionalRole::Dominant))
        || matches!(
            analysis.kind,
            HybridKind::SusFourNine
                | HybridKind::SusFourSevenFlatNine
                | HybridKind::SecondaryDominantThirdInBass
        );
    let is_minor = structure::is_minor_for(chord, interpreter.behavior());
    let is_diminished = structure::is_diminished_for(chord, interpreter.behavior())
        || (interpreter.behavior() == BehaviorProfile::StrictV1
            && analysis.functional_role == Some(FunctionalRole::HalfDiminished));

    AnalysisNode {
        effective_root,
        tonic,
        global_mode,
        is_dominant,
        is_minor,
        is_diminished,
        is_tonic_quality: structure::is_tonic_for(chord, interpreter.behavior()),
        quality: chord.quality_parsed.class,
        seventh: chord.quality_parsed.seventh,
        tonal_mode: tonal_mode(chord),
        hybrid_candidates,
        harmonic_interpretations: Vec::new(),
        harmonic_classifications: Vec::new(),
        is_ii_v_start: false,
        is_resolution_target: false,
        resolution_type: None,
    }
}

fn detect_ii_v_and_resolutions(
    nodes: &mut [Option<AnalysisNode>],
    next_chord: &[Option<usize>],
    behavior: BehaviorProfile,
) {
    for current_index in 0..nodes.len() {
        let Some(next_index) = next_chord[current_index] else {
            continue;
        };
        let Some(current) = nodes[current_index].clone() else {
            continue;
        };
        let Some(next) = nodes[next_index].clone() else {
            continue;
        };
        // All motions are measured from current to next in pitch-class space.
        // `5` means the target is a perfect fourth above, i.e. the usual
        // dominant-to-tonic root motion; `11` means one semitone down.
        let distance = semitone_distance(next.effective_root, current.effective_root);
        let marks_ii_v =
            distance == 5 && (current.is_minor || current.is_diminished) && next.is_dominant;
        let dominant_relation = if current.is_dominant && distance == 5 && next.is_tonic_quality {
            Some(DominantRelation::FifthRelated)
        } else if current.is_dominant && distance == 11 && next.is_tonic_quality {
            Some(DominantRelation::TritoneSubstitute)
        } else if current.is_dominant
            && distance == 2
            && next.is_tonic_quality
            // A bare diatonic V7-vi motion (for example G7-Am in C) is much
            // more naturally a deceptive cadence than a local backdoor. The
            // deterministic text-only rule therefore confirms backdoor only
            // at the global tonic. A future sequence scorer may reintroduce a
            // local backdoor candidate when a longer tonicization supports it.
            && next.effective_root.pitch_class() == current.tonic.pitch_class()
        {
            Some(DominantRelation::Backdoor)
        } else if behavior == BehaviorProfile::StrictV1
            && diminished_can_resolve_as_leading_tone(&current, &next)
        {
            // A fully diminished seventh is rotationally symmetric. Any of
            // its four notes can be spelled as the leading tone, so bIII°7 ->
            // V may be the same sounding collection as #IV°7 -> V. Triads and
            // half-diminished sevenths retain the written-root semitone check.
            Some(DominantRelation::LeadingTone)
        } else {
            None
        };
        let resolution = dominant_relation.map(|relation| match relation {
            DominantRelation::FifthRelated => ResolutionKind::Perfect,
            DominantRelation::TritoneSubstitute => ResolutionKind::Semitone,
            DominantRelation::Backdoor => ResolutionKind::Backdoor,
            DominantRelation::LeadingTone => ResolutionKind::LeadingTone,
        });

        // Compute both decisions with immutable borrows first, then mutate the
        // nodes. Besides satisfying Rust's borrow rules, this prevents the
        // first marker from influencing the second decision in the same pass.
        if marks_ii_v {
            if let Some(current) = nodes[current_index].as_mut() {
                current.is_ii_v_start = true;
            }
        }
        if let Some(resolution) = resolution {
            if let Some(next) = nodes[next_index].as_mut() {
                next.is_resolution_target = true;
                next.resolution_type = Some(resolution);
            }
        }
        if let Some(relation) = dominant_relation {
            let perspective =
                tonal_perspective(current.tonic, next.effective_root, next.tonal_mode);
            let mut dominant = HarmonicClassification::with_role(HarmonicRole::Dominant);
            dominant.dominant_relation = Some(relation);
            dominant.local_degree = Some(if relation == DominantRelation::LeadingTone {
                // This is the heard root inside the local interpretation. The
                // global written degree remains on `RomanizedChord`, exposing
                // both bIII°7 notation and its vii°7/V meaning.
                Degree::new(0, RomanDegree::Vii)
            } else {
                degree_from_spelling(current.effective_root, next.effective_root)
            });
            dominant.perspective = Some(perspective.clone());
            match relation {
                DominantRelation::TritoneSubstitute => {
                    dominant.add_family(InterpretationFamily::TritoneSubstitute);
                }
                DominantRelation::Backdoor => {
                    dominant.add_family(InterpretationFamily::Backdoor);
                }
                DominantRelation::LeadingTone => {
                    dominant.add_family(InterpretationFamily::AppliedLeadingTone);
                }
                DominantRelation::FifthRelated => {}
            }
            if let Some(current) = nodes[current_index].as_mut() {
                let (rule_id, score, explanation) = match relation {
                    DominantRelation::FifthRelated => (
                        "builtin.ordinary.dominant_resolution",
                        1.55,
                        "Dominant-quality chord resolves by descending fifth",
                    ),
                    DominantRelation::TritoneSubstitute => (
                        "builtin.ordinary.tritone_resolution",
                        1.35,
                        "Dominant-quality chord resolves down by semitone as a tritone substitute",
                    ),
                    DominantRelation::Backdoor => (
                        "builtin.ordinary.backdoor_resolution",
                        1.4,
                        "Flat-seven dominant resolves up by whole tone to the global tonic",
                    ),
                    DominantRelation::LeadingTone => (
                        "builtin.ordinary.leading_tone_resolution",
                        1.5,
                        "Diminished collection contains the leading tone of its local target",
                    ),
                };
                push_interpretation(
                    current,
                    HarmonicInterpretation::new(rule_id, score, explanation, dominant),
                );
            }

            let mut tonic = HarmonicClassification::with_role(HarmonicRole::Tonic);
            tonic.local_degree = Some(degree_from_spelling(
                next.effective_root,
                next.effective_root,
            ));
            tonic.perspective = Some(perspective);
            if let Some(next) = nodes[next_index].as_mut() {
                push_interpretation(
                    next,
                    HarmonicInterpretation::new(
                        "builtin.ordinary.tonicized_target",
                        0.9,
                        "Chord is the realized target of a dominant relation",
                        tonic,
                    ),
                );
            }
        }
    }

    if behavior != BehaviorProfile::StrictV1 {
        return;
    }

    // StrictV1 also preserves two kinds of local-key evidence which do not
    // contain a literal V-I arrival:
    //
    // * an applied dominant may resolve deceptively to VI/bVI of its implied
    //   local key;
    // * a short pair may have a clear IV-iii, ii-iii, or iv-V reading in
    //   another key even though that key is not confirmed by a cadence.
    //
    // These are candidates, not global key changes. The lattice can therefore
    // keep the caller's key and the temporary perspective in separate k-best
    // paths instead of rewriting the displayed Roman numerals.
    detect_secondary_dominant_deceptive(nodes, next_chord);
    detect_alternate_key_sequences(nodes, next_chord);

    // A pairwise dominant relation determines the local tonic. Looking one
    // chord farther back then recognizes applied ii-V-i/I spans, including
    // the common global `IIIm7b5-VI7-IIm7` = local `iiø-V-i` reading.
    for first_index in 0..nodes.len() {
        let Some(dominant_index) = next_chord[first_index] else {
            continue;
        };
        let Some(target_index) = next_chord[dominant_index] else {
            continue;
        };
        let (Some(first), Some(dominant), Some(target)) = (
            nodes[first_index].clone(),
            nodes[dominant_index].clone(),
            nodes[target_index].clone(),
        ) else {
            continue;
        };
        if !(first.is_minor || first.is_diminished)
            || !dominant.is_dominant
            || !target.is_tonic_quality
        {
            continue;
        }
        let Some(dominant_classification) = dominant
            .harmonic_classifications
            .iter()
            .find(|classification| {
                classification.role == Some(HarmonicRole::Dominant)
                    && classification
                        .perspective
                        .as_ref()
                        .is_some_and(|perspective| {
                            perspective.local_tonic.pitch_class()
                                == target.effective_root.pitch_class()
                        })
                    && matches!(
                        classification.dominant_relation,
                        Some(DominantRelation::FifthRelated | DominantRelation::TritoneSubstitute)
                    )
            })
            .cloned()
        else {
            continue;
        };
        let Some(perspective) = dominant_classification.perspective.clone() else {
            continue;
        };
        let target_related_two =
            matches!(
                dominant_classification.dominant_relation,
                Some(DominantRelation::FifthRelated | DominantRelation::TritoneSubstitute)
            ) && semitone_distance(first.effective_root, target.effective_root) == 2;
        let tritone_substitute_related_two = dominant_classification.dominant_relation
            == Some(DominantRelation::TritoneSubstitute)
            // A related ii prepares the written substitute dominant by the
            // same descending-fifth root motion as an ordinary ii-V. In C,
            // Abm7-Db7-C is therefore bVIm7-subV7-I, not local ii-V-I.
            && semitone_distance(dominant.effective_root, first.effective_root) == 5;
        if !target_related_two && !tritone_substitute_related_two {
            continue;
        }

        let mut predominant = HarmonicClassification::with_role(HarmonicRole::Predominant);
        predominant.add_family(InterpretationFamily::AppliedCadence);
        if tritone_substitute_related_two {
            predominant.add_family(InterpretationFamily::TritoneSubstituteRelatedTwo);
        }
        predominant.local_degree = Some(degree_from_spelling(
            first.effective_root,
            perspective.local_tonic,
        ));
        predominant.perspective = Some(perspective.clone());
        if let Some(first) = nodes[first_index].as_mut() {
            first.is_ii_v_start = true;
            let (rule_id, explanation) = if tritone_substitute_related_two {
                (
                    "builtin.ordinary.tritone_substitute_related_two.predominant",
                    "Minor chord acts as the related ii of a tritone-substitute dominant",
                )
            } else {
                (
                    "builtin.ordinary.applied_two_five.predominant",
                    "Minor or half-diminished chord acts as ii of a realized local ii-V cadence",
                )
            };
            push_interpretation(
                first,
                HarmonicInterpretation::new(rule_id, 1.8, explanation, predominant),
            );
        }
        reinforce_family_for_perspective(
            nodes[dominant_index].as_mut(),
            &perspective,
            InterpretationFamily::AppliedCadence,
            "builtin.ordinary.applied_two_five.dominant",
            0.55,
            "Dominant is preceded by its related local ii chord",
        );
        reinforce_family_for_perspective(
            nodes[target_index].as_mut(),
            &perspective,
            InterpretationFamily::AppliedCadence,
            "builtin.ordinary.applied_two_five.target",
            0.35,
            "Local tonic completes a ii-V-I/i span",
        );
    }
}

fn detect_secondary_dominant_deceptive(
    nodes: &mut [Option<AnalysisNode>],
    next_chord: &[Option<usize>],
) {
    for dominant_index in 0..nodes.len() {
        let Some(target_index) = next_chord[dominant_index] else {
            continue;
        };
        let (Some(dominant), Some(target)) =
            (nodes[dominant_index].clone(), nodes[target_index].clone())
        else {
            continue;
        };
        if !dominant.is_dominant || !target.is_tonic_quality {
            continue;
        }

        // Infer the tonic normally targeted by the written dominant. E7
        // therefore implies A. A following F major is bVI in that temporary
        // A-minor frame; D7 -> Em is vi in temporary G major.
        let local_tonic = target_of_dominant(dominant.effective_root);
        if local_tonic.pitch_class() == dominant.tonic.pitch_class() {
            // The ordinary global V-vi deceptive cadence is useful too, but
            // this rule is intentionally scoped to *secondary* dominants.
            continue;
        }
        let target_distance = semitone_distance(target.effective_root, local_tonic);
        let local_mode = match (target_distance, target.quality) {
            (8, QualityClass::Major) => TonalMode::Minor,
            (9, QualityClass::Minor) => TonalMode::Major,
            _ => continue,
        };
        let perspective = tonal_perspective(dominant.tonic, local_tonic, local_mode);

        let mut dominant_classification = HarmonicClassification::with_role(HarmonicRole::Dominant);
        dominant_classification.dominant_relation = Some(DominantRelation::FifthRelated);
        dominant_classification.local_degree =
            Some(degree_from_spelling(dominant.effective_root, local_tonic));
        dominant_classification.add_family(InterpretationFamily::SecondaryDominantDeceptive);
        dominant_classification.perspective = Some(perspective.clone());

        let mut target_classification = HarmonicClassification::with_role(HarmonicRole::Tonic);
        target_classification.local_degree =
            Some(degree_from_spelling(target.effective_root, local_tonic));
        target_classification.add_family(InterpretationFamily::SecondaryDominantDeceptive);
        target_classification.perspective = Some(perspective);

        if let Some(node) = nodes[dominant_index].as_mut() {
            push_interpretation(
                node,
                HarmonicInterpretation::new(
                    "builtin.ordinary.secondary_dominant_deceptive.dominant",
                    1.45,
                    "Secondary dominant targets a local tonic but moves to its VI/bVI substitute",
                    dominant_classification,
                ),
            );
        }
        if let Some(node) = nodes[target_index].as_mut() {
            node.is_resolution_target = true;
            node.resolution_type = Some(ResolutionKind::Deceptive);
            push_interpretation(
                node,
                HarmonicInterpretation::new(
                    "builtin.ordinary.secondary_dominant_deceptive.target",
                    0.8,
                    "Chord acts as the tonic substitute in a secondary deceptive resolution",
                    target_classification,
                ),
            );
        }
    }
}

fn detect_alternate_key_sequences(
    nodes: &mut [Option<AnalysisNode>],
    next_chord: &[Option<usize>],
) {
    for first_index in 0..nodes.len() {
        let Some(second_index) = next_chord[first_index] else {
            continue;
        };
        let (Some(first), Some(second)) = (nodes[first_index].clone(), nodes[second_index].clone())
        else {
            continue;
        };
        if first.tonic.pitch_class() != second.tonic.pitch_class() {
            continue;
        }

        let root_motion = semitone_distance(second.effective_root, first.effective_root);
        let pair = if first.quality == QualityClass::Major
            && !first.is_dominant
            && second.quality == QualityClass::Minor
            && root_motion == 11
        {
            // BbM7-Am7 in global C is IV-iii in temporary F.
            Some((
                spell_pitch_class(
                    first.effective_root.letter.shift(4),
                    first.effective_root.pitch_class().offset(7),
                ),
                TonalMode::Major,
                HarmonicRole::Subdominant,
                HarmonicRole::Tonic,
                0.8,
                0.65,
                "Major IV descends by semitone to minor iii in a temporary key",
            ))
        } else if first.quality == QualityClass::Minor
            && second.quality == QualityClass::Minor
            && root_motion == 2
        {
            // Fm7-Gm7 in global C is ii-iii in temporary Eb. Dm7-Em7
            // derives global C and is excluded below as an ordinary diatonic
            // pair rather than manufacturing a duplicate perspective.
            Some((
                spell_pitch_class(
                    first.effective_root.letter.shift(6),
                    first.effective_root.pitch_class().offset(10),
                ),
                TonalMode::Major,
                HarmonicRole::Predominant,
                HarmonicRole::Tonic,
                0.55,
                0.5,
                "Two minor chords a whole tone apart form local ii-iii motion",
            ))
        } else if first.quality == QualityClass::Minor && second.is_dominant && root_motion == 2 {
            // Abm7-Bb7 in global C is iv-V in temporary Eb minor.
            Some((
                spell_pitch_class(
                    first.effective_root.letter.shift(4),
                    first.effective_root.pitch_class().offset(7),
                ),
                TonalMode::Minor,
                HarmonicRole::Subdominant,
                HarmonicRole::Dominant,
                0.8,
                0.8,
                "Minor iv rises by whole tone to V in a temporary minor key",
            ))
        } else {
            None
        };
        let Some((
            local_tonic,
            mode,
            first_role,
            second_role,
            first_score,
            second_score,
            explanation,
        )) = pair
        else {
            continue;
        };
        if local_tonic.pitch_class() == first.tonic.pitch_class() {
            continue;
        }
        let perspective = tonal_perspective(first.tonic, local_tonic, mode);

        let mut first_classification = HarmonicClassification::with_role(first_role);
        first_classification.local_degree =
            Some(degree_from_spelling(first.effective_root, local_tonic));
        first_classification.add_family(InterpretationFamily::AlternateKeySequence);
        first_classification.perspective = Some(perspective.clone());

        let mut second_classification = HarmonicClassification::with_role(second_role);
        second_classification.local_degree =
            Some(degree_from_spelling(second.effective_root, local_tonic));
        second_classification.add_family(InterpretationFamily::AlternateKeySequence);
        second_classification.perspective = Some(perspective);
        if second_role == HarmonicRole::Dominant {
            second_classification.dominant_relation = Some(DominantRelation::FifthRelated);
        }

        if let Some(node) = nodes[first_index].as_mut() {
            push_interpretation(
                node,
                HarmonicInterpretation::new(
                    "builtin.ordinary.alternate_key_sequence.first",
                    first_score,
                    explanation,
                    first_classification,
                ),
            );
        }
        if let Some(node) = nodes[second_index].as_mut() {
            push_interpretation(
                node,
                HarmonicInterpretation::new(
                    "builtin.ordinary.alternate_key_sequence.second",
                    second_score,
                    explanation,
                    second_classification,
                ),
            );
        }
    }
}

fn target_of_dominant(dominant: SpelledNote) -> SpelledNote {
    // A written dominant resolves a fourth upward: E -> A, D -> G. Spelling
    // the letter first preserves the intended applied-key name.
    spell_pitch_class(dominant.letter.shift(3), dominant.pitch_class().offset(5))
}

fn diminished_can_resolve_as_leading_tone(current: &AnalysisNode, next: &AnalysisNode) -> bool {
    if !current.is_diminished || !next.is_tonic_quality {
        return false;
    }

    let distance = semitone_distance(next.effective_root, current.effective_root);
    let is_fully_diminished_seventh = current.quality == QualityClass::Diminished
        && current.seventh == Some(SeventhQuality::Diminished);
    if is_fully_diminished_seventh {
        // The four members are separated by minor thirds. A target a semitone
        // above any member is consequently 1, 4, 7, or 10 semitones above the
        // written root.
        matches!(distance, 1 | 4 | 7 | 10)
    } else {
        distance == 1
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

fn detect_ordinary_interpretations(
    nodes: &mut [Option<AnalysisNode>],
    previous_chord: &[Option<usize>],
    next_chord: &[Option<usize>],
) {
    // Convert the context nodes to the small immutable surface consumed by
    // `ordinary`.  That pass does not need Blackadder candidates, spelling
    // preferences, or deterministic resolution flags.
    let observations: Vec<_> = nodes
        .iter()
        .map(|node| {
            node.as_ref().map(|node| HarmonyObservation {
                root: node.effective_root,
                tonic: node.tonic,
                global_mode: node.global_mode,
                quality: node.quality,
                seventh: node.seventh,
                is_dominant: node.is_dominant,
            })
        })
        .collect();

    let interpretations = infer_ordinary_interpretations(&observations, previous_chord, next_chord);
    for (node, candidates) in nodes.iter_mut().zip(interpretations) {
        let Some(node) = node.as_mut() else {
            continue;
        };
        for interpretation in candidates {
            push_interpretation(node, interpretation);
        }
    }
}

fn push_classification(node: &mut AnalysisNode, classification: HarmonicClassification) {
    if !node.harmonic_classifications.contains(&classification) {
        node.harmonic_classifications.push(classification);
    }
}

fn push_interpretation(node: &mut AnalysisNode, interpretation: HarmonicInterpretation) {
    push_classification(node, interpretation.classification.clone());
    if !node.harmonic_interpretations.iter().any(|existing| {
        existing.rule_id == interpretation.rule_id
            && existing.classification == interpretation.classification
    }) {
        node.harmonic_interpretations.push(interpretation);
    }
}

fn reinforce_family_for_perspective(
    node: Option<&mut AnalysisNode>,
    perspective: &TonalPerspective,
    family: InterpretationFamily,
    rule_id: &str,
    contribution: f64,
    explanation: &str,
) {
    let Some(node) = node else {
        return;
    };
    if let Some(classification) = node
        .harmonic_classifications
        .iter_mut()
        .find(|classification| classification.perspective.as_ref() == Some(perspective))
    {
        classification.add_family(family);
    }
    // The union above serves annotation clients.  Update the independently
    // scored state as well, otherwise the lattice would know that an applied
    // cadence exists but could not reward choosing it coherently.
    if let Some(interpretation) = node
        .harmonic_interpretations
        .iter_mut()
        .find(|interpretation| {
            interpretation.classification.perspective.as_ref() == Some(perspective)
        })
    {
        interpretation.classification.add_family(family);
        interpretation.add_evidence(rule_id, contribution, explanation);
    }
}
