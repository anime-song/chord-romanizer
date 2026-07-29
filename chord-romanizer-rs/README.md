# chord-romanizer (Rust)

Python版`chord_romanizer` 0.1.9を移植し、曖昧な和声解釈を候補として保持できるようにした、外部依存なしのpure Rust libraryです。

`Romanizer::new`は修正版の`BehaviorProfile::StrictV1`を使います。Python 0.1.9の出力再現が必要な場合だけ`RomanizerOptions::python_019`を明示してください。

## 基本的な使用例

```rust
use chord_romanizer::{ProgressionItem, Romanizer, parse_chord};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let progression: Vec<_> = ["Dm7", "G7", "Cmaj7", "F/G"]
        .into_iter()
        .map(|symbol| Ok(ProgressionItem::new(parse_chord(symbol)?)))
        .collect::<Result<_, chord_romanizer::ParseError>>()?;

    let results = Romanizer::new("C")?.annotate_progression(&progression);
    for result in results {
        println!(
            "{} -> {} (normalized={}, candidates={})",
            result.chord.original_symbol,
            result.roman,
            result.normalized_symbol,
            result.functional_interpretations.len(),
        );
    }
    Ok(())
}
```

## 表示用API

`display_progression`は、Top-1の機能解釈と正規化済みコード綴りを
表示向けにまとめます。完成済みの`combined_label`に加え、
`global_label`、`local_label`、`function_label`、`role_label`を個別に返します。

```rust
let display = romanizer.display_progression(&progression);
for item in display {
    println!("{}", item.combined_label);
}
```

N.C.と境界は出力から省きますが、`event_index`は元の入力位置を保持します。

## N.C.と二つの出力API

Strict V1では`N.C.`を短い休符として扱い、既定では前後の文脈を接続します。

```rust
use chord_romanizer::{AnnotatedEvent, ProgressionItem, Romanizer, parse_chord};

let progression: Vec<_> = ["Dm7", "N.C.", "G7"]
    .into_iter()
    .map(|symbol| ProgressionItem::new(parse_chord(symbol).unwrap()))
    .collect();
let romanizer = Romanizer::new("C").unwrap();

// chordだけを返すcompact API
assert_eq!(romanizer.annotate_progression(&progression).len(), 2);

// 入力位置を保つaligned API
let events = romanizer.annotate_events(&progression);
assert_eq!(events.len(), 3);
assert!(matches!(events[1], AnnotatedEvent::NoChord { .. }));
```

長い空白やセクション境界では`ProgressionItem::boundary("long silence")`を挿入します。`N.C.`自体で常に区切る入力規約なら`RomanizerOptions::no_chord_policy`を`NoChordPolicy::Break`へ変更できます。

## コードごとのキー指定

```rust
use chord_romanizer::{ProgressionItem, Romanizer, SpelledNote, parse_chord};

let progression = vec![
    ProgressionItem::new(parse_chord("C").unwrap()),
    ProgressionItem::in_key(
        parse_chord("Dm7").unwrap(),
        SpelledNote::parse("F").unwrap(),
    ),
];
let results = Romanizer::new("C").unwrap().annotate_progression(&progression);
assert_eq!(results[0].roman, "I");
assert_eq!(results[1].roman, "VIm7");
```

Strict V1ではtonic変更が既定で文脈境界になります。意図的に跨ぐ場合は`KeyBoundaryPolicy::Continue`を指定します。

## 複数候補とk-best経路

```rust
use chord_romanizer::{ProgressionItem, Romanizer, parse_chord};

let progression: Vec<_> = ["Eaug/D", "G7", "C"]
    .into_iter()
    .map(|symbol| ProgressionItem::new(parse_chord(symbol).unwrap()))
    .collect();
let romanizer = Romanizer::new("C").unwrap();

let paths = romanizer.analyze_top_k(&progression, 3);
assert!(!paths.is_empty());
assert!(paths[0].total_score >= paths.last().unwrap().total_score);
```

`RomanizedChord::alter`は1-bestの簡便表示です。候補を失わずに扱う場合は`hybrid_candidates`、`functional_interpretations`、または`AnalysisLattice`を使ってください。各経路にはrule idと加点理由を持つ`ScoreEvidence`が残ります。

Blackadder候補は単一ラベルに潰さず、`BlackadderInterpretation`の`structure`、`function`、`origin`へ分離しています。裏コード、通常／secondary dominant、backdoor dominant、SDm、half-diminished、aug7転回、whole-tone、増六、分離型、偶成和音型を同時に保持できます。分離型や偶成和音型など文字列だけで確定できない候補には`unresolved_observations`が残ります。

将来MIDI等から声部・音価を取得した場合は、`BlackadderObservations`へ正規化して`ChordInterpreter::analyze_slash_candidates_with_context`へ渡せます。core解析器は生のMIDI形式に依存しません。

## Strict V1の主な修正

- `ChordQuality`をbase、seventh、extension、alteration、addition、omissionへ分解
- 構成音、転回形、綴りを単一の`ChordFormula`から生成
- `C7(b9,#11)`に存在しないnatural 9を補わない
- augmentedを`{0,4,8}`、diminished seventhを`{0,3,6,9}`として扱う
- 未知qualityのslash chordをmajor triadへfallbackせず`Indeterminate`にする
- 文字列置換ではなくASTから`normalized_symbol`を描画
- 同一pitch classのredundant slashを正規化出力から除去
- German notation由来の入口aliasは単独の`H = B`だけを許可し、`Hb`と`H#`は拒否
- typed alternate、全hybrid候補、候補ごとのrule idを結果に保持

`original_symbol`は常に入力を保持します。`theoretical_symbol`は理論綴り、`normalized_symbol`はaccidental簡略化後の正規化表示で、互換名`symbol_fixed`は`normalized_symbol`と同じ値です。

## Python 0.1.9互換profile

```rust
use chord_romanizer::{Romanizer, RomanizerOptions};

let options = RomanizerOptions::python_019("C").unwrap();
let romanizer = Romanizer::with_options(options).unwrap();
```

`tests/fixtures/python_golden.jsonl`はPython版から生成した正本です。57進行ケースで次の全フィールドを比較します。

- `roman`, `alternate_labels`
- `degree_root`, `degree_bass`, `roman_root_bass`
- `is_hybrid`, `alter`, `symbol_fixed`
- `is_ii_v_start`, `is_resolution_target`, `resolution_type`

golden dataを再生成する場合は、リポジトリルートで次を実行します。

```powershell
python tools\generate_python_golden.py
cd chord-romanizer-rs
cargo test --test python_compat
```

## 開発時の確認

```powershell
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

- edition: Rust 2024
- MSRV: Rust 1.85
- runtime dependencies: なし

文脈解析の拡張、N.C.境界、外部rule setの方針は[`../docs/CONTEXT_ANALYSIS.md`](../docs/CONTEXT_ANALYSIS.md)を参照してください。
