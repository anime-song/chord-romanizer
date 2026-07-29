//! Public orchestration layer.
//!
//! `Romanizer` connects notation parsing, deterministic theory, local hybrid
//! interpretation, and progression context. It intentionally returns both a
//! convenient 1-best view and the candidates used to obtain that view. This
//! prevents UI-oriented fields such as `roman` and `alter` from becoming the
//! only available representation of an ambiguous analysis.

use crate::analysis::context::{ContextHint, ResolutionKind, analyze_global_context};
use crate::analysis::{
    AnalysisLattice, AnalysisPath, BlackadderContext, HarmonicClassification,
    HarmonicInterpretation, HarmonicRole, InterpretationFamily, InterpretationTree,
    InterpretationTreeOptions, KeyAnalysisOptions, KeyedAnalysisPath, ScoreEvidence, TonalMode,
    TonalPerspective, TonalScope,
};
use crate::domain::{Degree, ParsedChord, ParsedSymbol, ProgressionItem, RomanDegree, SpelledNote};
use crate::error::ParseError;
use crate::interpreter::{ChordInterpreter, HybridCandidate, HybridKind, SlashClassification};
use crate::notation::formatter::{
    format_roman, render_symbol, rewrite_symbol, split_note_and_suffix,
};
use crate::profile::{BehaviorProfile, KeyBoundaryPolicy, NoChordPolicy};
use crate::speller::{
    MAJOR_SCALE_STEPS, calc_degree_base, degree_from_spelling, name_of_pitch_class,
    semitone_distance, simplify_spelling, spell_degree_note,
};
use crate::structure;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Resolution marker attached to the target chord.
pub enum ResolutionType {
    Perfect,
    Semitone,
    Backdoor,
    LeadingTone,
    Deceptive,
}

impl ResolutionType {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Perfect => "perfect",
            Self::Semitone => "semitone",
            Self::Backdoor => "backdoor",
            Self::LeadingTone => "leading_tone",
            Self::Deceptive => "deceptive",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Controls compatibility, spelling, and progression segmentation.
pub struct RomanizerOptions {
    pub default_tonic: SpelledNote,
    /// Mode of the caller-supplied global key.
    ///
    /// The historical romanizer surface accepted only a tonic and therefore
    /// implied major.  Key inference overrides this field for each major/minor
    /// hypothesis so contextual candidate generation itself becomes
    /// mode-aware, rather than merely re-ranking major-key candidates later.
    pub default_mode: TonalMode,
    pub simplify_accidentals: bool,
    pub behavior: BehaviorProfile,
    pub key_boundary_policy: KeyBoundaryPolicy,
    pub no_chord_policy: NoChordPolicy,
}

impl RomanizerOptions {
    /// Strict defaults for new applications.
    ///
    /// Key changes are hard boundaries, while N.C. is treated as a transparent
    /// rest unless the caller supplies an explicit boundary.
    pub fn new(default_tonic: &str) -> Result<Self, ParseError> {
        Ok(Self {
            default_tonic: SpelledNote::parse(default_tonic)?,
            default_mode: TonalMode::Major,
            simplify_accidentals: false,
            behavior: BehaviorProfile::StrictV1,
            key_boundary_policy: KeyBoundaryPolicy::Break,
            no_chord_policy: NoChordPolicy::Transparent,
        })
    }

    /// Options that reproduce Python 0.1.9's observable behavior.
    pub fn python_019(default_tonic: &str) -> Result<Self, ParseError> {
        Ok(Self {
            default_tonic: SpelledNote::parse(default_tonic)?,
            default_mode: TonalMode::Major,
            simplify_accidentals: false,
            behavior: BehaviorProfile::Python019,
            key_boundary_policy: KeyBoundaryPolicy::Continue,
            no_chord_policy: NoChordPolicy::Break,
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
/// Complete output for one chord event.
///
/// The struct contains three related but distinct views:
///
/// - `roman`/`alter`: convenient 1-best display labels;
/// - `alternates`/`hybrid_candidates`: ambiguity-preserving local analysis;
/// - progression flags: deterministic context attached to this chord.
pub struct RomanizedChord {
    /// Parsed input, including the untouched `original_symbol`.
    pub chord: ParsedChord,
    pub tonic: SpelledNote,
    pub roman: String,
    /// Legacy untyped alternate labels retained for Python compatibility.
    pub alternate_labels: Vec<String>,
    /// Preferred typed alternate API for new callers.
    pub alternates: Vec<AlternateLabel>,
    pub degree_root: Degree,
    pub degree_bass: Option<Degree>,
    pub roman_root_bass: Option<String>,
    pub is_hybrid: bool,
    pub hybrid_kind: Option<HybridKind>,
    pub slash_classification: SlashClassification,
    /// All local hybrid readings; `hybrid_kind` is only their 1-best view.
    pub hybrid_candidates: Vec<HybridCandidate>,
    pub functional_interpretations: Vec<FunctionalInterpretation>,
    /// Scored meanings of ordinary (non-hybrid) chords.  These are kept
    /// separate from `functional_interpretations`, whose structure metadata is
    /// specific to slash/Blackadder readings.
    pub harmonic_interpretations: Vec<HarmonicInterpretation>,
    /// Progression-level roles that also apply to ordinary, non-Blackadder
    /// chords. Multiple entries allow the same event to be viewed from global
    /// and temporary local keys without overwriting either reading.
    pub harmonic_classifications: Vec<HarmonicClassification>,
    pub alter: Option<String>,
    /// Compatibility alias for `normalized_symbol`.
    pub symbol_fixed: String,
    /// AST-rendered theoretical spelling before optional simplification.
    pub theoretical_symbol: String,
    /// AST-rendered display spelling after optional simplification.
    pub normalized_symbol: String,
    pub is_ii_v_start: bool,
    pub is_resolution_target: bool,
    pub resolution_type: Option<ResolutionType>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Semantic category of an alternate label.
pub enum AlternateKind {
    Enharmonic,
    WithoutBass,
    FunctionalInterpretation,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AlternateLabel {
    pub label: String,
    pub kind: AlternateKind,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FunctionalInterpretation {
    pub label: String,
    pub hybrid_kind: HybridKind,
    pub intrinsic_score: f64,
    pub rule_id: String,
    /// Factorized Blackadder semantics, when this interpretation came from an
    /// exact `{0,2,6,10}` augmented-over-bass sonority.
    pub blackadder: Option<crate::analysis::BlackadderInterpretation>,
    pub classification: HarmonicClassification,
    pub effective_root: Option<SpelledNote>,
    pub evidence: Vec<ScoreEvidence>,
}

#[derive(Clone, Debug, PartialEq)]
/// Event-aligned output used by timelines and sequence analysis.
pub enum AnnotatedEvent {
    /// Boxed because `RomanizedChord` is much larger than the marker variants;
    /// indirection keeps a long event vector compact.
    Chord(Box<RomanizedChord>),
    NoChord {
        original_symbol: String,
        tonic: SpelledNote,
    },
    Boundary {
        label: Option<String>,
    },
}

#[derive(Clone, Debug)]
pub struct Romanizer {
    options: RomanizerOptions,
    interpreter: ChordInterpreter,
}

impl Romanizer {
    /// Construct a romanizer using [`BehaviorProfile::StrictV1`] defaults.
    pub fn new(default_tonic: &str) -> Result<Self, ParseError> {
        Self::with_options(RomanizerOptions::new(default_tonic)?)
    }

    pub fn with_options(options: RomanizerOptions) -> Result<Self, ParseError> {
        let behavior = options.behavior;
        Ok(Self {
            options,
            interpreter: ChordInterpreter::new(behavior),
        })
    }

    pub const fn options(&self) -> &RomanizerOptions {
        &self.options
    }

    /// Compact API that omits N.C. and boundary markers from the returned list.
    ///
    /// Context still observes those events according to the configured
    /// policies; only the output shape is compacted.
    pub fn annotate_progression(&self, progression: &[ProgressionItem]) -> Vec<RomanizedChord> {
        self.annotate_events(progression)
            .into_iter()
            .filter_map(|event| match event {
                AnnotatedEvent::Chord(chord) => Some(*chord),
                AnnotatedEvent::NoChord { .. } | AnnotatedEvent::Boundary { .. } => None,
            })
            .collect()
    }

    /// Aligned API that preserves one output event per input event.
    ///
    /// This is the preferred entry point for editors, audio timelines, and
    /// other consumers that must retain rests and section boundaries.
    pub fn annotate_events(&self, progression: &[ProgressionItem]) -> Vec<AnnotatedEvent> {
        // Context is computed once for the complete sequence. Per-chord output
        // below reads from this shared result rather than independently
        // reinterpreting neighbors.
        let context = analyze_global_context(
            progression,
            &self.interpreter,
            self.options.default_tonic,
            self.options.default_mode,
            self.options.key_boundary_policy,
            self.options.no_chord_policy,
        );
        let mut results = Vec::with_capacity(progression.len());

        for (index, item) in progression.iter().enumerate() {
            let tonic = item.tonic.unwrap_or(self.options.default_tonic);
            match &item.symbol {
                ParsedSymbol::Chord(chord) => {
                    // Use semantic neighbors, not raw index +/- 1: transparent
                    // N.C. events can be skipped and boundaries can sever links.
                    let previous = context.previous_chord[index]
                        .and_then(|position| progression[position].chord());
                    let next = context.next_chord[index]
                        .and_then(|position| progression[position].chord());
                    if let Some(result) =
                        self.process_chord(chord, tonic, previous, next, context.hints.get(index))
                    {
                        results.push(AnnotatedEvent::Chord(Box::new(result)));
                    }
                }
                ParsedSymbol::NoChord { original_symbol } => {
                    results.push(AnnotatedEvent::NoChord {
                        original_symbol: original_symbol.clone(),
                        tonic,
                    });
                }
                ParsedSymbol::Boundary { label } => {
                    results.push(AnnotatedEvent::Boundary {
                        label: label.clone(),
                    });
                }
            }
        }
        results
    }

    /// Build a candidate graph suitable for 1-best or k-best path decoding.
    pub fn build_lattice(&self, progression: &[ProgressionItem]) -> AnalysisLattice {
        let events = self.annotate_events(progression);
        AnalysisLattice::from_annotated_events(progression, &events, &self.options)
    }

    /// Convenience API for callers that want ranked interpretations without
    /// inspecting or modifying the intermediate lattice.
    pub fn analyze_top_k(&self, progression: &[ProgressionItem], k: usize) -> Vec<AnalysisPath> {
        self.build_lattice(progression).decode_top_k(k)
    }

    /// High-level ranked analysis whose slots represent harmonic meanings.
    ///
    /// Enharmonic labels, augmented-triad rotations, and slash-bass omission
    /// renderings remain available as notation metadata but never consume a
    /// place in this result. `analyze_top_k` remains the lower-level path API;
    /// both currently share the semantic-only built-in lattice.
    pub fn analyze_top_k_interpretations(
        &self,
        progression: &[ProgressionItem],
        k: usize,
    ) -> Vec<AnalysisPath> {
        self.build_lattice(progression)
            .decode_top_k_interpretations(k)
    }

    /// Jointly rank global key, local key, and semantic function paths.
    ///
    /// Unlike `analyze_top_k_interpretations`, this method does not assume
    /// `self.options().default_tonic` is known. `KeyAnalysisOptions` decides
    /// whether all 24 major/minor keys are inferred, one key is used as a
    /// non-binding hint, or a key is fixed.
    pub fn analyze_keys_and_functions(
        &self,
        progression: &[ProgressionItem],
        options: KeyAnalysisOptions,
        k: usize,
    ) -> Vec<KeyedAnalysisPath> {
        crate::analysis::analyze_keys_and_functions(self.options, progression, options, k)
    }

    /// Return a prefix tree designed for interactive visualization.
    ///
    /// Each node contains a reusable condition. Supplying that condition on a
    /// later call pins the complete prefix and recomputes all descendants.
    pub fn analyze_interpretation_tree(
        &self,
        progression: &[ProgressionItem],
        options: InterpretationTreeOptions,
        k: usize,
    ) -> InterpretationTree {
        crate::analysis::analyze_interpretation_tree(self.options, progression, options, k)
    }

    fn process_chord(
        &self,
        chord: &ParsedChord,
        tonic: SpelledNote,
        previous: Option<&ParsedChord>,
        next: Option<&ParsedChord>,
        hint: Option<&ContextHint>,
    ) -> Option<RomanizedChord> {
        // Step 1: choose a primary scale-degree spelling and retain its
        // enharmonic alternatives. Degree selection is contextual, but it does
        // not change the input pitch class.
        let distance = semitone_distance(chord.root, tonic);
        let prefer_sharps = hint.and_then(|hint| hint.prefer_sharps);
        let (base_degree, alternatives) =
            self.determine_degree_name(distance, tonic, chord, previous, next, prefer_sharps);
        let primary_roman_root = format_roman(base_degree, &chord.quality);
        let mut alternates: Vec<AlternateLabel> = alternatives
            .into_iter()
            .map(|degree| AlternateLabel {
                label: format_roman(degree, &chord.quality),
                kind: AlternateKind::Enharmonic,
            })
            .collect();

        // Step 2: present the next chord in the current key's spelling before
        // local hybrid interpretation. This avoids accidental spelling alone
        // changing an otherwise identical pitch-class rule.
        let contextual_next_source = if self.options.behavior == BehaviorProfile::StrictV1 {
            hint.and_then(|hint| hint.hybrid_target_chord.as_ref())
                .or(next)
        } else {
            next
        };
        let contextual_next = contextual_next_source.map(|next| {
            let next_distance = semitone_distance(next.root, tonic);
            let next_degree = calc_degree_base(next_distance, None);
            let mut contextual = next.clone();
            contextual.root = spell_degree_note(next_degree, tonic);
            contextual
        });
        // Reuse candidates created by the context pass whenever possible.
        // Generating them twice with different look-ahead was the source of a
        // Python-version inconsistency between metadata and final `alter`.
        let mut hybrid_candidates = if self.options.behavior == BehaviorProfile::StrictV1 {
            hint.and_then(|hint| hint.node.as_ref())
                .map(|node| node.hybrid_candidates.clone())
                .unwrap_or_else(|| {
                    self.interpreter.analyze_slash_candidates_with_context(
                        chord,
                        BlackadderContext {
                            tonic: Some(tonic),
                            previous_chord: previous,
                            next_chord: contextual_next.as_ref(),
                            observations: None,
                        },
                    )
                })
        } else {
            self.interpreter
                .analyze_slash_candidates(chord, contextual_next.as_ref())
        };
        let paired_flat_bass = chord.bass.and_then(|bass| {
            (base_degree == Degree::new(-1, RomanDegree::I) && semitone_distance(bass, tonic) == 1)
                .then(|| spell_degree_note(Degree::new(-1, RomanDegree::Ii), tonic))
        });
        if let Some(paired_flat_bass) = paired_flat_bass {
            for candidate in &mut hybrid_candidates {
                match candidate.analysis.kind {
                    HybridKind::SusFourNine => {
                        candidate.analysis.alter = Some(format!("{paired_flat_bass}9sus4"));
                        candidate.analysis.bass_preference = Some(false);
                        candidate.analysis.effective_root = Some(paired_flat_bass);
                    }
                    HybridKind::SusFourSevenFlatNine => {
                        candidate.analysis.alter = Some(format!("{paired_flat_bass}7sus4(b9)"));
                        candidate.analysis.bass_preference = Some(false);
                        candidate.analysis.effective_root = Some(paired_flat_bass);
                    }
                    _ => {}
                }
            }
        }
        // `analysis` is a convenience 1-best projection. The full candidate
        // list is moved into the result later and remains available to callers.
        let mut analysis = hybrid_candidates
            .first()
            .map(|candidate| candidate.analysis.clone())
            .unwrap_or_default();
        let mut best_score = hybrid_candidates
            .first()
            .map_or(f64::NEG_INFINITY, |candidate| {
                self.interpreter.contextual_candidate_score(
                    candidate,
                    contextual_next.as_ref(),
                    Some(tonic),
                )
            });
        for candidate in hybrid_candidates.iter().skip(1) {
            let score = self.interpreter.contextual_candidate_score(
                candidate,
                contextual_next.as_ref(),
                Some(tonic),
            );
            if score > best_score {
                analysis = candidate.analysis.clone();
                best_score = score;
            }
        }

        // Step 3: determine written root and bass. Inversion basses are spelled
        // from the chord formula; hybrid basses follow functional/key context.
        let mut root_fixed = analysis
            .root_override
            .unwrap_or_else(|| spell_degree_note(base_degree, tonic));
        let mut degree_bass = None;
        let mut roman_root_bass = None;
        let mut bass_fixed = None;
        let mut redundant_bass = false;

        if let Some(bass) = chord.bass {
            redundant_bass = chord.root.pitch_class() == bass.pitch_class();
            let fixed = if !analysis.is_hybrid {
                let tones = structure::spelled_tones_for(chord, root_fixed, self.options.behavior);
                tones
                    .get(&bass.pitch_class())
                    .copied()
                    .or_else(|| self.diatonic_or_borrowed_bass(bass, tonic))
                    .unwrap_or(bass)
            } else if analysis.bass_preference.is_some() {
                name_of_pitch_class(bass.pitch_class(), analysis.bass_preference)
            } else if let Some(paired_flat_bass) = paired_flat_bass {
                // `determine_degree_name` deliberately prefers bI over VII
                // when the slash bass is bII. Keep the two spellings paired;
                // otherwise F#/G# in G becomes the mixed Gb/G#.
                paired_flat_bass
            } else {
                self.diatonic_or_borrowed_bass(bass, tonic).unwrap_or(bass)
            };
            bass_fixed = Some(fixed);
            let bass_degree = degree_from_spelling(fixed, tonic);
            degree_bass = Some(bass_degree);
            roman_root_bass = Some(format!("{base_degree}/{bass_degree}"));
        }

        // Step 4: build the primary Roman label. A slash with the same pitch
        // class as the root is redundant and is removed in StrictV1 output.
        let mut roman = primary_roman_root.clone();
        if chord.bass.is_some() && chord.original_symbol.contains('/') && !redundant_bass {
            if let Some(bass_degree) = degree_bass {
                roman = format!("{primary_roman_root}/{bass_degree}");
                if self.options.behavior == BehaviorProfile::StrictV1 {
                    // Enharmonic degree alternatives are notation metadata for
                    // the same slash chord. They must retain the observed bass
                    // just like the primary label does.
                    for alternate in alternates
                        .iter_mut()
                        .filter(|alternate| alternate.kind == AlternateKind::Enharmonic)
                    {
                        alternate.label = format!("{}/{bass_degree}", alternate.label);
                    }
                }
                // Python 0.1.9 exposed a root-only compatibility label for
                // slash input. StrictV1 treats a written non-redundant bass as
                // mandatory notation, so only the legacy profile retains it.
                if self.options.behavior == BehaviorProfile::Python019
                    && !alternates.iter().any(|alternate| {
                        alternate.label == primary_roman_root
                            && alternate.kind == AlternateKind::WithoutBass
                    })
                {
                    alternates.push(AlternateLabel {
                        label: primary_roman_root.clone(),
                        kind: AlternateKind::WithoutBass,
                    });
                }
            }
        }
        if redundant_bass {
            degree_bass = None;
            roman_root_bass = None;
        }

        // Step 5: render from parsed fields. StrictV1 avoids prefix replacement,
        // which failed for lowercase input and could leave a stale slash bass.
        let theoretical_symbol = if self.options.behavior == BehaviorProfile::StrictV1 {
            render_symbol(
                root_fixed,
                &chord.quality,
                if redundant_bass { None } else { bass_fixed },
            )
        } else {
            rewrite_symbol(
                &chord.original_symbol,
                &chord.root_lexeme,
                Some(root_fixed),
                chord.bass_lexeme.as_deref(),
                bass_fixed,
            )
        };

        // Theoretical and normalized symbols are intentionally separate. A
        // display simplification must never erase the theory-correct spelling.
        if self.options.simplify_accidentals {
            root_fixed = simplify_spelling(root_fixed);
            bass_fixed = bass_fixed.map(simplify_spelling);
        }
        let normalized_symbol = if self.options.behavior == BehaviorProfile::StrictV1 {
            render_symbol(
                root_fixed,
                &chord.quality,
                if redundant_bass { None } else { bass_fixed },
            )
        } else {
            rewrite_symbol(
                &chord.original_symbol,
                &chord.root_lexeme,
                Some(root_fixed),
                chord.bass_lexeme.as_deref(),
                bass_fixed,
            )
        };
        let symbol_fixed = normalized_symbol.clone();

        // Step 6: project candidate interpretations and context metadata into
        // the public result. Functional alternatives are typed; the old flat
        // string list excludes them to preserve Python golden output.
        let node = hint.and_then(|hint| hint.node.as_ref());
        let resolution_type = node.and_then(|node| match node.resolution_type {
            Some(ResolutionKind::Perfect) => Some(ResolutionType::Perfect),
            Some(ResolutionKind::Semitone) => Some(ResolutionType::Semitone),
            Some(ResolutionKind::Backdoor) => Some(ResolutionType::Backdoor),
            Some(ResolutionKind::LeadingTone) => Some(ResolutionType::LeadingTone),
            Some(ResolutionKind::Deceptive) => Some(ResolutionType::Deceptive),
            None => None,
        });
        let functional_interpretations: Vec<_> = hybrid_candidates
            .iter()
            .filter_map(|candidate| {
                candidate.analysis.alter.as_deref().map(|symbol| {
                    let blackadder = candidate.analysis.blackadder.clone();
                    let classification = blackadder.as_ref().map_or_else(
                        || {
                            functional_hybrid_classification(
                                candidate,
                                tonic,
                                self.options.default_mode,
                            )
                        },
                        |reading| reading.classification.clone(),
                    );
                    FunctionalInterpretation {
                        label: self.romanize_absolute_symbol(symbol, tonic),
                        hybrid_kind: candidate.analysis.kind,
                        intrinsic_score: candidate.intrinsic_score,
                        rule_id: candidate.rule_id.clone(),
                        blackadder,
                        classification,
                        effective_root: candidate.analysis.effective_root,
                        evidence: candidate.evidence.clone(),
                    }
                })
            })
            .collect();
        let alter = analysis
            .alter
            .as_deref()
            .map(|symbol| self.romanize_absolute_symbol(symbol, tonic));
        for interpretation in &functional_interpretations {
            if !alternates.iter().any(|alternate| {
                alternate.kind == AlternateKind::FunctionalInterpretation
                    && alternate.label == interpretation.label
            }) {
                alternates.push(AlternateLabel {
                    label: interpretation.label.clone(),
                    kind: AlternateKind::FunctionalInterpretation,
                });
            }
        }
        let alternate_labels = alternates
            .iter()
            .filter(|alternate| alternate.kind != AlternateKind::FunctionalInterpretation)
            .map(|alternate| alternate.label.clone())
            .collect();

        Some(RomanizedChord {
            chord: chord.clone(),
            tonic,
            roman,
            alternate_labels,
            alternates,
            degree_root: base_degree,
            degree_bass,
            roman_root_bass,
            is_hybrid: analysis.is_hybrid,
            hybrid_kind: analysis.is_hybrid.then_some(analysis.kind),
            slash_classification: analysis.slash_classification,
            hybrid_candidates,
            functional_interpretations,
            harmonic_interpretations: node
                .map(|node| node.harmonic_interpretations.clone())
                .unwrap_or_default(),
            harmonic_classifications: node
                .map(|node| node.harmonic_classifications.clone())
                .unwrap_or_default(),
            alter,
            symbol_fixed,
            theoretical_symbol,
            normalized_symbol,
            is_ii_v_start: node.is_some_and(|node| node.is_ii_v_start),
            is_resolution_target: node.is_some_and(|node| node.is_resolution_target),
            resolution_type,
        })
    }

    fn diatonic_or_borrowed_bass(
        &self,
        bass: SpelledNote,
        tonic: SpelledNote,
    ) -> Option<SpelledNote> {
        let distance = semitone_distance(bass, tonic);
        if MAJOR_SCALE_STEPS.contains(&distance) || matches!(distance, 3 | 8 | 10) {
            let degree = calc_degree_base(distance, Some(false));
            Some(spell_degree_note(degree, tonic))
        } else {
            None
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn determine_degree_name(
        &self,
        distance: u8,
        tonic: SpelledNote,
        chord: &ParsedChord,
        _previous: Option<&ParsedChord>,
        next: Option<&ParsedChord>,
        prefer_sharps: Option<bool>,
    ) -> (Degree, Vec<Degree>) {
        if distance == 6 {
            let lower = chord.quality.to_ascii_lowercase();
            let is_half_diminished = lower.contains("m7-5") || lower.contains("m7b5");
            let is_diminished = lower.contains("dim");
            let mut target_is_sharp = prefer_sharps.unwrap_or(true);
            if is_half_diminished || is_diminished {
                target_is_sharp = true;
            } else if next.is_some_and(|next| semitone_distance(next.root, tonic) == 5) {
                target_is_sharp = false;
            }

            if chord
                .bass
                .is_some_and(|bass| matches!(semitone_distance(bass, tonic), 3 | 8 | 10))
            {
                target_is_sharp = false;
            }

            if !is_half_diminished
                && !is_diminished
                && (lower.contains("m7") || lower.contains("maj7"))
            {
                target_is_sharp = false;
            }
            let sharp = Degree::new(1, RomanDegree::Iv);
            let flat = Degree::new(-1, RomanDegree::V);
            return if target_is_sharp {
                (sharp, vec![flat])
            } else {
                (flat, vec![sharp])
            };
        }

        if distance == 11
            && chord
                .bass
                .is_some_and(|bass| semitone_distance(bass, tonic) == 1)
            && prefer_sharps != Some(true)
        {
            return (
                Degree::new(-1, RomanDegree::I),
                vec![Degree::new(0, RomanDegree::Vii)],
            );
        }

        if distance == 1 && next.is_some_and(|next| semitone_distance(next.root, tonic) == 0) {
            return (Degree::new(-1, RomanDegree::Ii), Vec::new());
        }

        let prefer = prefer_sharps.unwrap_or(false);
        let base = calc_degree_base(distance, Some(prefer));
        let alternate = calc_degree_base(distance, Some(!prefer));
        let alternates = (base != alternate)
            .then_some(alternate)
            .into_iter()
            .collect();
        (base, alternates)
    }

    fn romanize_absolute_symbol(&self, symbol: &str, tonic: SpelledNote) -> String {
        let mut parts = symbol.split('/');
        let root_part = parts.next().unwrap_or(symbol);
        let bass_part = parts.next();
        let Some((root_text, suffix)) = split_note_and_suffix(root_part) else {
            return symbol.to_owned();
        };
        let Ok(root) = SpelledNote::parse(root_text) else {
            return symbol.to_owned();
        };
        let root_degree = degree_from_spelling(root, tonic);
        let roman_root = format!("{root_degree}{suffix}");
        if let Some(bass_text) = bass_part {
            if let Ok(bass) = SpelledNote::parse(bass_text) {
                return format!("{roman_root}/{}", degree_from_spelling(bass, tonic));
            }
        }
        roman_root
    }
}

fn functional_hybrid_classification(
    candidate: &HybridCandidate,
    tonic: SpelledNote,
    global_mode: TonalMode,
) -> HarmonicClassification {
    if !matches!(
        candidate.analysis.kind,
        HybridKind::SusFourNine | HybridKind::SusFourSevenFlatNine
    ) {
        return HarmonicClassification::default();
    }
    let Some(effective_root) = candidate.analysis.effective_root else {
        return HarmonicClassification::default();
    };

    // Dm7/G and similar upper-structure spellings describe a suspension over
    // G, not a D-minor modal borrowing. Giving the functional candidate its
    // own classification also prevents the lattice from copying the union of
    // unrelated ordinary-chord candidates onto the sus state.
    let mut classification = HarmonicClassification::with_role(HarmonicRole::Dominant);
    classification.local_degree = Some(degree_from_spelling(effective_root, tonic));
    classification.add_family(InterpretationFamily::SuspendedDominant);
    classification.perspective = Some(TonalPerspective {
        global_tonic: tonic,
        local_tonic: tonic,
        local_tonic_degree: degree_from_spelling(tonic, tonic),
        scope: TonalScope::Global,
        mode: global_mode,
    });
    classification
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ParsedSymbol;
    use crate::parser::parse_chord;

    fn item(symbol: &str) -> ProgressionItem {
        ProgressionItem::new(parse_chord(symbol).unwrap())
    }

    fn romans(key: &str, symbols: &[&str]) -> Vec<String> {
        let romanizer = Romanizer::new(key).unwrap();
        let items: Vec<_> = symbols.iter().map(|symbol| item(symbol)).collect();
        romanizer
            .annotate_progression(&items)
            .into_iter()
            .map(|result| result.roman)
            .collect()
    }

    #[test]
    fn diatonic_chords_in_c() {
        assert_eq!(
            romans("C", &["C", "Dm7", "Em7", "F", "G7", "Am7", "Bm7-5"]),
            ["I", "IIm7", "IIIm7", "IV", "V7", "VIm7", "VIIm7-5"]
        );
    }

    #[test]
    fn detects_ii_v_i_metadata() {
        let romanizer = Romanizer::new("C").unwrap();
        let items = [item("Dm7"), item("G7"), item("Cmaj7")];
        let results = romanizer.annotate_progression(&items);
        assert!(results[0].is_ii_v_start);
        assert_eq!(results[2].resolution_type, Some(ResolutionType::Perfect));
    }

    #[test]
    fn no_chord_is_omitted_from_compact_output_but_preserves_context() {
        let romanizer = Romanizer::new("C").unwrap();
        let items = [item("Dm7"), item("NC"), item("G7")];
        let results = romanizer.annotate_progression(&items);
        assert_eq!(results.len(), 2);
        assert!(results[0].is_ii_v_start);
        assert_eq!(romanizer.annotate_events(&items).len(), 3);
    }

    #[test]
    fn explicit_boundary_breaks_context() {
        let romanizer = Romanizer::new("C").unwrap();
        let items = [
            item("Dm7"),
            ProgressionItem::boundary("section"),
            item("G7"),
        ];
        let results = romanizer.annotate_progression(&items);
        assert!(!results[0].is_ii_v_start);
    }

    #[test]
    fn parser_symbol_variant_is_used() {
        assert!(matches!(item("C").symbol, ParsedSymbol::Chord(_)));
    }
}
