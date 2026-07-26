# 転調判定とピボットコード

## 判定の考え方

コードシンボルだけを見ると、短い `V7-I` は副属和音による一時的な tonicization
にも、短い転調にもなり得る。この実装は単独のクロマティックコードから転調を
確定せず、候補となる新調の終止から前後へ文脈を広げる。

```text
旧調の確立
    ↓
ピボット／橋渡し
    ↓
新調の predominant → V7 → I
    ↓
新調の和音が持続
```

最低限、新調の `V7-I/i` を確認材料にする。直前に `ii` または `IV` がある
完全な終止形、新調に属する後続和音、同じ新調での反復終止があるほど転調候補を
高くする。転調は追加の複雑さを持つため、裸の `V7-I` だけなら通常は
tonicization の候補が上に残る。

点数は確率ではない。`KeyedAnalysisPath.evidence` と
`ModulationSpan.evidence` に、比較点の加点・減点をすべて残す。

## 実装済みの接続型

| `mechanism` | 意味 | 例 |
| --- | --- | --- |
| `diatonic_pivot` | 同じrootとqualityを持ち、旧調・新調の両方でダイアトニックな共通和音 | C majorの`Am7`をviからG majorのiiへ読み替える |
| `chromatic_pivot` | 旧調の借用・副次・ナポリ・増六系候補を、新調の機能和音へ読み替える | C majorの借用`Fm7`をE♭ majorのiiへ読み替える |
| `dominant_bridge` | 旧文脈のdominantから新調のdominantへ接続する | C majorの`G7`からG majorの`D7-G`へ進む |
| `dominant_sequence` | 五度関係のdominant 7th列が新調の`V7-I`へ到達する | `A7-D7-G` |
| `direct_dominant` | 共通和音なしで新調の`V7-I`へ入る | `C-A7-D` |

`V7（旧調）→V7（新調）→I（新調）` は
`dominant_bridge` であり、共通和音ピボットとは呼ばない。ピボットには、同じ
和音が旧調と新調で別の度数・機能を同時に持つことが必要である。

クロマティック・ピボットの細分類は `PivotKind` に保持する。

- `secondary_common_chord`
- `borrowed_common_chord`
- `neapolitan_common_chord`
- `augmented_sixth_common_chord`

ただし、増六和音とdominant seventhの異名同音読み替えをコードシンボルだけで
完全に認定することはしない。現在は既存の増六分類と新調側の明示的な和音適合が
同時にある場合だけ候補になる。

## Top-kとの統合

同じ機能候補列から、少なくとも次の2種類を作る。

1. global keyを維持し、副属終止をtonicizationとして扱う候補
2. 終止・持続・ピボット根拠を加え、区間をmodulationへ昇格する候補

転調候補のイベントでは次を返す。

- `active_key`: その時点で確立している構造上の調
- `local_key`: さらに局所的なtonicizationがあればその調。なければ
  `active_key`
- `scope`: `global` / `tonicization` / `modulation`
- `is_pivot`: 旧調と新調の読み替え位置
- `is_modulation_confirmation`: 新調を確認するtonic到達

`KeyedAnalysisPath.modulations` は区間全体を返す。

```python
paths = analyzer.analyze_keys_and_functions(
    ["C", "Am7", "D7", "G", "C", "D7", "G"],
    global_key="C",
    global_mode="major",
    k=5,
)

for path in paths:
    for span in path.modulations:
        print(span.from_key, span.to_key, span.mechanism, span.pivot)
```

転調状態はcandidate IDにもversioned suffixとして含まれる。そのため
`analyze_interpretation_tree` は、機能候補が同じでもtonicizationとmodulationを
別の枝として表示でき、転調枝の`condition`を使って子孫を再計算できる。

## 複数回転調と原調復帰

転調区間はsegmental key-state k-best探索で連鎖する。各確認終止で、保持中の
各状態から次の2枝を作る。

1. 現在のactive keyを維持し、その終止をtonicizationとして残す
2. 終止のtonicを新しいactive keyとして選択する

重要なのは、2回目以降の遷移がglobal keyからではなく、直前に選ばれた
`active_key`から採点されることである。

```text
C major
  └─ D7-G       → G major
       └─ A7-D  → D major
            └─ G7-C → C major（原調復帰）
```

`ModulationSpan.from_key`と`to_key`がこの状態列を表す。次の転調が選ばれると、
前区間の`end_event_index`は次のpivot／preparation直前へ切り詰められる。
1つの和音が前調を確認すると同時に次調のpivotになる場合は、その共有イベントを
両区間に保持する。

明示boundaryや長いN.C.はpivot探索を分断するが、active keyを無条件にglobal keyへ
戻さない。境界後に同じ調が続く場合と、phrase冒頭でdirect modulationする場合の
両方を残す。

短い`V7-I-V7-I`を機械的な往復転調にしないため、前の調を確認してから2コード以内
に再転調する枝にはrapid-key-reversal penaltyを与える。これは枝を削除する規則
ではなく、tonicization候補を上位へ残すための比較点である。

推定global keyが一度もstable tonicとして現れないまま別調へ遷移する枝にも減点する。
これにより、たとえばF majorの`F-Bb-C7-F`を「D minorからF majorへ転調した」と
逆算する候補が、素直なF major解釈を押しのけにくくなる。

## 現在確定しないもの

次の型は理論上の候補として把握しているが、コード文字列だけでは必要な根拠が
欠けるため、この段階では確定ラベルを出さない。

- 1音だけを保持するcommon-tone modulation
- メロディと伴奏パターンの反復によるsequential modulation
- Ger+6とV7、fully diminished seventhなどの異名同音転調
- フレーズ境界、拍節上の重み、実音の持続を必要とするdirect modulation

これらは将来、MIDIから得る実音、bass、duration、meter、melody、
phrase boundaryを同じscore evidenceへ追加する。現在のAPI形状はその拡張で
変更しない。

探索は厳密な確率モデルではなく、通常は32状態以上を保持するsegmental k-best
beamである。各状態の採点根拠は従来どおり`ModulationSpan.evidence`へ残る。MIDI導入後は
duration、meter、melody、phrase boundaryをstate transitionの観測項として加える。

選択後の各区間は、実際に含むコードイベント数を`duration_chords`へ保存する。
2和音だけの短い区間はtonicization寄り、4和音以上の持続はmodulation寄りの
小さなsemi-Markov potentialとして採点する。長距離のdominant目標とは責務を分け、
後者は[`HARMONIC_MEMORY.md`](HARMONIC_MEMORY.md)で扱う。
