"""Thin public Python facade over the Rust analysis engine.

Only input normalization and Python-friendly result data classes live here.
Chord parsing for the historical ``ParsedChord`` API remains available in
Python, but every harmonic decision is made by ``chord_romanizer._native``.
"""

from __future__ import annotations

import json
from dataclasses import dataclass, field
from importlib import import_module
from typing import Any, Dict, Iterable, List, Optional, Tuple, Union

from .chord_parser import ChordParser, ParsedChord

try:
    # import_module avoids Python's misleading "partially initialized module"
    # message when somebody imports the source tree before building the Rust
    # extension.  A wheel already contains this module, while contributors can
    # create it in-place with `maturin develop --release`.
    _native_backend = import_module("._native", __package__)
except ImportError as error:
    raise ImportError(
        "chord_romanizer's native extension is not built; install a wheel or "
        "run `python -m maturin develop --release` in the project root"
    ) from error


@dataclass(frozen=True)
class ScoreEvidence:
    rule_id: str
    contribution: float
    explanation: str


@dataclass(frozen=True)
class TonalPerspective:
    """Global and temporary key centers supporting one interpretation."""

    global_tonic: str
    local_tonic: str
    local_tonic_degree: str
    scope: str
    mode: str


@dataclass(frozen=True)
class HarmonicClassification:
    """Independent role, dominant-relation, source, and tonal axes."""

    role: Optional[str] = None
    dominant_relation: Optional[str] = None
    local_degree: Optional[str] = None
    sources: List[str] = field(default_factory=list)
    families: List[str] = field(default_factory=list)
    perspective: Optional[TonalPerspective] = None


@dataclass(frozen=True)
class HarmonicInterpretation:
    """One scored meaning of an ordinary chord in its progression."""

    intrinsic_score: float
    rule_id: str
    classification: HarmonicClassification
    evidence: List[ScoreEvidence] = field(default_factory=list)


@dataclass(frozen=True)
class BlackadderInterpretation:
    canonical_bass: str
    written_upper_root: str
    canonical_upper_root: str
    structure: str
    function: Optional[str]
    origin: Optional[str]
    effective_root: Optional[str]
    target_root: Optional[str]
    scale: Optional[str]
    tritone_spelling: Optional[str] = None
    whole_tone_collection: Optional[str] = None
    unresolved_observations: List[str] = field(default_factory=list)
    classification: HarmonicClassification = field(
        default_factory=HarmonicClassification
    )

    @property
    def canonical_shape(self) -> str:
        """Return one stable augmented-over-bass spelling for this sonority.

        The written augmented root remains available separately because its
        spelling can be analytical evidence, but symmetric rotations must not
        become additional k-best interpretations.
        """

        return f"{self.canonical_upper_root}aug/{self.canonical_bass}"


@dataclass(frozen=True)
class PathSelection:
    event_index: int
    candidate_id: str
    label: str
    hybrid_kind: Optional[str] = None
    blackadder: Optional[BlackadderInterpretation] = None
    harmonic_classifications: List[HarmonicClassification] = field(
        default_factory=list
    )
    emission_score: float = 0.0
    transition_score: float = 0.0
    step_score: float = 0.0
    cumulative_score: float = 0.0
    evidence: List[ScoreEvidence] = field(default_factory=list)


@dataclass(frozen=True)
class AnalysisPath:
    selections: List[PathSelection]
    total_score: float
    evidence: List[ScoreEvidence]


@dataclass(frozen=True)
class TonalKey:
    """One inferred or caller-constrained tonal center."""

    tonic: str
    mode: str


@dataclass(frozen=True)
class PivotChord:
    """One chord read in both the departing and arriving keys."""

    event_index: int
    chord_symbol: str
    kind: str
    old_key: TonalKey
    new_key: TonalKey
    old_degree: str
    new_degree: str
    old_role: Optional[str]
    new_role: Optional[str]


@dataclass(frozen=True)
class ModulationSpan:
    """A cadence-confirmed non-global key region."""

    from_key: TonalKey
    to_key: TonalKey
    start_event_index: int
    dominant_event_index: int
    confirmation_event_index: int
    end_event_index: int
    mechanism: str
    cadence: str
    pivot: Optional[PivotChord]
    score: float
    evidence: List[ScoreEvidence]
    duration_chords: int = 0


@dataclass(frozen=True)
class PendingResolution:
    """A dominant target still audible after the current event."""

    source_event_index: int
    target_key: TonalKey
    relation: str
    intervening_chords: int
    depth: int
    predominant_event_index: Optional[int] = None
    predominant_intervening_chords: Optional[int] = None


@dataclass(frozen=True)
class PendingPredominant:
    """A cadential preparation waiting for a compatible dominant."""

    source_event_index: int
    target_key: TonalKey
    intervening_chords: int


@dataclass(frozen=True)
class HarmonicResolution:
    """One immediate or delayed resolution recovered by whole-path memory."""

    source_event_index: int
    resolution_event_index: int
    target_key: TonalKey
    relation: str
    kind: str
    intervening_chords: int
    depth: int
    score: float
    evidence: List[ScoreEvidence]
    predominant_event_index: Optional[int] = None
    predominant_intervening_chords: Optional[int] = None


@dataclass(frozen=True)
class CadentialSpan:
    """A complete predominant–dominant–resolution phrase."""

    predominant_event_index: int
    dominant_event_index: int
    resolution_event_index: int
    target_key: TonalKey
    dominant_relation: str
    resolution_kind: str
    intervening_before_dominant: int
    intervening_before_resolution: int
    score: float
    evidence: List[ScoreEvidence]


@dataclass(frozen=True)
class KeyedPathSelection:
    """One chord's function viewed from the active local key."""

    event_index: int
    candidate_id: str
    label: str
    active_key: TonalKey
    local_key: TonalKey
    scope: str
    local_degree: Optional[str]
    role: Optional[str]
    is_pivot: bool = False
    is_modulation_confirmation: bool = False
    hybrid_kind: Optional[str] = None
    blackadder: Optional[BlackadderInterpretation] = None
    harmonic_classifications: List[HarmonicClassification] = field(
        default_factory=list
    )
    emission_score: float = 0.0
    transition_score: float = 0.0
    step_score: float = 0.0
    cumulative_score: float = 0.0
    evidence: List[ScoreEvidence] = field(default_factory=list)
    key_region_age_chords: int = 0
    pending_resolutions: List[PendingResolution] = field(default_factory=list)
    resolved_resolution_sources: List[int] = field(default_factory=list)
    pending_predominant: Optional[PendingPredominant] = None
    resolved_cadence_predominant_sources: List[int] = field(
        default_factory=list
    )


@dataclass(frozen=True)
class KeyedAnalysisPath:
    """Joint global-key, local-key, and harmonic-function interpretation."""

    global_key: TonalKey
    selections: List[KeyedPathSelection]
    modulations: List[ModulationSpan]
    function_score: float
    key_score: float
    modulation_score: float
    total_score: float
    evidence: List[ScoreEvidence]
    harmonic_resolutions: List[HarmonicResolution] = field(default_factory=list)
    memory_score: float = 0.0
    cadential_spans: List[CadentialSpan] = field(default_factory=list)


@dataclass(frozen=True)
class CandidateConstraint:
    """One pinned event/candidate pair from a versioned interpretation tree."""

    event_index: int
    candidate_id: str


@dataclass(frozen=True)
class TreeCondition:
    """Stateless payload for recomputing descendants of a selected node."""

    rule_set_version: str
    progression_fingerprint: str
    global_key: TonalKey
    prefix: List[CandidateConstraint]


@dataclass(frozen=True)
class InterpretationTreeNode:
    """One renderable chord/function branch in an interpretation tree."""

    node_id: str
    parent_id: str
    event_index: int
    chord_index: int
    input_symbol: str
    candidate_id: str
    label: str
    active_key: TonalKey
    local_key: TonalKey
    scope: str
    local_degree: Optional[str]
    role: Optional[str]
    is_pivot: bool
    is_modulation_confirmation: bool
    emission_score: float
    transition_score: float
    step_score: float
    cumulative_score: float
    evidence: List[ScoreEvidence]
    best_rank: int
    best_path_score: float
    score_delta_from_best: float
    supporting_path_ranks: List[int]
    terminal_path_ranks: List[int]
    top_k_support_count: int
    top_k_support_ratio: float
    is_top_k_consensus: bool
    condition: TreeCondition
    children: List["InterpretationTreeNode"]
    hybrid_kind: Optional[str] = None
    blackadder: Optional[BlackadderInterpretation] = None
    harmonic_classifications: List[HarmonicClassification] = field(
        default_factory=list
    )
    key_region_age_chords: int = 0
    pending_resolutions: List[PendingResolution] = field(default_factory=list)
    resolved_resolution_sources: List[int] = field(default_factory=list)
    pending_predominant: Optional[PendingPredominant] = None
    resolved_cadence_predominant_sources: List[int] = field(
        default_factory=list
    )


@dataclass(frozen=True)
class KeyTreeRoot:
    """Global-key branch above the chord-by-chord prefix tree."""

    node_id: str
    global_key: TonalKey
    key_score: float
    best_rank: int
    best_path_score: float
    score_delta_from_best: float
    supporting_path_ranks: List[int]
    top_k_support_count: int
    top_k_support_ratio: float
    is_top_k_consensus: bool
    condition: TreeCondition
    children: List[InterpretationTreeNode]


@dataclass(frozen=True)
class InterpretationTree:
    """UI-ready grouping of shared Top-k interpretation prefixes."""

    rule_set_version: str
    progression_fingerprint: str
    requested_k: int
    returned_path_count: int
    best_score: Optional[float]
    condition: Optional[TreeCondition]
    condition_applied: bool
    condition_satisfied: bool
    consensus_node_ids: List[str]
    roots: List[KeyTreeRoot]


@dataclass(frozen=True)
class Boundary:
    """Explicit context boundary, normally a section break or long silence."""

    label: str


@dataclass
class RomanizedChord:
    """Python projection of the Rust ``RomanizedChord`` result.

    The original Python fields stay in their historical order.  New StrictV1
    and ambiguity-preserving fields are appended with defaults, so callers that
    instantiate or inspect the legacy surface remain source-compatible.
    """

    chord: ParsedChord
    roman: str
    alternate_labels: List[str]
    degree_root: str
    degree_bass: Optional[str] = None
    roman_root_bass: Optional[str] = None
    is_hybrid: bool = False
    alter: Optional[str] = None
    symbol_fixed: Optional[str] = None
    is_ii_v_start: bool = False
    is_resolution_target: bool = False
    resolution_type: Optional[str] = None
    tonic: Optional[str] = None
    hybrid_kind: Optional[str] = None
    slash_classification: Optional[str] = None
    alternates: List[Dict[str, Any]] = field(default_factory=list)
    functional_interpretations: List[Dict[str, Any]] = field(default_factory=list)
    harmonic_interpretations: List[HarmonicInterpretation] = field(
        default_factory=list
    )
    harmonic_classifications: List[HarmonicClassification] = field(
        default_factory=list
    )
    theoretical_symbol: Optional[str] = None
    normalized_symbol: Optional[str] = None


@dataclass(frozen=True)
class AnalysisDisplay:
    """Reader-facing projection of the selected 1-best harmonic path.

    ``combined_label`` is ready for MIDI text markers.  The remaining labels
    let a UI render the chord spelling and its theory/function separately.
    """

    event_index: int
    symbol: str
    theoretical_symbol: str
    global_label: str
    local_label: Optional[str]
    function_label: Optional[str]
    role_label: Optional[str]
    analysis_label: str
    combined_label: str


ProgressionInput = Union[
    str,
    ParsedChord,
    Boundary,
    Tuple[Union[str, ParsedChord], str],
]


class Romanizer:
    """High-level Python API backed entirely by the Rust implementation.

    ``python019`` remains the constructor default so upgrading the Python wheel
    does not silently change existing analyses.  New applications should pass
    ``behavior="strict_v1"`` (or use :meth:`strict`) to enable the corrected,
    candidate-preserving rule set and Blackadder k-best analysis.
    """

    def __init__(
        self,
        default_tonic: str = "C",
        simplify_accidentals: bool = False,
        behavior: str = "python019",
    ):
        self.default_tonic = default_tonic
        self.simplify_accidentals = simplify_accidentals
        self.behavior = _normalize_behavior(behavior)

    @classmethod
    def strict(
        cls, default_tonic: str = "C", simplify_accidentals: bool = False
    ) -> "Romanizer":
        return cls(default_tonic, simplify_accidentals, behavior="strict_v1")

    @property
    def native_backend(self) -> Dict[str, str]:
        """Diagnostic information without exposing low-level binding objects."""

        return {
            "version": _native_backend.__version__,
            "abi": _native_backend.ABI,
        }

    def romanize(self, symbol: str, tonic: Optional[str] = None) -> RomanizedChord:
        """Analyze one chord symbol."""

        parsed = _parse_symbol(symbol)
        item: ProgressionInput = (parsed, tonic) if tonic is not None else parsed
        results = self.annotate_progression([item])
        if not results:
            raise ValueError(f"symbol does not produce a chord result: {symbol!r}")
        return results[0]

    def romanize_progression(
        self, symbols: Iterable[Union[str, Tuple[str, str]]]
    ) -> List[RomanizedChord]:
        """String-oriented convenience form of :meth:`annotate_progression`."""

        return self.annotate_progression(symbols)

    def annotate_progression(
        self, progression: Iterable[ProgressionInput]
    ) -> List[RomanizedChord]:
        normalized, originals = _normalize_progression(progression, self.default_tonic)
        payload = _native_backend.annotate_progression_json(
            self.default_tonic,
            self.simplify_accidentals,
            self.behavior,
            json.dumps(normalized, ensure_ascii=False),
        )
        values = json.loads(payload)
        return [_romanized_chord(value, originals[value["event_index"]]) for value in values]

    def annotate_events(self, progression: Iterable[ProgressionInput]) -> List[Any]:
        """Return an input-aligned sequence including N.C. and boundaries.

        Chord entries are ``RomanizedChord`` objects.  Marker entries are small
        dictionaries whose ``kind`` is ``no_chord`` or ``boundary``.
        """

        normalized, originals = _normalize_progression(progression, self.default_tonic)
        payload = _native_backend.annotate_events_json(
            self.default_tonic,
            self.simplify_accidentals,
            self.behavior,
            json.dumps(normalized, ensure_ascii=False),
        )
        output: List[Any] = []
        for value in json.loads(payload):
            if value["kind"] == "chord":
                output.append(_romanized_chord(value, originals[value["event_index"]]))
            else:
                output.append(value)
        return output

    def display_progression(
        self, progression: Iterable[ProgressionInput]
    ) -> List[AnalysisDisplay]:
        """Return compact labels from the selected 1-best harmonic path.

        Chord spelling uses ``normalized_symbol`` while
        ``theoretical_symbol`` is retained separately. N.C. and boundaries are
        omitted; ``event_index`` preserves each chord's input position.
        """

        normalized, _ = _normalize_progression(progression, self.default_tonic)
        payload = _native_backend.display_progression_json(
            self.default_tonic,
            self.simplify_accidentals,
            self.behavior,
            json.dumps(normalized, ensure_ascii=False),
        )
        return [_analysis_display(value) for value in json.loads(payload)]

    def analyze_top_k(
        self, progression: Iterable[ProgressionInput], k: int = 3
    ) -> List[AnalysisPath]:
        """Return the k highest-scoring low-level progression paths."""

        if not isinstance(k, int) or isinstance(k, bool) or k < 0:
            raise ValueError("k must be a non-negative integer")
        normalized, _ = _normalize_progression(progression, self.default_tonic)
        payload = _native_backend.analyze_top_k_json(
            self.default_tonic,
            self.simplify_accidentals,
            self.behavior,
            json.dumps(normalized, ensure_ascii=False),
            k,
        )
        return [_analysis_path(value) for value in json.loads(payload)]

    def analyze_top_k_interpretations(
        self, progression: Iterable[ProgressionInput], k: int = 3
    ) -> List[AnalysisPath]:
        """Return up to k harmonically distinct progression interpretations.

        Enharmonic labels, rotations of a symmetric augmented triad, and a
        rendering that merely drops a written slash bass do not consume result
        slots. Those notation details remain available from
        :meth:`annotate_progression`.
        """

        if not isinstance(k, int) or isinstance(k, bool) or k < 0:
            raise ValueError("k must be a non-negative integer")
        normalized, _ = _normalize_progression(progression, self.default_tonic)
        payload = _native_backend.analyze_top_k_interpretations_json(
            self.default_tonic,
            self.simplify_accidentals,
            self.behavior,
            json.dumps(normalized, ensure_ascii=False),
            k,
        )
        return [_analysis_path(value) for value in json.loads(payload)]

    def analyze_keys_and_functions(
        self,
        progression: Iterable[ProgressionInput],
        k: int = 3,
        *,
        global_key: Optional[str] = None,
        global_mode: Optional[str] = None,
        global_key_hint: Optional[str] = None,
        global_key_hint_mode: Optional[str] = None,
    ) -> List[KeyedAnalysisPath]:
        """Jointly infer keys and harmonic functions.

        With no key arguments, all 12 pitch classes in major and minor are
        evaluated. ``global_key_hint`` adds a non-binding prior, while
        ``global_key`` constrains every returned path to that key.  A supplied
        key defaults to major unless its corresponding mode argument is set.

        Scores are comparison weights, not probabilities.  Local keys come
        from the selected functional candidate. ``active_key`` changes only
        when a cadence-confirmed modulation is selected; ``local_key`` can
        still hold a nested tonicization.  Short secondary cadences and
        modulation upgrades remain separate Top-k paths. Multiple confirmed
        regions form an ordered ``from_key -> to_key`` chain, including a
        possible return to the global key. ``harmonic_resolutions`` and the
        per-selection pending snapshots expose delayed dominant goals used by
        the whole-path reranker. ``cadential_spans`` additionally connects a
        remembered predominant to its dominant and normal/deceptive arrival.
        """

        if not isinstance(k, int) or isinstance(k, bool) or k < 0:
            raise ValueError("k must be a non-negative integer")
        if global_key is not None and global_key_hint is not None:
            raise ValueError("global_key and global_key_hint are mutually exclusive")
        if global_key is None and global_mode is not None:
            raise ValueError("global_mode requires global_key")
        if global_key_hint is None and global_key_hint_mode is not None:
            raise ValueError("global_key_hint_mode requires global_key_hint")

        key_request = {
            "global_key": global_key,
            "global_mode": _normalize_key_mode(global_mode, global_key),
            "global_key_hint": global_key_hint,
            "global_key_hint_mode": _normalize_key_mode(
                global_key_hint_mode, global_key_hint
            ),
        }
        normalized, _ = _normalize_progression(progression, self.default_tonic)
        payload = _native_backend.analyze_keys_and_functions_json(
            self.default_tonic,
            self.simplify_accidentals,
            self.behavior,
            json.dumps(normalized, ensure_ascii=False),
            json.dumps(key_request),
            k,
        )
        return [_keyed_analysis_path(value) for value in json.loads(payload)]

    def analyze_interpretation_tree(
        self,
        progression: Iterable[ProgressionInput],
        k: int = 5,
        *,
        global_key: Optional[str] = None,
        global_mode: Optional[str] = None,
        global_key_hint: Optional[str] = None,
        global_key_hint_mode: Optional[str] = None,
        condition: Optional[TreeCondition] = None,
    ) -> InterpretationTree:
        """Return a prefix tree ready for an interactive analysis UI.

        Roots represent global-key candidates. Chord nodes group shared Top-k
        prefixes and include score deltas, supporting path ranks, active/local
        keys, key-region age, pending/resolved dominant and predominant
        goals, pivot and modulation-confirmation markers, function, and
        node-local evidence.

        Passing ``condition=node.condition`` fixes the selected prefix and
        recomputes its descendants from the complete lattice. It is not merely
        a filter over the previous Top-k result.
        """

        if not isinstance(k, int) or isinstance(k, bool) or k < 0:
            raise ValueError("k must be a non-negative integer")
        key_arguments = (
            global_key,
            global_mode,
            global_key_hint,
            global_key_hint_mode,
        )
        if condition is not None:
            if not isinstance(condition, TreeCondition):
                raise TypeError("condition must be a TreeCondition")
            if any(value is not None for value in key_arguments):
                raise ValueError(
                    "condition cannot be combined with global key arguments"
                )
            key_request = {
                "global_key": None,
                "global_mode": None,
                "global_key_hint": None,
                "global_key_hint_mode": None,
            }
        else:
            if global_key is not None and global_key_hint is not None:
                raise ValueError(
                    "global_key and global_key_hint are mutually exclusive"
                )
            if global_key is None and global_mode is not None:
                raise ValueError("global_mode requires global_key")
            if global_key_hint is None and global_key_hint_mode is not None:
                raise ValueError(
                    "global_key_hint_mode requires global_key_hint"
                )
            key_request = {
                "global_key": global_key,
                "global_mode": _normalize_key_mode(global_mode, global_key),
                "global_key_hint": global_key_hint,
                "global_key_hint_mode": _normalize_key_mode(
                    global_key_hint_mode, global_key_hint
                ),
            }

        normalized, _ = _normalize_progression(progression, self.default_tonic)
        tree_request = {
            "key_request": key_request,
            "condition": (
                _tree_condition_value(condition)
                if condition is not None
                else None
            ),
        }
        payload = _native_backend.analyze_interpretation_tree_json(
            self.default_tonic,
            self.simplify_accidentals,
            self.behavior,
            json.dumps(normalized, ensure_ascii=False),
            json.dumps(tree_request, ensure_ascii=False),
            k,
        )
        return _interpretation_tree(json.loads(payload))


def _normalize_behavior(value: str) -> str:
    normalized = value.strip().lower()
    aliases = {
        "strict": "strict_v1",
        "strict_v1": "strict_v1",
        "python019": "python019",
        "python_019": "python019",
        "legacy": "python019",
    }
    try:
        return aliases[normalized]
    except KeyError as error:
        raise ValueError(
            "behavior must be 'strict_v1' or 'python019'"
        ) from error


def _normalize_key_mode(value: Optional[str], tonic: Optional[str]) -> Optional[str]:
    if tonic is None:
        return None
    normalized = "major" if value is None else value.strip().lower()
    aliases = {
        "major": "major",
        "maj": "major",
        "minor": "minor",
        "min": "minor",
    }
    try:
        return aliases[normalized]
    except KeyError as error:
        raise ValueError("key mode must be 'major' or 'minor'") from error


def _parse_symbol(symbol: str) -> ParsedChord:
    parsed = ChordParser.parse(symbol)
    if parsed is None:
        raise ValueError(f"invalid chord symbol: {symbol!r}")
    return parsed


def _symbol_from_parsed(chord: ParsedChord) -> str:
    # Some historical callers construct ParsedChord manually and leave the
    # slash bass out of `symbol`.  Rebuild from structured fields so Rust sees
    # the same chord that the Python object represents.
    if chord.root == "NC":
        return chord.symbol
    body = f"{chord.root}{chord.quality}"
    return f"{body}/{chord.bass}" if chord.bass else body


def _normalize_progression(
    progression: Iterable[ProgressionInput], default_tonic: str
) -> Tuple[List[Dict[str, Optional[str]]], List[Optional[ParsedChord]]]:
    native_items: List[Dict[str, Optional[str]]] = []
    originals: List[Optional[ParsedChord]] = []
    for raw_item in progression:
        if isinstance(raw_item, Boundary):
            native_items.append({"boundary": raw_item.label})
            originals.append(None)
            continue

        tonic: Optional[str] = None
        item: Union[str, ParsedChord]
        if isinstance(raw_item, tuple):
            if len(raw_item) != 2:
                raise TypeError("progression tuples must be (chord, tonic)")
            item, tonic = raw_item
            if not isinstance(tonic, str):
                raise TypeError("per-chord tonic must be a string")
        else:
            item = raw_item

        chord = _parse_symbol(item) if isinstance(item, str) else item
        if not isinstance(chord, ParsedChord):
            raise TypeError(
                "progression items must be chord strings, ParsedChord, Boundary, "
                "or (chord, tonic) tuples"
            )
        native_items.append(
            {
                "symbol": _symbol_from_parsed(chord),
                "tonic": tonic if tonic is not None else None,
            }
        )
        originals.append(chord)
    return native_items, originals


def _romanized_chord(value: Dict[str, Any], chord: Optional[ParsedChord]) -> RomanizedChord:
    if chord is None:
        # This should only occur if the Rust/Python event alignment contract is
        # broken.  Fail loudly instead of constructing a misleading object.
        raise RuntimeError("native chord result points to a boundary event")
    return RomanizedChord(
        chord=chord,
        roman=value["roman"],
        alternate_labels=value["alternate_labels"],
        degree_root=value["degree_root"],
        degree_bass=value["degree_bass"],
        roman_root_bass=value["roman_root_bass"],
        is_hybrid=value["is_hybrid"],
        alter=value["alter"],
        symbol_fixed=value["symbol_fixed"],
        is_ii_v_start=value["is_ii_v_start"],
        is_resolution_target=value["is_resolution_target"],
        resolution_type=value["resolution_type"],
        tonic=value["tonic"],
        hybrid_kind=value["hybrid_kind"],
        slash_classification=value["slash_classification"],
        alternates=value["alternates"],
        functional_interpretations=value["functional_interpretations"],
        harmonic_interpretations=[
            HarmonicInterpretation(
                intrinsic_score=item["intrinsic_score"],
                rule_id=item["rule_id"],
                classification=_harmonic_classification(item["classification"]),
                evidence=[_evidence(evidence) for evidence in item["evidence"]],
            )
            for item in value["harmonic_interpretations"]
        ],
        harmonic_classifications=[
            _harmonic_classification(item)
            for item in value["harmonic_classifications"]
        ],
        theoretical_symbol=value["theoretical_symbol"],
        normalized_symbol=value["normalized_symbol"],
    )


def _analysis_display(value: Dict[str, Any]) -> AnalysisDisplay:
    return AnalysisDisplay(
        event_index=value["event_index"],
        symbol=value["symbol"],
        theoretical_symbol=value["theoretical_symbol"],
        global_label=value["global_label"],
        local_label=value["local_label"],
        function_label=value["function_label"],
        role_label=value["role_label"],
        analysis_label=value["analysis_label"],
        combined_label=value["combined_label"],
    )


def _blackadder(value: Optional[Dict[str, Any]]) -> Optional[BlackadderInterpretation]:
    if value is None:
        return None
    return BlackadderInterpretation(
        canonical_bass=value["canonical_bass"],
        written_upper_root=value["written_upper_root"],
        canonical_upper_root=value["canonical_upper_root"],
        structure=value["structure"],
        function=value["function"],
        origin=value["origin"],
        effective_root=value["effective_root"],
        target_root=value["target_root"],
        scale=value["scale"],
        tritone_spelling=value["tritone_spelling"],
        whole_tone_collection=value["whole_tone_collection"],
        unresolved_observations=value["unresolved_observations"],
        classification=_harmonic_classification(value["classification"]),
    )


def _harmonic_classification(value: Dict[str, Any]) -> HarmonicClassification:
    perspective = value["perspective"]
    return HarmonicClassification(
        role=value["role"],
        dominant_relation=value["dominant_relation"],
        local_degree=value["local_degree"],
        sources=value["sources"],
        families=value["families"],
        perspective=(
            TonalPerspective(
                global_tonic=perspective["global_tonic"],
                local_tonic=perspective["local_tonic"],
                local_tonic_degree=perspective["local_tonic_degree"],
                scope=perspective["scope"],
                mode=perspective["mode"],
            )
            if perspective is not None
            else None
        ),
    )


def _evidence(value: Dict[str, Any]) -> ScoreEvidence:
    return ScoreEvidence(
        rule_id=value["rule_id"],
        contribution=value["contribution"],
        explanation=value["explanation"],
    )


def _analysis_path(value: Dict[str, Any]) -> AnalysisPath:
    return AnalysisPath(
        selections=[
            PathSelection(
                event_index=selection["event_index"],
                candidate_id=selection["candidate_id"],
                label=selection["label"],
                hybrid_kind=selection["hybrid_kind"],
                blackadder=_blackadder(selection["blackadder"]),
                harmonic_classifications=[
                    _harmonic_classification(item)
                    for item in selection["harmonic_classifications"]
                ],
                emission_score=selection["emission_score"],
                transition_score=selection["transition_score"],
                step_score=selection["step_score"],
                cumulative_score=selection["cumulative_score"],
                evidence=[_evidence(item) for item in selection["evidence"]],
            )
            for selection in value["selections"]
        ],
        total_score=value["total_score"],
        evidence=[_evidence(item) for item in value["evidence"]],
    )


def _tonal_key(value: Dict[str, Any]) -> TonalKey:
    return TonalKey(tonic=value["tonic"], mode=value["mode"])


def _keyed_analysis_path(value: Dict[str, Any]) -> KeyedAnalysisPath:
    return KeyedAnalysisPath(
        global_key=_tonal_key(value["global_key"]),
        selections=[
            KeyedPathSelection(
                event_index=selection["event_index"],
                candidate_id=selection["candidate_id"],
                label=selection["label"],
                active_key=_tonal_key(selection["active_key"]),
                local_key=_tonal_key(selection["local_key"]),
                scope=selection["scope"],
                local_degree=selection["local_degree"],
                role=selection["role"],
                is_pivot=selection["is_pivot"],
                is_modulation_confirmation=selection[
                    "is_modulation_confirmation"
                ],
                key_region_age_chords=selection["key_region_age_chords"],
                pending_resolutions=[
                    _pending_resolution(item)
                    for item in selection["pending_resolutions"]
                ],
                resolved_resolution_sources=selection[
                    "resolved_resolution_sources"
                ],
                pending_predominant=(
                    _pending_predominant(selection["pending_predominant"])
                    if selection["pending_predominant"] is not None
                    else None
                ),
                resolved_cadence_predominant_sources=selection[
                    "resolved_cadence_predominant_sources"
                ],
                hybrid_kind=selection["hybrid_kind"],
                blackadder=_blackadder(selection["blackadder"]),
                harmonic_classifications=[
                    _harmonic_classification(item)
                    for item in selection["harmonic_classifications"]
                ],
                emission_score=selection["emission_score"],
                transition_score=selection["transition_score"],
                step_score=selection["step_score"],
                cumulative_score=selection["cumulative_score"],
                evidence=[_evidence(item) for item in selection["evidence"]],
            )
            for selection in value["selections"]
        ],
        modulations=[_modulation_span(span) for span in value["modulations"]],
        harmonic_resolutions=[
            _harmonic_resolution(item)
            for item in value["harmonic_resolutions"]
        ],
        cadential_spans=[
            _cadential_span(item) for item in value["cadential_spans"]
        ],
        function_score=value["function_score"],
        key_score=value["key_score"],
        modulation_score=value["modulation_score"],
        memory_score=value["memory_score"],
        total_score=value["total_score"],
        evidence=[_evidence(item) for item in value["evidence"]],
    )


def _modulation_span(value: Dict[str, Any]) -> ModulationSpan:
    pivot = value["pivot"]
    return ModulationSpan(
        from_key=_tonal_key(value["from_key"]),
        to_key=_tonal_key(value["to_key"]),
        start_event_index=value["start_event_index"],
        dominant_event_index=value["dominant_event_index"],
        confirmation_event_index=value["confirmation_event_index"],
        end_event_index=value["end_event_index"],
        duration_chords=value["duration_chords"],
        mechanism=value["mechanism"],
        cadence=value["cadence"],
        pivot=(
            PivotChord(
                event_index=pivot["event_index"],
                chord_symbol=pivot["chord_symbol"],
                kind=pivot["kind"],
                old_key=_tonal_key(pivot["old_key"]),
                new_key=_tonal_key(pivot["new_key"]),
                old_degree=pivot["old_degree"],
                new_degree=pivot["new_degree"],
                old_role=pivot["old_role"],
                new_role=pivot["new_role"],
            )
            if pivot is not None
            else None
        ),
        score=value["score"],
        evidence=[_evidence(item) for item in value["evidence"]],
    )


def _pending_resolution(value: Dict[str, Any]) -> PendingResolution:
    return PendingResolution(
        source_event_index=value["source_event_index"],
        target_key=_tonal_key(value["target_key"]),
        relation=value["relation"],
        intervening_chords=value["intervening_chords"],
        depth=value["depth"],
        predominant_event_index=value["predominant_event_index"],
        predominant_intervening_chords=value[
            "predominant_intervening_chords"
        ],
    )


def _pending_predominant(value: Dict[str, Any]) -> PendingPredominant:
    return PendingPredominant(
        source_event_index=value["source_event_index"],
        target_key=_tonal_key(value["target_key"]),
        intervening_chords=value["intervening_chords"],
    )


def _harmonic_resolution(value: Dict[str, Any]) -> HarmonicResolution:
    return HarmonicResolution(
        source_event_index=value["source_event_index"],
        resolution_event_index=value["resolution_event_index"],
        target_key=_tonal_key(value["target_key"]),
        relation=value["relation"],
        kind=value["kind"],
        intervening_chords=value["intervening_chords"],
        depth=value["depth"],
        score=value["score"],
        evidence=[_evidence(item) for item in value["evidence"]],
        predominant_event_index=value["predominant_event_index"],
        predominant_intervening_chords=value[
            "predominant_intervening_chords"
        ],
    )


def _cadential_span(value: Dict[str, Any]) -> CadentialSpan:
    return CadentialSpan(
        predominant_event_index=value["predominant_event_index"],
        dominant_event_index=value["dominant_event_index"],
        resolution_event_index=value["resolution_event_index"],
        target_key=_tonal_key(value["target_key"]),
        dominant_relation=value["dominant_relation"],
        resolution_kind=value["resolution_kind"],
        intervening_before_dominant=value["intervening_before_dominant"],
        intervening_before_resolution=value["intervening_before_resolution"],
        score=value["score"],
        evidence=[_evidence(item) for item in value["evidence"]],
    )


def _tree_condition_value(condition: TreeCondition) -> Dict[str, Any]:
    return {
        "rule_set_version": condition.rule_set_version,
        "progression_fingerprint": condition.progression_fingerprint,
        "global_key": {
            "tonic": condition.global_key.tonic,
            "mode": condition.global_key.mode,
        },
        "prefix": [
            {
                "event_index": constraint.event_index,
                "candidate_id": constraint.candidate_id,
            }
            for constraint in condition.prefix
        ],
    }


def _tree_condition(value: Dict[str, Any]) -> TreeCondition:
    return TreeCondition(
        rule_set_version=value["rule_set_version"],
        progression_fingerprint=value["progression_fingerprint"],
        global_key=_tonal_key(value["global_key"]),
        prefix=[
            CandidateConstraint(
                event_index=constraint["event_index"],
                candidate_id=constraint["candidate_id"],
            )
            for constraint in value["prefix"]
        ],
    )


def _interpretation_tree_node(value: Dict[str, Any]) -> InterpretationTreeNode:
    return InterpretationTreeNode(
        node_id=value["node_id"],
        parent_id=value["parent_id"],
        event_index=value["event_index"],
        chord_index=value["chord_index"],
        input_symbol=value["input_symbol"],
        candidate_id=value["candidate_id"],
        label=value["label"],
        active_key=_tonal_key(value["active_key"]),
        local_key=_tonal_key(value["local_key"]),
        scope=value["scope"],
        local_degree=value["local_degree"],
        role=value["role"],
        is_pivot=value["is_pivot"],
        is_modulation_confirmation=value["is_modulation_confirmation"],
        key_region_age_chords=value["key_region_age_chords"],
        pending_resolutions=[
            _pending_resolution(item) for item in value["pending_resolutions"]
        ],
        resolved_resolution_sources=value["resolved_resolution_sources"],
        pending_predominant=(
            _pending_predominant(value["pending_predominant"])
            if value["pending_predominant"] is not None
            else None
        ),
        resolved_cadence_predominant_sources=value[
            "resolved_cadence_predominant_sources"
        ],
        emission_score=value["emission_score"],
        transition_score=value["transition_score"],
        step_score=value["step_score"],
        cumulative_score=value["cumulative_score"],
        evidence=[_evidence(item) for item in value["evidence"]],
        best_rank=value["best_rank"],
        best_path_score=value["best_path_score"],
        score_delta_from_best=value["score_delta_from_best"],
        supporting_path_ranks=value["supporting_path_ranks"],
        terminal_path_ranks=value["terminal_path_ranks"],
        top_k_support_count=value["top_k_support_count"],
        top_k_support_ratio=value["top_k_support_ratio"],
        is_top_k_consensus=value["is_top_k_consensus"],
        condition=_tree_condition(value["condition"]),
        children=[
            _interpretation_tree_node(child) for child in value["children"]
        ],
        hybrid_kind=value["hybrid_kind"],
        blackadder=_blackadder(value["blackadder"]),
        harmonic_classifications=[
            _harmonic_classification(item)
            for item in value["harmonic_classifications"]
        ],
    )


def _key_tree_root(value: Dict[str, Any]) -> KeyTreeRoot:
    return KeyTreeRoot(
        node_id=value["node_id"],
        global_key=_tonal_key(value["global_key"]),
        key_score=value["key_score"],
        best_rank=value["best_rank"],
        best_path_score=value["best_path_score"],
        score_delta_from_best=value["score_delta_from_best"],
        supporting_path_ranks=value["supporting_path_ranks"],
        top_k_support_count=value["top_k_support_count"],
        top_k_support_ratio=value["top_k_support_ratio"],
        is_top_k_consensus=value["is_top_k_consensus"],
        condition=_tree_condition(value["condition"]),
        children=[
            _interpretation_tree_node(child) for child in value["children"]
        ],
    )


def _interpretation_tree(value: Dict[str, Any]) -> InterpretationTree:
    return InterpretationTree(
        rule_set_version=value["rule_set_version"],
        progression_fingerprint=value["progression_fingerprint"],
        requested_k=value["requested_k"],
        returned_path_count=value["returned_path_count"],
        best_score=value["best_score"],
        condition=(
            _tree_condition(value["condition"])
            if value["condition"] is not None
            else None
        ),
        condition_applied=value["condition_applied"],
        condition_satisfied=value["condition_satisfied"],
        consensus_node_ids=value["consensus_node_ids"],
        roots=[_key_tree_root(root) for root in value["roots"]],
    )
