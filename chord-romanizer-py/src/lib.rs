//! Private, high-level Python boundary for `chord-romanizer`.
//!
//! The extension deliberately exports JSON functions rather than mirroring all
//! Rust domain types as `#[pyclass]` objects.  Python owns its small public data
//! classes, while parsing, spelling, context analysis, and k-best decoding run
//! exclusively in the Rust core.  This boundary has three useful properties:
//!
//! - PyO3 remains an adapter dependency and never leaks into the pure core;
//! - the `abi3-py38` wheel needs only one build per OS/architecture;
//! - adding fields to a Rust result does not require a graph of Python classes.

use chord_romanizer::{
    AlternateKind, AnalysisDisplay, AnalysisPath, AnnotatedEvent, BlackadderFunction,
    BlackadderInterpretation, BlackadderObservationKind, BlackadderOrigin, BlackadderScale,
    BlackadderStructure, CadentialSpan, CandidateConstraint, DominantRelation, GlobalKeyRequest,
    HarmonicClassification, HarmonicResolution, HarmonicResolutionKind, HarmonicRole,
    HarmonicSource, HybridKind, InterpretationFamily, InterpretationTree, InterpretationTreeNode,
    InterpretationTreeOptions, KeyAnalysisOptions, KeyTreeRoot, KeyedAnalysisPath,
    ModulationCadence, ModulationMechanism, ModulationSpan, PendingPredominant, PendingResolution,
    PivotKind, ProgressionItem, RomanizedChord, Romanizer, RomanizerOptions, ScoreEvidence,
    SlashClassification, SpelledNote, TonalKey, TonalMode, TonalScope, TreeCondition,
    TritoneSpelling, WholeToneCollection, parse_chord,
};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Debug, Deserialize)]
struct JsonProgressionItem {
    /// Chord or N.C. symbol.  Mutually exclusive with `boundary`.
    #[serde(default)]
    symbol: Option<String>,
    /// Per-event tonic override.
    #[serde(default)]
    tonic: Option<String>,
    /// Explicit section/long-silence boundary label.
    #[serde(default)]
    boundary: Option<String>,
}

#[derive(Debug, Deserialize)]
struct JsonKeyRequest {
    /// Binding strings stay intentionally small; Rust domain types are
    /// reconstructed here so Python never becomes a second theory engine.
    #[serde(default)]
    global_key: Option<String>,
    #[serde(default)]
    global_mode: Option<String>,
    #[serde(default)]
    global_key_hint: Option<String>,
    #[serde(default)]
    global_key_hint_mode: Option<String>,
}

#[derive(Debug, Deserialize)]
struct JsonTreeRequest {
    key_request: JsonKeyRequest,
    #[serde(default)]
    condition: Option<JsonTreeCondition>,
}

#[derive(Debug, Deserialize)]
struct JsonTreeCondition {
    rule_set_version: String,
    progression_fingerprint: String,
    global_key: JsonTonalKey,
    prefix: Vec<JsonCandidateConstraint>,
}

#[derive(Debug, Deserialize)]
struct JsonTonalKey {
    tonic: String,
    mode: String,
}

#[derive(Debug, Deserialize)]
struct JsonCandidateConstraint {
    event_index: usize,
    candidate_id: String,
}

#[pyfunction]
fn annotate_progression_json(
    default_tonic: &str,
    simplify_accidentals: bool,
    behavior: &str,
    items_json: &str,
) -> PyResult<String> {
    annotate_progression_impl(
        default_tonic,
        simplify_accidentals,
        behavior,
        items_json,
        false,
    )
}

#[pyfunction]
fn annotate_events_json(
    default_tonic: &str,
    simplify_accidentals: bool,
    behavior: &str,
    items_json: &str,
) -> PyResult<String> {
    annotate_progression_impl(
        default_tonic,
        simplify_accidentals,
        behavior,
        items_json,
        true,
    )
}

#[pyfunction]
fn display_progression_json(
    default_tonic: &str,
    simplify_accidentals: bool,
    behavior: &str,
    items_json: &str,
) -> PyResult<String> {
    let romanizer = create_romanizer(default_tonic, simplify_accidentals, behavior)?;
    let items = parse_items(items_json)?;
    let values = romanizer
        .display_progression(&items)
        .iter()
        .map(analysis_display_value)
        .collect::<Vec<_>>();
    serde_json::to_string(&values).map_err(json_error)
}

#[pyfunction]
fn analyze_top_k_json(
    default_tonic: &str,
    simplify_accidentals: bool,
    behavior: &str,
    items_json: &str,
    k: usize,
) -> PyResult<String> {
    let romanizer = create_romanizer(default_tonic, simplify_accidentals, behavior)?;
    let items = parse_items(items_json)?;
    let paths = romanizer.analyze_top_k(&items, k);
    serde_json::to_string(&paths.iter().map(analysis_path_value).collect::<Vec<_>>())
        .map_err(json_error)
}

#[pyfunction]
fn analyze_top_k_interpretations_json(
    default_tonic: &str,
    simplify_accidentals: bool,
    behavior: &str,
    items_json: &str,
    k: usize,
) -> PyResult<String> {
    let romanizer = create_romanizer(default_tonic, simplify_accidentals, behavior)?;
    let items = parse_items(items_json)?;
    let paths = romanizer.analyze_top_k_interpretations(&items, k);
    serde_json::to_string(&paths.iter().map(analysis_path_value).collect::<Vec<_>>())
        .map_err(json_error)
}

#[pyfunction]
fn analyze_keys_and_functions_json(
    default_tonic: &str,
    simplify_accidentals: bool,
    behavior: &str,
    items_json: &str,
    key_request_json: &str,
    k: usize,
) -> PyResult<String> {
    let romanizer = create_romanizer(default_tonic, simplify_accidentals, behavior)?;
    let items = parse_items(items_json)?;
    let request: JsonKeyRequest = serde_json::from_str(key_request_json).map_err(json_error)?;
    let options = KeyAnalysisOptions {
        global_key: parse_global_key_request(request)?,
    };
    let paths = romanizer.analyze_keys_and_functions(&items, options, k);
    serde_json::to_string(
        &paths
            .iter()
            .map(keyed_analysis_path_value)
            .collect::<Vec<_>>(),
    )
    .map_err(json_error)
}

#[pyfunction]
fn analyze_interpretation_tree_json(
    default_tonic: &str,
    simplify_accidentals: bool,
    behavior: &str,
    items_json: &str,
    tree_request_json: &str,
    k: usize,
) -> PyResult<String> {
    let romanizer = create_romanizer(default_tonic, simplify_accidentals, behavior)?;
    let items = parse_items(items_json)?;
    let request: JsonTreeRequest = serde_json::from_str(tree_request_json).map_err(json_error)?;
    let options = InterpretationTreeOptions {
        key_analysis: KeyAnalysisOptions {
            global_key: parse_global_key_request(request.key_request)?,
        },
        condition: request.condition.map(parse_tree_condition).transpose()?,
    };
    let tree = romanizer.analyze_interpretation_tree(&items, options, k);
    serde_json::to_string(&interpretation_tree_value(&tree)).map_err(json_error)
}

fn annotate_progression_impl(
    default_tonic: &str,
    simplify_accidentals: bool,
    behavior: &str,
    items_json: &str,
    aligned: bool,
) -> PyResult<String> {
    let romanizer = create_romanizer(default_tonic, simplify_accidentals, behavior)?;
    let items = parse_items(items_json)?;

    // Always start from the aligned Rust API.  Even the compact Python result
    // needs `event_index` so it can reattach the exact caller-owned ParsedChord
    // object after N.C. and boundary events have been filtered out.
    let events = romanizer.annotate_events(&items);
    let values = events
        .iter()
        .enumerate()
        .filter_map(|(event_index, event)| match event {
            AnnotatedEvent::Chord(result) => Some(romanized_chord_value(event_index, result)),
            AnnotatedEvent::NoChord {
                original_symbol,
                tonic,
            } if aligned => Some(json!({
                "event_index": event_index,
                "kind": "no_chord",
                "original_symbol": original_symbol,
                "tonic": tonic.to_string(),
            })),
            AnnotatedEvent::Boundary { label } if aligned => Some(json!({
                "event_index": event_index,
                "kind": "boundary",
                "label": label,
            })),
            AnnotatedEvent::NoChord { .. } | AnnotatedEvent::Boundary { .. } => None,
        })
        .collect::<Vec<_>>();
    serde_json::to_string(&values).map_err(json_error)
}

fn create_romanizer(
    default_tonic: &str,
    simplify_accidentals: bool,
    behavior: &str,
) -> PyResult<Romanizer> {
    let mut options = match behavior.to_ascii_lowercase().as_str() {
        "strict" | "strict_v1" => RomanizerOptions::new(default_tonic),
        "python019" | "python_019" | "legacy" => RomanizerOptions::python_019(default_tonic),
        other => {
            return Err(PyValueError::new_err(format!(
                "unknown behavior profile '{other}'; expected 'strict_v1' or 'python019'"
            )));
        }
    }
    .map_err(value_error)?;
    options.simplify_accidentals = simplify_accidentals;
    Romanizer::with_options(options).map_err(value_error)
}

fn parse_items(items_json: &str) -> PyResult<Vec<ProgressionItem>> {
    let raw: Vec<JsonProgressionItem> = serde_json::from_str(items_json).map_err(json_error)?;
    raw.into_iter()
        .map(|item| {
            let mut progression_item = if let Some(label) = item.boundary {
                ProgressionItem::boundary(label)
            } else if let Some(symbol) = item.symbol {
                ProgressionItem::new(parse_chord(&symbol).map_err(value_error)?)
            } else {
                return Err(PyValueError::new_err(
                    "each progression item needs either 'symbol' or 'boundary'",
                ));
            };
            if let Some(tonic) = item.tonic {
                progression_item.tonic = Some(SpelledNote::parse(&tonic).map_err(value_error)?);
            }
            Ok(progression_item)
        })
        .collect()
}

fn parse_global_key_request(request: JsonKeyRequest) -> PyResult<GlobalKeyRequest> {
    if request.global_key.is_some() && request.global_key_hint.is_some() {
        return Err(PyValueError::new_err(
            "global_key and global_key_hint are mutually exclusive",
        ));
    }
    if request.global_key.is_none() && request.global_mode.is_some() {
        return Err(PyValueError::new_err("global_mode requires global_key"));
    }
    if request.global_key_hint.is_none() && request.global_key_hint_mode.is_some() {
        return Err(PyValueError::new_err(
            "global_key_hint_mode requires global_key_hint",
        ));
    }

    if let Some(tonic) = request.global_key {
        return Ok(GlobalKeyRequest::Fixed(TonalKey::new(
            SpelledNote::parse(&tonic).map_err(value_error)?,
            parse_key_mode(request.global_mode.as_deref().unwrap_or("major"))?,
        )));
    }
    if let Some(tonic) = request.global_key_hint {
        return Ok(GlobalKeyRequest::Hint(TonalKey::new(
            SpelledNote::parse(&tonic).map_err(value_error)?,
            parse_key_mode(request.global_key_hint_mode.as_deref().unwrap_or("major"))?,
        )));
    }
    Ok(GlobalKeyRequest::Infer)
}

fn parse_key_mode(mode: &str) -> PyResult<TonalMode> {
    match mode.trim().to_ascii_lowercase().as_str() {
        "major" | "maj" => Ok(TonalMode::Major),
        "minor" | "min" => Ok(TonalMode::Minor),
        _ => Err(PyValueError::new_err("key mode must be 'major' or 'minor'")),
    }
}

fn parse_tree_condition(condition: JsonTreeCondition) -> PyResult<TreeCondition> {
    Ok(TreeCondition {
        rule_set_version: condition.rule_set_version,
        progression_fingerprint: condition.progression_fingerprint,
        global_key: TonalKey::new(
            SpelledNote::parse(&condition.global_key.tonic).map_err(value_error)?,
            parse_key_mode(&condition.global_key.mode)?,
        ),
        prefix: condition
            .prefix
            .into_iter()
            .map(|constraint| CandidateConstraint {
                event_index: constraint.event_index,
                candidate_id: constraint.candidate_id,
            })
            .collect(),
    })
}

fn romanized_chord_value(event_index: usize, result: &RomanizedChord) -> Value {
    json!({
        "event_index": event_index,
        "kind": "chord",
        "chord": {
            "original_symbol": result.chord.original_symbol,
            "root": result.chord.root.to_string(),
            "quality": result.chord.quality,
            "bass": result.chord.bass.map(|bass| bass.to_string()),
        },
        "tonic": result.tonic.to_string(),
        "roman": result.roman,
        "alternate_labels": result.alternate_labels,
        "alternates": result.alternates.iter().map(|alternate| json!({
            "label": alternate.label,
            "kind": alternate_kind_name(alternate.kind),
        })).collect::<Vec<_>>(),
        "degree_root": result.degree_root.to_string(),
        "degree_bass": result.degree_bass.map(|degree| degree.to_string()),
        "roman_root_bass": result.roman_root_bass,
        "is_hybrid": result.is_hybrid,
        "hybrid_kind": result.hybrid_kind.map(HybridKind::as_str),
        "slash_classification": slash_classification_name(result.slash_classification),
        "functional_interpretations": result.functional_interpretations.iter().map(|item| json!({
            "label": item.label,
            "hybrid_kind": item.hybrid_kind.as_str(),
            "intrinsic_score": item.intrinsic_score,
            "rule_id": item.rule_id,
            "effective_root": item.effective_root.map(|root| root.to_string()),
            "blackadder": item.blackadder.as_ref().map(blackadder_value),
            "classification": harmonic_classification_value(&item.classification),
            "evidence": item.evidence.iter().map(evidence_value).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
        "harmonic_interpretations": result.harmonic_interpretations.iter().map(|item| json!({
            "intrinsic_score": item.intrinsic_score,
            "rule_id": item.rule_id,
            "classification": harmonic_classification_value(&item.classification),
            "evidence": item.evidence.iter().map(evidence_value).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
        "harmonic_classifications": result.harmonic_classifications.iter()
            .map(harmonic_classification_value)
            .collect::<Vec<_>>(),
        "alter": result.alter,
        "symbol_fixed": result.symbol_fixed,
        "theoretical_symbol": result.theoretical_symbol,
        "normalized_symbol": result.normalized_symbol,
        "is_ii_v_start": result.is_ii_v_start,
        "is_resolution_target": result.is_resolution_target,
        "resolution_type": result.resolution_type.map(|resolution| resolution.as_str()),
    })
}

fn analysis_display_value(display: &AnalysisDisplay) -> Value {
    json!({
        "event_index": display.event_index,
        "symbol": display.symbol,
        "theoretical_symbol": display.theoretical_symbol,
        "global_label": display.global_label,
        "local_label": display.local_label,
        "function_label": display.function_label,
        "role_label": display.role_label,
        "analysis_label": display.analysis_label,
        "combined_label": display.combined_label,
    })
}

fn analysis_path_value(path: &AnalysisPath) -> Value {
    json!({
        "total_score": path.total_score,
        "selections": path.selections.iter().map(|selection| json!({
            "event_index": selection.event_index,
            "candidate_id": selection.candidate_id,
            "label": selection.label,
            "hybrid_kind": selection.hybrid_kind.map(HybridKind::as_str),
            "blackadder": selection.blackadder.as_ref().map(blackadder_value),
            "harmonic_classifications": selection.harmonic_classifications.iter()
                .map(harmonic_classification_value)
                .collect::<Vec<_>>(),
            "emission_score": selection.emission_score,
            "transition_score": selection.transition_score,
            "step_score": selection.step_score,
            "cumulative_score": selection.cumulative_score,
            "evidence": selection.evidence.iter().map(evidence_value).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
        "evidence": path.evidence.iter().map(evidence_value).collect::<Vec<_>>(),
    })
}

fn keyed_analysis_path_value(path: &KeyedAnalysisPath) -> Value {
    json!({
        "global_key": tonal_key_value(path.global_key),
        "function_score": path.function_score,
        "key_score": path.key_score,
        "modulation_score": path.modulation_score,
        "memory_score": path.memory_score,
        "total_score": path.total_score,
        "modulations": path.modulations.iter().map(modulation_span_value).collect::<Vec<_>>(),
        "harmonic_resolutions": path.harmonic_resolutions.iter()
            .map(harmonic_resolution_value)
            .collect::<Vec<_>>(),
        "cadential_spans": path.cadential_spans.iter()
            .map(cadential_span_value)
            .collect::<Vec<_>>(),
        "selections": path.selections.iter().map(|keyed| {
            let selection = &keyed.selection;
            json!({
                "event_index": selection.event_index,
                "candidate_id": selection.candidate_id,
                "label": selection.label,
                "hybrid_kind": selection.hybrid_kind.map(HybridKind::as_str),
                "blackadder": selection.blackadder.as_ref().map(blackadder_value),
                "harmonic_classifications": selection.harmonic_classifications.iter()
                    .map(harmonic_classification_value)
                    .collect::<Vec<_>>(),
                "emission_score": selection.emission_score,
                "transition_score": selection.transition_score,
                "step_score": selection.step_score,
                "cumulative_score": selection.cumulative_score,
                "evidence": selection.evidence.iter().map(evidence_value).collect::<Vec<_>>(),
                "active_key": tonal_key_value(keyed.active_key),
                "local_key": tonal_key_value(keyed.local_key),
                "scope": tonal_scope_name(keyed.scope),
                "local_degree": keyed.local_degree.map(|degree| degree.to_string()),
                "role": keyed.role.map(harmonic_role_name),
                "is_pivot": keyed.is_pivot,
                "is_modulation_confirmation": keyed.is_modulation_confirmation,
                "key_region_age_chords": keyed.key_region_age_chords,
                "pending_resolutions": keyed.pending_resolutions.iter()
                    .map(pending_resolution_value)
                    .collect::<Vec<_>>(),
                "resolved_resolution_sources": keyed.resolved_resolution_sources,
                "pending_predominant": keyed.pending_predominant.as_ref()
                    .map(pending_predominant_value),
                "resolved_cadence_predominant_sources":
                    keyed.resolved_cadence_predominant_sources,
            })
        }).collect::<Vec<_>>(),
        "evidence": path.evidence.iter().map(evidence_value).collect::<Vec<_>>(),
    })
}

fn tonal_key_value(key: TonalKey) -> Value {
    json!({
        "tonic": key.tonic.to_string(),
        "mode": tonal_mode_name(key.mode),
    })
}

fn interpretation_tree_value(tree: &InterpretationTree) -> Value {
    json!({
        "rule_set_version": tree.rule_set_version,
        "progression_fingerprint": tree.progression_fingerprint,
        "requested_k": tree.requested_k,
        "returned_path_count": tree.returned_path_count,
        "best_score": tree.best_score,
        "condition": tree.condition.as_ref().map(tree_condition_value),
        "condition_applied": tree.condition_applied,
        "condition_satisfied": tree.condition_satisfied,
        "consensus_node_ids": tree.consensus_node_ids,
        "roots": tree.roots.iter().map(key_tree_root_value).collect::<Vec<_>>(),
    })
}

fn key_tree_root_value(root: &KeyTreeRoot) -> Value {
    json!({
        "node_id": root.node_id,
        "global_key": tonal_key_value(root.global_key),
        "key_score": root.key_score,
        "best_rank": root.best_rank,
        "best_path_score": root.best_path_score,
        "score_delta_from_best": root.score_delta_from_best,
        "supporting_path_ranks": root.supporting_path_ranks,
        "top_k_support_count": root.top_k_support_count,
        "top_k_support_ratio": root.top_k_support_ratio,
        "is_top_k_consensus": root.is_top_k_consensus,
        "condition": tree_condition_value(&root.condition),
        "children": root.children.iter().map(interpretation_tree_node_value).collect::<Vec<_>>(),
    })
}

fn interpretation_tree_node_value(node: &InterpretationTreeNode) -> Value {
    let selection = &node.selection.selection;
    json!({
        "node_id": node.node_id,
        "parent_id": node.parent_id,
        "event_index": node.event_index,
        "chord_index": node.chord_index,
        "input_symbol": node.input_symbol,
        "candidate_id": selection.candidate_id,
        "label": selection.label,
        "active_key": tonal_key_value(node.selection.active_key),
        "local_key": tonal_key_value(node.selection.local_key),
        "scope": tonal_scope_name(node.selection.scope),
        "local_degree": node.selection.local_degree.map(|degree| degree.to_string()),
        "role": node.selection.role.map(harmonic_role_name),
        "is_pivot": node.selection.is_pivot,
        "is_modulation_confirmation": node.selection.is_modulation_confirmation,
        "key_region_age_chords": node.selection.key_region_age_chords,
        "pending_resolutions": node.selection.pending_resolutions.iter()
            .map(pending_resolution_value)
            .collect::<Vec<_>>(),
        "resolved_resolution_sources": node.selection.resolved_resolution_sources,
        "pending_predominant": node.selection.pending_predominant.as_ref()
            .map(pending_predominant_value),
        "resolved_cadence_predominant_sources":
            node.selection.resolved_cadence_predominant_sources,
        "hybrid_kind": selection.hybrid_kind.map(HybridKind::as_str),
        "blackadder": selection.blackadder.as_ref().map(blackadder_value),
        "harmonic_classifications": selection.harmonic_classifications.iter()
            .map(harmonic_classification_value)
            .collect::<Vec<_>>(),
        "emission_score": selection.emission_score,
        "transition_score": selection.transition_score,
        "step_score": selection.step_score,
        "cumulative_score": selection.cumulative_score,
        "evidence": selection.evidence.iter().map(evidence_value).collect::<Vec<_>>(),
        "best_rank": node.best_rank,
        "best_path_score": node.best_path_score,
        "score_delta_from_best": node.score_delta_from_best,
        "supporting_path_ranks": node.supporting_path_ranks,
        "terminal_path_ranks": node.terminal_path_ranks,
        "top_k_support_count": node.top_k_support_count,
        "top_k_support_ratio": node.top_k_support_ratio,
        "is_top_k_consensus": node.is_top_k_consensus,
        "condition": tree_condition_value(&node.condition),
        "children": node.children.iter().map(interpretation_tree_node_value).collect::<Vec<_>>(),
    })
}

fn tree_condition_value(condition: &TreeCondition) -> Value {
    json!({
        "rule_set_version": condition.rule_set_version,
        "progression_fingerprint": condition.progression_fingerprint,
        "global_key": tonal_key_value(condition.global_key),
        "prefix": condition.prefix.iter().map(|constraint| json!({
            "event_index": constraint.event_index,
            "candidate_id": constraint.candidate_id,
        })).collect::<Vec<_>>(),
    })
}

fn evidence_value(evidence: &ScoreEvidence) -> Value {
    json!({
        "rule_id": evidence.rule_id,
        "contribution": evidence.contribution,
        "explanation": evidence.explanation,
    })
}

fn modulation_span_value(span: &ModulationSpan) -> Value {
    json!({
        "from_key": tonal_key_value(span.from_key),
        "to_key": tonal_key_value(span.to_key),
        "start_event_index": span.start_event_index,
        "dominant_event_index": span.dominant_event_index,
        "confirmation_event_index": span.confirmation_event_index,
        "end_event_index": span.end_event_index,
        "duration_chords": span.duration_chords,
        "mechanism": modulation_mechanism_name(span.mechanism),
        "cadence": modulation_cadence_name(span.cadence),
        "pivot": span.pivot.as_ref().map(|pivot| json!({
            "event_index": pivot.event_index,
            "chord_symbol": pivot.chord_symbol,
            "kind": pivot_kind_name(pivot.kind),
            "old_key": tonal_key_value(pivot.old_key),
            "new_key": tonal_key_value(pivot.new_key),
            "old_degree": pivot.old_degree.to_string(),
            "new_degree": pivot.new_degree.to_string(),
            "old_role": pivot.old_role.map(harmonic_role_name),
            "new_role": pivot.new_role.map(harmonic_role_name),
        })),
        "score": span.score,
        "evidence": span.evidence.iter().map(evidence_value).collect::<Vec<_>>(),
    })
}

fn pending_resolution_value(pending: &PendingResolution) -> Value {
    json!({
        "source_event_index": pending.source_event_index,
        "target_key": tonal_key_value(pending.target_key),
        "relation": dominant_relation_name(pending.relation),
        "intervening_chords": pending.intervening_chords,
        "depth": pending.depth,
        "predominant_event_index": pending.predominant_event_index,
        "predominant_intervening_chords": pending.predominant_intervening_chords,
    })
}

fn pending_predominant_value(pending: &PendingPredominant) -> Value {
    json!({
        "source_event_index": pending.source_event_index,
        "target_key": tonal_key_value(pending.target_key),
        "intervening_chords": pending.intervening_chords,
    })
}

fn harmonic_resolution_value(resolution: &HarmonicResolution) -> Value {
    json!({
        "source_event_index": resolution.source_event_index,
        "resolution_event_index": resolution.resolution_event_index,
        "target_key": tonal_key_value(resolution.target_key),
        "relation": dominant_relation_name(resolution.relation),
        "kind": harmonic_resolution_kind_name(resolution.kind),
        "intervening_chords": resolution.intervening_chords,
        "depth": resolution.depth,
        "predominant_event_index": resolution.predominant_event_index,
        "predominant_intervening_chords": resolution.predominant_intervening_chords,
        "score": resolution.score,
        "evidence": resolution.evidence.iter().map(evidence_value).collect::<Vec<_>>(),
    })
}

fn cadential_span_value(cadence: &CadentialSpan) -> Value {
    json!({
        "predominant_event_index": cadence.predominant_event_index,
        "dominant_event_index": cadence.dominant_event_index,
        "resolution_event_index": cadence.resolution_event_index,
        "target_key": tonal_key_value(cadence.target_key),
        "dominant_relation": dominant_relation_name(cadence.dominant_relation),
        "resolution_kind": harmonic_resolution_kind_name(cadence.resolution_kind),
        "intervening_before_dominant": cadence.intervening_before_dominant,
        "intervening_before_resolution": cadence.intervening_before_resolution,
        "score": cadence.score,
        "evidence": cadence.evidence.iter().map(evidence_value).collect::<Vec<_>>(),
    })
}

fn harmonic_resolution_kind_name(kind: HarmonicResolutionKind) -> &'static str {
    match kind {
        HarmonicResolutionKind::TonicArrival => "tonic_arrival",
        HarmonicResolutionKind::DominantChainLink => "dominant_chain_link",
        HarmonicResolutionKind::RootArrival => "root_arrival",
        HarmonicResolutionKind::DeceptiveArrival => "deceptive_arrival",
    }
}

fn modulation_mechanism_name(mechanism: ModulationMechanism) -> &'static str {
    match mechanism {
        ModulationMechanism::DiatonicPivot => "diatonic_pivot",
        ModulationMechanism::ChromaticPivot => "chromatic_pivot",
        ModulationMechanism::DominantBridge => "dominant_bridge",
        ModulationMechanism::DominantSequence => "dominant_sequence",
        ModulationMechanism::DirectDominant => "direct_dominant",
    }
}

fn pivot_kind_name(kind: PivotKind) -> &'static str {
    match kind {
        PivotKind::DiatonicCommonChord => "diatonic_common_chord",
        PivotKind::SecondaryCommonChord => "secondary_common_chord",
        PivotKind::BorrowedCommonChord => "borrowed_common_chord",
        PivotKind::NeapolitanCommonChord => "neapolitan_common_chord",
        PivotKind::AugmentedSixthCommonChord => "augmented_sixth_common_chord",
    }
}

fn modulation_cadence_name(cadence: ModulationCadence) -> &'static str {
    match cadence {
        ModulationCadence::Authentic => "authentic",
        ModulationCadence::PredominantAuthentic => "predominant_authentic",
    }
}

fn blackadder_value(reading: &BlackadderInterpretation) -> Value {
    let (structure, tritone_spelling) = structure_value(reading.structure);
    let (scale, whole_tone_collection) = scale_value(reading.scale);
    json!({
        "canonical_bass": reading.canonical_bass.to_string(),
        "written_upper_root": reading.written_upper_root.to_string(),
        "canonical_upper_root": reading.canonical_upper_root.to_string(),
        "structure": structure,
        "tritone_spelling": tritone_spelling,
        "function": reading.function.map(function_name),
        "origin": reading.origin.map(origin_name),
        "effective_root": reading.effective_root.map(|root| root.to_string()),
        "target_root": reading.target_root.map(|root| root.to_string()),
        "scale": scale,
        "whole_tone_collection": whole_tone_collection,
        "classification": harmonic_classification_value(&reading.classification),
        "unresolved_observations": reading.unresolved_observations.iter()
            .copied()
            .map(observation_name)
            .collect::<Vec<_>>(),
    })
}

fn harmonic_classification_value(classification: &HarmonicClassification) -> Value {
    json!({
        "role": classification.role.map(harmonic_role_name),
        "dominant_relation": classification.dominant_relation.map(dominant_relation_name),
        "local_degree": classification.local_degree.map(|degree| degree.to_string()),
        "sources": classification.sources.iter().copied()
            .map(harmonic_source_name)
            .collect::<Vec<_>>(),
        "families": classification.families.iter().copied()
            .map(interpretation_family_name)
            .collect::<Vec<_>>(),
        "perspective": classification.perspective.as_ref().map(|perspective| json!({
            "global_tonic": perspective.global_tonic.to_string(),
            "local_tonic": perspective.local_tonic.to_string(),
            "local_tonic_degree": perspective.local_tonic_degree.to_string(),
            "scope": tonal_scope_name(perspective.scope),
            "mode": tonal_mode_name(perspective.mode),
        })),
    })
}

fn harmonic_role_name(role: HarmonicRole) -> &'static str {
    match role {
        HarmonicRole::Tonic => "tonic",
        HarmonicRole::Predominant => "predominant",
        HarmonicRole::Dominant => "dominant",
        HarmonicRole::Subdominant => "subdominant",
        HarmonicRole::NonFunctional => "non_functional",
    }
}

fn dominant_relation_name(relation: DominantRelation) -> &'static str {
    match relation {
        DominantRelation::FifthRelated => "fifth_related",
        DominantRelation::TritoneSubstitute => "tritone_substitute",
        DominantRelation::Backdoor => "backdoor",
        DominantRelation::LeadingTone => "leading_tone",
    }
}

fn harmonic_source_name(source: HarmonicSource) -> &'static str {
    match source {
        HarmonicSource::ParallelMinor => "parallel_minor",
        HarmonicSource::Phrygian => "phrygian",
        HarmonicSource::Dorian => "dorian",
        HarmonicSource::Mixolydian => "mixolydian",
        HarmonicSource::Lydian => "lydian",
        HarmonicSource::Chromatic => "chromatic",
        HarmonicSource::SubdominantMinor => "subdominant_minor",
        HarmonicSource::LydianDominant => "lydian_dominant",
        HarmonicSource::LocrianNaturalTwo => "locrian_natural_two",
        HarmonicSource::WholeTone => "whole_tone",
    }
}

fn interpretation_family_name(family: InterpretationFamily) -> &'static str {
    match family {
        InterpretationFamily::AppliedCadence => "applied_cadence",
        InterpretationFamily::SecondaryDominantDeceptive => "secondary_dominant_deceptive",
        InterpretationFamily::AppliedLeadingTone => "applied_leading_tone",
        InterpretationFamily::RootlessDominantNinth => "rootless_dominant_ninth",
        InterpretationFamily::AugmentedSixth => "augmented_sixth",
        InterpretationFamily::Backdoor => "backdoor",
        InterpretationFamily::TritoneSubstitute => "tritone_substitute",
        InterpretationFamily::SubdominantMinor => "subdominant_minor",
        InterpretationFamily::ModalInterchange => "modal_interchange",
        InterpretationFamily::Neapolitan => "neapolitan",
        InterpretationFamily::ChromaticMediant => "chromatic_mediant",
        InterpretationFamily::ChromaticApproach => "chromatic_approach",
        InterpretationFamily::CommonToneNeighbor => "common_tone_neighbor",
        InterpretationFamily::PassingDiminished => "passing_diminished",
        InterpretationFamily::CommonToneDiminished => "common_tone_diminished",
        InterpretationFamily::AuxiliaryDiminished => "auxiliary_diminished",
        InterpretationFamily::TonicSubstitute => "tonic_substitute",
        InterpretationFamily::TritoneSubstituteRelatedTwo => "tritone_substitute_related_two",
        InterpretationFamily::AlternateKeySequence => "alternate_key_sequence",
        InterpretationFamily::SuspendedDominant => "suspended_dominant",
        InterpretationFamily::VoiceLeadingRequired => "voice_leading_required",
        InterpretationFamily::WholeTone => "whole_tone",
        InterpretationFamily::SplitVoiceLeading => "split_voice_leading",
        InterpretationFamily::Incidental => "incidental",
    }
}

fn tonal_scope_name(scope: TonalScope) -> &'static str {
    match scope {
        TonalScope::Global => "global",
        TonalScope::Tonicization => "tonicization",
        TonalScope::Modulation => "modulation",
    }
}

fn tonal_mode_name(mode: TonalMode) -> &'static str {
    match mode {
        TonalMode::Major => "major",
        TonalMode::Minor => "minor",
        TonalMode::Unknown => "unknown",
    }
}

fn structure_value(structure: BlackadderStructure) -> (&'static str, Option<&'static str>) {
    match structure {
        BlackadderStructure::AugmentedTriadOverBass => ("augmented_triad_over_bass", None),
        BlackadderStructure::DominantNinthOmitThirdAndFifth { tritone_spelling } => (
            "dominant_ninth_omit_third_and_fifth",
            Some(tritone_spelling_name(tritone_spelling)),
        ),
        BlackadderStructure::HalfDiminishedAddNineOmitThird => {
            ("half_diminished_add_nine_omit_third", None)
        }
        BlackadderStructure::AugmentedSeventhThirdInversion => {
            ("augmented_seventh_third_inversion", None)
        }
        BlackadderStructure::AugmentedSixth => ("augmented_sixth", None),
        BlackadderStructure::WholeToneSubset => ("whole_tone_subset", None),
        BlackadderStructure::RootlessDominantThirdInBass => {
            ("rootless_dominant_third_in_bass", None)
        }
    }
}

fn scale_value(scale: Option<BlackadderScale>) -> (Option<&'static str>, Option<&'static str>) {
    match scale {
        Some(BlackadderScale::LydianDominant) => (Some("lydian_dominant"), None),
        Some(BlackadderScale::LocrianNaturalTwo) => (Some("locrian_natural_two"), None),
        Some(BlackadderScale::WholeTone(collection)) => (
            Some("whole_tone"),
            Some(match collection {
                WholeToneCollection::EvenPitchClasses => "even_pitch_classes",
                WholeToneCollection::OddPitchClasses => "odd_pitch_classes",
            }),
        ),
        None => (None, None),
    }
}

fn tritone_spelling_name(spelling: TritoneSpelling) -> &'static str {
    match spelling {
        TritoneSpelling::SharpEleventh => "sharp_eleventh",
        TritoneSpelling::FlatFifth => "flat_fifth",
        TritoneSpelling::Ambiguous => "ambiguous",
    }
}

fn function_name(function: BlackadderFunction) -> &'static str {
    match function {
        BlackadderFunction::Dominant => "dominant",
        BlackadderFunction::SecondaryDominant => "secondary_dominant",
        BlackadderFunction::TritoneSubstitute => "tritone_substitute",
        BlackadderFunction::BackdoorDominant => "backdoor_dominant",
        BlackadderFunction::SubdominantMinor => "subdominant_minor",
        BlackadderFunction::Predominant => "predominant",
    }
}

fn origin_name(origin: BlackadderOrigin) -> &'static str {
    match origin {
        BlackadderOrigin::UpperStructureWithIndependentBass => {
            "upper_structure_with_independent_bass"
        }
        BlackadderOrigin::SplitVoiceLeading => "split_voice_leading",
        BlackadderOrigin::Incidental => "incidental",
        BlackadderOrigin::ChordScaleSonority => "chord_scale_sonority",
    }
}

fn observation_name(observation: BlackadderObservationKind) -> &'static str {
    match observation {
        BlackadderObservationKind::VoiceLeading => "voice_leading",
        BlackadderObservationKind::Timing => "timing",
        BlackadderObservationKind::MeterPosition => "meter_position",
        BlackadderObservationKind::PartSeparation => "part_separation",
        BlackadderObservationKind::MelodicScaleContext => "melodic_scale_context",
        BlackadderObservationKind::AugmentedSixthResolution => "augmented_sixth_resolution",
    }
}

fn alternate_kind_name(kind: AlternateKind) -> &'static str {
    match kind {
        AlternateKind::Enharmonic => "enharmonic",
        AlternateKind::WithoutBass => "without_bass",
        AlternateKind::FunctionalInterpretation => "functional_interpretation",
    }
}

fn slash_classification_name(classification: SlashClassification) -> &'static str {
    match classification {
        SlashClassification::None => "none",
        SlashClassification::Inversion => "inversion",
        SlashClassification::Hybrid(_) => "hybrid",
        SlashClassification::Indeterminate => "indeterminate",
    }
}

fn value_error(error: impl std::fmt::Display) -> PyErr {
    PyValueError::new_err(error.to_string())
}

fn json_error(error: serde_json::Error) -> PyErr {
    PyValueError::new_err(format!("invalid binding JSON: {error}"))
}

#[pymodule]
fn _native(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_function(wrap_pyfunction!(annotate_progression_json, module)?)?;
    module.add_function(wrap_pyfunction!(annotate_events_json, module)?)?;
    module.add_function(wrap_pyfunction!(display_progression_json, module)?)?;
    module.add_function(wrap_pyfunction!(analyze_top_k_json, module)?)?;
    module.add_function(wrap_pyfunction!(
        analyze_top_k_interpretations_json,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(analyze_keys_and_functions_json, module)?)?;
    module.add_function(wrap_pyfunction!(analyze_interpretation_tree_json, module)?)?;
    module.add("__version__", env!("CARGO_PKG_VERSION"))?;
    module.add("ABI", "abi3-py38")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn high_level_json_boundary_contains_legacy_and_blackadder_fields() {
        let input = r#"[{"symbol":"Daug/C"},{"symbol":"B"}]"#;
        let output = annotate_progression_impl("B", false, "strict_v1", input, false).unwrap();
        let value: Value = serde_json::from_str(&output).unwrap();
        assert_eq!(value[0]["roman"], "bIIIaug/bII");
        assert!(value[0]["functional_interpretations"].is_array());
    }

    #[test]
    fn display_json_exposes_ready_and_structured_labels() {
        let input = r#"[{"symbol":"Bm7"},{"symbol":"Eaug/A#"},{"symbol":"AM7"}]"#;
        let output = display_progression_json("E", false, "strict_v1", input).unwrap();
        let value: Value = serde_json::from_str(&output).unwrap();
        assert_eq!(value[0]["combined_label"], "Bm7 [ii7/IV|PD]");
        assert_eq!(value[1]["function_label"], "subV/IV");
        assert_eq!(value[2]["local_label"], "I/IV");
    }

    #[test]
    fn keyed_json_exposes_long_harmonic_memory() {
        let input =
            r#"[{"symbol":"C"},{"symbol":"D7"},{"symbol":"Am7"},{"symbol":"G"},{"symbol":"C"}]"#;
        let request = r#"{"global_key":"C","global_mode":"major","global_key_hint":null,"global_key_hint_mode":null}"#;
        let output =
            analyze_keys_and_functions_json("C", false, "strict_v1", input, request, 5).unwrap();
        let value: Value = serde_json::from_str(&output).unwrap();
        let path = value
            .as_array()
            .unwrap()
            .iter()
            .find(|path| {
                path["harmonic_resolutions"]
                    .as_array()
                    .is_some_and(|resolutions| {
                        resolutions.iter().any(|resolution| {
                            resolution["source_event_index"] == 1
                                && resolution["resolution_event_index"] == 3
                        })
                    })
            })
            .expect("delayed D7-to-G path");

        assert!(path["memory_score"].as_f64().unwrap() > 0.0);
        assert_eq!(
            path["selections"][1]["pending_resolutions"][0]["target_key"]["tonic"],
            "G"
        );
        assert_eq!(path["selections"][3]["resolved_resolution_sources"][0], 1);
    }

    #[test]
    fn keyed_json_exposes_cadential_phase_memory() {
        let input = r#"[{"symbol":"Dm7"},{"symbol":"Em7"},{"symbol":"G7"},{"symbol":"C"}]"#;
        let request = r#"{"global_key":"C","global_mode":"major","global_key_hint":null,"global_key_hint_mode":null}"#;
        let output =
            analyze_keys_and_functions_json("C", false, "strict_v1", input, request, 5).unwrap();
        let value: Value = serde_json::from_str(&output).unwrap();
        let path = value
            .as_array()
            .unwrap()
            .iter()
            .find(|path| {
                path["cadential_spans"].as_array().is_some_and(|cadences| {
                    cadences.iter().any(|cadence| {
                        cadence["predominant_event_index"] == 0
                            && cadence["dominant_event_index"] == 2
                            && cadence["resolution_event_index"] == 3
                    })
                })
            })
            .expect("delayed predominant-dominant-tonic path");

        assert_eq!(
            path["selections"][0]["pending_predominant"]["source_event_index"],
            0
        );
        assert_eq!(
            path["selections"][3]["resolved_cadence_predominant_sources"][0],
            0
        );
    }
}
