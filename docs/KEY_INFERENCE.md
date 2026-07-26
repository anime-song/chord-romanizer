# Global key / local key / 機能の統合推定

`analyze_keys_and_functions` は、global key を先に一意決定してから機能解析する
APIではない。12の主音と major/minor を組み合わせた24調について既存の意味候補
ラティスを生成し、次の合計点で `(global key, 機能パス)` をまとめて順位付けする。

```text
total_score = key_score + function_score + modulation_score + memory_score
```

- `key_score`: 50%のglobal/home-key priorと、50%の選択済みactive-key区間スコア。
  音階内の根音、コード品質、主和音の出現、開始・終了位置、終止、綴り、
  任意のkey hintを含む
- `function_score`: 既存ラティスの emission と、前後の候補に依存する transition
- `modulation_score`: key-state遷移、確認終止、pivot、持続、複雑さの比較点
- `memory_score`: 介在和音を越えたdominant解決と、限定深度のtonicization
  stackを使う完全パス再採点
- `local_key`: 選択された機能候補の `TonalPerspective`。局所調の根拠がない位置は
  global key

転調を含むpathでは、終盤の終止を最初からglobal keyの証拠だったかのように扱わない。
global priorは作品のhome候補を保持し、active-key区間スコアは選択された転調列に沿って
和音適合と終止を再計算する。`builtin.key.*` evidenceの説明には
`Global-key prior`または`Active-key path`を付け、両成分を監査できる。

点数は確率ではない。採用された規則と加点・減点はすべて `evidence` に残る。
したがって、UIは1位だけを「確定」と表示せず、通常は `k=3` または `k=5` の
候補と点差を表示する。

## Python API

```python
from chord_romanizer import Romanizer

analyzer = Romanizer.strict()

# global keyも含めて推定
paths = analyzer.analyze_keys_and_functions(
    ["Em7", "A7", "Dm7", "G7", "Cmaj7"],
    k=5,
)

best = paths[0]
print(best.global_key)                    # TonalKey("C", "major")
print(best.selections[0].local_key)       # TonalKey("D", "minor")
print(best.selections[0].scope)           # "tonicization"
print(best.selections[0].role)            # "predominant"

# 候補全体を残しつつ、A minorを少し優先
hinted = analyzer.analyze_keys_and_functions(
    ["Am7", "Fmaj7", "Cmaj7", "G"],
    global_key_hint="A",
    global_key_hint_mode="minor",
    k=5,
)

# global keyを固定し、機能とlocal keyだけを比較
fixed = analyzer.analyze_keys_and_functions(
    ["Am7", "D7", "Gmaj7"],
    global_key="G",
    global_mode="major",
    k=5,
)
```

`global_key` と `global_key_hint` は同時に指定できない。modeを省略した固定keyと
hintはmajorとして扱う。従来の `Romanizer.strict("C")` と
`analyze_top_k_interpretations` は互換性のため、引き続き「C majorが既知」のAPI
として動作する。

## Rust API

```rust
use chord_romanizer::{KeyAnalysisOptions, Romanizer};

let paths = Romanizer::new("C")?
    .analyze_keys_and_functions(
        &progression,
        KeyAnalysisOptions::default(),
        5,
    );
```

`GlobalKeyRequest::Hint(TonalKey)` と `GlobalKeyRequest::Fixed(TonalKey)` も利用できる。
`RomanizerOptions::default_mode` はkey候補を生成する前に設定されるため、minor keyの
主和音が「major keyから借用したminor tonic」と誤分類された後で並べ替えられる
ことはない。

## 現時点の境界

- 1つの返却パスは進行全体に1つのglobal keyを持つ。
- 適用和音は `scope=tonicization` としてlocal keyに保持する。
- 新調の終止、持続、ピボット／橋渡しが競争力を持つ場合は
  `scope=modulation` として独立区間へ昇格する。
- 同じ機能列のtonicization候補とmodulation候補をTop-kに併存させる。
- 複数のmodulation区間とglobal keyへの復帰は、選択済みactive keyを状態に持つ
  segmental k-best探索で連鎖する。
- 選択済みパスは最大2段の未解決dominant目標を保持し、2個までの介在和音を
  越えた解決を`harmonic_resolutions`として返す。
- predominant準備からdominant、通常／deceptive解決までを
  `cadential_spans`として返す。
- コード記号だけを使用し、音価、拍位置、メロディ、bassの実音、休符長は未使用。

将来MIDIを統合するときは、同じjoint stateへ継続時間、強拍、導音の実音、
フレーズ境界を加える。短い副次ドミナントを転調と誤認しないため、modulationの
昇格には「局所調の持続」と「局所終止」の両方を要求する。

転調区間、ピボットの二重度数、dominant bridgeの分類は
[`MODULATION.md`](MODULATION.md)を参照する。
長距離の機能目標と継続時間は
[`HARMONIC_MEMORY.md`](HARMONIC_MEMORY.md)を参照する。
