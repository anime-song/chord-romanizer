//! Longer-range harmonic obligations and whole-path rescoring.
//!
//! The local function lattice deliberately uses first-order transitions: that
//! keeps candidate generation inspectable and prevents a combinatorial
//! second/third-order Markov state.  A first-order decoder can nevertheless
//! remember longer context when its state is structured.  This module supplies
//! that structured memory after function and key paths have been combined.
//!
//! The memory currently contains two independent facts:
//!
//! - how long the selected `active_key` region has persisted;
//! - up to two unresolved dominant targets.
//!
//! A dominant target is an *obligation*, not a promise.  It may survive a small
//! number of decorative chords, resolve into a tonic-quality chord, or resolve
//! into another dominant as part of a dominant chain.  Explicit boundaries
//! clear unresolved obligations, while the already-established active key is
//! intentionally preserved by the modulation layer.

use crate::analysis::{
    DominantRelation, HarmonicRole, InterpretationFamily, KeyedAnalysisPath, ScoreEvidence,
    TonalKey, TonalMode,
};
use crate::domain::{ParsedChord, ParsedSymbol, ProgressionItem, QualityClass, SeventhQuality};
use crate::profile::NoChordPolicy;
use crate::speller::{semitone_distance, spell_pitch_class};

/// Maximum number of unresolved dominant goals retained at once.
///
/// Depth two covers the common `V/V/V -> V/V -> V -> I` family without
/// allowing an old, abandoned target to contaminate a much later phrase.
const MAX_PENDING_DEPTH: usize = 2;

/// A target may remain audible across this many intervening chord events.
const MAX_INTERVENING_CHORDS: usize = 2;

#[derive(Clone, Debug, Eq, PartialEq)]
/// One unresolved functional expectation at a particular point in a path.
pub struct PendingResolution {
    /// Dominant/leading-tone event which opened this expectation.
    pub source_event_index: usize,
    /// Tonal target selected by this path, not merely guessed from display
    /// spelling.  Competing function paths can therefore retain different
    /// targets for the same printed chord.
    pub target_key: TonalKey,
    pub relation: DominantRelation,
    /// Number of chord events heard since the source without resolution.
    /// Zero means the target can still arrive on the immediately next chord.
    pub intervening_chords: usize,
    /// One-based position in the bounded tonicization stack.
    pub depth: usize,
    /// Optional earlier predominant/subdominant which prepared this goal.
    pub predominant_event_index: Option<usize>,
    /// Chords heard between that preparation and this dominant.
    pub predominant_intervening_chords: Option<usize>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// A predominant/subdominant waiting for a dominant in the same tonal frame.
pub struct PendingPredominant {
    pub source_event_index: usize,
    pub target_key: TonalKey,
    pub intervening_chords: usize,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
/// What kind of event discharged a remembered dominant goal.
pub enum HarmonicResolutionKind {
    /// Major/minor target arrival.
    TonicArrival,
    /// The expected root arrives as another dominant.  This both closes the
    /// old goal and normally opens the next link in a dominant sequence.
    DominantChainLink,
    /// The expected root arrives with a non-tonic, non-dominant quality.
    /// Chord symbols support the root arrival, but voicing or melody would be
    /// needed to call the resolution fully confirmed.
    RootArrival,
    /// A selected secondary-dominant-deceptive candidate discharges the
    /// expectation without arriving on the literal tonic root.
    DeceptiveArrival,
}

#[derive(Clone, Debug, PartialEq)]
/// A resolved long-range harmonic obligation in a complete path.
pub struct HarmonicResolution {
    pub source_event_index: usize,
    pub resolution_event_index: usize,
    pub target_key: TonalKey,
    pub relation: DominantRelation,
    pub kind: HarmonicResolutionKind,
    pub intervening_chords: usize,
    pub depth: usize,
    pub predominant_event_index: Option<usize>,
    pub predominant_intervening_chords: Option<usize>,
    /// Additive whole-path reranker contribution.  Immediate ordinary V-I
    /// resolutions normally have zero here because the local lattice already
    /// scored them.
    pub score: f64,
    pub evidence: Vec<ScoreEvidence>,
}

#[derive(Clone, Debug, PartialEq)]
/// One complete predominant-dominant-resolution phrase recovered from memory.
pub struct CadentialSpan {
    pub predominant_event_index: usize,
    pub dominant_event_index: usize,
    pub resolution_event_index: usize,
    pub target_key: TonalKey,
    pub dominant_relation: DominantRelation,
    pub resolution_kind: HarmonicResolutionKind,
    pub intervening_before_dominant: usize,
    pub intervening_before_resolution: usize,
    /// Only dependencies not already scored by adjacent lattice transitions
    /// contribute here.
    pub score: f64,
    pub evidence: Vec<ScoreEvidence>,
}

/// Populate per-event memory snapshots and rescore a complete joint path.
///
/// This pass is deterministic for a selected function/key path.  Ambiguity is
/// still preserved because different k-best function candidates can name
/// different `local_key` targets; each is rescored against what happens later.
pub(super) fn apply_harmonic_memory(
    progression: &[ProgressionItem],
    path: &mut KeyedAnalysisPath,
    no_chord_policy: NoChordPolicy,
) {
    let mut pending: Vec<PendingResolution> = Vec::new();
    let mut pending_predominant: Option<PendingPredominant> = None;
    let mut resolutions = Vec::new();
    let mut cadential_spans = Vec::new();
    let mut memory_evidence = Vec::new();
    let mut memory_score = 0.0;
    let mut previous_event_index = None;
    let mut previous_active_key = None;
    let mut key_region_age = 0;

    for keyed in &mut path.selections {
        let event_index = keyed.selection.event_index;

        // A phrase/section boundary cancels an unresolved dominant
        // expectation.  It does not reset `active_key`; a new phrase can
        // continue a modulation already established before the boundary.
        if previous_event_index.is_some_and(|previous| {
            has_memory_boundary(progression, previous, event_index, no_chord_policy)
        }) {
            if !pending.is_empty() || pending_predominant.is_some() {
                memory_evidence.push(ScoreEvidence::new(
                    "builtin.memory.boundary_clears_pending",
                    0.0,
                    format!(
                        "A context boundary clears {} dominant target(s) and {} cadential preparation(s)",
                        pending.len(),
                        usize::from(pending_predominant.is_some())
                    ),
                ));
            }
            pending.clear();
            pending_predominant = None;
        }

        if previous_active_key.is_some_and(|key| same_key(key, keyed.active_key)) {
            key_region_age += 1;
        } else {
            key_region_age = 1;
        }
        keyed.key_region_age_chords = key_region_age;

        let Some(chord) = progression
            .get(event_index)
            .and_then(ProgressionItem::chord)
        else {
            // Keyed selections currently contain chord events only.  Keeping
            // this guard makes the memory layer safe if that representation
            // later grows explicit rest nodes.
            previous_event_index = Some(event_index);
            previous_active_key = Some(keyed.active_key);
            continue;
        };

        let mut resolved_sources = Vec::new();
        let mut resolved_cadence_sources = Vec::new();

        // Only the most recently opened obligation may resolve.  Skipping a
        // still-pending inner goal to satisfy an older outer goal would make
        // the bounded stack behave like an unordered bag.
        if pending.last().is_some_and(|goal| {
            chord.root.pitch_class() == goal.target_key.tonic.pitch_class()
                || is_selected_deceptive_arrival(keyed, goal)
        }) {
            let goal = pending.pop().expect("last goal was just checked");
            let deceptive = is_selected_deceptive_arrival(keyed, &goal);
            let resolution = resolved_resolution(goal, chord, event_index, deceptive);
            memory_score += resolution.score;
            memory_evidence.extend(resolution.evidence.iter().cloned());
            resolved_sources.push(resolution.source_event_index);
            if let Some(cadence) = cadential_span(&resolution) {
                memory_score += cadence.score;
                memory_evidence.extend(cadence.evidence.iter().cloned());
                resolved_cadence_sources.push(cadence.predominant_event_index);
                cadential_spans.push(cadence);
            }
            resolutions.push(resolution);
        }

        // Resolve first, then open a new goal.  Thus D7 -> G7 simultaneously
        // closes D7's target and lets G7 point onward to C.
        let mut opened_source = None;
        if let Some((target_key, relation)) = dominant_goal(keyed, chord) {
            let preparation = if pending_predominant
                .as_ref()
                .is_some_and(|preparation| same_tonic(preparation.target_key, target_key))
            {
                pending_predominant.take()
            } else {
                None
            };

            if let Some(existing) = pending
                .iter_mut()
                .rev()
                .find(|goal| same_tonic(goal.target_key, target_key))
            {
                // Repeated dominants of one target do not consume another
                // stack slot.  A newly observed predominant can still enrich
                // the already-open goal.
                if existing.predominant_event_index.is_none()
                    && let Some(preparation) = preparation
                {
                    existing.predominant_event_index = Some(preparation.source_event_index);
                    existing.predominant_intervening_chords = Some(preparation.intervening_chords);
                    if existing.target_key.mode == TonalMode::Unknown {
                        existing.target_key.mode = preparation.target_key.mode;
                    }
                }
                existing.intervening_chords = 0;
                opened_source = Some(existing.source_event_index);
            } else {
                if pending.len() == MAX_PENDING_DEPTH {
                    let forgotten = pending.remove(0);
                    memory_evidence.push(ScoreEvidence::new(
                        "builtin.memory.stack_limit",
                        0.0,
                        format!(
                            "The bounded tonicization stack drops the older {} target opened at event {}",
                            forgotten.target_key, forgotten.source_event_index
                        ),
                    ));
                }
                let depth = pending.len() + 1;
                pending.push(PendingResolution {
                    source_event_index: event_index,
                    target_key: if target_key.mode == TonalMode::Unknown {
                        preparation
                            .as_ref()
                            .map_or(target_key, |preparation| preparation.target_key)
                    } else {
                        target_key
                    },
                    relation,
                    intervening_chords: 0,
                    depth,
                    predominant_event_index: preparation
                        .as_ref()
                        .map(|preparation| preparation.source_event_index),
                    predominant_intervening_chords: preparation
                        .as_ref()
                        .map(|preparation| preparation.intervening_chords),
                });
                opened_source = Some(event_index);
            }
        }

        // Open the next cadential preparation only after the current chord
        // had a chance to act as a dominant.  A dominant seventh can receive a
        // scale-degree fallback role of "predominant"; excluding that quality
        // avoids inventing a second, unrelated preparation.
        let mut opened_predominant = None;
        if let Some(target_key) = predominant_goal(keyed, chord) {
            pending_predominant = Some(PendingPredominant {
                source_event_index: event_index,
                target_key,
                intervening_chords: 0,
            });
            opened_predominant = Some(event_index);
        }

        // Every previously pending target has now heard one more intervening
        // chord.  Goals opened by the current chord remain at age zero.
        for goal in &mut pending {
            if Some(goal.source_event_index) != opened_source {
                goal.intervening_chords += 1;
            }
        }
        if let Some(preparation) = &mut pending_predominant
            && Some(preparation.source_event_index) != opened_predominant
        {
            preparation.intervening_chords += 1;
        }

        // Expiration is intentionally a very small penalty.  Dominants may
        // resolve deceptively, so failure to find the literal target is weaker
        // evidence than a positive delayed resolution.
        let mut expired = Vec::new();
        pending.retain(|goal| {
            if goal.intervening_chords > MAX_INTERVENING_CHORDS {
                expired.push(goal.clone());
                false
            } else {
                true
            }
        });
        for goal in expired {
            let contribution = -0.12;
            memory_score += contribution;
            memory_evidence.push(ScoreEvidence::new(
                "builtin.memory.expired_dominant_target",
                contribution,
                format!(
                    "The {} target opened at event {} did not arrive within the memory window",
                    goal.target_key, goal.source_event_index
                ),
            ));
        }
        if pending_predominant
            .as_ref()
            .is_some_and(|preparation| preparation.intervening_chords > MAX_INTERVENING_CHORDS)
        {
            pending_predominant = None;
        }

        keyed.pending_resolutions = pending.clone();
        keyed.resolved_resolution_sources = resolved_sources;
        keyed.pending_predominant = pending_predominant.clone();
        keyed.resolved_cadence_predominant_sources = resolved_cadence_sources;
        previous_event_index = Some(event_index);
        previous_active_key = Some(keyed.active_key);
    }

    path.memory_score = memory_score;
    path.harmonic_resolutions = resolutions;
    path.cadential_spans = cadential_spans;
    path.total_score += memory_score;
    path.evidence.extend(memory_evidence);
}

fn predominant_goal(
    keyed: &crate::analysis::KeyedPathSelection,
    chord: &ParsedChord,
) -> Option<TonalKey> {
    matches!(
        keyed.role,
        Some(HarmonicRole::Predominant | HarmonicRole::Subdominant)
    )
    .then_some(keyed.local_key)
    .filter(|_| !is_dominant_quality(chord))
}

fn is_selected_deceptive_arrival(
    keyed: &crate::analysis::KeyedPathSelection,
    goal: &PendingResolution,
) -> bool {
    same_tonic(keyed.local_key, goal.target_key)
        && keyed
            .selection
            .harmonic_classifications
            .iter()
            .any(|classification| {
                classification
                    .families
                    .contains(&InterpretationFamily::SecondaryDominantDeceptive)
            })
}

fn dominant_goal(
    keyed: &crate::analysis::KeyedPathSelection,
    chord: &ParsedChord,
) -> Option<(TonalKey, DominantRelation)> {
    // A selected semantic dominant carries the best target and relation.
    // When no local rule could see far enough ahead, a dominant-seventh chord
    // still opens the conservative fifth-related target implied by its own
    // root.  That fallback is precisely what lets D7 remember G across one or
    // two decorative events.
    let target_key = if keyed.role == Some(HarmonicRole::Dominant) {
        keyed.local_key
    } else if is_dominant_quality(chord) {
        target_of_dominant(chord)
    } else {
        return None;
    };
    if chord.root.pitch_class() == target_key.tonic.pitch_class() {
        return None;
    }

    // Prefer the relation selected by the semantic candidate.  The fallback
    // keeps plain diatonic candidates useful when they were projected to a
    // dominant role by the key layer rather than by a specialized rule.
    let relation = keyed
        .selection
        .harmonic_classifications
        .iter()
        .filter(|classification| classification.role == Some(HarmonicRole::Dominant))
        .find_map(|classification| classification.dominant_relation)
        .or_else(|| infer_relation(chord, target_key))
        .or_else(|| is_dominant_quality(chord).then_some(DominantRelation::FifthRelated))?;
    Some((target_key, relation))
}

fn target_of_dominant(chord: &ParsedChord) -> TonalKey {
    let tonic = spell_pitch_class(
        chord.root.letter.shift(3),
        chord.root.pitch_class().offset(5),
    );
    TonalKey::new(tonic, TonalMode::Unknown)
}

fn infer_relation(chord: &ParsedChord, target_key: TonalKey) -> Option<DominantRelation> {
    match semitone_distance(target_key.tonic, chord.root) {
        5 if is_dominant_quality(chord) => Some(DominantRelation::FifthRelated),
        11 if is_dominant_quality(chord) => Some(DominantRelation::TritoneSubstitute),
        2 if is_dominant_quality(chord) => Some(DominantRelation::Backdoor),
        1 if matches!(
            chord.quality_parsed.class,
            QualityClass::Diminished | QualityClass::HalfDiminished
        ) =>
        {
            Some(DominantRelation::LeadingTone)
        }
        _ => None,
    }
}

fn resolved_resolution(
    mut goal: PendingResolution,
    chord: &ParsedChord,
    resolution_event_index: usize,
    deceptive: bool,
) -> HarmonicResolution {
    let tonic_mode = stable_tonic_mode(chord);
    let kind = if deceptive {
        HarmonicResolutionKind::DeceptiveArrival
    } else if tonic_mode.is_some() {
        HarmonicResolutionKind::TonicArrival
    } else if is_dominant_quality(chord) {
        HarmonicResolutionKind::DominantChainLink
    } else {
        HarmonicResolutionKind::RootArrival
    };
    if goal.target_key.mode == TonalMode::Unknown
        && let Some(mode) = tonic_mode
    {
        goal.target_key.mode = mode;
    }

    // Adjacent resolutions have already contributed to the first-order
    // lattice transition.  Only genuinely delayed evidence and nested-chain
    // continuity alter whole-path ranking here.
    let delayed = goal.intervening_chords > 0;
    let nested = goal.depth > 1;
    let mut score = 0.0;
    let mut evidence = Vec::new();
    if delayed {
        let contribution = 0.65 - (goal.intervening_chords.saturating_sub(1) as f64 * 0.12);
        score += contribution;
        evidence.push(ScoreEvidence::new(
            "builtin.memory.delayed_dominant_resolution",
            contribution,
            format!(
                "The target {} opened at event {} resolves after {} intervening chord(s)",
                goal.target_key, goal.source_event_index, goal.intervening_chords
            ),
        ));
    }
    if nested {
        let contribution = 0.18;
        score += contribution;
        evidence.push(ScoreEvidence::new(
            "builtin.memory.nested_tonicization_resolution",
            contribution,
            format!(
                "A depth-{} tonicization goal resolves without discarding its outer context",
                goal.depth
            ),
        ));
    }

    HarmonicResolution {
        source_event_index: goal.source_event_index,
        resolution_event_index,
        target_key: goal.target_key,
        relation: goal.relation,
        kind,
        intervening_chords: goal.intervening_chords,
        depth: goal.depth,
        predominant_event_index: goal.predominant_event_index,
        predominant_intervening_chords: goal.predominant_intervening_chords,
        score,
        evidence,
    }
}

fn cadential_span(resolution: &HarmonicResolution) -> Option<CadentialSpan> {
    let predominant_event_index = resolution.predominant_event_index?;
    let intervening_before_dominant = resolution.predominant_intervening_chords?;
    let mut score = 0.0;
    let mut evidence = Vec::new();

    // Adjacent predominant-dominant motion was scored by the function
    // lattice.  This term exists only when the preparation survives a genuine
    // intervening chord.
    if intervening_before_dominant > 0 {
        let contribution = 0.35 - (intervening_before_dominant.saturating_sub(1) as f64 * 0.08);
        score += contribution;
        evidence.push(ScoreEvidence::new(
            "builtin.memory.delayed_predominant_to_dominant",
            contribution,
            format!(
                "The predominant at event {predominant_event_index} reaches its dominant after {intervening_before_dominant} intervening chord(s)"
            ),
        ));
    }

    Some(CadentialSpan {
        predominant_event_index,
        dominant_event_index: resolution.source_event_index,
        resolution_event_index: resolution.resolution_event_index,
        target_key: resolution.target_key,
        dominant_relation: resolution.relation,
        resolution_kind: resolution.kind,
        intervening_before_dominant,
        intervening_before_resolution: resolution.intervening_chords,
        score,
        evidence,
    })
}

fn has_memory_boundary(
    progression: &[ProgressionItem],
    previous_event_index: usize,
    event_index: usize,
    no_chord_policy: NoChordPolicy,
) -> bool {
    progression[previous_event_index + 1..event_index]
        .iter()
        .any(|item| match item.symbol {
            ParsedSymbol::Boundary { .. } => true,
            ParsedSymbol::NoChord { .. } => no_chord_policy == NoChordPolicy::Break,
            ParsedSymbol::Chord(_) => false,
        })
}

fn stable_tonic_mode(chord: &ParsedChord) -> Option<TonalMode> {
    if is_dominant_quality(chord) {
        return None;
    }
    match chord.quality_parsed.class {
        QualityClass::Major => Some(TonalMode::Major),
        QualityClass::Minor => Some(TonalMode::Minor),
        _ => None,
    }
}

fn is_dominant_quality(chord: &ParsedChord) -> bool {
    chord.quality_parsed.class == QualityClass::Major
        && matches!(
            chord.quality_parsed.seventh,
            Some(SeventhQuality::Minor | SeventhQuality::Diminished)
        )
}

fn same_key(left: TonalKey, right: TonalKey) -> bool {
    left.tonic.pitch_class() == right.tonic.pitch_class() && left.mode == right.mode
}

fn same_tonic(left: TonalKey, right: TonalKey) -> bool {
    left.tonic.pitch_class() == right.tonic.pitch_class()
}
