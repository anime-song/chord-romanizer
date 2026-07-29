//! Reader-facing labels projected from the selected harmonic path.
//!
//! Analysis types intentionally remain factorized so callers can inspect
//! competing meanings. This module supplies a compact 1-best presentation
//! without asking every UI or MIDI exporter to reimplement theory rules.

use crate::analysis::{
    AnalysisLattice, BlackadderFunction, BlackadderStructure, HarmonicClassification, HarmonicRole,
    HarmonicSource, InterpretationFamily, PathSelection, TonalMode, TonalPerspective, TonalScope,
};
use crate::domain::{Degree, ParsedChord, ProgressionItem, QualityClass};
use crate::interpreter::SlashClassification;
use crate::romanizer::{AnnotatedEvent, RomanizedChord, Romanizer};
use crate::speller::semitone_distance;

#[derive(Clone, Debug, Eq, PartialEq)]
/// Compact labels for one chord on the selected 1-best interpretation path.
///
/// `global_label`, `local_label`, `function_label`, and `role_label` remain
/// separate so a client can choose its own layout. `analysis_label` and
/// `combined_label` provide ready-to-render ASCII forms suitable for MIDI text
/// markers.
pub struct AnalysisDisplay {
    /// Original event index, preserved across N.C. and explicit boundaries.
    pub event_index: usize,
    /// Display-oriented chord spelling (`normalized_symbol`).
    pub symbol: String,
    /// Theory-correct spelling before optional accidental simplification.
    pub theoretical_symbol: String,
    /// Chord degree in the caller-supplied/global key.
    pub global_label: String,
    /// Chord degree inside a temporary key, including its global target.
    pub local_label: Option<String>,
    /// Specific relation such as `subV/IV`, `V/ii`, `SDm`, `D`, or `T`.
    pub function_label: Option<String>,
    /// Coarse role: `T`, `PD`, `D`, `S`, or `NF`.
    pub role_label: Option<String>,
    /// Bracket content without the chord symbol.
    pub analysis_label: String,
    /// Ready-to-render form such as `Eaug/Bb [bV7(9,#11)|subV/IV]`.
    pub combined_label: String,
}

impl Romanizer {
    /// Return one compact display projection per chord event.
    ///
    /// N.C. and boundary events are omitted, while `event_index` retains input
    /// alignment. The semantic labels come from the top-ranked interpretation
    /// path, not from the first locally generated candidate.
    pub fn display_progression(&self, progression: &[ProgressionItem]) -> Vec<AnalysisDisplay> {
        let events = self.annotate_events(progression);
        let lattice = AnalysisLattice::from_annotated_events(progression, &events, self.options());
        let path = lattice.decode_top_k_interpretations(1).into_iter().next();

        events
            .iter()
            .enumerate()
            .filter_map(|(event_index, event)| {
                let AnnotatedEvent::Chord(result) = event else {
                    return None;
                };
                let selection = path.as_ref().and_then(|path| {
                    path.selections
                        .iter()
                        .find(|selection| selection.event_index == event_index)
                });
                Some(display_for_selection(event_index, result, selection))
            })
            .collect()
    }
}

fn display_for_selection(
    event_index: usize,
    result: &RomanizedChord,
    selection: Option<&PathSelection>,
) -> AnalysisDisplay {
    let global_label = global_label(result);
    let classification = selected_classification(result, selection);
    let local_label =
        classification.and_then(|classification| local_label(&result.chord, classification));
    let role_label = classification
        .and_then(|classification| classification.role)
        .map(role_label_for)
        .map(str::to_owned);
    let function_label = function_label(selection, classification);

    // An applied predominant is most legible as `ii7/IV|PD`; targets and
    // dominants retain their global label and place local function second.
    let primary_label = if classification.is_some_and(|classification| {
        classification.role == Some(HarmonicRole::Predominant)
            && classification
                .perspective
                .as_ref()
                .is_some_and(|perspective| perspective.scope != TonalScope::Global)
    }) {
        local_label.as_deref().unwrap_or(&global_label)
    } else {
        &global_label
    };
    let analysis_label = match function_label.as_deref() {
        Some(function) if function != primary_label => format!("{primary_label}|{function}"),
        _ => primary_label.to_owned(),
    };
    let symbol = result.normalized_symbol.clone();
    let combined_label = format!("{symbol} [{analysis_label}]");

    AnalysisDisplay {
        event_index,
        symbol,
        theoretical_symbol: result.theoretical_symbol.clone(),
        global_label,
        local_label,
        function_label,
        role_label,
        analysis_label,
        combined_label,
    }
}

fn selected_classification<'a>(
    result: &'a RomanizedChord,
    selection: Option<&'a PathSelection>,
) -> Option<&'a HarmonicClassification> {
    selection
        .and_then(|selection| selection.blackadder.as_ref())
        .map(|reading| &reading.classification)
        .or_else(|| selection.and_then(|selection| selection.harmonic_classifications.first()))
        .or_else(|| result.harmonic_classifications.first())
}

fn global_label(result: &RomanizedChord) -> String {
    if let Some(alter) = &result.alter {
        return alter.clone();
    }
    if result.slash_classification == SlashClassification::Inversion
        && result.chord.quality_parsed.seventh.is_none()
    {
        if let Some(figure) = inversion_figure(&result.chord) {
            return format!(
                "{}{}",
                compact_roman(result.degree_root, &result.chord),
                figure
            );
        }
    }
    compact_roman(result.degree_root, &result.chord)
}

fn inversion_figure(chord: &ParsedChord) -> Option<&'static str> {
    let bass = chord.bass?;
    match semitone_distance(bass, chord.root) {
        3 | 4 => Some("6"),
        6..=8 => Some("64"),
        _ => None,
    }
}

fn compact_roman(degree: Degree, chord: &ParsedChord) -> String {
    let mut degree_text = degree.to_string();
    let mut quality = chord.quality.trim_start_matches(':').to_owned();
    quality = quality.replace("maj", "M").replace("ma", "M");

    if chord.quality_parsed.class == QualityClass::Minor {
        degree_text.make_ascii_lowercase();
        quality = quality
            .strip_prefix("minor")
            .or_else(|| quality.strip_prefix("min"))
            .or_else(|| quality.strip_prefix('m'))
            .unwrap_or(&quality)
            .to_owned();
    } else if matches!(
        chord.quality_parsed.class,
        QualityClass::Diminished | QualityClass::HalfDiminished
    ) {
        degree_text.make_ascii_lowercase();
    }
    format!("{degree_text}{quality}")
}

fn local_label(chord: &ParsedChord, classification: &HarmonicClassification) -> Option<String> {
    let perspective = classification.perspective.as_ref()?;
    if perspective.scope == TonalScope::Global {
        return None;
    }
    let local = if classification.role == Some(HarmonicRole::Tonic) {
        if perspective.mode == TonalMode::Minor {
            "i".to_owned()
        } else {
            "I".to_owned()
        }
    } else {
        compact_roman(classification.local_degree?, chord)
    };
    Some(format!("{}/{}", local, target_degree(perspective)))
}

fn function_label(
    selection: Option<&PathSelection>,
    classification: Option<&HarmonicClassification>,
) -> Option<String> {
    if let Some(reading) = selection.and_then(|selection| selection.blackadder.as_ref()) {
        let target = reading
            .classification
            .perspective
            .as_ref()
            .map(target_degree);
        return match reading.function {
            Some(BlackadderFunction::TritoneSubstitute) => {
                target.map(|target| format!("subV/{target}"))
            }
            Some(BlackadderFunction::SecondaryDominant)
                if reading.structure == BlackadderStructure::AugmentedTriadOverBass =>
            {
                target.map(|target| format!("V+/{target}"))
            }
            Some(BlackadderFunction::Dominant | BlackadderFunction::SecondaryDominant) => {
                target.map(|target| format!("V/{target}"))
            }
            Some(BlackadderFunction::BackdoorDominant) => {
                target.map(|target| format!("backdoorV/{target}"))
            }
            Some(BlackadderFunction::SubdominantMinor) => Some("SDm".to_owned()),
            Some(BlackadderFunction::Predominant) => Some("PD".to_owned()),
            None => None,
        };
    }

    let classification = classification?;
    let perspective = classification.perspective.as_ref();
    let target = perspective.map(target_degree);

    if classification
        .families
        .contains(&InterpretationFamily::CommonToneNeighbor)
    {
        return Some("CT".to_owned());
    }

    if classification
        .families
        .contains(&InterpretationFamily::PassingDiminished)
    {
        return Some("passdim".to_owned());
    }

    if classification
        .sources
        .contains(&HarmonicSource::SubdominantMinor)
    {
        return Some("SDm".to_owned());
    }
    if classification.role == Some(HarmonicRole::Dominant) {
        if let Some(target) = target.as_deref() {
            if perspective.is_some_and(|perspective| perspective.scope != TonalScope::Global) {
                return match classification.dominant_relation {
                    Some(crate::analysis::DominantRelation::TritoneSubstitute) => {
                        Some(format!("subV/{target}"))
                    }
                    Some(crate::analysis::DominantRelation::FifthRelated) => {
                        Some(format!("V/{target}"))
                    }
                    Some(crate::analysis::DominantRelation::Backdoor) => {
                        Some(format!("backdoorV/{target}"))
                    }
                    Some(crate::analysis::DominantRelation::LeadingTone) => {
                        Some(format!("vii/{target}"))
                    }
                    None => Some("D".to_owned()),
                };
            }
        }
    }
    if classification.role == Some(HarmonicRole::Tonic) {
        if let Some(perspective) = perspective {
            if perspective.scope != TonalScope::Global {
                let local_tonic = if perspective.mode == TonalMode::Minor {
                    "i"
                } else {
                    "I"
                };
                return Some(format!("{local_tonic}/{}", target_degree(perspective)));
            }
        }
    }
    classification.role.map(role_label_for).map(str::to_owned)
}

fn target_degree(perspective: &TonalPerspective) -> String {
    let mut target = perspective.local_tonic_degree.to_string();
    if perspective.mode == TonalMode::Minor {
        target.make_ascii_lowercase();
    }
    target
}

const fn role_label_for(role: HarmonicRole) -> &'static str {
    match role {
        HarmonicRole::Tonic => "T",
        HarmonicRole::Predominant => "PD",
        HarmonicRole::Dominant => "D",
        HarmonicRole::Subdominant => "S",
        HarmonicRole::NonFunctional => "NF",
    }
}
