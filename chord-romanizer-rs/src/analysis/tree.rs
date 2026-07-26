//! UI-oriented prefix tree for joint key/function interpretations.
//!
//! A flat top-k list repeats the same early choices and hides the meaningful
//! branch point. This module folds ranked paths into a trie:
//!
//! ```text
//! global key
//! └─ event 0 interpretation
//!    ├─ event 1 interpretation A
//!    └─ event 1 interpretation B
//! ```
//!
//! Every node contains enough information for a stateless UI to draw a branch,
//! open its evidence inspector, highlight top-k consensus, and send the node's
//! condition back to recompute descendants after a click.

use crate::analysis::{
    BUILTIN_RULE_SET_VERSION, CandidateConstraint, GlobalKeyRequest, KeyAnalysisOptions,
    KeyedAnalysisPath, KeyedPathSelection, TonalKey,
};
use crate::domain::{ParsedSymbol, ProgressionItem};
use crate::romanizer::RomanizerOptions;

#[derive(Clone, Debug, Eq, PartialEq)]
/// Reusable payload for pinning a tree prefix.
pub struct TreeCondition {
    pub rule_set_version: String,
    pub progression_fingerprint: String,
    pub global_key: TonalKey,
    /// Complete root-to-node prefix, not only the final candidate. Sending
    /// this back is sufficient to recompute descendants from the full lattice.
    pub prefix: Vec<CandidateConstraint>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct InterpretationTreeOptions {
    pub key_analysis: KeyAnalysisOptions,
    pub condition: Option<TreeCondition>,
}

#[derive(Clone, Debug, PartialEq)]
/// Root grouping all returned paths which share one global key.
pub struct KeyTreeRoot {
    pub node_id: String,
    pub global_key: TonalKey,
    pub key_score: f64,
    pub best_rank: usize,
    pub best_path_score: f64,
    /// Positive distance from the best complete path. Zero means this branch
    /// contains rank 1.
    pub score_delta_from_best: f64,
    /// One-based ranks among the returned top-k paths.
    pub supporting_path_ranks: Vec<usize>,
    pub top_k_support_count: usize,
    /// Share of the returned top-k list, never a probability.
    pub top_k_support_ratio: f64,
    pub is_top_k_consensus: bool,
    pub condition: TreeCondition,
    pub children: Vec<InterpretationTreeNode>,
}

#[derive(Clone, Debug, PartialEq)]
/// One selected interpretation at one chord event.
pub struct InterpretationTreeNode {
    pub node_id: String,
    pub parent_id: String,
    /// Index in the original event stream, including N.C./boundary positions.
    pub event_index: usize,
    /// Dense chord-only position, useful as a tree UI column index.
    pub chord_index: usize,
    pub input_symbol: String,
    pub selection: KeyedPathSelection,
    pub best_rank: usize,
    pub best_path_score: f64,
    pub score_delta_from_best: f64,
    pub supporting_path_ranks: Vec<usize>,
    /// One-based path ranks which end exactly at this node.
    pub terminal_path_ranks: Vec<usize>,
    pub top_k_support_count: usize,
    pub top_k_support_ratio: f64,
    pub is_top_k_consensus: bool,
    pub condition: TreeCondition,
    pub children: Vec<InterpretationTreeNode>,
}

#[derive(Clone, Debug, PartialEq)]
/// Complete, directly renderable result for an interpretation-tree UI.
pub struct InterpretationTree {
    pub rule_set_version: String,
    pub progression_fingerprint: String,
    pub requested_k: usize,
    pub returned_path_count: usize,
    pub best_score: Option<f64>,
    pub condition: Option<TreeCondition>,
    pub condition_applied: bool,
    /// False means a persisted condition no longer identifies a valid path,
    /// usually because the rule set or input progression changed.
    pub condition_satisfied: bool,
    /// Root and event node ids shared by every returned path. This is top-k
    /// consensus only; it must not be presented as statistical certainty.
    pub consensus_node_ids: Vec<String>,
    pub roots: Vec<KeyTreeRoot>,
}

pub(crate) fn analyze_interpretation_tree(
    base_options: RomanizerOptions,
    progression: &[ProgressionItem],
    options: InterpretationTreeOptions,
    k: usize,
) -> InterpretationTree {
    let fingerprint = progression_fingerprint(progression, base_options);
    if options.condition.as_ref().is_some_and(|condition| {
        condition.rule_set_version != BUILTIN_RULE_SET_VERSION
            || condition.progression_fingerprint != fingerprint
    }) {
        return tree_from_paths(progression, Vec::new(), k, options.condition, fingerprint);
    }

    let (key_options, constraints) = options.condition.as_ref().map_or_else(
        || (options.key_analysis, &[][..]),
        |condition| {
            (
                KeyAnalysisOptions {
                    global_key: GlobalKeyRequest::Fixed(condition.global_key),
                },
                condition.prefix.as_slice(),
            )
        },
    );
    let paths = crate::analysis::analyze_keys_and_functions_conditioned(
        base_options,
        progression,
        key_options,
        constraints,
        k,
    );
    tree_from_paths(progression, paths, k, options.condition, fingerprint)
}

fn tree_from_paths(
    progression: &[ProgressionItem],
    paths: Vec<KeyedAnalysisPath>,
    requested_k: usize,
    condition: Option<TreeCondition>,
    fingerprint: String,
) -> InterpretationTree {
    let returned_path_count = paths.len();
    let best_score = paths.first().map(|path| path.total_score);
    let condition_applied = condition.is_some();
    let mut roots: Vec<KeyTreeRoot> = Vec::new();

    for (path_index, path) in paths.iter().enumerate() {
        let rank = path_index + 1;
        let root_index = roots
            .iter()
            .position(|root| root.global_key == path.global_key)
            .unwrap_or_else(|| {
                let node_id = key_node_id(path.global_key);
                roots.push(KeyTreeRoot {
                    node_id: node_id.clone(),
                    global_key: path.global_key,
                    key_score: path.key_score,
                    best_rank: rank,
                    best_path_score: path.total_score,
                    score_delta_from_best: best_score.unwrap_or(path.total_score)
                        - path.total_score,
                    supporting_path_ranks: Vec::new(),
                    top_k_support_count: 0,
                    top_k_support_ratio: 0.0,
                    is_top_k_consensus: false,
                    condition: TreeCondition {
                        rule_set_version: BUILTIN_RULE_SET_VERSION.to_owned(),
                        progression_fingerprint: fingerprint.clone(),
                        global_key: path.global_key,
                        prefix: Vec::new(),
                    },
                    children: Vec::new(),
                });
                roots.len() - 1
            });

        let root = &mut roots[root_index];
        root.supporting_path_ranks.push(rank);
        let mut prefix = Vec::new();
        insert_path(
            &mut root.children,
            progression,
            &fingerprint,
            &path.selections,
            0,
            rank,
            path.total_score,
            best_score.unwrap_or(path.total_score),
            path.global_key,
            &root.node_id,
            &mut prefix,
        );
    }

    for root in &mut roots {
        finalize_root(root, returned_path_count);
    }
    let consensus_node_ids = consensus_prefix(&roots, returned_path_count);
    let condition_satisfied = !condition_applied || !paths.is_empty();

    InterpretationTree {
        rule_set_version: BUILTIN_RULE_SET_VERSION.to_owned(),
        progression_fingerprint: fingerprint,
        requested_k,
        returned_path_count,
        best_score,
        condition,
        condition_applied,
        condition_satisfied,
        consensus_node_ids,
        roots,
    }
}

#[allow(clippy::too_many_arguments)]
fn insert_path(
    nodes: &mut Vec<InterpretationTreeNode>,
    progression: &[ProgressionItem],
    progression_fingerprint: &str,
    selections: &[KeyedPathSelection],
    chord_index: usize,
    rank: usize,
    path_score: f64,
    best_score: f64,
    global_key: TonalKey,
    parent_id: &str,
    prefix: &mut Vec<CandidateConstraint>,
) {
    let Some(selection) = selections.first() else {
        return;
    };
    prefix.push(CandidateConstraint {
        event_index: selection.selection.event_index,
        candidate_id: selection.selection.candidate_id.clone(),
    });

    let node_index = nodes
        .iter()
        .position(|node| {
            node.event_index == selection.selection.event_index
                && node.selection.selection.candidate_id == selection.selection.candidate_id
        })
        .unwrap_or_else(|| {
            let node_id = format!("{parent_id}/{}", selection.selection.candidate_id);
            nodes.push(InterpretationTreeNode {
                node_id: node_id.clone(),
                parent_id: parent_id.to_owned(),
                event_index: selection.selection.event_index,
                chord_index,
                input_symbol: input_symbol(progression, selection.selection.event_index),
                selection: selection.clone(),
                best_rank: rank,
                best_path_score: path_score,
                score_delta_from_best: best_score - path_score,
                supporting_path_ranks: Vec::new(),
                terminal_path_ranks: Vec::new(),
                top_k_support_count: 0,
                top_k_support_ratio: 0.0,
                is_top_k_consensus: false,
                condition: TreeCondition {
                    rule_set_version: BUILTIN_RULE_SET_VERSION.to_owned(),
                    progression_fingerprint: progression_fingerprint.to_owned(),
                    global_key,
                    prefix: prefix.clone(),
                },
                children: Vec::new(),
            });
            nodes.len() - 1
        });

    let node = &mut nodes[node_index];
    node.supporting_path_ranks.push(rank);
    if selections.len() == 1 {
        node.terminal_path_ranks.push(rank);
    } else {
        let node_id = node.node_id.clone();
        insert_path(
            &mut node.children,
            progression,
            progression_fingerprint,
            &selections[1..],
            chord_index + 1,
            rank,
            path_score,
            best_score,
            global_key,
            &node_id,
            prefix,
        );
    }
    prefix.pop();
}

fn input_symbol(progression: &[ProgressionItem], event_index: usize) -> String {
    match progression.get(event_index).map(|item| &item.symbol) {
        Some(ParsedSymbol::Chord(chord)) => chord.original_symbol.clone(),
        Some(ParsedSymbol::NoChord { original_symbol }) => original_symbol.clone(),
        Some(ParsedSymbol::Boundary { label }) => label.clone().unwrap_or_default(),
        None => String::new(),
    }
}

fn finalize_root(root: &mut KeyTreeRoot, returned_path_count: usize) {
    root.top_k_support_count = root.supporting_path_ranks.len();
    root.top_k_support_ratio = support_ratio(root.top_k_support_count, returned_path_count);
    root.is_top_k_consensus =
        returned_path_count > 0 && root.top_k_support_count == returned_path_count;
    for child in &mut root.children {
        finalize_node(child, returned_path_count);
    }
}

fn finalize_node(node: &mut InterpretationTreeNode, returned_path_count: usize) {
    node.top_k_support_count = node.supporting_path_ranks.len();
    node.top_k_support_ratio = support_ratio(node.top_k_support_count, returned_path_count);
    node.is_top_k_consensus =
        returned_path_count > 0 && node.top_k_support_count == returned_path_count;
    for child in &mut node.children {
        finalize_node(child, returned_path_count);
    }
}

fn support_ratio(count: usize, total: usize) -> f64 {
    if total == 0 {
        0.0
    } else {
        count as f64 / total as f64
    }
}

fn consensus_prefix(roots: &[KeyTreeRoot], returned_path_count: usize) -> Vec<String> {
    if returned_path_count == 0 {
        return Vec::new();
    }
    let Some(root) = roots
        .iter()
        .find(|root| root.top_k_support_count == returned_path_count)
    else {
        return Vec::new();
    };

    let mut output = vec![root.node_id.clone()];
    let mut children = root.children.as_slice();
    loop {
        let mut consensus = children
            .iter()
            .filter(|node| node.top_k_support_count == returned_path_count);
        let Some(node) = consensus.next() else {
            break;
        };
        if consensus.next().is_some() {
            break;
        }
        output.push(node.node_id.clone());
        children = node.children.as_slice();
    }
    output
}

fn key_node_id(key: TonalKey) -> String {
    let mode = match key.mode {
        crate::analysis::TonalMode::Major => "major",
        crate::analysis::TonalMode::Minor => "minor",
        crate::analysis::TonalMode::Unknown => "unknown",
    };
    format!("key:{}:{mode}", key.tonic)
}

fn progression_fingerprint(progression: &[ProgressionItem], options: RomanizerOptions) -> String {
    // FNV-1a is used only as a deterministic change detector, not for
    // cryptographic identity. Include event kind, original text, and explicit
    // per-event tonic so a condition cannot silently bind to a different
    // progression whose candidate ids happen to have the same ordinals.
    let mut hash = 0xcbf29ce484222325_u64;
    let mut update = |bytes: &[u8]| {
        for byte in bytes {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
    };
    update(match options.behavior {
        crate::profile::BehaviorProfile::Python019 => b"profile:python019\0",
        crate::profile::BehaviorProfile::StrictV1 => b"profile:strict-v1\0",
    });
    update(if options.simplify_accidentals {
        b"simplify:true\0"
    } else {
        b"simplify:false\0"
    });
    update(match options.key_boundary_policy {
        crate::profile::KeyBoundaryPolicy::Break => b"key-boundary:break\0",
        crate::profile::KeyBoundaryPolicy::Continue => b"key-boundary:continue\0",
    });
    update(match options.no_chord_policy {
        crate::profile::NoChordPolicy::Transparent => b"no-chord:transparent\0",
        crate::profile::NoChordPolicy::Break => b"no-chord:break\0",
    });
    for item in progression {
        match &item.symbol {
            ParsedSymbol::Chord(chord) => {
                update(b"chord\0");
                update(chord.original_symbol.as_bytes());
            }
            ParsedSymbol::NoChord { original_symbol } => {
                update(b"no-chord\0");
                update(original_symbol.as_bytes());
            }
            ParsedSymbol::Boundary { label } => {
                update(b"boundary\0");
                update(label.as_deref().unwrap_or_default().as_bytes());
            }
        }
        update(b"\0tonic\0");
        if let Some(tonic) = item.tonic {
            update(tonic.to_string().as_bytes());
        }
        update(b"\0event-end\0");
    }
    format!("fnv1a64:{hash:016x}")
}
