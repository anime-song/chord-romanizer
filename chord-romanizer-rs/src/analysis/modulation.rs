//! Progression-level modulation and pivot-chord inference.
//!
//! A secondary dominant and a modulation can contain the same local `V7-I`.
//! The distinction is therefore not made from one chord in isolation.  This
//! module works backwards from a cadence in a possible new key, following the
//! standard analytical workflow:
//!
//! 1. find a cadence that can confirm a target key;
//! 2. measure how much surrounding harmony persists in that key;
//! 3. search backwards for a plausible reinterpretation point;
//! 4. keep both the tonicization and modulation readings in the k-best space.
//!
//! The detector is intentionally symbol-only.  It can establish common-*chord*
//! pivots because root and quality are explicit.  It does not assert
//! common-*tone* modulation, melodic sequence, or enharmonic voice-leading
//! without note-level evidence; future MIDI observations can add those terms
//! without changing the public span model.

use std::cmp::Ordering;

use crate::analysis::{
    HarmonicRole, InterpretationFamily, KeyedAnalysisPath, ScoreEvidence, TonalKey, TonalMode,
    TonalScope,
};
use crate::domain::{
    Degree, ParsedChord, ParsedSymbol, ProgressionItem, QualityClass, SeventhQuality,
};
use crate::profile::NoChordPolicy;
use crate::speller::{degree_from_spelling, semitone_distance};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
/// The harmonic device which connects the old and new key regions.
pub enum ModulationMechanism {
    /// One chord has the same root and diatonic quality in both keys.
    DiatonicPivot,
    /// A borrowed/secondary/Neapolitan/augmented-sixth interpretation in the
    /// old key becomes a functional chord in the new key.
    ChromaticPivot,
    /// A dominant in the old context is immediately followed by the new
    /// key's dominant.  This is a dominant bridge, not a common-chord pivot.
    DominantBridge,
    /// Fifth-related dominant sevenths form a chain ending in the new V7-I.
    DominantSequence,
    /// The new dominant and tonic arrive without a supported pivot.
    DirectDominant,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
/// More specific provenance for a chromatic common chord.
pub enum PivotKind {
    DiatonicCommonChord,
    SecondaryCommonChord,
    BorrowedCommonChord,
    NeapolitanCommonChord,
    AugmentedSixthCommonChord,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
/// Strength of the cadence which confirms a target key.
pub enum ModulationCadence {
    Authentic,
    PredominantAuthentic,
}

#[derive(Clone, Debug, PartialEq)]
/// Dual interpretation of one chord at a key boundary.
pub struct PivotChord {
    pub event_index: usize,
    pub chord_symbol: String,
    pub kind: PivotKind,
    pub old_key: TonalKey,
    pub new_key: TonalKey,
    pub old_degree: Degree,
    pub new_degree: Degree,
    pub old_role: Option<HarmonicRole>,
    pub new_role: Option<HarmonicRole>,
}

#[derive(Clone, Debug, PartialEq)]
/// One confirmed non-global key region in a complete interpretation path.
pub struct ModulationSpan {
    pub from_key: TonalKey,
    pub to_key: TonalKey,
    /// First event interpreted from `to_key`.  For a common-chord modulation
    /// this is the pivot; otherwise it is the first bridge/preparation chord.
    pub start_event_index: usize,
    /// Event containing the target-key dominant.
    pub dominant_event_index: usize,
    /// Event containing the tonic arrival that confirms the new key.
    pub confirmation_event_index: usize,
    /// Last event retained in the new key before a confirmed return to the
    /// global key or an explicit context boundary.
    pub end_event_index: usize,
    /// Number of chord events in the selected key region.  Rests/boundaries
    /// do not inflate this value.
    pub duration_chords: usize,
    pub mechanism: ModulationMechanism,
    pub cadence: ModulationCadence,
    pub pivot: Option<PivotChord>,
    /// Additive comparison score.  It is not a probability.
    pub score: f64,
    pub evidence: Vec<ScoreEvidence>,
}

#[derive(Clone, Copy)]
struct IndexedChord<'a> {
    event_index: usize,
    chord: &'a ParsedChord,
}

#[derive(Clone, Debug)]
/// One partial route through the segmental key-state graph.
///
/// `last_confirmation_index` is local to the segment currently being
/// expanded.  It prevents a later transition from reaching backwards past the
/// cadence which established the present key, while still allowing that tonic
/// chord itself to become the next pivot.
struct TonalStatePath {
    active_key: TonalKey,
    spans: Vec<ModulationSpan>,
    score: f64,
    last_confirmation_index: Option<usize>,
}

struct SpanCandidate {
    span: ModulationSpan,
    start_index: usize,
}

/// Add k-best multi-stage key-state alternatives for one function path.
///
/// Returning the unchanged path is essential: a short `V7-I` can be either a
/// tonicization or a brief modulation, and chord symbols alone should not
/// erase that ambiguity.  Every changed path gets a tonal suffix on candidate
/// ids so the interpretation tree can keep two otherwise-identical function
/// prefixes as separate, conditionable branches.
///
/// This is a segmental dynamic program rather than a greedy sequence of local
/// upgrades. At every authentic cadence, each retained state can either keep
/// its current key or transition to the cadence's tonic. Therefore a later
/// transition is scored from the key actually selected earlier:
///
/// ```text
/// C major --D7/G--> G major --A7/D--> D major --G7/C--> C major
/// ```
///
/// The beam is deliberately wider than the caller-visible top-k because the
/// function lattice, key sequence, and a persisted tree condition are ranked
/// together only after all three layers have been expanded.
pub(super) fn expand_modulation_paths(
    progression: &[ProgressionItem],
    path: KeyedAnalysisPath,
    no_chord_policy: NoChordPolicy,
    k: usize,
) -> Vec<KeyedAnalysisPath> {
    let beam_width = k.max(32);
    let mut states = vec![TonalStatePath {
        active_key: path.global_key,
        spans: Vec::new(),
        score: 0.0,
        last_confirmation_index: None,
    }];

    for segment in indexed_chord_segments(progression, no_chord_policy) {
        states = expand_segment(states, &segment, progression, &path, beam_width);
    }

    // Duration is a segment-level (semi-Markov) potential: it can only be
    // scored after later key changes have fixed every span's actual end.
    for state in &mut states {
        finalize_duration_scores(state, progression);
    }

    let mut alternatives = states
        .into_iter()
        .map(|state| apply_modulations(progression, path.clone(), state.spans))
        .collect::<Vec<_>>();
    alternatives.sort_by(compare_modulation_paths);
    alternatives.truncate(k);
    alternatives
}

fn expand_segment(
    mut states: Vec<TonalStatePath>,
    segment: &[IndexedChord<'_>],
    progression: &[ProgressionItem],
    path: &KeyedAnalysisPath,
    beam_width: usize,
) -> Vec<TonalStatePath> {
    if segment.is_empty() {
        return states;
    }

    // A context boundary blocks pivot search across the gap, but does not
    // silently force the active key back to the global key. A new phrase may
    // continue the modulation or begin a direct modulation from it.
    for state in &mut states {
        state.last_confirmation_index = None;
    }

    for cadence_index in 1..segment.len() {
        let dominant = segment[cadence_index - 1];
        let tonic = segment[cadence_index];
        if !is_dominant_quality(dominant.chord) {
            continue;
        }
        let Some(mode) = stable_tonic_mode(tonic.chord) else {
            continue;
        };
        let target_key = TonalKey::new(tonic.chord.root, mode);
        if semitone_distance(dominant.chord.root, target_key.tonic) != 7 {
            continue;
        }

        // A cadence in the already-active key does not create a new span, but
        // it does establish that state. A different cadence immediately after
        // it must therefore pay the same rapid-reversal cost as a state entered
        // by modulation.
        for state in &mut states {
            if same_key(state.active_key, target_key) {
                state.last_confirmation_index = Some(cadence_index);
            }
        }
        let retained = states.clone();
        let mut expanded = retained.clone(); // staying in the current key
        for state in retained {
            if same_key(state.active_key, target_key) {
                continue;
            }

            let earliest_pivot = state.last_confirmation_index.unwrap_or(0);
            let mut candidate = build_span_candidate(
                segment,
                cadence_index,
                progression,
                path,
                state.active_key,
                target_key,
                earliest_pivot,
            );
            if state
                .last_confirmation_index
                .is_some_and(|previous| cadence_index.saturating_sub(previous) <= 2)
            {
                add_score(
                    &mut candidate.span.score,
                    &mut candidate.span.evidence,
                    "builtin.modulation.rapid_key_reversal",
                    -1.6,
                    "A second key change follows before the previous key has persisted".to_owned(),
                );
            }
            let mut transitioned = state;
            clip_previous_span(&mut transitioned.spans, segment, candidate.start_index);
            transitioned.score += candidate.span.score;
            transitioned.active_key = target_key;
            transitioned.last_confirmation_index = Some(cadence_index);
            transitioned.spans.push(candidate.span);
            expanded.push(transitioned);
        }
        states = prune_states(expanded, beam_width);
    }

    // A selected key remains active through the end of the current context
    // segment unless another selected transition clipped it earlier.
    let segment_end = segment.last().expect("non-empty segment").event_index;
    for state in &mut states {
        if let Some(last) = state.spans.last_mut()
            && same_key(last.to_key, state.active_key)
        {
            last.end_event_index = segment_end;
        }
    }
    prune_states(states, beam_width)
}

fn build_span_candidate(
    segment: &[IndexedChord<'_>],
    cadence_index: usize,
    progression: &[ProgressionItem],
    path: &KeyedAnalysisPath,
    from_key: TonalKey,
    target_key: TonalKey,
    earliest_pivot: usize,
) -> SpanCandidate {
    let dominant_index = cadence_index - 1;
    let dominant = segment[dominant_index];
    let tonic = segment[cadence_index];
    let scoring_end_index = scoring_end_before_foreign_cadence(segment, cadence_index, target_key);

    let cadence = if dominant_index > 0
        && is_predominant_for(segment[dominant_index - 1].chord, target_key)
    {
        ModulationCadence::PredominantAuthentic
    } else {
        ModulationCadence::Authentic
    };

    let dominant_run_start = dominant_sequence_start(segment, dominant_index, earliest_pivot);
    let immediate_previous_is_dominant =
        dominant_index > earliest_pivot && is_dominant_quality(segment[dominant_index - 1].chord);

    // A consecutive dominant bridge is analytically more specific than an
    // earlier shared triad.  Treating that earlier triad as the pivot would
    // misdescribe examples such as old V7 -> new V7 -> new I.
    let (mechanism, pivot, start_index) = if immediate_previous_is_dominant {
        let mechanism = if dominant_run_start < dominant_index {
            ModulationMechanism::DominantSequence
        } else {
            ModulationMechanism::DominantBridge
        };
        (mechanism, None, dominant_index - 1)
    } else if let Some((pivot_index, pivot)) = find_pivot(
        segment,
        dominant_index,
        path,
        from_key,
        target_key,
        earliest_pivot,
    ) {
        let mechanism = if pivot.kind == PivotKind::DiatonicCommonChord {
            ModulationMechanism::DiatonicPivot
        } else {
            ModulationMechanism::ChromaticPivot
        };
        (mechanism, Some(pivot), pivot_index)
    } else {
        (ModulationMechanism::DirectDominant, None, dominant_index)
    };

    let target_support = segment[start_index..=scoring_end_index]
        .iter()
        .filter(|event| is_diatonic_chord(event.chord, target_key))
        .count();
    let persistence = if cadence_index < scoring_end_index {
        segment[cadence_index + 1..=scoring_end_index]
            .iter()
            .filter(|event| is_diatonic_chord(event.chord, target_key))
            .count()
    } else {
        0
    };
    let prior_key_support = segment[earliest_pivot..start_index]
        .iter()
        .rev()
        .take(4)
        .filter(|event| is_diatonic_chord(event.chord, from_key))
        .count();
    let repeated_target_cadences = if cadence_index < scoring_end_index {
        count_authentic_cadences(&segment[cadence_index + 1..=scoring_end_index], target_key)
    } else {
        0
    };
    let from_key_tonic_stated = progression
        .iter()
        .take(segment[start_index].event_index + 1)
        .filter_map(ProgressionItem::chord)
        .any(|chord| {
            semitone_distance(chord.root, from_key.tonic) == 0
                && stable_tonic_mode(chord) == Some(from_key.mode)
        });

    let mut score = 0.0;
    let mut evidence = Vec::new();
    add_score(
        &mut score,
        &mut evidence,
        "builtin.modulation.key_change_complexity",
        if same_key(target_key, path.global_key) {
            -1.8
        } else {
            -2.4
        },
        format!(
            "Changing active key from {} to {} pays a complexity cost",
            from_key, target_key
        ),
    );
    add_score(
        &mut score,
        &mut evidence,
        "builtin.modulation.authentic_confirmation",
        1.7,
        format!(
            "{} -> {} confirms V7-I/i in {}",
            dominant.chord.original_symbol, tonic.chord.original_symbol, target_key
        ),
    );
    if cadence == ModulationCadence::PredominantAuthentic {
        add_score(
            &mut score,
            &mut evidence,
            "builtin.modulation.predominant_prefix",
            1.05,
            format!("A target-key predominant precedes V7-I/i in {}", target_key),
        );
    }
    if let Some(pivot) = &pivot {
        let contribution = if pivot.kind == PivotKind::DiatonicCommonChord {
            0.9
        } else {
            0.65
        };
        add_score(
            &mut score,
            &mut evidence,
            "builtin.modulation.pivot",
            contribution,
            format!(
                "{} is reinterpreted from {} in {} to {} in {}",
                pivot.chord_symbol,
                pivot.old_degree,
                pivot.old_key,
                pivot.new_degree,
                pivot.new_key
            ),
        );
        if matches!(pivot.old_role, Some(HarmonicRole::Tonic))
            && matches!(
                pivot.new_role,
                Some(HarmonicRole::Predominant | HarmonicRole::Subdominant)
            )
        {
            add_score(
                &mut score,
                &mut evidence,
                "builtin.modulation.smooth_pivot_roles",
                0.5,
                "The pivot changes from tonic-prolonging to predominant/subdominant function"
                    .to_owned(),
            );
        }
    } else {
        match mechanism {
            ModulationMechanism::DominantBridge => add_score(
                &mut score,
                &mut evidence,
                "builtin.modulation.dominant_bridge",
                0.45,
                "An old-context dominant is followed by the new key's dominant".to_owned(),
            ),
            ModulationMechanism::DominantSequence => add_score(
                &mut score,
                &mut evidence,
                "builtin.modulation.dominant_sequence",
                0.7,
                "A fifth-related dominant chain reaches the new key's V7-I/i".to_owned(),
            ),
            ModulationMechanism::DirectDominant => add_score(
                &mut score,
                &mut evidence,
                "builtin.modulation.short_direct_penalty",
                -0.6,
                "A bare target V7-I/i can still be only a tonicization".to_owned(),
            ),
            ModulationMechanism::DiatonicPivot | ModulationMechanism::ChromaticPivot => {}
        }
    }

    let contextual_support = target_support.saturating_sub(2).min(4);
    if contextual_support > 0 {
        add_score(
            &mut score,
            &mut evidence,
            "builtin.modulation.target_key_persistence",
            contextual_support as f64 * 0.35,
            format!(
                "{contextual_support} additional chord(s) support {} around the cadence",
                target_key
            ),
        );
    }
    if persistence >= 2 {
        add_score(
            &mut score,
            &mut evidence,
            "builtin.modulation.post_cadential_persistence",
            0.6,
            format!(
                "Harmony remains in {} after its confirming cadence",
                target_key
            ),
        );
    }
    if prior_key_support >= 2 {
        add_score(
            &mut score,
            &mut evidence,
            "builtin.modulation.departure_from_established_key",
            0.35,
            format!("The progression establishes {} before departing", from_key),
        );
    } else if prior_key_support == 0 {
        add_score(
            &mut score,
            &mut evidence,
            "builtin.modulation.unestablished_departure",
            -0.75,
            "No earlier active-key context precedes the proposed modulation".to_owned(),
        );
    }
    if !from_key_tonic_stated {
        add_score(
            &mut score,
            &mut evidence,
            "builtin.modulation.unstated_departure_key",
            -1.5,
            format!(
                "No stable tonic chord has stated {} before the proposed departure",
                from_key
            ),
        );
    }
    if repeated_target_cadences > 0 {
        add_score(
            &mut score,
            &mut evidence,
            "builtin.modulation.repeated_confirmation",
            repeated_target_cadences as f64 * 0.65,
            format!(
                "{repeated_target_cadences} later authentic cadence(s) reinforce {}",
                target_key
            ),
        );
    }

    SpanCandidate {
        start_index,
        span: ModulationSpan {
            from_key,
            to_key: target_key,
            start_event_index: segment[start_index].event_index,
            dominant_event_index: dominant.event_index,
            confirmation_event_index: tonic.event_index,
            // This provisional end is clipped when the DP selects a later key
            // transition and otherwise extended to the segment's final chord.
            end_event_index: segment.last().expect("non-empty segment").event_index,
            duration_chords: 0,
            mechanism,
            cadence,
            pivot,
            score,
            evidence,
        },
    }
}

fn finalize_duration_scores(state: &mut TonalStatePath, progression: &[ProgressionItem]) {
    for span in &mut state.spans {
        let duration = progression[span.start_event_index..=span.end_event_index]
            .iter()
            .filter(|item| item.chord().is_some())
            .count();
        span.duration_chords = duration;

        // A bare two-chord key region remains more naturally a tonicization.
        // Persistence beyond the cadence gradually supports modulation, but
        // the capped reward prevents a long section from overwhelming its
        // actual harmonic evidence.
        let contribution = match duration {
            0 | 1 => -0.6,
            2 => -0.35,
            3 => 0.0,
            _ => ((duration - 3).min(3) as f64) * 0.12,
        };
        add_score(
            &mut span.score,
            &mut span.evidence,
            "builtin.modulation.key_region_duration",
            contribution,
            format!(
                "{} remains active for {duration} chord event(s)",
                span.to_key
            ),
        );
        state.score += contribution;
    }
}

fn find_pivot(
    segment: &[IndexedChord<'_>],
    dominant_index: usize,
    path: &KeyedAnalysisPath,
    from_key: TonalKey,
    target_key: TonalKey,
    earliest_pivot: usize,
) -> Option<(usize, PivotChord)> {
    let lower_bound = dominant_index.saturating_sub(5).max(earliest_pivot);
    for index in (lower_bound..dominant_index).rev() {
        let event = segment[index];

        // Once target-key continuity is broken, an earlier common chord is no
        // longer the natural hinge for this cadence.
        if index + 1 < dominant_index
            && !is_diatonic_chord(segment[index + 1].chord, target_key)
            && !is_dominant_quality(segment[index + 1].chord)
        {
            break;
        }

        let old_diatonic = is_diatonic_chord(event.chord, from_key);
        let new_diatonic = is_diatonic_chord(event.chord, target_key);
        let kind = if old_diatonic && new_diatonic {
            Some(PivotKind::DiatonicCommonChord)
        } else if new_diatonic {
            // Prefer the richer classification already selected under the
            // global key, then fall back to key-relative facts which remain
            // valid after an earlier modulation. This is what lets a later
            // borrowed iv become ii in the next key without incorrectly
            // reusing a family generated from the original global key.
            same_key(from_key, path.global_key)
                .then(|| chromatic_pivot_kind(path, event.event_index))
                .flatten()
                .or_else(|| infer_chromatic_pivot_kind(event.chord, from_key))
        } else {
            None
        };
        let Some(kind) = kind else {
            continue;
        };

        return Some((
            index,
            PivotChord {
                event_index: event.event_index,
                chord_symbol: event.chord.original_symbol.clone(),
                kind,
                old_key: from_key,
                new_key: target_key,
                old_degree: degree_from_spelling(event.chord.root, from_key.tonic),
                new_degree: degree_from_spelling(event.chord.root, target_key.tonic),
                old_role: infer_role(event.chord, from_key),
                new_role: infer_role(event.chord, target_key),
            },
        ));
    }
    None
}

fn infer_chromatic_pivot_kind(chord: &ParsedChord, from_key: TonalKey) -> Option<PivotKind> {
    let root_distance = semitone_distance(chord.root, from_key.tonic);
    if root_distance == 1 && chord.quality_parsed.class == QualityClass::Major {
        return Some(PivotKind::NeapolitanCommonChord);
    }

    let parallel_mode = match from_key.mode {
        TonalMode::Major => TonalMode::Minor,
        TonalMode::Minor => TonalMode::Major,
        TonalMode::Unknown => return None,
    };
    if is_diatonic_chord(chord, TonalKey::new(from_key.tonic, parallel_mode)) {
        return Some(PivotKind::BorrowedCommonChord);
    }

    if is_dominant_quality(chord)
        || matches!(
            chord.quality_parsed.class,
            QualityClass::Diminished | QualityClass::HalfDiminished
        )
    {
        return Some(PivotKind::SecondaryCommonChord);
    }
    None
}

fn chromatic_pivot_kind(path: &KeyedAnalysisPath, event_index: usize) -> Option<PivotKind> {
    let selection = path
        .selections
        .iter()
        .find(|selection| selection.selection.event_index == event_index)?;
    let families = selection
        .selection
        .harmonic_classifications
        .iter()
        .flat_map(|classification| classification.families.iter());

    let mut result = None;
    for family in families {
        result = match family {
            InterpretationFamily::Neapolitan => Some(PivotKind::NeapolitanCommonChord),
            InterpretationFamily::AugmentedSixth => Some(PivotKind::AugmentedSixthCommonChord),
            InterpretationFamily::ModalInterchange | InterpretationFamily::SubdominantMinor => {
                result.or(Some(PivotKind::BorrowedCommonChord))
            }
            InterpretationFamily::AppliedCadence
            | InterpretationFamily::AppliedLeadingTone
            | InterpretationFamily::RootlessDominantNinth => {
                result.or(Some(PivotKind::SecondaryCommonChord))
            }
            _ => result,
        };
    }
    result
}

fn apply_modulations(
    progression: &[ProgressionItem],
    mut path: KeyedAnalysisPath,
    spans: Vec<ModulationSpan>,
) -> KeyedAnalysisPath {
    for span in spans {
        apply_modulation_span(progression, &mut path, span);
    }
    path
}

fn apply_modulation_span(
    progression: &[ProgressionItem],
    path: &mut KeyedAnalysisPath,
    span: ModulationSpan,
) {
    let state_suffix = format!(
        "@mod:{}:{}:{}",
        span.to_key.tonic,
        mode_id(span.to_key.mode),
        mechanism_id(span.mechanism)
    );

    for selection in &mut path.selections {
        let event_index = selection.selection.event_index;
        if event_index < span.start_event_index || event_index > span.end_event_index {
            continue;
        }

        selection.active_key = span.to_key;
        selection.is_pivot |= span
            .pivot
            .as_ref()
            .is_some_and(|pivot| pivot.event_index == event_index);
        selection.is_modulation_confirmation |= event_index == span.confirmation_event_index;

        // Keep an explicitly selected tonicization nested inside the new key.
        // Plain global selections, however, must be reprojected so their
        // degree and role describe the active modulation rather than the home
        // key.
        if selection.scope == TonalScope::Global
            || selection.local_key.tonic.pitch_class() == span.from_key.tonic.pitch_class()
            || (selection.scope == TonalScope::Tonicization
                && selection.local_key.tonic.pitch_class() == span.to_key.tonic.pitch_class())
        {
            selection.local_key = span.to_key;
            selection.scope = if same_key(span.to_key, path.global_key) {
                TonalScope::Global
            } else {
                TonalScope::Modulation
            };
            if let Some(chord) = progression
                .get(event_index)
                .and_then(ProgressionItem::chord)
            {
                selection.local_degree = Some(degree_from_spelling(chord.root, span.to_key.tonic));
                selection.role = infer_role(chord, span.to_key);
            }
        }

        selection.selection.candidate_id.push_str(&state_suffix);
        if selection.is_pivot {
            selection.selection.evidence.push(ScoreEvidence::new(
                "builtin.modulation.pivot_event",
                0.0,
                format!("This event is the pivot into {}", span.to_key),
            ));
        }
        if selection.is_modulation_confirmation {
            selection.selection.evidence.push(ScoreEvidence::new(
                "builtin.modulation.confirmation_event",
                0.0,
                format!("This tonic arrival confirms {}", span.to_key),
            ));
        }
    }

    path.modulation_score += span.score;
    path.total_score += span.score;
    path.evidence.extend(span.evidence.iter().cloned());
    path.modulations.push(span);
}

fn clip_previous_span(
    spans: &mut [ModulationSpan],
    segment: &[IndexedChord<'_>],
    next_start_index: usize,
) {
    let Some(previous) = spans.last_mut() else {
        return;
    };
    if next_start_index == 0 {
        // The next segment begins with the preparation of a new key. The
        // previous span already ends at the last event of the preceding
        // segment, so there is nothing in this segment to clip to.
        return;
    }

    let event_before_next = segment[next_start_index - 1].event_index;
    // One chord may confirm a key and simultaneously become the pivot into the
    // following key. Never truncate a region before its own confirmation.
    previous.end_event_index = event_before_next.max(previous.confirmation_event_index);
}

fn prune_states(mut states: Vec<TonalStatePath>, beam_width: usize) -> Vec<TonalStatePath> {
    states.sort_by(compare_states);
    let mut distinct = Vec::with_capacity(states.len().min(beam_width));
    for state in states {
        if distinct
            .iter()
            .any(|existing| same_state_history(existing, &state))
        {
            continue;
        }
        distinct.push(state);
        if distinct.len() == beam_width {
            break;
        }
    }
    distinct
}

fn same_state_history(left: &TonalStatePath, right: &TonalStatePath) -> bool {
    same_key(left.active_key, right.active_key)
        && left.spans.len() == right.spans.len()
        && left.spans.iter().zip(&right.spans).all(|(left, right)| {
            same_key(left.from_key, right.from_key)
                && same_key(left.to_key, right.to_key)
                && left.start_event_index == right.start_event_index
                && left.confirmation_event_index == right.confirmation_event_index
                && left.mechanism == right.mechanism
        })
}

fn compare_states(left: &TonalStatePath, right: &TonalStatePath) -> Ordering {
    right
        .score
        .partial_cmp(&left.score)
        .unwrap_or(Ordering::Equal)
        .then_with(|| left.spans.len().cmp(&right.spans.len()))
        .then_with(|| {
            left.active_key
                .tonic
                .pitch_class()
                .value()
                .cmp(&right.active_key.tonic.pitch_class().value())
        })
}

fn indexed_chord_segments(
    progression: &[ProgressionItem],
    no_chord_policy: NoChordPolicy,
) -> Vec<Vec<IndexedChord<'_>>> {
    let mut segments = Vec::new();
    let mut current = Vec::new();
    for (event_index, item) in progression.iter().enumerate() {
        match &item.symbol {
            ParsedSymbol::Chord(chord) => current.push(IndexedChord { event_index, chord }),
            ParsedSymbol::Boundary { .. } => finish_segment(&mut segments, &mut current),
            ParsedSymbol::NoChord { .. } if no_chord_policy == NoChordPolicy::Break => {
                finish_segment(&mut segments, &mut current);
            }
            ParsedSymbol::NoChord { .. } => {}
        }
    }
    finish_segment(&mut segments, &mut current);
    segments
}

fn finish_segment<'a>(
    segments: &mut Vec<Vec<IndexedChord<'a>>>,
    current: &mut Vec<IndexedChord<'a>>,
) {
    if !current.is_empty() {
        segments.push(std::mem::take(current));
    }
}

fn scoring_end_before_foreign_cadence(
    segment: &[IndexedChord<'_>],
    cadence_index: usize,
    target_key: TonalKey,
) -> usize {
    for index in cadence_index + 2..segment.len() {
        let dominant = segment[index - 1].chord;
        let tonic = segment[index].chord;
        let Some(mode) = stable_tonic_mode(tonic) else {
            continue;
        };
        let cadence_key = TonalKey::new(tonic.root, mode);
        if is_dominant_quality(dominant)
            && semitone_distance(dominant.root, cadence_key.tonic) == 7
            && !same_key(cadence_key, target_key)
        {
            // Do not let a candidate borrow persistence evidence from beyond
            // the next cadence which could establish a different key. The DP
            // may later choose to stay, but that decision should not inflate
            // the earlier transition score.
            return index.saturating_sub(2).max(cadence_index);
        }
    }
    segment.len() - 1
}

fn dominant_sequence_start(
    segment: &[IndexedChord<'_>],
    dominant_index: usize,
    lower_bound: usize,
) -> usize {
    let mut start = dominant_index;
    while start > lower_bound {
        let previous = segment[start - 1].chord;
        let current = segment[start].chord;
        if !is_dominant_quality(previous) || semitone_distance(current.root, previous.root) != 5 {
            break;
        }
        start -= 1;
    }
    start
}

fn same_key(left: TonalKey, right: TonalKey) -> bool {
    left.tonic.pitch_class() == right.tonic.pitch_class() && left.mode == right.mode
}

fn count_authentic_cadences(segment: &[IndexedChord<'_>], key: TonalKey) -> usize {
    segment
        .windows(2)
        .filter(|pair| {
            semitone_distance(pair[0].chord.root, key.tonic) == 7
                && is_dominant_quality(pair[0].chord)
                && semitone_distance(pair[1].chord.root, key.tonic) == 0
                && stable_tonic_mode(pair[1].chord) == Some(key.mode)
        })
        .count()
}

fn is_predominant_for(chord: &ParsedChord, key: TonalKey) -> bool {
    match semitone_distance(chord.root, key.tonic) {
        2 => matches!(
            chord.quality_parsed.class,
            QualityClass::Minor | QualityClass::Diminished | QualityClass::HalfDiminished
        ),
        5 => matches!(
            chord.quality_parsed.class,
            QualityClass::Major | QualityClass::Minor
        ),
        _ => false,
    }
}

fn is_diatonic_chord(chord: &ParsedChord, key: TonalKey) -> bool {
    let distance = semitone_distance(chord.root, key.tonic);
    let Some(degree) = scale_degree_for_distance(distance, key.mode) else {
        return false;
    };
    if !diatonic_triad_quality(chord.quality_parsed.class, degree, key.mode) {
        return false;
    }

    // A written seventh is part of the chord quality.  G7 is therefore not a
    // common chord between C major (V7) and G major (I would require Gmaj7),
    // even though the underlying G major triad occurs in both keys.
    chord.quality_parsed.seventh.is_none_or(|seventh| {
        expected_sevenths(degree, key.mode, chord.quality_parsed.class).contains(&seventh)
    })
}

fn scale_degree_for_distance(distance: u8, mode: TonalMode) -> Option<usize> {
    let steps = match mode {
        TonalMode::Major => &[0_u8, 2, 4, 5, 7, 9, 11][..],
        TonalMode::Minor => &[0_u8, 2, 3, 5, 7, 8, 10][..],
        TonalMode::Unknown => return None,
    };
    steps.iter().position(|step| *step == distance)
}

fn diatonic_triad_quality(class: QualityClass, degree: usize, mode: TonalMode) -> bool {
    match mode {
        TonalMode::Major => match degree {
            0 | 3 | 4 => class == QualityClass::Major,
            1 | 2 | 5 => class == QualityClass::Minor,
            6 => matches!(
                class,
                QualityClass::Diminished | QualityClass::HalfDiminished
            ),
            _ => false,
        },
        TonalMode::Minor => match degree {
            0 | 3 => class == QualityClass::Minor,
            1 => matches!(
                class,
                QualityClass::Diminished | QualityClass::HalfDiminished
            ),
            2 | 5 | 6 => class == QualityClass::Major,
            4 => matches!(class, QualityClass::Major | QualityClass::Minor),
            _ => false,
        },
        TonalMode::Unknown => false,
    }
}

fn expected_sevenths(
    degree: usize,
    mode: TonalMode,
    class: QualityClass,
) -> &'static [SeventhQuality] {
    match (mode, degree, class) {
        (TonalMode::Major, 0 | 3, QualityClass::Major) => &[SeventhQuality::Major],
        (TonalMode::Major, 4, QualityClass::Major) => &[SeventhQuality::Minor],
        (TonalMode::Major, 1 | 2 | 5, QualityClass::Minor) => &[SeventhQuality::Minor],
        (TonalMode::Major, 6, _) => &[SeventhQuality::Minor],
        (TonalMode::Minor, 0, QualityClass::Minor) => {
            &[SeventhQuality::Minor, SeventhQuality::Major]
        }
        (TonalMode::Minor, 1, _) => &[SeventhQuality::Minor],
        (TonalMode::Minor, 2 | 5, QualityClass::Major) => &[SeventhQuality::Major],
        (TonalMode::Minor, 3, QualityClass::Minor) => &[SeventhQuality::Minor],
        (TonalMode::Minor, 4, QualityClass::Major | QualityClass::Minor) => {
            &[SeventhQuality::Minor]
        }
        (TonalMode::Minor, 6, QualityClass::Major) => &[SeventhQuality::Minor],
        _ => &[],
    }
}

fn infer_role(chord: &ParsedChord, key: TonalKey) -> Option<HarmonicRole> {
    let distance = semitone_distance(chord.root, key.tonic);
    match key.mode {
        TonalMode::Major => match distance {
            0 | 4 | 9 => Some(HarmonicRole::Tonic),
            2 => Some(HarmonicRole::Predominant),
            5 => Some(HarmonicRole::Subdominant),
            7 | 11 => Some(HarmonicRole::Dominant),
            _ => None,
        },
        TonalMode::Minor => match distance {
            0 | 3 | 8 => Some(HarmonicRole::Tonic),
            2 => Some(HarmonicRole::Predominant),
            5 | 10 => Some(HarmonicRole::Subdominant),
            7 | 11 => Some(HarmonicRole::Dominant),
            _ => None,
        },
        TonalMode::Unknown => is_dominant_quality(chord).then_some(HarmonicRole::Dominant),
    }
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
        && chord.quality_parsed.seventh == Some(SeventhQuality::Minor)
}

fn add_score(
    score: &mut f64,
    evidence: &mut Vec<ScoreEvidence>,
    rule_id: &str,
    contribution: f64,
    explanation: String,
) {
    *score += contribution;
    evidence.push(ScoreEvidence::new(rule_id, contribution, explanation));
}

fn compare_modulation_paths(left: &KeyedAnalysisPath, right: &KeyedAnalysisPath) -> Ordering {
    right
        .total_score
        .partial_cmp(&left.total_score)
        .unwrap_or(Ordering::Equal)
        .then_with(|| left.modulations.len().cmp(&right.modulations.len()))
}

pub(super) fn base_candidate_id(candidate_id: &str) -> &str {
    candidate_id
        .split_once("@mod:")
        .map_or(candidate_id, |(base, _)| base)
}

fn mode_id(mode: TonalMode) -> &'static str {
    match mode {
        TonalMode::Major => "major",
        TonalMode::Minor => "minor",
        TonalMode::Unknown => "unknown",
    }
}

fn mechanism_id(mechanism: ModulationMechanism) -> &'static str {
    match mechanism {
        ModulationMechanism::DiatonicPivot => "diatonic-pivot",
        ModulationMechanism::ChromaticPivot => "chromatic-pivot",
        ModulationMechanism::DominantBridge => "dominant-bridge",
        ModulationMechanism::DominantSequence => "dominant-sequence",
        ModulationMechanism::DirectDominant => "direct-dominant",
    }
}
