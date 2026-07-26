//! Local interpretation of slash chords and augmented upper structures.
//!
//! This module answers a deliberately local question: "what could this one
//! symbol mean?" It does not decide the best interpretation of the complete
//! progression. Every plausible reading is returned as a [`HybridCandidate`]
//! with an intrinsic score and a stable rule id; progression-level code can
//! then add evidence from neighboring chords.

use crate::analysis::ScoreEvidence;
use crate::analysis::blackadder::{
    BlackadderContext, BlackadderFunction, BlackadderInterpretation, BlackadderStructure,
    analyze_blackadder, transition_score,
};
use crate::domain::{ParsedChord, QualityClass, SpelledNote};
use crate::profile::BehaviorProfile;
use crate::speller::{name_of_pitch_class, semitone_distance, spell_pitch_class};
use crate::structure;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Named functional readings recognized by the built-in rule set.
pub enum HybridKind {
    None,
    Blackadder,
    SecondaryDominantThirdInBass,
    HalfDiminishedNine,
    SusFourNine,
    SusFourSevenFlatNine,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Structural classification of a slash symbol before display formatting.
pub enum SlashClassification {
    /// The chord has no slash bass.
    None,
    /// The bass belongs to the chord formula.
    Inversion,
    /// The bass is not a chord tone and a functional reading was attempted.
    Hybrid(HybridKind),
    /// The quality was unknown, so inversion vs. hybrid cannot be established
    /// safely without inventing a major-triad formula.
    Indeterminate,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FunctionalRole {
    Dominant,
    HalfDiminished,
    SubdominantMinor,
    Predominant,
}

impl HybridKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Blackadder => "blackadder",
            Self::SecondaryDominantThirdInBass => "sec_dom_3inbass",
            Self::HalfDiminishedNine => "halfdim9",
            Self::SusFourNine => "9sus4",
            Self::SusFourSevenFlatNine => "7sus4(b9)",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
/// One local reading of a slash chord.
///
/// `root_override` is a spelling/display correction for the written upper
/// structure. `effective_root` is the functional root used for progression
/// motion. They are separate because those concepts need not name the same
/// pitch in an augmented-over-bass chord.
pub struct HybridAnalysis {
    pub is_hybrid: bool,
    pub alter: Option<String>,
    pub bass_preference: Option<bool>,
    pub root_override: Option<SpelledNote>,
    pub kind: HybridKind,
    pub slash_classification: SlashClassification,
    pub effective_root: Option<SpelledNote>,
    pub functional_role: Option<FunctionalRole>,
    /// Factorized interpretation for an exact Blackadder sonority.  Legacy
    /// callers can continue to inspect `kind`; new sequence analyzers should
    /// use this field so structure, function, and origin are not collapsed.
    pub blackadder: Option<BlackadderInterpretation>,
}

#[derive(Clone, Debug, PartialEq)]
/// A local reading plus the score and rule that produced it.
pub struct HybridCandidate {
    pub analysis: HybridAnalysis,
    pub intrinsic_score: f64,
    pub rule_id: String,
    pub evidence: Vec<ScoreEvidence>,
}

impl Default for HybridAnalysis {
    fn default() -> Self {
        Self {
            is_hybrid: false,
            alter: None,
            bass_preference: None,
            root_override: None,
            kind: HybridKind::None,
            slash_classification: SlashClassification::None,
            effective_root: None,
            functional_role: None,
            blackadder: None,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ChordInterpreter {
    behavior: BehaviorProfile,
}

impl Default for ChordInterpreter {
    fn default() -> Self {
        Self::new(BehaviorProfile::StrictV1)
    }
}

impl ChordInterpreter {
    // These are heuristic comparison weights, not calibrated probabilities.
    // Keeping them named makes the relative preference auditable and allows a
    // future external rule set to replace them without changing control flow.
    const SCORE_SEMITONE_RESOLUTION: f64 = 3.0;
    const SCORE_BACKDOOR_RESOLUTION: f64 = 5.0;
    const SCORE_STRONG_RESOLUTION: f64 = 6.0;
    const SCORE_WEAK_RESOLUTION: f64 = 2.0;
    const SCORE_BASE_BIAS: f64 = 0.5;

    pub const fn new(behavior: BehaviorProfile) -> Self {
        Self { behavior }
    }

    pub const fn behavior(self) -> BehaviorProfile {
        self.behavior
    }

    pub fn analyze_slash_chord(
        &self,
        chord: &ParsedChord,
        next_chord: Option<&ParsedChord>,
    ) -> HybridAnalysis {
        // Convenience 1-best API. The lossless API below should be used by
        // sequence analysis so a later chord can overturn a local decision.
        let candidates = self.analyze_slash_candidates(chord, next_chord);
        let mut best = candidates
            .first()
            .map(|candidate| candidate.analysis.clone())
            .unwrap_or_default();
        let mut best_score = candidates.first().map_or(f64::NEG_INFINITY, |candidate| {
            self.contextual_candidate_score(candidate, next_chord, None)
        });
        for candidate in candidates.iter().skip(1) {
            let score = self.contextual_candidate_score(candidate, next_chord, None);
            if score > best_score {
                best = candidate.analysis.clone();
                best_score = score;
            }
        }
        best
    }

    pub fn analyze_slash_candidates(
        &self,
        chord: &ParsedChord,
        next_chord: Option<&ParsedChord>,
    ) -> Vec<HybridCandidate> {
        self.analyze_slash_candidates_with_context(
            chord,
            BlackadderContext {
                next_chord,
                ..BlackadderContext::default()
            },
        )
    }

    /// Analyze a slash symbol with optional key and observation context.
    ///
    /// `analyze_slash_candidates` remains the small compatibility entry point.
    /// MIDI/MusicXML integrations should call this method and supply normalized
    /// [`crate::analysis::BlackadderObservations`] when those observations are
    /// available.
    pub fn analyze_slash_candidates_with_context(
        &self,
        chord: &ParsedChord,
        context: BlackadderContext<'_>,
    ) -> Vec<HybridCandidate> {
        // No slash means there is no ambiguity to classify, but returning one
        // neutral candidate keeps downstream lattice construction uniform.
        if chord.bass.is_none() {
            return vec![hybrid_candidate(
                HybridAnalysis::default(),
                0.0,
                "builtin.slash.none",
            )];
        }

        // Without a trusted formula we cannot know whether the bass is a chord
        // tone. StrictV1 reports uncertainty instead of treating every unknown
        // quality as a major triad.
        if self.behavior == BehaviorProfile::StrictV1
            && structure::formula(chord, self.behavior).is_none()
        {
            return vec![hybrid_candidate(
                HybridAnalysis {
                    slash_classification: SlashClassification::Indeterminate,
                    ..HybridAnalysis::default()
                },
                0.0,
                "builtin.slash.indeterminate",
            )];
        }

        // Inversion is checked before hybrid rules. A slash bass that belongs
        // to the formula needs no speculative functional reinterpretation.
        if structure::is_inversion_for(chord, self.behavior) {
            return vec![hybrid_candidate(
                HybridAnalysis {
                    slash_classification: SlashClassification::Inversion,
                    ..HybridAnalysis::default()
                },
                0.0,
                "builtin.slash.inversion",
            )];
        }

        let is_augmented = if self.behavior == BehaviorProfile::Python019 {
            structure::is_aug_quality(&chord.quality)
        } else {
            chord.quality_parsed.class == QualityClass::Augmented
        };
        // Augmented triads are symmetrical in pitch-class space, so the same
        // notes often support several functional roots. Preserve every match.
        if is_augmented {
            let candidates = self.infer_aug_candidates(chord, context);
            if !candidates.is_empty() {
                return candidates;
            }
        }

        // Non-augmented, non-inversion slash chords use the smaller sus-family
        // rule set. An unrecognized form remains a generic hybrid rather than
        // being silently converted to a specific named chord.
        let (alter, kind) = self.infer_normal_hybrid(chord);
        let intrinsic_score = match kind {
            // A complete 9sus4/b9sus4 pitch formula above the written bass is
            // already meaningful symbolic evidence. Give it a modest local
            // score; a following unsuspended dominant can reinforce it in the
            // progression lattice.
            HybridKind::SusFourNine | HybridKind::SusFourSevenFlatNine => 0.75,
            _ => 0.0,
        };
        vec![hybrid_candidate(
            HybridAnalysis {
                is_hybrid: true,
                alter,
                kind,
                slash_classification: SlashClassification::Hybrid(kind),
                effective_root: chord.bass,
                functional_role: matches!(
                    kind,
                    HybridKind::SusFourNine | HybridKind::SusFourSevenFlatNine
                )
                .then_some(FunctionalRole::Dominant),
                ..HybridAnalysis::default()
            },
            intrinsic_score,
            match kind {
                HybridKind::SusFourNine => "builtin.hybrid.sus4_9",
                HybridKind::SusFourSevenFlatNine => "builtin.hybrid.sus4_7_b9",
                _ => "builtin.hybrid.unclassified",
            },
        )]
    }

    fn infer_aug_candidates(
        &self,
        chord: &ParsedChord,
        context: BlackadderContext<'_>,
    ) -> Vec<HybridCandidate> {
        if self.behavior == BehaviorProfile::StrictV1 {
            return analyze_blackadder(chord, context, self.behavior)
                .into_iter()
                .map(|reading| {
                    let kind = match reading.interpretation.structure {
                        BlackadderStructure::HalfDiminishedAddNineOmitThird => {
                            HybridKind::HalfDiminishedNine
                        }
                        BlackadderStructure::RootlessDominantThirdInBass => {
                            HybridKind::SecondaryDominantThirdInBass
                        }
                        _ => HybridKind::Blackadder,
                    };
                    let functional_role = match reading.interpretation.function {
                        Some(
                            BlackadderFunction::Dominant
                            | BlackadderFunction::SecondaryDominant
                            | BlackadderFunction::TritoneSubstitute
                            | BlackadderFunction::BackdoorDominant,
                        ) => Some(FunctionalRole::Dominant),
                        Some(BlackadderFunction::SubdominantMinor) => {
                            Some(FunctionalRole::SubdominantMinor)
                        }
                        Some(BlackadderFunction::Predominant)
                            if reading.interpretation.structure
                                == BlackadderStructure::HalfDiminishedAddNineOmitThird =>
                        {
                            Some(FunctionalRole::HalfDiminished)
                        }
                        Some(BlackadderFunction::Predominant) => Some(FunctionalRole::Predominant),
                        None => None,
                    };
                    let rule_id = reading
                        .evidence
                        .first()
                        .map_or("builtin.blackadder", |evidence| evidence.rule_id.as_str())
                        .to_owned();
                    HybridCandidate {
                        analysis: HybridAnalysis {
                            is_hybrid: true,
                            alter: Some(reading.alter),
                            bass_preference: reading.bass_preference,
                            root_override: reading.root_override,
                            kind,
                            slash_classification: SlashClassification::Hybrid(kind),
                            effective_root: reading.interpretation.effective_root,
                            functional_role,
                            blackadder: Some(reading.interpretation),
                        },
                        intrinsic_score: reading.intrinsic_score,
                        rule_id,
                        evidence: reading.evidence,
                    }
                })
                .collect();
        }

        // Blackadder/secondary-dominant and half-diminished-nine readings are
        // not mutually exclusive. Their scores express preference only.
        let mut candidates = Vec::new();
        if let Some(candidate) = self.check_blackadder(chord, context.next_chord) {
            let rule_id = if candidate.1.kind == HybridKind::SecondaryDominantThirdInBass {
                "builtin.hybrid.secondary_dominant_third_in_bass"
            } else {
                "builtin.hybrid.blackadder"
            };
            candidates.push(hybrid_candidate(candidate.1, candidate.0, rule_id));
        }
        if let Some(candidate) = self.check_half_diminished(chord, context.next_chord) {
            candidates.push(hybrid_candidate(
                candidate.1,
                candidate.0,
                "builtin.hybrid.half_diminished_nine",
            ));
        }
        candidates
    }

    fn check_blackadder(
        &self,
        chord: &ParsedChord,
        next_chord: Option<&ParsedChord>,
    ) -> Option<(f64, HybridAnalysis)> {
        let bass = chord.bass?;
        let bass_pc = bass.pitch_class();
        let triad = structure::aug_triad_pitch_classes(chord.root);
        // The Blackadder shape is recognized when the pitch a tritone above
        // the bass occurs in the augmented upper structure. Because an
        // augmented triad is symmetrical, `anchor` gives the contextual
        // spelling used to present that structure.
        let anchor_pc = bass_pc.offset(6);
        if !triad.contains(&anchor_pc) {
            return None;
        }

        let mut score = 0.0;
        let bass_to_next = next_chord.map(|next| semitone_distance(next.root, bass));
        let bass_preference = self.bass_preference_from_resolution(bass, next_chord);
        let bass_fixed = name_of_pitch_class(bass_pc, bass_preference);
        let anchor = name_of_pitch_class(anchor_pc, bass_preference);
        let mut alter = format!("{bass_fixed}7(9,#11)");
        let mut kind = HybridKind::Blackadder;
        let mut effective_root = bass_fixed;

        // Local motion contributes evidence but does not make a rule certain.
        // A semitone continuation is suggestive; a dominant-to-tonic motion
        // from the inferred dominant root is stronger.
        if matches!(bass_to_next, Some(1 | 11)) {
            score += Self::SCORE_SEMITONE_RESOLUTION;
        }
        if next_chord.is_some_and(|next| {
            bass_to_next == Some(2) && structure::is_tonic_for(next, self.behavior)
        }) {
            score += Self::SCORE_BACKDOOR_RESOLUTION;
        }

        if let Some(next) = next_chord {
            // Reinterpret the same pitch collection as a dominant with its
            // third in the bass when that inferred dominant resolves to the
            // next tonic-like chord. This changes `effective_root`.
            let dominant_pc = anchor_pc.offset(2);
            let dominant = spell_pitch_class(anchor.letter.shift(1), dominant_pc);
            let dominant_to_next = semitone_distance(next.root, dominant);
            if matches!(dominant_to_next, 5 | 7) && structure::is_tonic_for(next, self.behavior) {
                alter = format!("{dominant}7(9,#11)/{bass_fixed}");
                kind = HybridKind::SecondaryDominantThirdInBass;
                effective_root = dominant;
                score += Self::SCORE_STRONG_RESOLUTION;
            }
        }

        let root_override = (chord.root.pitch_class() != anchor_pc).then_some(anchor);
        Some((
            score,
            HybridAnalysis {
                is_hybrid: true,
                alter: Some(alter),
                bass_preference,
                root_override,
                kind,
                slash_classification: SlashClassification::Hybrid(kind),
                effective_root: Some(effective_root),
                functional_role: Some(FunctionalRole::Dominant),
                blackadder: None,
            },
        ))
    }

    fn check_half_diminished(
        &self,
        chord: &ParsedChord,
        next_chord: Option<&ParsedChord>,
    ) -> Option<(f64, HybridAnalysis)> {
        let bass = chord.bass?;
        let bass_pc = bass.pitch_class();
        let triad = structure::aug_triad_pitch_classes(chord.root);
        // Viewed from the bass, {2, 6, 10} is the characteristic upper
        // structure of the supported m7b5(add9) interpretation.
        let relative: std::collections::HashSet<u8> =
            triad.iter().map(|pc| pc.distance_from(bass_pc)).collect();
        if ![2, 6, 10]
            .into_iter()
            .all(|value| relative.contains(&value))
        {
            return None;
        }

        // A small base bias makes this reading win a context-free tie, while a
        // following dominant provides much stronger iiø-V evidence.
        let mut score = Self::SCORE_BASE_BIAS;
        if let Some(next) = next_chord {
            let bass_to_next = semitone_distance(next.root, bass);
            let next_is_dominant = structure::is_dominant_for(next, self.behavior);
            if matches!(bass_to_next, 5 | 7) && next_is_dominant {
                score += Self::SCORE_STRONG_RESOLUTION;
            } else if next_is_dominant {
                score += Self::SCORE_WEAK_RESOLUTION;
            }
        }

        let bass_fixed = name_of_pitch_class(bass_pc, None);
        Some((
            score,
            HybridAnalysis {
                is_hybrid: true,
                alter: Some(format!("{bass_fixed}m7-5(9)")),
                bass_preference: None,
                root_override: None,
                kind: HybridKind::HalfDiminishedNine,
                slash_classification: SlashClassification::Hybrid(HybridKind::HalfDiminishedNine),
                effective_root: Some(bass_fixed),
                functional_role: Some(FunctionalRole::HalfDiminished),
                blackadder: None,
            },
        ))
    }

    fn infer_normal_hybrid(&self, chord: &ParsedChord) -> (Option<String>, HybridKind) {
        let Some(bass) = chord.bass else {
            return (None, HybridKind::None);
        };
        let distance = semitone_distance(chord.root, bass);
        let relative: std::collections::HashSet<u8> =
            structure::formula_intervals(chord, self.behavior)
                .unwrap_or_default()
                .into_iter()
                .map(|interval| (interval + distance) % 12)
                .collect();
        // Sus readings require the absence of a third above the functional
        // bass. Otherwise the same interval set could describe a conventional
        // tertian chord and the sus label would be misleading.
        let has_third = relative.contains(&3) || relative.contains(&4);
        if !has_third {
            if [2, 5, 10]
                .into_iter()
                .all(|value| relative.contains(&value))
            {
                return (
                    Some(format!(
                        "{}9sus4",
                        chord.bass_lexeme.as_deref().unwrap_or("")
                    )),
                    HybridKind::SusFourNine,
                );
            }
            if [1, 5, 10]
                .into_iter()
                .all(|value| relative.contains(&value))
            {
                return (
                    Some(format!(
                        "{}7sus4(b9)",
                        chord.bass_lexeme.as_deref().unwrap_or("")
                    )),
                    HybridKind::SusFourSevenFlatNine,
                );
            }
        }
        (None, HybridKind::None)
    }

    fn bass_preference_from_resolution(
        &self,
        bass: SpelledNote,
        next_chord: Option<&ParsedChord>,
    ) -> Option<bool> {
        let distance = semitone_distance(next_chord?.root, bass);
        match distance {
            1 => Some(true),
            11 => Some(false),
            _ => None,
        }
    }

    /// Score a candidate for the immediate symbolic continuation.  The
    /// intrinsic part remains local; Blackadder root motion is reused by the
    /// lattice as candidate-specific transition evidence.
    pub(crate) fn contextual_candidate_score(
        &self,
        candidate: &HybridCandidate,
        next_chord: Option<&ParsedChord>,
        tonic: Option<SpelledNote>,
    ) -> f64 {
        candidate.intrinsic_score
            + candidate
                .analysis
                .blackadder
                .as_ref()
                .map_or(0.0, |reading| {
                    transition_score(reading, next_chord, tonic, self.behavior)
                })
    }
}

fn hybrid_candidate(
    analysis: HybridAnalysis,
    intrinsic_score: f64,
    rule_id: &str,
) -> HybridCandidate {
    HybridCandidate {
        analysis,
        intrinsic_score,
        rule_id: rule_id.to_owned(),
        evidence: vec![ScoreEvidence::new(
            rule_id,
            intrinsic_score,
            format!("Candidate generated by {rule_id}"),
        )],
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
    fn distinguishes_inversion_and_sus_hybrid() {
        let interpreter = ChordInterpreter::default();
        assert!(
            !interpreter
                .analyze_slash_chord(&chord("E/G#"), None)
                .is_hybrid
        );
        let hybrid = interpreter.analyze_slash_chord(&chord("F/G"), None);
        assert!(hybrid.is_hybrid);
        assert_eq!(hybrid.kind, HybridKind::SusFourNine);
        assert_eq!(hybrid.alter.as_deref(), Some("G9sus4"));
    }
}
