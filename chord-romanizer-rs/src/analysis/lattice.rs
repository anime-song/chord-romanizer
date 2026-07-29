//! Candidate lattice and ranked path decoding.
//!
//! A layer corresponds to one chord event and contains alternative labels or
//! functions for that event. Directed edges contain evidence about moving from
//! one candidate to a candidate in the next layer. The decoder maximizes the
//! sum of local (emission) and pairwise (transition) scores.
//!
//! Scores are currently hand-authored comparison weights. They must not be
//! presented as probabilities; the attached [`ScoreEvidence`] is the source of
//! truth for explaining why a path received its score.

use std::cmp::Ordering;

use crate::analysis::blackadder::{
    BlackadderFunction, BlackadderInterpretation, transition_evidence,
};
use crate::analysis::{HarmonicClassification, ScoreEvidence};
use crate::domain::{Degree, ProgressionItem, SpelledNote};
use crate::interpreter::HybridKind;
use crate::profile::{KeyBoundaryPolicy, NoChordPolicy};
use crate::romanizer::{AnnotatedEvent, ResolutionType, RomanizedChord, RomanizerOptions};

/// Version of candidate identities and built-in ranking rules.
pub const BUILTIN_RULE_SET_VERSION: &str = "builtin-strict-v13-dominant-prolongation";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Why a candidate exists in a layer.
pub enum InterpretationKind {
    PrimaryDegree,
    /// Legacy notation-only state. New semantic lattices keep enharmonic
    /// spellings on `RomanizedChord::alternates` instead of materializing it.
    EnharmonicDegree,
    /// Legacy notation-only state. A non-redundant written bass is never
    /// removed from a semantic interpretation path.
    RootWithoutBass,
    /// One scored meaning of an ordinary chord, such as modal interchange,
    /// a local applied cadence, or chromatic-mediant motion.
    ContextualHarmony,
    FunctionalHybrid,
}

#[derive(Clone, Debug, PartialEq)]
/// One possible interpretation for one chord event.
pub struct InterpretationCandidate {
    /// Stable within this lattice. External persistence should also record the
    /// lattice/rule-set version rather than storing this id alone.
    pub id: String,
    pub event_index: usize,
    pub label: String,
    pub tonic: SpelledNote,
    pub degree_root: Option<Degree>,
    pub kind: InterpretationKind,
    /// Score derived only from this event. Neighbor-dependent evidence belongs
    /// on [`CandidateTransition`].
    pub emission_score: f64,
    pub evidence: Vec<ScoreEvidence>,
    pub hybrid_kind: Option<HybridKind>,
    pub effective_root: Option<SpelledNote>,
    pub blackadder: Option<BlackadderInterpretation>,
    pub harmonic_classifications: Vec<HarmonicClassification>,
}

#[derive(Clone, Debug, PartialEq)]
/// All candidates associated with a single chord event.
pub struct CandidateLayer {
    pub event_index: usize,
    /// Context boundaries create independent segments while preserving one
    /// aligned list of layers for the complete input.
    pub segment_id: usize,
    pub candidates: Vec<InterpretationCandidate>,
}

#[derive(Clone, Debug, PartialEq)]
/// Pairwise evidence between candidates in adjacent layers.
pub struct CandidateTransition {
    pub from_candidate: String,
    pub to_candidate: String,
    pub score: f64,
    pub evidence: Vec<ScoreEvidence>,
}

#[derive(Clone, Debug, PartialEq)]
/// A layered directed graph of interpretations for a progression.
pub struct AnalysisLattice {
    /// Makes ranking changes reproducible when rule weights evolve.
    pub rule_set_version: String,
    pub layers: Vec<CandidateLayer>,
    pub transitions: Vec<Vec<CandidateTransition>>,
}

#[derive(Clone, Debug, PartialEq)]
/// A lightweight reference to the selected candidate at one event.
pub struct PathSelection {
    pub event_index: usize,
    pub candidate_id: String,
    pub label: String,
    /// Semantic fields are copied into the path so callers do not need to join
    /// candidate ids back to the lattice merely to inspect a k-best result.
    pub hybrid_kind: Option<HybridKind>,
    pub blackadder: Option<BlackadderInterpretation>,
    pub harmonic_classifications: Vec<HarmonicClassification>,
    /// Candidate-local contribution at this event.
    pub emission_score: f64,
    /// Contribution from the selected predecessor. Zero at the first event
    /// and after an explicit context boundary.
    pub transition_score: f64,
    /// `emission_score + transition_score`.
    pub step_score: f64,
    /// Function-path score through this event. Key-level evidence is kept
    /// separately by `KeyedAnalysisPath`.
    pub cumulative_score: f64,
    /// Evidence attributable to this node and its incoming edge. Keeping this
    /// local copy makes tree/detail UIs independent of the flattened path log.
    pub evidence: Vec<ScoreEvidence>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// Pin one event to a candidate while re-running k-best decoding.
///
/// Candidate ids are versioned lattice-local identifiers. A UI should retain
/// the tree's rule-set version with these constraints and refresh stale trees
/// after a rule-set update.
pub struct CandidateConstraint {
    pub event_index: usize,
    pub candidate_id: String,
}

#[derive(Clone, Debug, PartialEq)]
/// One complete interpretation path and its flattened explanation trail.
pub struct AnalysisPath {
    pub selections: Vec<PathSelection>,
    pub total_score: f64,
    pub evidence: Vec<ScoreEvidence>,
}

impl AnalysisLattice {
    /// Convert aligned romanizer output into candidate layers and edges.
    ///
    /// This constructor is crate-private because callers should use
    /// `Romanizer::build_lattice`, ensuring that event and option alignment has
    /// already been validated by the normal analysis pipeline.
    pub(crate) fn from_annotated_events(
        progression: &[ProgressionItem],
        events: &[AnnotatedEvent],
        options: &RomanizerOptions,
    ) -> Self {
        // Segment ids are calculated on original events before N.C./Boundary
        // entries are omitted from layers. Each retained chord therefore still
        // knows whether an edge may cross from the previous chord.
        let segment_ids = segment_ids(progression, options);
        let mut layers = Vec::new();
        let mut chord_results = Vec::new();

        for (event_index, event) in events.iter().enumerate() {
            if let AnnotatedEvent::Chord(result) = event {
                layers.push(CandidateLayer {
                    event_index,
                    segment_id: segment_ids[event_index],
                    candidates: candidates_for_result(event_index, result),
                });
                chord_results.push(result);
            }
        }

        // One transition vector is stored per gap between chord layers. Empty
        // vectors represent a hard boundary, not "zero-scored evidence".
        let mut transitions = Vec::new();
        for index in 0..layers.len().saturating_sub(1) {
            if layers[index].segment_id != layers[index + 1].segment_id {
                transitions.push(Vec::new());
                continue;
            }
            // Materialize the full cross-product because Blackadder functions
            // now produce candidate-specific transition evidence.  A
            // tritone-substitute path and an aug7-inversion path no longer get
            // the same resolution score merely because they share one symbol.
            let mut edges = Vec::new();
            for from in &layers[index].candidates {
                for to in &layers[index + 1].candidates {
                    let (transition_score, evidence) = progression_transition_score(
                        chord_results[index],
                        chord_results[index + 1],
                        from,
                        to,
                        options,
                    );
                    edges.push(CandidateTransition {
                        from_candidate: from.id.clone(),
                        to_candidate: to.id.clone(),
                        score: transition_score,
                        evidence: evidence.clone(),
                    });
                }
            }
            transitions.push(edges);
        }

        Self {
            rule_set_version: BUILTIN_RULE_SET_VERSION.to_owned(),
            layers,
            transitions,
        }
    }

    /// Decode the `k` highest-scoring complete paths.
    ///
    /// This is a k-best dynamic program over the candidate lattice; `k=1` is
    /// ordinary Viterbi output. The implementation retains `k` prefixes for
    /// every terminal candidate, not a single global beam, because a locally
    /// weaker prefix can win after a strong candidate-specific transition.
    pub fn decode_top_k(&self, k: usize) -> Vec<AnalysisPath> {
        self.decode_top_k_with_constraints(k, &[])
    }

    /// Decode after pinning zero or more event/candidate choices.
    ///
    /// This is used by an interactive tree: clicking a node fixes the complete
    /// prefix represented by that node, then descendants are recomputed from
    /// the full lattice rather than merely filtered from an old top-k list.
    pub fn decode_top_k_conditioned(
        &self,
        k: usize,
        constraints: &[CandidateConstraint],
    ) -> Vec<AnalysisPath> {
        self.decode_top_k_with_constraints(k, constraints)
    }

    fn decode_top_k_with_constraints(
        &self,
        k: usize,
        constraints: &[CandidateConstraint],
    ) -> Vec<AnalysisPath> {
        if k == 0 || self.layers.is_empty() {
            return Vec::new();
        }

        // Keep k paths *per terminal state*. A global beam at each layer is
        // insufficient once transition scores depend on the candidate pair:
        // a weaker partial path can still become one of the final k best.
        let mut paths_by_state: Vec<Vec<AnalysisPath>> = self.layers[0]
            .candidates
            .iter()
            .map(|candidate| {
                if !candidate_allowed(candidate, constraints) {
                    Vec::new()
                } else {
                    vec![AnalysisPath {
                        selections: vec![selection(candidate, 0.0, candidate.emission_score, &[])],
                        total_score: candidate.emission_score,
                        evidence: candidate.evidence.clone(),
                    }]
                }
            })
            .collect();

        // Recurrence:
        // best[t][state] = top_k over (
        //     best[t-1][previous_state]
        //     + transition(previous_state, state)
        //     + emission(state)
        // )
        for layer_index in 1..self.layers.len() {
            let layer = &self.layers[layer_index];
            let starts_new_segment = self.layers[layer_index - 1].segment_id != layer.segment_id;
            let previous_layer = &self.layers[layer_index - 1];
            let mut next_paths_by_state = Vec::with_capacity(layer.candidates.len());

            for candidate in &layer.candidates {
                if !candidate_allowed(candidate, constraints) {
                    next_paths_by_state.push(Vec::new());
                    continue;
                }
                // Gather every path that can terminate at this candidate, then
                // prune only within this terminal state.
                let mut expanded = Vec::new();
                for (previous_state, state_paths) in paths_by_state.iter().enumerate() {
                    let previous_id = &previous_layer.candidates[previous_state].id;
                    for path in state_paths {
                        // A new segment restarts transition scoring but keeps a
                        // single combined result path for convenient output.
                        let edge = if starts_new_segment {
                            None
                        } else {
                            self.transitions[layer_index - 1].iter().find(|edge| {
                                edge.from_candidate == *previous_id
                                    && edge.to_candidate == candidate.id
                            })
                        };
                        let mut next_path = path.clone();
                        let transition_score = edge.map_or(0.0, |edge| edge.score);
                        let step_score = candidate.emission_score + transition_score;
                        next_path.total_score += step_score;
                        let edge_evidence = edge.map_or(&[][..], |edge| edge.evidence.as_slice());
                        next_path.selections.push(selection(
                            candidate,
                            transition_score,
                            next_path.total_score,
                            edge_evidence,
                        ));
                        next_path.evidence.extend(candidate.evidence.clone());
                        if let Some(edge) = edge {
                            next_path.evidence.extend(edge.evidence.clone());
                        }
                        expanded.push(next_path);
                    }
                }
                sort_and_truncate(&mut expanded, k);
                next_paths_by_state.push(expanded);
            }
            paths_by_state = next_paths_by_state;
        }

        // Only after the final layer is it safe to compare terminal states and
        // select the global top k.
        let mut paths: Vec<_> = paths_by_state.into_iter().flatten().collect();
        sort_and_truncate(&mut paths, k);
        paths
    }

    /// Return ranked progression interpretations without notation-only paths.
    ///
    /// Candidate construction already excludes enharmonic display variants
    /// and slash-bass omission labels, and merges exact semantic duplicates in
    /// each layer. Consequently every returned path differs in at least one
    /// harmonic state rather than only in rendering.
    pub fn decode_top_k_interpretations(&self, k: usize) -> Vec<AnalysisPath> {
        self.decode_top_k(k)
    }

    pub fn decode_top_k_interpretations_conditioned(
        &self,
        k: usize,
        constraints: &[CandidateConstraint],
    ) -> Vec<AnalysisPath> {
        self.decode_top_k_conditioned(k, constraints)
    }
}

fn candidate_allowed(
    candidate: &InterpretationCandidate,
    constraints: &[CandidateConstraint],
) -> bool {
    constraints
        .iter()
        .find(|constraint| constraint.event_index == candidate.event_index)
        .is_none_or(|constraint| constraint.candidate_id == candidate.id)
}

fn candidates_for_result(
    event_index: usize,
    result: &RomanizedChord,
) -> Vec<InterpretationCandidate> {
    // The primary romanization is deliberately neutral (score 0). Alternate
    // penalties express a display/notation preference, not impossibility.
    let mut primary = candidate(
        event_index,
        0,
        &result.roman,
        result.tonic,
        Some(result.degree_root),
        InterpretationKind::PrimaryDegree,
        0.0,
        "builtin.degree.primary",
        "Primary contextual degree spelling",
    );
    // Before ordinary candidates existed, the primary degree state carried
    // the union of all contextual labels.  Once alternatives are available
    // that would collapse ambiguity again, so the neutral state remains
    // unclassified.  Legacy/profile results with no scored alternatives keep
    // the old annotation projection.
    if result.harmonic_interpretations.is_empty() {
        primary
            .harmonic_classifications
            .clone_from(&result.harmonic_classifications);
    }
    let mut candidates = vec![primary];

    // `result.alternates` contains notation choices: enharmonic degree labels
    // and, for compatibility, a root-only rendering of slash input. Neither is
    // a different sounding or functional interpretation, so neither belongs
    // in the Viterbi state space. Callers can still inspect those labels on the
    // annotated chord without letting them consume k-best result slots.

    // Keep every semantic interpretation even when two candidates render the
    // same label.  For example, backdoor-dominant and SDm readings often have
    // identical chord symbols but must remain different Viterbi states.
    for interpretation in &result.harmonic_interpretations {
        let ordinal = candidates.len();
        let candidate = InterpretationCandidate {
            id: format!("event-{event_index}:candidate-{ordinal}"),
            event_index,
            label: result.roman.clone(),
            tonic: result.tonic,
            degree_root: Some(result.degree_root),
            kind: InterpretationKind::ContextualHarmony,
            emission_score: interpretation.intrinsic_score,
            evidence: interpretation.evidence.clone(),
            hybrid_kind: None,
            effective_root: Some(result.chord.root),
            blackadder: None,
            harmonic_classifications: vec![interpretation.classification.clone()],
        };
        push_semantic_candidate(&mut candidates, candidate);
    }

    for interpretation in &result.functional_interpretations {
        let ordinal = candidates.len();
        let evidence = if interpretation.evidence.is_empty() {
            vec![ScoreEvidence::new(
                interpretation.rule_id.clone(),
                interpretation.intrinsic_score,
                "Functional interpretation of a slash chord",
            )]
        } else {
            interpretation.evidence.clone()
        };
        let harmonic_classifications =
            if interpretation.classification == HarmonicClassification::default() {
                result.harmonic_classifications.clone()
            } else {
                vec![interpretation.classification.clone()]
            };
        let candidate = InterpretationCandidate {
            id: format!("event-{event_index}:candidate-{ordinal}"),
            event_index,
            label: interpretation.label.clone(),
            tonic: result.tonic,
            degree_root: Some(result.degree_root),
            kind: InterpretationKind::FunctionalHybrid,
            emission_score: interpretation.intrinsic_score,
            evidence,
            hybrid_kind: Some(interpretation.hybrid_kind),
            effective_root: interpretation.effective_root,
            blackadder: interpretation.blackadder.clone(),
            harmonic_classifications,
        };
        push_semantic_candidate(&mut candidates, candidate);
    }
    candidates
}

fn push_semantic_candidate(
    candidates: &mut Vec<InterpretationCandidate>,
    mut candidate: InterpretationCandidate,
) {
    if let Some(existing) = candidates
        .iter_mut()
        .find(|existing| same_semantic_candidate(existing, &candidate))
    {
        // Rules may independently discover the same latent state. Preserve
        // only its strongest emission while retaining the original stable id
        // so transition references remain deterministic.
        if candidate.emission_score > existing.emission_score {
            candidate.id.clone_from(&existing.id);
            *existing = candidate;
        }
    } else {
        candidates.push(candidate);
    }
}

fn same_semantic_candidate(
    left: &InterpretationCandidate,
    right: &InterpretationCandidate,
) -> bool {
    left.kind == right.kind
        && left.degree_root == right.degree_root
        && left.hybrid_kind == right.hybrid_kind
        && left.effective_root == right.effective_root
        && left.blackadder == right.blackadder
        && left.harmonic_classifications == right.harmonic_classifications
}

#[allow(clippy::too_many_arguments)]
fn candidate(
    event_index: usize,
    ordinal: usize,
    label: &str,
    tonic: SpelledNote,
    degree_root: Option<Degree>,
    kind: InterpretationKind,
    score: f64,
    rule_id: &str,
    explanation: &str,
) -> InterpretationCandidate {
    InterpretationCandidate {
        id: format!("event-{event_index}:candidate-{ordinal}"),
        event_index,
        label: label.to_owned(),
        tonic,
        degree_root,
        kind,
        emission_score: score,
        evidence: vec![ScoreEvidence {
            rule_id: rule_id.to_owned(),
            contribution: score,
            explanation: explanation.to_owned(),
        }],
        hybrid_kind: None,
        effective_root: None,
        blackadder: None,
        harmonic_classifications: Vec::new(),
    }
}

fn selection(
    candidate: &InterpretationCandidate,
    transition_score: f64,
    cumulative_score: f64,
    transition_evidence: &[ScoreEvidence],
) -> PathSelection {
    let mut evidence = candidate.evidence.clone();
    evidence.extend_from_slice(transition_evidence);
    PathSelection {
        event_index: candidate.event_index,
        candidate_id: candidate.id.clone(),
        label: candidate.label.clone(),
        hybrid_kind: candidate.hybrid_kind,
        blackadder: candidate.blackadder.clone(),
        harmonic_classifications: candidate.harmonic_classifications.clone(),
        emission_score: candidate.emission_score,
        transition_score,
        step_score: candidate.emission_score + transition_score,
        cumulative_score,
        evidence,
    }
}

fn progression_transition_score(
    previous: &RomanizedChord,
    current: &RomanizedChord,
    from_candidate: &InterpretationCandidate,
    to_candidate: &InterpretationCandidate,
    options: &RomanizerOptions,
) -> (f64, Vec<ScoreEvidence>) {
    // This is the built-in transition baseline. Candidate-specific root,
    // function, and voice-leading features can later add edge-local evidence
    // without changing the decoder.
    let mut score = 0.0;
    let mut evidence = Vec::new();
    if previous.is_ii_v_start {
        add_evidence(
            &mut score,
            &mut evidence,
            "builtin.progression.ii_v",
            1.5,
            "Minor or diminished chord moves by fifth to a dominant",
        );
    }
    match current.resolution_type {
        Some(ResolutionType::Perfect) => add_evidence(
            &mut score,
            &mut evidence,
            "builtin.progression.perfect_resolution",
            2.0,
            "Dominant resolves by perfect fifth",
        ),
        Some(ResolutionType::Semitone) => add_evidence(
            &mut score,
            &mut evidence,
            "builtin.progression.semitone_resolution",
            1.0,
            "Dominant resolves downward by semitone",
        ),
        Some(ResolutionType::Backdoor) => add_evidence(
            &mut score,
            &mut evidence,
            "builtin.progression.backdoor_resolution",
            1.5,
            "Flat-seven dominant resolves upward by whole tone",
        ),
        Some(ResolutionType::LeadingTone) => add_evidence(
            &mut score,
            &mut evidence,
            "builtin.progression.leading_tone_resolution",
            1.5,
            "Leading-tone chord resolves upward by semitone",
        ),
        Some(ResolutionType::Deceptive) => add_evidence(
            &mut score,
            &mut evidence,
            "builtin.progression.deceptive_resolution",
            0.75,
            "Applied dominant diverts to a tonic substitute in its temporary key",
        ),
        None => {}
    }

    add_semantic_transition_evidence(&mut score, &mut evidence, from_candidate, to_candidate);
    add_degree_specific_transition_evidence(
        &mut score,
        &mut evidence,
        previous,
        current,
        from_candidate,
    );

    if let Some(blackadder) = &from_candidate.blackadder {
        let blackadder_evidence =
            transition_evidence(blackadder, &current.chord, previous.tonic, options.behavior);
        for item in &blackadder_evidence {
            score += item.contribution;
            evidence.push(item.clone());
        }
        if !blackadder_evidence.is_empty()
            && from_candidate
                .harmonic_classifications
                .iter()
                .any(|classification| {
                    classification
                        .families
                        .contains(&crate::analysis::InterpretationFamily::VoiceLeadingRequired)
                })
        {
            add_evidence(
                &mut score,
                &mut evidence,
                "builtin.progression.voice_leading_required",
                -5.25,
                "Retained augmented upper structure weakens a function inferred from bass motion alone",
            );
        }
    }

    let selected_sus = matches!(
        from_candidate.hybrid_kind,
        Some(HybridKind::SusFourNine | HybridKind::SusFourSevenFlatNine)
    );
    let next_keeps_functional_root = from_candidate.effective_root.is_some()
        && from_candidate.effective_root == to_candidate.effective_root;
    let next_is_dominant = to_candidate
        .harmonic_classifications
        .iter()
        .any(|classification| classification.role == Some(crate::analysis::HarmonicRole::Dominant));
    if selected_sus && next_keeps_functional_root && next_is_dominant {
        add_evidence(
            &mut score,
            &mut evidence,
            "builtin.progression.suspension_to_dominant",
            0.8,
            "Suspended dominant retains its functional bass as the suspension resolves",
        );
    }
    (score, evidence)
}

fn add_degree_specific_transition_evidence(
    score: &mut f64,
    evidence: &mut Vec<ScoreEvidence>,
    previous: &RomanizedChord,
    current: &RomanizedChord,
    from: &InterpretationCandidate,
) {
    use crate::analysis::{HarmonicSource, InterpretationFamily};

    // These cadence gestures are meaningful only inside one global key. A
    // per-event tonic change represents a hard analytical boundary here even
    // if the two sounding roots happen to be a semitone apart.
    if previous.tonic.pitch_class() != current.tonic.pitch_class() {
        return;
    }
    let from_degree = previous
        .chord
        .root
        .pitch_class()
        .distance_from(previous.tonic.pitch_class());
    let to_degree = current
        .chord
        .root
        .pitch_class()
        .distance_from(current.tonic.pitch_class());

    for classification in &from.harmonic_classifications {
        // bVI -> V is more specific than a generic equal-quality chromatic
        // slide. When the selected bVI state explicitly comes from the
        // subdominant-minor collection, reward that predominant-to-dominant
        // hearing so it outranks `chromatic_approach`.
        if from_degree == 8
            && to_degree == 7
            && classification
                .families
                .contains(&InterpretationFamily::SubdominantMinor)
        {
            add_evidence(
                score,
                evidence,
                "builtin.progression.flat_six_to_dominant",
                0.7,
                "Subdominant-minor flat-six harmony descends to the global dominant",
            );
        }

        // A plain bII -> I can be described as a chromatic slide, modal
        // Phrygian motion, or a direct Neapolitan/Phrygian cadence. Preserve
        // all candidates but rank the functionally/modal-specific readings
        // above the structure-only chromatic approach. Dominant-quality bII7
        // remains governed by the stronger tritone-substitute rules.
        if from_degree == 1 && to_degree == 0 {
            if classification
                .families
                .contains(&InterpretationFamily::Neapolitan)
            {
                add_evidence(
                    score,
                    evidence,
                    "builtin.progression.flat_two_neapolitan_to_tonic",
                    0.65,
                    "Neapolitan/Phrygian flat-two resolves directly to the global tonic",
                );
            } else if classification.sources.contains(&HarmonicSource::Phrygian) {
                add_evidence(
                    score,
                    evidence,
                    "builtin.progression.flat_two_phrygian_to_tonic",
                    0.45,
                    "Phrygian flat-two descends directly to the global tonic",
                );
            }
        }
    }
}

fn add_semantic_transition_evidence(
    score: &mut f64,
    evidence: &mut Vec<ScoreEvidence>,
    from: &InterpretationCandidate,
    to: &InterpretationCandidate,
) {
    use crate::analysis::{HarmonicRole, InterpretationFamily};

    for from_classification in &from.harmonic_classifications {
        for to_classification in &to.harmonic_classifications {
            let same_perspective = from_classification.perspective.is_some()
                && from_classification.perspective == to_classification.perspective;
            if !same_perspective {
                continue;
            }

            let coherent_applied_two_five = from_classification.role
                == Some(HarmonicRole::Predominant)
                && to_classification.role == Some(HarmonicRole::Dominant)
                && from_classification
                    .families
                    .contains(&InterpretationFamily::AppliedCadence)
                && to_classification
                    .families
                    .contains(&InterpretationFamily::AppliedCadence);
            if coherent_applied_two_five {
                add_evidence(
                    score,
                    evidence,
                    "builtin.progression.coherent_applied_two_five",
                    0.85,
                    "Adjacent candidates select ii and V in the same temporary key",
                );
            }

            let coherent_dominant_target = from_classification.role == Some(HarmonicRole::Dominant)
                && from_classification.dominant_relation.is_some()
                && to_classification.role == Some(HarmonicRole::Tonic);
            if coherent_dominant_target {
                add_evidence(
                    score,
                    evidence,
                    "builtin.progression.coherent_dominant_target",
                    0.9,
                    "Dominant and target candidates use the same tonal perspective",
                );
            }

            let coherent_dominant_prolongation = from_classification.role
                == Some(HarmonicRole::Dominant)
                && to_classification.role == Some(HarmonicRole::Dominant)
                && from.blackadder.as_ref().is_some_and(|reading| {
                    reading.function == Some(BlackadderFunction::TritoneSubstitute)
                });
            if coherent_dominant_prolongation {
                add_evidence(
                    score,
                    evidence,
                    "builtin.progression.dominant_prolongation",
                    0.8,
                    "Tritone-substitute dominant moves to the dominant that shares its tonic target",
                );
            }

            let coherent_neapolitan = from_classification.role == Some(HarmonicRole::Predominant)
                && from_classification
                    .families
                    .contains(&InterpretationFamily::Neapolitan)
                && to_classification.role == Some(HarmonicRole::Dominant);
            if coherent_neapolitan {
                add_evidence(
                    score,
                    evidence,
                    "builtin.progression.neapolitan_to_dominant",
                    0.65,
                    "Neapolitan candidate proceeds to the dominant in the same key",
                );
            }

            let coherent_secondary_deceptive = from_classification.role
                == Some(HarmonicRole::Dominant)
                && to_classification.role == Some(HarmonicRole::Tonic)
                && from_classification
                    .families
                    .contains(&InterpretationFamily::SecondaryDominantDeceptive)
                && to_classification
                    .families
                    .contains(&InterpretationFamily::SecondaryDominantDeceptive);
            if coherent_secondary_deceptive {
                add_evidence(
                    score,
                    evidence,
                    "builtin.progression.coherent_secondary_deceptive",
                    0.75,
                    "Dominant and tonic-substitute candidates share one temporary-key reading",
                );
            }

            let coherent_alternate_key_pair = from_classification
                .families
                .contains(&InterpretationFamily::AlternateKeySequence)
                && to_classification
                    .families
                    .contains(&InterpretationFamily::AlternateKeySequence);
            if coherent_alternate_key_pair {
                add_evidence(
                    score,
                    evidence,
                    "builtin.progression.coherent_alternate_key_pair",
                    0.7,
                    "Adjacent chord roles are coherent in the same alternate key",
                );
            }

            // This is the actual tonal-state continuation term used by the
            // Viterbi decoder. It is intentionally small: a local key should
            // persist when several adjacent candidates support it, but a
            // single speculative pair must not erase stronger global
            // functional evidence.
            let continues_local_key =
                from_classification
                    .perspective
                    .as_ref()
                    .is_some_and(|perspective| {
                        perspective.scope == crate::analysis::TonalScope::Tonicization
                    });
            if continues_local_key {
                add_evidence(
                    score,
                    evidence,
                    "builtin.progression.continue_local_tonal_state",
                    0.25,
                    "Adjacent candidates continue the same temporary tonal state",
                );
            }
        }
    }
}

fn add_evidence(
    score: &mut f64,
    evidence: &mut Vec<ScoreEvidence>,
    rule_id: &str,
    contribution: f64,
    explanation: &str,
) {
    *score += contribution;
    evidence.push(ScoreEvidence {
        rule_id: rule_id.to_owned(),
        contribution,
        explanation: explanation.to_owned(),
    });
}

fn segment_ids(progression: &[ProgressionItem], options: &RomanizerOptions) -> Vec<usize> {
    let mut segment = 0;
    let mut output = vec![0; progression.len()];
    let mut last_chord_tonic = None;
    let mut pending_boundary = false;

    // `pending_boundary` delays the numeric segment increment until the next
    // chord. Boundary and N.C. events therefore remain assigned to the segment
    // before them, while the next actual candidate layer starts a new segment.
    for (index, item) in progression.iter().enumerate() {
        match &item.symbol {
            crate::domain::ParsedSymbol::Boundary { .. } => {
                pending_boundary = true;
                last_chord_tonic = None;
            }
            crate::domain::ParsedSymbol::NoChord { .. } => {
                if options.no_chord_policy == NoChordPolicy::Break {
                    pending_boundary = true;
                    last_chord_tonic = None;
                }
            }
            crate::domain::ParsedSymbol::Chord(_) => {
                let tonic = item.tonic.unwrap_or(options.default_tonic);
                let key_break = options.key_boundary_policy == KeyBoundaryPolicy::Break
                    && last_chord_tonic.is_some_and(|previous| previous != tonic);
                if pending_boundary || key_break {
                    segment += 1;
                    pending_boundary = false;
                }
                last_chord_tonic = Some(tonic);
            }
        }
        output[index] = segment;
    }
    output
}

fn sort_and_truncate(paths: &mut Vec<AnalysisPath>, k: usize) {
    // Candidate ids provide deterministic ordering when floating-point scores
    // tie, keeping golden tests and UI output stable across runs.
    paths.sort_by(|left, right| {
        right
            .total_score
            .partial_cmp(&left.total_score)
            .unwrap_or(Ordering::Equal)
            .then_with(|| {
                let left_ids = left
                    .selections
                    .iter()
                    .map(|selection| selection.candidate_id.as_str())
                    .collect::<Vec<_>>();
                let right_ids = right
                    .selections
                    .iter()
                    .map(|selection| selection.candidate_id.as_str())
                    .collect::<Vec<_>>();
                left_ids.cmp(&right_ids)
            })
    });
    paths.truncate(k);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ProgressionItem, Romanizer, parse_chord};

    #[test]
    fn returns_multiple_ranked_paths_with_evidence() {
        // A plain progression now has one semantic path; notation-only degree
        // spellings no longer manufacture alternatives. Use a genuine
        // Blackadder ambiguity to exercise k-best decoding.
        let progression: Vec<_> = ["Daug/C", "B"]
            .into_iter()
            .map(|symbol| ProgressionItem::new(parse_chord(symbol).unwrap()))
            .collect();
        let romanizer = Romanizer::new("B").unwrap();
        let lattice = romanizer.build_lattice(&progression);
        let paths = lattice.decode_top_k(3);
        assert_eq!(paths.len(), 3);
        assert!(paths[0].total_score >= paths[1].total_score);
        assert!(paths[0].evidence.iter().any(|evidence| {
            evidence.rule_id == "builtin.blackadder.transition.tritone_substitute"
        }));
    }

    #[test]
    fn decoder_preserves_a_weaker_prefix_that_wins_on_transition() {
        let tonic = SpelledNote::parse("C").unwrap();
        let first = CandidateLayer {
            event_index: 0,
            segment_id: 0,
            candidates: vec![
                candidate(
                    0,
                    0,
                    "A",
                    tonic,
                    None,
                    InterpretationKind::PrimaryDegree,
                    10.0,
                    "test.a",
                    "locally best",
                ),
                candidate(
                    0,
                    1,
                    "B",
                    tonic,
                    None,
                    InterpretationKind::PrimaryDegree,
                    9.0,
                    "test.b",
                    "locally second",
                ),
            ],
        };
        let second_candidate = candidate(
            1,
            0,
            "C",
            tonic,
            None,
            InterpretationKind::PrimaryDegree,
            0.0,
            "test.c",
            "target",
        );
        let second = CandidateLayer {
            event_index: 1,
            segment_id: 0,
            candidates: vec![second_candidate.clone()],
        };
        let lattice = AnalysisLattice {
            rule_set_version: "test".to_owned(),
            layers: vec![first.clone(), second],
            transitions: vec![vec![
                CandidateTransition {
                    from_candidate: first.candidates[0].id.clone(),
                    to_candidate: second_candidate.id.clone(),
                    score: -100.0,
                    evidence: Vec::new(),
                },
                CandidateTransition {
                    from_candidate: first.candidates[1].id.clone(),
                    to_candidate: second_candidate.id,
                    score: 0.0,
                    evidence: Vec::new(),
                },
            ]],
        };

        let best = lattice.decode_top_k(1);
        assert_eq!(best[0].selections[0].label, "B");
        assert_eq!(best[0].total_score, 9.0);
    }
}
