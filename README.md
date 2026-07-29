# Chord Romanizer

[![PyPI version](https://badge.fury.io/py/chord-romanizer.svg)](https://badge.fury.io/py/chord-romanizer)

コード進行をローマ数字と機能ラベルへ変換するPythonライブラリです。
調を指定した通常解析、表示・MIDIマーカー用ラベル、複数の機能解釈、調・転調推定を利用できます。
解析本体はRustで実装されています。

## インストール

```bash
python -m pip install chord-romanizer
```

Python 3.8以上に対応しています。新しく解析を始める場合は、修正版の規則を使う
`Romanizer.strict()`を利用してください。

## 3分で使う

### 1. 調を指定してローマ数字へ変換する

```python
from chord_romanizer import Romanizer

romanizer = Romanizer.strict(default_tonic="C")
results = romanizer.romanize_progression(["Dm7", "G7", "Cmaj7"])

print([result.roman for result in results])
# ['IIm7', 'V7', 'IM7']
```

1コードだけなら`romanize()`を使います。

```python
result = Romanizer.strict("G").romanize("B7/D#")

print(result.roman)
print(result.normalized_symbol)   # 表示向けスペリング
print(result.theoretical_symbol)  # 理論スペリング
```

### 2. 画面やMIDIマーカー向けのラベルを作る

`display_progression()`は、進行全体で選ばれたTop-1解釈を表示用にまとめます。
`combined_label`はそのまま画面、ログ、MIDIテキストマーカーへ渡せます。

```python
romanizer = Romanizer.strict("E")
display = romanizer.display_progression(
    ["Bm7", "Eaug/A#", "AM7", "G#aug/D", "C#m7"]
)

for item in display:
    print(item.combined_label)
```

```text
Bm7 [ii7/IV|PD]
Eaug/Bb [bV7(9,#11)|subV/IV]
AM7 [IVM7|I/IV]
G#aug/D [bVII7(9,#11)|subV/vi]
C#m7 [vi7|i/vi]
```

入力位置と対応させる場合は`event_index`を使います。

```python
markers = {
    item.event_index: item.combined_label
    for item in romanizer.display_progression(progression)
}
```

`AnalysisDisplay`の主なフィールドは次のとおりです。

| フィールド | 用途 | 例 |
| --- | --- | --- |
| `combined_label` | 完成済み表示 | `Eaug/Bb [bV7(9,#11)\|subV/IV]` |
| `symbol` | 表示向けコード名 | `Eaug/Bb` |
| `theoretical_symbol` | 理論上のスペリング | `Eaug/Bb` |
| `global_label` | 全体調から見た度数 | `bV7(9,#11)` |
| `local_label` | 一時的な調での度数 | `ii7/IV`, `I/IV` |
| `function_label` | 機能 | `subV/IV`, `SDm`, `D` |
| `role_label` | 大分類 | `T`, `PD`, `D`, `S`, `NF` |
| `analysis_label` | 角括弧内だけの表示 | `bV7(9,#11)\|subV/IV` |
| `event_index` | 元の進行での位置 | `1` |

### 3. 必要なAPIを選ぶ

| やりたいこと | API |
| --- | --- |
| 1コードを解析する | `romanize()` |
| コード列をローマ数字へ変換する | `romanize_progression()` |
| 詳細な解析結果を得る | `annotate_progression()` |
| 表示・MIDI用ラベルを得る | `display_progression()` |
| N.C.や境界を含めて入力位置を保つ | `annotate_events()` |
| 機能的に異なる候補を比較する | `analyze_top_k_interpretations()` |
| 調・転調・機能をまとめて推定する | `analyze_keys_and_functions()` |

## コード表記を表示用と保存用に分ける

`annotate_progression()`の結果では、コード表記を次のように使い分けます。

- `normalized_symbol`: 画面やMIDI表示向け
- `theoretical_symbol`: 理論表記の保存向け
- `symbol_fixed`: `normalized_symbol`と同じ値を返す互換フィールド
- `chord.original_symbol`: 入力された表記

`simplify_accidentals=True`では、複重変化記号を表示しやすい異名同音へ置き換えます。
理論表記は失われません。

```python
romanizer = Romanizer.strict("Db", simplify_accidentals=True)
item = romanizer.display_progression(["A"])[0]

print(item.symbol)              # A
print(item.theoretical_symbol)  # Bbb
print(item.global_label)        # bVI
```

理論表記を保存して画面だけ簡略化する場合は、同じ結果の
`theoretical_symbol`と`symbol`をそれぞれ保存・表示へ割り当てます。

ルートとslash bassは一組として綴りを決定します。たとえばG調の`F#/G#`は、
rootだけを異名同音にして`Gb/G#`とはせず、`Gb/Ab [bII9sus4|S]`と表示します。

## 詳細な解析結果を使う

`annotate_progression()`は、表示文字列へまとめる前の結果を返します。

```python
results = Romanizer.strict("C").annotate_progression(
    ["Em7-5", "Eb7", "Dm7"]
)

for result in results:
    print(result.normalized_symbol, result.roman)
    for classification in result.harmonic_classifications:
        print(
            classification.role,
            classification.dominant_relation,
            classification.perspective,
        )
```

代表的なフィールド:

- `roman`: 基本のローマ数字
- `alter`: 文脈から得た拡張度数表記
- `normalized_symbol`, `theoretical_symbol`: コードスペリング
- `harmonic_classifications`: 役割、dominant関係、借用元、局所調
- `functional_interpretations`: Blackadderを含むコード機能候補

## 複数の解釈を比較する

単一の表示結果ではなく、曖昧性を残して確認したい場合に使います。

```python
romanizer = Romanizer.strict("B")
paths = romanizer.analyze_top_k_interpretations(
    ["Daug/C", "B"],
    k=3,
)

for path in paths:
    print("score:", path.total_score)
    for selection in path.selections:
        print(selection.event_index, selection.label, selection.blackadder)
```

`paths[0]`が最上位です。`display_progression()`も同じTop-1経路を使うため、
画面の機能ラベルと解析で選ばれた候補が食い違いません。

通常は`analyze_top_k_interpretations()`を使用してください。
表記だけ異なる候補も含む低水準の経路が必要な場合に限り`analyze_top_k()`を使います。

## 調や転調も推定する

調が既知なら`global_key`を指定します。

```python
paths = Romanizer.strict().analyze_keys_and_functions(
    ["C", "Am7", "D7", "G", "C", "D7", "G"],
    global_key="C",
    global_mode="major",
    k=5,
)

best = paths[0]
for selection in best.selections:
    print(selection.event_index, selection.active_key, selection.scope)

for span in best.modulations:
    print(span.from_key, "->", span.to_key, span.mechanism)
```

調も不明な場合は`global_key`を省略します。

```python
paths = Romanizer.strict().analyze_keys_and_functions(
    ["Dm7", "G7", "Cmaj7"],
    k=3,
)
print(paths[0].global_key)
```

詳しい転調判定は[`docs/MODULATION.md`](docs/MODULATION.md)、
長い範囲にまたがるdominant解決は
[`docs/HARMONIC_MEMORY.md`](docs/HARMONIC_MEMORY.md)を参照してください。

## コードごとに調を指定する

`(コード, 調)`のタプルを渡します。

```python
results = Romanizer.strict("C").romanize_progression(
    [
        ("C", "C"),
        ("F", "C"),
        ("Dm7", "F"),
        ("G7", "F"),
    ]
)
```

調の指定が変わる位置は、StrictV1では既定で文脈の境界になります。

## N.C.とセクション境界を扱う

```python
from chord_romanizer import Boundary, Romanizer

progression = ["Dm7", "N.C.", Boundary("chorus"), "G7"]
romanizer = Romanizer.strict("C")

# コードだけを返す。N.C.と境界は省略する。
chords = romanizer.annotate_progression(progression)

# 入力と同じ長さで返す。N.C.と境界も確認できる。
events = romanizer.annotate_events(progression)
```

`N.C.`は短い休符として扱い、前後の和声文脈を接続します。
長い空白、セクション変更などで文脈を切る場合は`Boundary`を置きます。
`display_progression()`はN.C.と境界を省略しますが、各結果の`event_index`は
元の入力位置を保持します。

## 機能ラベルの読み方

| 表記 | 意味 |
| --- | --- |
| `T` | tonic |
| `PD` | predominant |
| `D` | dominant |
| `S` | subdominant |
| `NF` | non-functional |
| `CT` | 共通音を保持するchromatic neighbor |
| `passdim` | 半音進行をつなぐpassing diminished |
| `CS` | 同型コードを反復するconstant structure |
| `V/ii` | iiを一時主音とするdominant |
| `V+/IV` | IVへ向かうaugmented dominant |
| `subV/IV` | IVへ向かうtritone substitute |
| `I/IV`, `i/vi` | 一時的な調でのtonic |
| `SDm` | subdominant minor由来 |

`Bm7 [ii7/IV|PD]`のように`|`がある場合、左側が具体的な度数、
右側が機能または役割です。

`C#m7-5 → CM7`ではE、G、Bの3音を保持し、C#だけがCへ半音下行します。
この場合はpredominantではなく共通音装飾として`C#m7-5 [#im7-5|CT]`と表示します。
この判定はtargetがglobal tonicでなくても成立し、G majorの
`C#:m7-5/G → C#:m7-5 → C:M7`では最初の2和音を`|CT`と表示します。

`Bm7 → Bb:dim7 → Am7`ではroot lineのB–Bb–Aを半音下行でつなぐため、
`Bb:dim7 [biiidim7|passdim]`と表示します。

`Eb/F → Gb/Ab → A/B → C/D → Eb/F → G`では、最初の5和音が
同じ`9sus4`構造を保ち、functional bassを短3度周期で移動します。3和音以上
続いた場合は個別のS/Dよりconstant structureを優先し、`|CS`と表示します。
`F#/G#`や`Gb/G#`から`Gb/Ab`へ正規化されたmemberも同じpitch classとして
系列を維持します。単独の`C/D → G`は従来どおり`C/D [V9sus4|D]`です。

`C → Caug/F# → FM7`では、Caugを`V+/IV`、F# bassをFへ半音下行する
独立したapproach bassとして扱います。したがってsubVではなく、
`Caug/F# [Iaug/#IV|V+/IV]`を1-bestとして表示します。

## StrictV1と旧バージョン互換

新規コードでは次を使います。

```python
romanizer = Romanizer.strict(default_tonic="C")
```

`Romanizer(default_tonic="C")`は、Python 0.1.9の出力を再現する
`python019`プロファイルです。既存結果との互換性が必要な場合だけ利用してください。

## 開発用インストール

```bash
python -m pip install "maturin>=1.9.4,<2" pytest
python -m maturin develop --release
python -m pytest -q
```

Rust側だけを検証する場合:

```bash
cargo test --manifest-path chord-romanizer-rs/Cargo.toml
cargo test --manifest-path chord-romanizer-py/Cargo.toml
```

Rust APIの使用例は
[`chord-romanizer-rs/README.md`](chord-romanizer-rs/README.md)を参照してください。
解析器の設計や詳細な判定規則は[`docs/`](docs/)にあります。

## 動作環境

- CPython 3.8以上
- Rust 1.85以上（ソースからビルドする場合のみ）
- Python wheel: PyO3 / `abi3-py38`

## ライセンス

[MIT License](LICENSE)
