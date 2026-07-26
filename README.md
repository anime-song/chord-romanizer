# Chord Romanizer

[![PyPI version](https://badge.fury.io/py/chord-romanizer.svg)](https://badge.fury.io/py/chord-romanizer)

コード進行をローマ数字へ変換し、文脈を考慮した複数の機能解釈を返すライブラリです。
解析エンジンはRustで実装し、Pythonパッケージは高水準APIだけを公開する薄いラッパーになっています。

## 動作環境

- CPython 3.8以上
- Rust 1.85以上（ソースからビルドする場合のみ）
- ビルド方式: PyO3 + maturin
- Python ABI: `abi3-py38`

`abi3-py38` wheelは、同じOS・CPUアーキテクチャであればCPython 3.8以降の複数バージョンから利用できます。OSとCPUアーキテクチャをまたいで同じwheelを使うことはできません。

## インストール

公開済みwheelを利用する場合:

```bash
python -m pip install chord-romanizer
```

リポジトリから開発用にインストールする場合:

```bash
python -m pip install "maturin>=1.9.4,<2" pytest
python -m maturin develop --release
python -m pytest -q
```

## 基本的な使い方

既存Python版との互換性を保つため、通常のコンストラクタは`python019`プロファイルを使用します。

```python
from chord_romanizer import Romanizer

romanizer = Romanizer(default_tonic="C")
results = romanizer.romanize_progression(["Dm7", "G7", "Cmaj7"])

print([result.roman for result in results])
# ['IIm7', 'V7', 'IM7']
```

コードごとに調を指定することもできます。

```python
results = romanizer.romanize_progression(
    [
        ("C", "C"),
        ("F", "C"),
        ("Dm7", "F"),
        ("G7", "F"),
    ]
)
```

## StrictV1とk-best解析

修正済みの規則、候補を保持する解析、Blackadderコード解釈を利用する新規コードでは`Romanizer.strict()`を推奨します。

```python
from chord_romanizer import Romanizer

romanizer = Romanizer.strict(default_tonic="B")
paths = romanizer.analyze_top_k_interpretations(["Daug/C", "B"], k=3)

for path in paths:
    print(path.total_score)
    for selection in path.selections:
        print(selection.label, selection.blackadder)
```

`analyze_top_k_interpretations` は和声的に異なる候補だけを返す高水準APIです。
異名同音の度数表記、増三和音の対称な回転表記、slash bassを省いただけの
表示は候補数を消費しません。進行格子の低水準パスを直接取得する場合は
`analyze_top_k`を使用します。

Blackadderの増三和音部分は、入力表記を`written_upper_root`へ保持したまま、
bassのtritone上を`canonical_upper_root`として一意に表示します。たとえば
`G#aug/F#`、`Eaug/F#`、`Caug/F#`はいずれもcanonical shapeが`Caug/F#`です。
意味候補を増やさず、元の綴りは解釈の証拠として利用できます。

`analyze_top_k`は単一の正解に固定せず、進行全体として自然な解釈をスコア順に返します。各`AnalysisPath`には次が含まれます。

- `selections`: 各イベントで選んだ候補
- `total_score`: パス全体のスコア
- `evidence`: 加点・減点した規則と説明
- `blackadder`: 裏コード型、サブドミナント型などの構造・機能解釈

将来MIDIから得る調性・ボイシング・声部進行などをスコアリングへ加えられるよう、候補生成と経路選択は分離しています。

完全減七では異名同音の回転表記を別候補として水増しせず、和声的に異なる意味を
候補化します。たとえばC majorの`I -> Idim7 -> IIm7`は、
`rootless_dominant_ninth`、`passing_diminished`、`tonic_substitute`を
小さなtop-kで比較できます。`bIIIdim7 -> V`もwritten rootだけで除外せず、
対称音集合から`vii°7/V`とrootless `II7(b9)`の両方を検討します。
`I -> Idim7 -> I`には`common_tone_diminished`と
`auxiliary_diminished`を付けます。解決先にはmajor/minor qualityを要求するため、
`G7 -> Cdim`の`Cdim`をtonic targetとして扱いません。

裏コードでは`bVIm7 -> bII7 -> I`をrelated ii–subV–Iとして候補化します。
また、`bVII7`のmodal、subdominant-minor、backdoor候補を分離し、
`bVI -> V`ではSDm候補、plain `bII -> I`ではNeapolitan/Phrygian候補が
genericな半音接近より上位になるよう候補固有の遷移証拠を使用します。

### 機能・借用元・局所調の分類

`harmonic_classifications`では、和声的役割、dominantの解決関係、借用元、
全体調と局所調を別々の軸として返します。

```python
results = Romanizer.strict("C").annotate_progression(["Em7-5", "Eb7", "Dm7"])

substitute = next(
    item
    for item in results[1].harmonic_classifications
    if item.dominant_relation == "tritone_substitute"
)

print(substitute.role)  # dominant
print(substitute.perspective.global_tonic)  # C
print(substitute.perspective.local_tonic)  # D
print(substitute.perspective.local_tonic_degree)  # II
print(substitute.perspective.scope)  # tonicization
```

したがって`IIIm7-5–VI7–IIm7`は、全体調の表記を失わずに、IIを一時主音とする
`iiø–V–i`として分類できます。通常のV、裏コード、バックドアは同じ
`dominant_relation`軸を使用し、SDmなどの借用元とは同時に保持できます。

### Global key・転調・ピボット

高水準APIはglobal keyと機能候補を同時に順位付けし、確認終止のある別調区間を
modulation候補として返します。短い副属終止を転調へ強制せず、tonicizationの
読みもTop-kに残します。

```python
paths = Romanizer.strict().analyze_keys_and_functions(
    ["C", "Am7", "D7", "G", "C", "D7", "G"],
    global_key="C",
    global_mode="major",
    k=5,
)

for span in paths[0].modulations:
    print(span.from_key, span.to_key, span.mechanism, span.pivot)
```

`diatonic_pivot`、`chromatic_pivot`、`dominant_bridge`、
`dominant_sequence`、`direct_dominant`を区別します。各イベントの
`active_key`、`scope`、`is_pivot`、`is_modulation_confirmation`は
解釈ツリーにも渡るため、UIで転調枝を選択して後続を再計算できます。
複数回の転調と原調復帰も`from_key → to_key`の区間列として返します。

### 長期和声記憶

高水準key/function APIは、選択済みのactive keyに加えて最大2段の未解決
dominant目標を保持します。たとえば`D7 → Am7 → G`では、介在する`Am7`を
越えてD7のG目標を追跡します。

```python
path = Romanizer.strict().analyze_keys_and_functions(
    ["C", "D7", "Am7", "G", "C"],
    global_key="C",
    global_mode="major",
    k=5,
)[0]

print(path.selections[1].pending_resolutions)
print(path.harmonic_resolutions)
print(path.memory_score)
```

各ツリーノードにも`key_region_age_chords`、`pending_resolutions`、
`resolved_resolution_sources`が渡るため、UIは「どの期待がどこで解決したか」を
線やbadgeで表示できます。通常の隣接V–Iは既存transitionと二重加点しません。

`pending_predominant`と`cadential_spans`は、介在和音を含む
`Predominant → Dominant → Resolution`を3点の区間として返します。
secondary deceptive候補は`deceptive_arrival`として目標を閉じるため、
正しい代理解決が未解決ペナルティを受けることはありません。
詳細は[`docs/HARMONIC_MEMORY.md`](docs/HARMONIC_MEMORY.md)を参照してください。
詳しい判定基準と現時点の限界は
[`docs/MODULATION.md`](docs/MODULATION.md)を参照してください。

## N.C.と文脈境界

用途に合わせて2種類のAPIを用意しています。

```python
from chord_romanizer import Boundary, Romanizer

progression = ["Dm7", "N.C.", Boundary("long silence"), "G7"]
romanizer = Romanizer.strict("C")

# コード結果だけが必要な従来型API。N.C.と境界は出力から除外される。
chords_only = romanizer.annotate_progression(progression)

# 入力位置を保つAPI。N.C.と境界もイベントとして返る。
aligned = romanizer.annotate_events(progression)
```

`N.C.`は休符表現として扱い、それだけでは前後の文脈を切りません。長い空白やセクション変更など、明示的に文脈を切る必要がある箇所へ`Boundary`を置きます。

## Python APIの境界

Pythonから公開するのは`Romanizer`を中心とする高水準APIです。Rustの内部構造を大量の`PyClass`として公開せず、ネイティブ拡張との境界は内部JSONに限定しています。このため、Rust側の候補表現やスコアリング実装を変更してもPython公開APIへの影響を抑えられます。

互換性のため、`ChordParser`、`ParsedChord`、`ChordInterpreter`も引き続きimportできます。ただし`Romanizer`による調性・機能・進行の解析判断はRust側で行われます。

ネイティブバックエンドは次のように確認できます。

```python
print(Romanizer().native_backend)
# {'version': '0.1.10', 'abi': 'abi3-py38'}
```

## wheelのCIビルド

`.github/workflows/wheels.yml`はpull request、`main`/`master`へのpush、`v*`タグ、手動実行で次をビルドします。

| OS | アーキテクチャ | 成果物 |
| --- | --- | --- |
| Linux | x86_64 | manylinux2014 `cp38-abi3` wheel + sdist |
| Windows | x86_64 | `cp38-abi3` wheel |
| macOS | Intel x86_64 | `cp38-abi3` wheel |
| macOS | Apple Silicon arm64 | `cp38-abi3` wheel |

各jobはwheelをインストールし、ソースツリー外へコピーしたテストを実行します。これにより、ローカルのPythonファイルではなく、実際にwheelへ収録されたネイティブ拡張が読み込まれることを確認します。workflowは成果物をGitHub Actions artifactへ保存しますが、PyPIへの公開は行いません。

## ディレクトリ構成

```text
chord_romanizer/       Python公開APIと互換データ型
chord-romanizer-py/    PyO3による非公開ネイティブbinding
chord-romanizer-rs/    Rust解析エンジン
docs/                   設計文書と調査資料
tests/                  Python APIテスト
.github/workflows/      wheelビルドCI
```

Rust側だけを検証する場合:

```bash
cargo test --manifest-path chord-romanizer-rs/Cargo.toml
cargo test --manifest-path chord-romanizer-py/Cargo.toml
```

## ライセンス

[MIT License](LICENSE)
