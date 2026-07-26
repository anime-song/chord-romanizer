//! Joint global-key, local-key, and harmonic-function inference.
//!
//! The ordinary romanizer starts with a caller-supplied tonic.  That remains
//! the right API for editors which already know the key, but it cannot answer
//! "which key makes this functional path most natural?".  This module treats
//! each `(tonic, mode)` pair as a global state, builds the existing semantic
//! lattice in that state, and ranks the cross product:
//!
//! ```text
//! joint score =
//!     key evidence + function path + modulation span + harmonic memory
//! ```
//!
//! This is deliberately not a probability model.  The weights are auditable
//! comparison scores and every contribution is returned as `ScoreEvidence`.
//! When MIDI observations become available, duration, meter, melody, and
//! phrase-boundary terms can be added here without collapsing the public
//! function, key-region, and memory axes into one opaque state.

use std::cmp::Ordering;
use std::fmt;

use crate::analysis::{
    AnalysisPath, CadentialSpan, CandidateConstraint, HarmonicResolution, HarmonicRole,
    ModulationSpan, PathSelection, PendingPredominant, PendingResolution, ScoreEvidence, TonalMode,
    TonalScope,
};
use crate::domain::{
    Degree, ParsedChord, ParsedSymbol, ProgressionItem, QualityClass, RomanDegree, SeventhQuality,
    SpelledNote,
};
use crate::profile::NoChordPolicy;
use crate::romanizer::{Romanizer, RomanizerOptions};
use crate::speller::{degree_from_spelling, name_of_pitch_class, semitone_distance};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
/// A spelling-preserving tonic and its mode.
///
/// Inferred global keys always use `Major` or `Minor`.  A local key can remain
/// `Unknown` when a dominant points to a target whose quality is not present
/// in the chord-symbol evidence.
pub struct TonalKey {
    pub tonic: SpelledNote,
    pub mode: TonalMode,
}

impl TonalKey {
    pub const fn new(tonic: SpelledNote, mode: TonalMode) -> Self {
        Self { tonic, mode }
    }
}

impl fmt::Display for TonalKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mode = match self.mode {
            TonalMode::Major => "major",
            TonalMode::Minor => "minor",
            TonalMode::Unknown => "unknown",
        };
        write!(formatter, "{} {mode}", self.tonic)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// How strongly the caller constrains the global-key state.
pub enum GlobalKeyRequest {
    /// Evaluate all twelve tonics in major and minor.
    Infer,
    /// Evaluate all keys, adding a modest prior to this one.
    Hint(TonalKey),
    /// Evaluate exactly this key.
    Fixed(TonalKey),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct KeyAnalysisOptions {
    pub global_key: GlobalKeyRequest,
}

impl Default for KeyAnalysisOptions {
    fn default() -> Self {
        Self {
            global_key: GlobalKeyRequest::Infer,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
/// One event in a joint key/function path.
pub struct KeyedPathSelection {
    /// The original semantic lattice selection, including all specialized
    /// Blackadder and ordinary-harmony classifications.
    pub selection: PathSelection,
    /// Structural key region active at this event.  It is the home/global key
    /// outside a modulation and the newly established key inside one.
    pub active_key: TonalKey,
    /// Most local tonal perspective.  This equals `active_key` unless the
    /// selected harmonic candidate tonicizes another degree inside that key.
    pub local_key: TonalKey,
    pub scope: TonalScope,
    pub local_degree: Option<Degree>,
    pub role: Option<HarmonicRole>,
    /// UI markers are stored on the event as well as in `ModulationSpan`, so a
    /// tree renderer can highlight the hinge and confirmation without joining
    /// path-level metadata back to every node.
    pub is_pivot: bool,
    pub is_modulation_confirmation: bool,
    /// One-based number of chord events spent continuously in `active_key`.
    /// The counter resets when a selected modulation changes the active key,
    /// but not merely because a phrase boundary occurs.
    pub key_region_age_chords: usize,
    /// Unresolved dominant targets after this event has been processed.
    pub pending_resolutions: Vec<PendingResolution>,
    /// Source event indices whose remembered goals resolve on this event.
    pub resolved_resolution_sources: Vec<usize>,
    /// Most recent predominant/subdominant still waiting for a compatible
    /// dominant after this event.
    pub pending_predominant: Option<PendingPredominant>,
    /// Predominant event indices whose full cadence completes here.
    pub resolved_cadence_predominant_sources: Vec<usize>,
}

#[derive(Clone, Debug, PartialEq)]
/// One ranked joint interpretation of the complete progression.
pub struct KeyedAnalysisPath {
    pub global_key: TonalKey,
    pub selections: Vec<KeyedPathSelection>,
    pub modulations: Vec<ModulationSpan>,
    /// Resolved immediate, delayed, and nested dominant obligations.
    pub harmonic_resolutions: Vec<HarmonicResolution>,
    /// Complete predominant-dominant-resolution spans.
    pub cadential_spans: Vec<CadentialSpan>,
    /// Existing emission + transition score of the chosen function path.
    pub function_score: f64,
    /// Diatonic fit, tonic/cadence, spelling, and optional hint score.
    pub key_score: f64,
    /// Segment-level evidence for changing the active key.  Zero retains the
    /// tonicization-only interpretation.
    pub modulation_score: f64,
    /// Whole-path contribution from longer-range harmonic memory.
    pub memory_score: f64,
    pub total_score: f64,
    pub evidence: Vec<ScoreEvidence>,
}

/// Evaluate global keys and semantic function paths as one ranked state space.
pub(crate) fn analyze_keys_and_functions(
    base_options: RomanizerOptions,
    progression: &[ProgressionItem],
    request: KeyAnalysisOptions,
    k: usize,
) -> Vec<KeyedAnalysisPath> {
    analyze_keys_and_functions_conditioned(base_options, progression, request, &[], k)
}

pub(crate) fn analyze_keys_and_functions_conditioned(
    base_options: RomanizerOptions,
    progression: &[ProgressionItem],
    request: KeyAnalysisOptions,
    constraints: &[CandidateConstraint],
    k: usize,
) -> Vec<KeyedAnalysisPath> {
    if k == 0 || !progression.iter().any(|item| item.chord().is_some()) {
        return Vec::new();
    }

    let global_keys = candidate_global_keys(progression, request.global_key);
    let mut joint_paths = Vec::new();
    // Tree constraints may contain an `@mod:` tonal-state suffix.  The
    // function lattice knows only its base candidate ids, so decode the
    // functional prefix first and validate the full state after modulation
    // alternatives have been expanded.
    let lattice_constraints = constraints
        .iter()
        .map(|constraint| CandidateConstraint {
            event_index: constraint.event_index,
            candidate_id: super::modulation::base_candidate_id(&constraint.candidate_id).to_owned(),
        })
        .collect::<Vec<_>>();

    for global_key in global_keys {
        // Crucially, mode is installed before annotations and lattice
        // candidates are generated.  A-minor's tonic chord must not first be
        // mislabeled as modal borrowing from an assumed A major.
        let mut options = base_options;
        options.default_tonic = global_key.tonic;
        options.default_mode = global_key.mode;
        let romanizer =
            Romanizer::with_options(options).expect("a validated tonic always builds a romanizer");

        let (key_score, key_evidence) = score_global_key(
            progression,
            global_key,
            request.global_key,
            options.no_chord_policy,
        );

        // Keep more function prefixes than the caller will see.  Modulation
        // duration and delayed-resolution memory are whole-path terms, so a
        // locally lower function prefix can overtake an early leader after
        // later evidence arrives.  Sixteen is a bounded search width, not a
        // request to expose sixteen interpretations in the public result.
        let internal_function_k = k.max(16);
        for function_path in romanizer
            .build_lattice(progression)
            .decode_top_k_interpretations_conditioned(internal_function_k, &lattice_constraints)
        {
            let base_path = keyed_path(
                progression,
                global_key,
                function_path,
                key_score,
                &key_evidence,
            );
            let mut expanded = super::modulation::expand_modulation_paths(
                progression,
                base_path,
                options.no_chord_policy,
                // Keep enough internal alternatives for a condition to select
                // a lower modulation branch even when the UI asks for k=1.
                k.max(32),
            );
            for path in &mut expanded {
                rescore_segmented_key_path(
                    progression,
                    path,
                    request.global_key,
                    options.no_chord_policy,
                );
                super::memory::apply_harmonic_memory(progression, path, options.no_chord_policy);
            }
            joint_paths.extend(expanded);
        }
    }

    if !constraints.is_empty() {
        joint_paths.retain(|path| keyed_path_satisfies(path, constraints));
    }
    joint_paths.sort_by(compare_joint_paths);
    joint_paths.truncate(k);
    joint_paths
}

fn rescore_segmented_key_path(
    progression: &[ProgressionItem],
    path: &mut KeyedAnalysisPath,
    request: GlobalKeyRequest,
    no_chord_policy: NoChordPolicy,
) {
    let global_key_score = path.key_score;
    let global_key_evidence = path
        .evidence
        .iter()
        .filter(|evidence| evidence.rule_id.starts_with("builtin.key."))
        .cloned()
        .collect::<Vec<_>>();
    let (segmented_score, segmented_evidence) =
        score_segmented_key_path(progression, path, request, no_chord_policy);
    // The global pass remains a prior for the work's home key, while the
    // segmented pass stops later cadences from being treated as though they
    // had occurred in that home key all along. Equal blending preserves the
    // legacy result exactly when no modulation is selected because the two
    // passes then observe the same key sequence.
    path.key_score = global_key_score * 0.5 + segmented_score * 0.5;
    path.total_score =
        path.function_score + path.key_score + path.modulation_score + path.memory_score;

    // `keyed_path` initially carries evidence from the one-global-key scoring
    // pass used to build the function lattice. Replace only that layer; local
    // function and modulation evidence remain in their original order.
    path.evidence
        .retain(|evidence| !evidence.rule_id.starts_with("builtin.key."));
    let blended_evidence = global_key_evidence
        .into_iter()
        .map(|mut evidence| {
            evidence.contribution *= 0.5;
            evidence.explanation = format!("Global-key prior: {}", evidence.explanation);
            evidence
        })
        .chain(segmented_evidence.into_iter().map(|mut evidence| {
            evidence.contribution *= 0.5;
            evidence.explanation = format!("Active-key path: {}", evidence.explanation);
            evidence
        }))
        .collect::<Vec<_>>();
    path.evidence.splice(0..0, blended_evidence);
}

fn score_segmented_key_path(
    progression: &[ProgressionItem],
    path: &KeyedAnalysisPath,
    request: GlobalKeyRequest,
    no_chord_policy: NoChordPolicy,
) -> (f64, Vec<ScoreEvidence>) {
    let mut score = 0.0;
    let mut evidence = Vec::new();

    for segment in keyed_chord_segments(progression, path, no_chord_policy) {
        for (position, (_, chord, key)) in segment.iter().copied().enumerate() {
            let distance = semitone_distance(chord.root, key.tonic);
            let degree = scale_degree_for_distance(distance, key.mode);
            if let Some(degree_index) = degree {
                add_score(
                    &mut score,
                    &mut evidence,
                    "builtin.key.scale_membership",
                    0.3,
                    format!("{} has a root in the {} scale", chord.original_symbol, key),
                );

                let quality_score = diatonic_quality_score(chord, degree_index, key.mode);
                if quality_score != 0.0 {
                    add_score(
                        &mut score,
                        &mut evidence,
                        "builtin.key.diatonic_quality",
                        quality_score,
                        format!(
                            "{} quality {} the diatonic degree in {}",
                            chord.original_symbol,
                            if quality_score > 0.0 {
                                "supports"
                            } else {
                                "conflicts with"
                            },
                            key
                        ),
                    );
                }

                if spelling_matches_mode(chord.root, key) {
                    add_score(
                        &mut score,
                        &mut evidence,
                        "builtin.key.spelling",
                        0.08,
                        format!(
                            "{} uses a scale-degree spelling consistent with {}",
                            chord.root, key
                        ),
                    );
                }
            } else {
                add_score(
                    &mut score,
                    &mut evidence,
                    "builtin.key.chromatic_root",
                    -0.18,
                    format!("{} has a chromatic root in {}", chord.original_symbol, key),
                );
            }

            if distance == 0 && is_stable_tonic_quality(chord, key.mode) {
                add_score(
                    &mut score,
                    &mut evidence,
                    "builtin.key.tonic_presence",
                    0.65,
                    format!("{} states the tonic of {}", chord.original_symbol, key),
                );
                if position == 0 {
                    add_score(
                        &mut score,
                        &mut evidence,
                        "builtin.key.opening_tonic",
                        0.45,
                        format!("The segment opens on the tonic of {}", key),
                    );
                }
                if position + 1 == segment.len() {
                    add_score(
                        &mut score,
                        &mut evidence,
                        "builtin.key.closing_tonic",
                        1.0,
                        format!("The segment closes on the tonic of {}", key),
                    );
                }
            }
        }

        // Cadences may only support the key active on both events. Splitting
        // the chord stream at every selected key change prevents a late D
        // cadence from being credited to a global-D hypothesis during the
        // opening C region.
        let mut tonal_run = Vec::new();
        let mut run_key = None;
        for (_, chord, key) in segment {
            if run_key.is_some_and(|current| current != key) {
                add_cadence_evidence(&mut score, &mut evidence, &tonal_run, run_key.unwrap());
                tonal_run.clear();
            }
            run_key = Some(key);
            tonal_run.push(chord);
        }
        if let Some(key) = run_key {
            add_cadence_evidence(&mut score, &mut evidence, &tonal_run, key);
        }
    }

    if let GlobalKeyRequest::Hint(hint) = request
        && hint == path.global_key
    {
        add_score(
            &mut score,
            &mut evidence,
            "builtin.key.caller_hint",
            2.0,
            format!("Caller supplied {} as a non-binding key hint", hint),
        );
    }
    (score, evidence)
}

fn keyed_chord_segments<'a>(
    progression: &'a [ProgressionItem],
    path: &KeyedAnalysisPath,
    no_chord_policy: NoChordPolicy,
) -> Vec<Vec<(usize, &'a ParsedChord, TonalKey)>> {
    let mut segments = Vec::new();
    let mut current = Vec::new();
    for (event_index, item) in progression.iter().enumerate() {
        match &item.symbol {
            ParsedSymbol::Chord(chord) => {
                let key = path
                    .selections
                    .iter()
                    .find(|selection| selection.selection.event_index == event_index)
                    .map_or(path.global_key, |selection| selection.active_key);
                current.push((event_index, chord, key));
            }
            ParsedSymbol::Boundary { .. } => {
                if !current.is_empty() {
                    segments.push(std::mem::take(&mut current));
                }
            }
            ParsedSymbol::NoChord { .. } if no_chord_policy == NoChordPolicy::Break => {
                if !current.is_empty() {
                    segments.push(std::mem::take(&mut current));
                }
            }
            ParsedSymbol::NoChord { .. } => {}
        }
    }
    if !current.is_empty() {
        segments.push(current);
    }
    segments
}

fn keyed_path_satisfies(path: &KeyedAnalysisPath, constraints: &[CandidateConstraint]) -> bool {
    constraints.iter().all(|constraint| {
        path.selections.iter().any(|selection| {
            selection.selection.event_index == constraint.event_index
                && selection.selection.candidate_id == constraint.candidate_id
        })
    })
}

fn candidate_global_keys(
    progression: &[ProgressionItem],
    request: GlobalKeyRequest,
) -> Vec<TonalKey> {
    if let GlobalKeyRequest::Fixed(key) = request {
        return vec![key];
    }

    let preferred = match request {
        GlobalKeyRequest::Hint(key) => Some(key),
        GlobalKeyRequest::Infer | GlobalKeyRequest::Fixed(_) => None,
    };
    let accidental_preference = progression_accidental_preference(progression);
    let mut keys = Vec::with_capacity(24);

    for pitch_class in 0..12 {
        for mode in [TonalMode::Major, TonalMode::Minor] {
            // Preserve an explicitly hinted spelling.  Otherwise select one
            // spelling per pitch class so enharmonic notation does not consume
            // separate top-k semantic slots.
            let tonic = preferred
                .filter(|key| key.mode == mode && key.tonic.pitch_class().value() == pitch_class)
                .map_or_else(
                    || {
                        let prefer_sharps = accidental_preference
                            .unwrap_or_else(|| conventional_sharp_key(pitch_class, mode));
                        name_of_pitch_class(
                            crate::domain::PitchClass::new(pitch_class),
                            Some(prefer_sharps),
                        )
                    },
                    |key| key.tonic,
                );
            keys.push(TonalKey::new(tonic, mode));
        }
    }
    keys
}

fn progression_accidental_preference(progression: &[ProgressionItem]) -> Option<bool> {
    let mut sharps = 0_u32;
    let mut flats = 0_u32;
    for chord in progression.iter().filter_map(ProgressionItem::chord) {
        for note in [Some(chord.root), chord.bass].into_iter().flatten() {
            if note.accidental > 0 {
                sharps += u32::from(note.accidental.unsigned_abs());
            } else if note.accidental < 0 {
                flats += u32::from(note.accidental.unsigned_abs());
            }
        }
    }
    match sharps.cmp(&flats) {
        Ordering::Greater => Some(true),
        Ordering::Less => Some(false),
        Ordering::Equal => None,
    }
}

fn conventional_sharp_key(pitch_class: u8, mode: TonalMode) -> bool {
    // With no spelling evidence, choose common key-signature names rather than
    // a blanket sharp or flat policy (Db major but C# minor, for example).
    match mode {
        TonalMode::Major => matches!(pitch_class, 6),
        TonalMode::Minor => matches!(pitch_class, 1 | 6 | 8),
        TonalMode::Unknown => true,
    }
}

fn score_global_key(
    progression: &[ProgressionItem],
    key: TonalKey,
    request: GlobalKeyRequest,
    no_chord_policy: NoChordPolicy,
) -> (f64, Vec<ScoreEvidence>) {
    let mut score = 0.0;
    let mut evidence = Vec::new();

    for segment in chord_segments(progression, no_chord_policy) {
        for (position, chord) in segment.iter().copied().enumerate() {
            let distance = semitone_distance(chord.root, key.tonic);
            let degree = scale_degree_for_distance(distance, key.mode);
            if let Some(degree_index) = degree {
                add_score(
                    &mut score,
                    &mut evidence,
                    "builtin.key.scale_membership",
                    0.3,
                    format!("{} has a root in the {} scale", chord.original_symbol, key),
                );

                let quality_score = diatonic_quality_score(chord, degree_index, key.mode);
                if quality_score != 0.0 {
                    add_score(
                        &mut score,
                        &mut evidence,
                        "builtin.key.diatonic_quality",
                        quality_score,
                        format!(
                            "{} quality {} the diatonic degree in {}",
                            chord.original_symbol,
                            if quality_score > 0.0 {
                                "supports"
                            } else {
                                "conflicts with"
                            },
                            key
                        ),
                    );
                }

                if spelling_matches_mode(chord.root, key) {
                    add_score(
                        &mut score,
                        &mut evidence,
                        "builtin.key.spelling",
                        0.08,
                        format!(
                            "{} uses a scale-degree spelling consistent with {}",
                            chord.root, key
                        ),
                    );
                }
            } else {
                // Chromatic roots are common and may already receive a strong
                // applied/modal function from the semantic lattice.  Keep this
                // penalty modest so one borrowed chord cannot erase the key.
                add_score(
                    &mut score,
                    &mut evidence,
                    "builtin.key.chromatic_root",
                    -0.18,
                    format!("{} has a chromatic root in {}", chord.original_symbol, key),
                );
            }

            if distance == 0 && is_stable_tonic_quality(chord, key.mode) {
                add_score(
                    &mut score,
                    &mut evidence,
                    "builtin.key.tonic_presence",
                    0.65,
                    format!("{} states the tonic of {}", chord.original_symbol, key),
                );
                if position == 0 {
                    add_score(
                        &mut score,
                        &mut evidence,
                        "builtin.key.opening_tonic",
                        0.45,
                        format!("The segment opens on the tonic of {}", key),
                    );
                }
                if position + 1 == segment.len() {
                    add_score(
                        &mut score,
                        &mut evidence,
                        "builtin.key.closing_tonic",
                        1.0,
                        format!("The segment closes on the tonic of {}", key),
                    );
                }
            }
        }

        add_cadence_evidence(&mut score, &mut evidence, &segment, key);
    }

    if let GlobalKeyRequest::Hint(hint) = request {
        if hint == key {
            add_score(
                &mut score,
                &mut evidence,
                "builtin.key.caller_hint",
                2.0,
                format!("Caller supplied {} as a non-binding key hint", hint),
            );
        }
    }

    (score, evidence)
}

fn chord_segments(
    progression: &[ProgressionItem],
    no_chord_policy: NoChordPolicy,
) -> Vec<Vec<&ParsedChord>> {
    let mut segments = Vec::new();
    let mut current = Vec::new();
    for item in progression {
        match &item.symbol {
            ParsedSymbol::Chord(chord) => current.push(chord),
            ParsedSymbol::Boundary { .. } => {
                if !current.is_empty() {
                    segments.push(std::mem::take(&mut current));
                }
            }
            ParsedSymbol::NoChord { .. } if no_chord_policy == NoChordPolicy::Break => {
                if !current.is_empty() {
                    segments.push(std::mem::take(&mut current));
                }
            }
            ParsedSymbol::NoChord { .. } => {}
        }
    }
    if !current.is_empty() {
        segments.push(current);
    }
    segments
}

fn add_cadence_evidence(
    score: &mut f64,
    evidence: &mut Vec<ScoreEvidence>,
    segment: &[&ParsedChord],
    key: TonalKey,
) {
    for pair in segment.windows(2) {
        let previous = pair[0];
        let target = pair[1];
        if semitone_distance(target.root, key.tonic) != 0
            || !is_stable_tonic_quality(target, key.mode)
        {
            continue;
        }

        let previous_distance = semitone_distance(previous.root, key.tonic);
        if previous_distance == 7 && is_dominant_quality(previous) {
            add_score(
                score,
                evidence,
                "builtin.key.authentic_cadence",
                2.35,
                format!(
                    "{} -> {} forms V7-I/i in {}",
                    previous.original_symbol, target.original_symbol, key
                ),
            );
        } else if previous_distance == 11
            && matches!(
                previous.quality_parsed.class,
                QualityClass::Diminished | QualityClass::HalfDiminished
            )
        {
            add_score(
                score,
                evidence,
                "builtin.key.leading_tone_cadence",
                1.45,
                format!(
                    "{} -> {} forms vii°-I/i in {}",
                    previous.original_symbol, target.original_symbol, key
                ),
            );
        } else if key.mode == TonalMode::Minor && previous_distance == 10 {
            add_score(
                score,
                evidence,
                "builtin.key.minor_subtonic_arrival",
                0.45,
                format!(
                    "{} -> {} supports a bVII-i arrival in {}",
                    previous.original_symbol, target.original_symbol, key
                ),
            );
        }
    }

    for triple in segment.windows(3) {
        let related_two = triple[0];
        let dominant = triple[1];
        let tonic = triple[2];
        if semitone_distance(related_two.root, key.tonic) == 2
            && matches!(
                related_two.quality_parsed.class,
                QualityClass::Minor | QualityClass::HalfDiminished
            )
            && semitone_distance(dominant.root, key.tonic) == 7
            && is_dominant_quality(dominant)
            && semitone_distance(tonic.root, key.tonic) == 0
            && is_stable_tonic_quality(tonic, key.mode)
        {
            add_score(
                score,
                evidence,
                "builtin.key.two_five_one",
                1.2,
                format!("The segment contains a complete ii-V-I/i in {}", key),
            );
        }
    }
}

fn scale_degree_for_distance(distance: u8, mode: TonalMode) -> Option<usize> {
    let steps = match mode {
        TonalMode::Major => &[0_u8, 2, 4, 5, 7, 9, 11][..],
        TonalMode::Minor => &[0_u8, 2, 3, 5, 7, 8, 10][..],
        TonalMode::Unknown => return None,
    };
    steps.iter().position(|step| *step == distance)
}

fn diatonic_quality_score(chord: &ParsedChord, degree: usize, mode: TonalMode) -> f64 {
    let class = chord.quality_parsed.class;
    if matches!(
        class,
        QualityClass::Augmented
            | QualityClass::Suspended2
            | QualityClass::Suspended4
            | QualityClass::Power
            | QualityClass::Unknown
    ) {
        return 0.0;
    }

    let class_matches = match mode {
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
            // Natural-minor v and harmonic-minor V are both valid.
            4 => matches!(class, QualityClass::Major | QualityClass::Minor),
            _ => false,
        },
        TonalMode::Unknown => false,
    };
    if !class_matches {
        return -0.5;
    }

    // Base-triad agreement is already useful.  When a seventh is written,
    // matching its diatonic quality adds a smaller refinement.
    let mut score = 0.75;
    if let Some(seventh) = chord.quality_parsed.seventh {
        let expected = expected_seventh(degree, mode, class);
        score += if expected.contains(&seventh) {
            0.18
        } else {
            -0.12
        };
    }
    score
}

fn expected_seventh(
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
        (TonalMode::Minor, 4, QualityClass::Major) => &[SeventhQuality::Minor],
        (TonalMode::Minor, 4, QualityClass::Minor) => &[SeventhQuality::Minor],
        (TonalMode::Minor, 6, QualityClass::Major) => &[SeventhQuality::Minor],
        _ => &[],
    }
}

fn spelling_matches_mode(note: SpelledNote, key: TonalKey) -> bool {
    let degree = degree_from_spelling(note, key.tonic);
    let expected_accidental = match key.mode {
        TonalMode::Major => 0,
        TonalMode::Minor => match degree.degree {
            RomanDegree::I | RomanDegree::Ii | RomanDegree::Iv | RomanDegree::V => 0,
            RomanDegree::Iii | RomanDegree::Vi | RomanDegree::Vii => -1,
        },
        TonalMode::Unknown => return false,
    };
    degree.accidental == expected_accidental
}

fn is_dominant_quality(chord: &ParsedChord) -> bool {
    chord.quality_parsed.class == QualityClass::Major
        && chord.quality_parsed.seventh == Some(SeventhQuality::Minor)
}

fn is_stable_tonic_quality(chord: &ParsedChord, mode: TonalMode) -> bool {
    (match mode {
        TonalMode::Major => chord.quality_parsed.class == QualityClass::Major,
        TonalMode::Minor => chord.quality_parsed.class == QualityClass::Minor,
        TonalMode::Unknown => false,
    }) && !is_dominant_quality(chord)
}

fn keyed_path(
    progression: &[ProgressionItem],
    global_key: TonalKey,
    mut function_path: AnalysisPath,
    key_score: f64,
    key_evidence: &[ScoreEvidence],
) -> KeyedAnalysisPath {
    let function_score = function_path.total_score;
    let mut selections = Vec::with_capacity(function_path.selections.len());

    for mut selection in function_path.selections.drain(..) {
        normalize_global_mode(&mut selection, global_key);
        selections.push(keyed_selection(progression, global_key, selection));
    }

    let mut evidence = key_evidence.to_vec();
    evidence.extend(function_path.evidence);
    KeyedAnalysisPath {
        global_key,
        selections,
        modulations: Vec::new(),
        harmonic_resolutions: Vec::new(),
        cadential_spans: Vec::new(),
        function_score,
        key_score,
        modulation_score: 0.0,
        memory_score: 0.0,
        total_score: function_score + key_score,
        evidence,
    }
}

fn normalize_global_mode(selection: &mut PathSelection, global_key: TonalKey) {
    for classification in &mut selection.harmonic_classifications {
        let Some(perspective) = classification.perspective.as_mut() else {
            continue;
        };
        perspective.global_tonic = global_key.tonic;
        if perspective.local_tonic.pitch_class() == global_key.tonic.pitch_class()
            && perspective.scope == TonalScope::Global
        {
            perspective.local_tonic = global_key.tonic;
            perspective.mode = global_key.mode;
        }
    }
}

fn keyed_selection(
    progression: &[ProgressionItem],
    global_key: TonalKey,
    selection: PathSelection,
) -> KeyedPathSelection {
    // A semantic candidate normally has one classification.  If a legacy
    // state contains several, prefer an explicit local perspective over a
    // global annotation because it carries more information.
    let classification = selection
        .harmonic_classifications
        .iter()
        .filter(|item| item.perspective.is_some())
        .max_by_key(|item| {
            item.perspective
                .as_ref()
                .is_some_and(|perspective| perspective.scope == TonalScope::Tonicization)
        });

    let chord = progression
        .get(selection.event_index)
        .and_then(ProgressionItem::chord);
    let (local_key, scope) = classification
        .and_then(|item| item.perspective.as_ref())
        .map_or((global_key, TonalScope::Global), |perspective| {
            let mode = if perspective.mode != TonalMode::Unknown {
                perspective.mode
            } else if perspective.local_tonic.pitch_class() == global_key.tonic.pitch_class() {
                global_key.mode
            } else {
                infer_local_mode(progression, perspective.local_tonic)
            };
            (
                TonalKey::new(perspective.local_tonic, mode),
                perspective.scope,
            )
        });

    let local_degree = classification
        .and_then(|item| item.local_degree)
        .or_else(|| chord.map(|chord| degree_from_spelling(chord.root, local_key.tonic)));
    let role = classification
        .and_then(|item| item.role)
        .or_else(|| infer_role(chord, local_key));

    KeyedPathSelection {
        selection,
        active_key: global_key,
        local_key,
        scope,
        local_degree,
        role,
        is_pivot: false,
        is_modulation_confirmation: false,
        key_region_age_chords: 0,
        pending_resolutions: Vec::new(),
        resolved_resolution_sources: Vec::new(),
        pending_predominant: None,
        resolved_cadence_predominant_sources: Vec::new(),
    }
}

fn infer_local_mode(progression: &[ProgressionItem], tonic: SpelledNote) -> TonalMode {
    for chord in progression.iter().filter_map(ProgressionItem::chord) {
        if chord.root.pitch_class() != tonic.pitch_class() {
            continue;
        }
        return match chord.quality_parsed.class {
            QualityClass::Major => TonalMode::Major,
            QualityClass::Minor => TonalMode::Minor,
            _ => TonalMode::Unknown,
        };
    }
    TonalMode::Unknown
}

fn infer_role(chord: Option<&ParsedChord>, key: TonalKey) -> Option<HarmonicRole> {
    let chord = chord?;
    let distance = semitone_distance(chord.root, key.tonic);
    let dominant = is_dominant_quality(chord);
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
            5 => Some(HarmonicRole::Subdominant),
            7 if dominant => Some(HarmonicRole::Dominant),
            7 => Some(HarmonicRole::Dominant),
            10 => Some(HarmonicRole::Subdominant),
            11 => Some(HarmonicRole::Dominant),
            _ => None,
        },
        TonalMode::Unknown => dominant.then_some(HarmonicRole::Dominant),
    }
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

fn compare_joint_paths(left: &KeyedAnalysisPath, right: &KeyedAnalysisPath) -> Ordering {
    right
        .total_score
        .partial_cmp(&left.total_score)
        .unwrap_or(Ordering::Equal)
        .then_with(|| {
            left.global_key
                .tonic
                .pitch_class()
                .value()
                .cmp(&right.global_key.tonic.pitch_class().value())
        })
        .then_with(|| {
            tonal_mode_order(left.global_key.mode).cmp(&tonal_mode_order(right.global_key.mode))
        })
        .then_with(|| {
            let left_ids = left
                .selections
                .iter()
                .map(|selection| selection.selection.candidate_id.as_str())
                .collect::<Vec<_>>();
            let right_ids = right
                .selections
                .iter()
                .map(|selection| selection.selection.candidate_id.as_str())
                .collect::<Vec<_>>();
            left_ids.cmp(&right_ids)
        })
}

const fn tonal_mode_order(mode: TonalMode) -> u8 {
    match mode {
        TonalMode::Major => 0,
        TonalMode::Minor => 1,
        TonalMode::Unknown => 2,
    }
}
