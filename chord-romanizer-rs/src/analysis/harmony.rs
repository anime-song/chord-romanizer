//! Common, multi-axis harmonic classification.
//!
//! A label such as "backdoor" does not describe the same kind of fact as
//! "predominant" or "whole tone": the first is a dominant-to-target
//! relationship, the second is a broad harmonic role, and the third is a
//! possible pitch source.  Keeping those facts on independent axes prevents a
//! flat enum from forcing musically compatible statements to compete.

use crate::domain::{Degree, SpelledNote};

use super::evidence::ScoreEvidence;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
/// Broad role heard from one tonal perspective.
pub enum HarmonicRole {
    Tonic,
    Predominant,
    Dominant,
    Subdominant,
    NonFunctional,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
/// How a dominant-related sonority approaches its local target.
pub enum DominantRelation {
    /// Root a perfect fifth above the target (ordinary V-to-I motion).
    FifthRelated,
    /// Dominant root replaced by the root a tritone away; normally resolves
    /// down by semitone to the same target.
    TritoneSubstitute,
    /// Flat-seven dominant approaching the target from a whole tone below.
    Backdoor,
    /// Diminished or half-diminished chord rooted a semitone below its local
    /// target.  This is kept separate from a root-position V because the two
    /// sonorities can be viable competing readings of incomplete chords.
    LeadingTone,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
/// Pitch vocabulary or modal borrowing that can supply a sonority.
pub enum HarmonicSource {
    /// Chord tones imported from the parallel natural-minor collection.
    ParallelMinor,
    /// Flat-two/modal vocabulary associated with the parallel Phrygian mode.
    Phrygian,
    /// Raised-six minor/modal vocabulary associated with Dorian.
    Dorian,
    /// Flat-seven major/modal vocabulary associated with Mixolydian.
    Mixolydian,
    /// Raised-four vocabulary associated with Lydian.
    Lydian,
    /// A relation justified by root/voice-leading motion rather than one
    /// seven-note parent collection.
    Chromatic,
    SubdominantMinor,
    LydianDominant,
    LocrianNaturalTwo,
    WholeTone,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
/// User-facing families derived from the independent analytical axes.
///
/// Families intentionally overlap.  For example, one candidate may be both
/// `Backdoor` and `SubdominantMinor` without creating contradictory states.
pub enum InterpretationFamily {
    AppliedCadence,
    /// A secondary dominant that reaches a tonic substitute instead of its
    /// implied local tonic, such as V7/vi -> IV in a major key.
    SecondaryDominantDeceptive,
    AppliedLeadingTone,
    /// A fully diminished seventh heard as the four upper notes of V7(b9).
    ///
    /// This is separate from `AppliedLeadingTone`: the two readings contain
    /// the same pitch classes, but one names the sonority after vii°7 while
    /// the other treats the dominant root as omitted.
    RootlessDominantNinth,
    AugmentedSixth,
    Backdoor,
    TritoneSubstitute,
    SubdominantMinor,
    ModalInterchange,
    Neapolitan,
    ChromaticMediant,
    ChromaticApproach,
    /// A chromatic neighbor chord that retains most tones of its target.
    /// For example, C#m7b5 -> Cmaj7 keeps E, G, and B while C# moves to C.
    CommonToneNeighbor,
    /// A slash bass that approaches the next harmony by semitone while an
    /// independently functional upper structure follows its own root motion.
    ChromaticApproachBass,
    /// Three or more same-shape hybrid chords moving through a symmetric
    /// minor-third root cycle. The repeated structure is primary; local
    /// dominant or modal readings remain available as alternatives.
    ConstantStructure,
    /// A diminished seventh created by chromatic motion between surrounding
    /// harmonies.  Chord symbols alone can propose this reading; MIDI/voicing
    /// evidence will eventually decide whether the required lines exist.
    PassingDiminished,
    /// A common-tone diminished seventh decorating a stable harmony, such as
    /// I - I°7 - I.
    CommonToneDiminished,
    /// Jazz/pop auxiliary-diminished view of a common-tone decoration.
    ///
    /// It accompanies `CommonToneDiminished` so clients can search either the
    /// voice-leading description or the conventional diminished category.
    AuxiliaryDiminished,
    /// A chromatic sonority retaining enough tonic pitch content to function
    /// as a weak tonic surrogate.  It remains a low-confidence alternative
    /// unless longer context or voicing evidence supports it.
    TonicSubstitute,
    /// Minor related ii that prepares a tritone-substitute dominant, e.g.
    /// bVIm7 - bII7 - I.
    TritoneSubstituteRelatedTwo,
    /// A short span that is diatonic and role-coherent in a temporary key
    /// even though it does not contain a confirming V-I cadence.
    AlternateKeySequence,
    /// A slash sonority whose upper notes form an unresolved dominant
    /// suspension above the functional bass.
    SuspendedDominant,
    /// Chord symbols suggest a functional reading, but the immediately
    /// preceding sonority also supplies strong linear/upper-structure
    /// evidence. MIDI or explicit voicing should decide the final balance.
    VoiceLeadingRequired,
    WholeTone,
    SplitVoiceLeading,
    Incidental,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum TonalScope {
    /// The local target is the caller-supplied key center.
    Global,
    /// A non-tonic degree is temporarily treated as a local tonic.  A longer
    /// span may later be upgraded to a modulation by a dedicated key model.
    Tonicization,
    /// A cadence and the surrounding harmonic span establish a key other than
    /// the global/home key.
    ///
    /// This is deliberately stronger than `Tonicization`.  A secondary
    /// dominant by itself never creates this scope: the key-segmentation model
    /// must find a confirming cadence and enough persistence or pivot evidence
    /// to make modulation a competitive complete-path interpretation.
    Modulation,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum TonalMode {
    Major,
    Minor,
    Unknown,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
/// The key frame from which one interpretation makes sense.
pub struct TonalPerspective {
    pub global_tonic: SpelledNote,
    pub local_tonic: SpelledNote,
    /// Local tonic expressed in the caller's global key, e.g. `II` for the D
    /// in C-major's `Em7b5-A7-Dm7` applied cadence.
    pub local_tonic_degree: Degree,
    pub scope: TonalScope,
    pub mode: TonalMode,
}

#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
/// Orthogonal facts attached to either a local candidate or a progression
/// interpretation.  Empty axes mean "not established", not "false".
pub struct HarmonicClassification {
    pub role: Option<HarmonicRole>,
    pub dominant_relation: Option<DominantRelation>,
    /// Chord root as heard inside `perspective.local_tonic`.  The displayed
    /// Roman numeral remains relative to the caller's global key, so this
    /// field is what lets `#IVm7` also say "ii of temporary III" without
    /// replacing either spelling.
    pub local_degree: Option<Degree>,
    pub sources: Vec<HarmonicSource>,
    pub families: Vec<InterpretationFamily>,
    pub perspective: Option<TonalPerspective>,
}

#[derive(Clone, Debug, PartialEq)]
/// One scored meaning for an ordinary chord event.
///
/// `HarmonicClassification` is intentionally just a fact bundle.  This
/// wrapper supplies the rule identity and comparison weight required by the
/// k-best lattice.  Several instances may therefore carry the same displayed
/// Roman numeral while disagreeing about function or tonal perspective.
pub struct HarmonicInterpretation {
    pub intrinsic_score: f64,
    pub rule_id: String,
    pub classification: HarmonicClassification,
    pub evidence: Vec<ScoreEvidence>,
}

impl HarmonicInterpretation {
    pub(crate) fn new(
        rule_id: impl Into<String>,
        intrinsic_score: f64,
        explanation: impl Into<String>,
        classification: HarmonicClassification,
    ) -> Self {
        let rule_id = rule_id.into();
        Self {
            intrinsic_score,
            evidence: vec![ScoreEvidence::new(
                rule_id.clone(),
                intrinsic_score,
                explanation,
            )],
            rule_id,
            classification,
        }
    }

    pub(crate) fn add_evidence(
        &mut self,
        rule_id: impl Into<String>,
        contribution: f64,
        explanation: impl Into<String>,
    ) {
        self.intrinsic_score += contribution;
        self.evidence
            .push(ScoreEvidence::new(rule_id, contribution, explanation));
    }
}

impl HarmonicClassification {
    pub(crate) fn with_role(role: HarmonicRole) -> Self {
        Self {
            role: Some(role),
            ..Self::default()
        }
    }

    pub(crate) fn add_source(&mut self, source: HarmonicSource) {
        if !self.sources.contains(&source) {
            self.sources.push(source);
        }
    }

    pub(crate) fn add_family(&mut self, family: InterpretationFamily) {
        if !self.families.contains(&family) {
            self.families.push(family);
        }
    }
}
