# 通常コードの文脈解釈

`StrictV1` は、非ダイアトニック・コードに一つの名称を固定せず、同じ表示コードに対する複数の意味を `HarmonicInterpretation` として保持する。

たとえば C major の `EbM7` は、少なくとも次の候補を持つ。

- 平行短調から借用した `bIIIM7`（tonic 系の modal interchange）
- I から短3度離れた chromatic mediant
- 前後に Eb のカデンツがあれば、一時的な Eb tonic

これらは排他的なコード名ではない。候補ごとに独立した score と `ScoreEvidence` を持ち、進行全体の k-best decoder が順位を決める。

## 現在の汎用規則

### Applied cadence

minor または half-diminished chord、dominant chord、解決先が `ii-V-I/i` の根音関係を作る場合、3コードを同じ `TonalPerspective` に置く。

```text
global C: F#m7  B7  Em7
global RN: #IVm7 VII7 IIIm7
local E:     ii   V     i
```

表示上のglobal Roman numeralは変更しない。各解釈の `local_degree` に `II`、`V`、`I`を保存する。

tritone substituteにも専用のrelated iiを持たせる。

```text
global C: Abm7  Db7  C
global RN: bVIm7 bII7 I
function: related ii - subV7 - I
```

この場合、`Abm7`をCの`II`へ読み替えない。global/local degreeは`bVI`のまま、
`tritone_substitute_related_two` familyによって`Db7`との関係を表す。

### Secondary-dominant deceptive resolution

secondary dominantが想定したlocal tonicそのものではなく、その調の`VI`または
`bVI`へ進む場合、通常の未解決dominantとして捨てず
`secondary_dominant_deceptive`候補を作る。

```text
global C: E7  FM7
global RN: III7 IVM7
local A minor: V7 bVI
```

dominant側とtonic-substitute側は同じ`TonalPerspective(local_tonic=A)`を持つ。
解決先には`resolution_type=deceptive`を付ける。通常のglobal `V-vi`と混同しない
よう、この規則はimplied local tonicがglobal tonicと異なるsecondary dominant
だけに適用する。

### Alternate-key sequence

dominant-tonic終止がなくても、短い隣接区間が別調から自然に説明できる場合は
`alternate_key_sequence`候補を生成する。現在の保守的なテンプレートは次の3種である。

- `IVmaj7 -> iiim7`
- `iim7 -> iiim7`
- `ivm7 -> V7`

たとえばCをglobal keyとする`BbM7 -> Am7`は、global表示
`bVIIM7 -> VIm7`を保ったまま、temporary Fの`IVM7 -> IIIm7`候補も持つ。
複数の隣接ペアが同じlocal tonicを選ぶと、Viterbi transitionに小さな
`continue_local_tonal_state`加点を行う。これはmodulation確定ではなく、
top-k内に一貫した別調視点を残すための状態継続priorである。

### Applied leading-tone

diminishedまたはhalf-diminished chordが半音上のコードへ解決するとき、root-position dominantとは別の `leading_tone` 関係として候補化する。

```text
global C: F#m7b5 -> G
local G:    viiø  -> I
```

完全減七は `{0, 3, 6, 9}` の対称音集合なので、written rootだけでは解決関係を
判定しない。たとえばC majorの `bIIIdim7 -> V` は、表記上のrootがE-flatでも
F-sharp diminishedと同じ音集合を持つため、局所Gの `vii°7 -> I` 候補になる。
dim triadとhalf-diminished seventhはこの回転対称性を持たないため、従来どおり
written rootが半音上へ進む場合だけをleading-tone候補にする。

解決先にはmajor/minorの安定したtonic qualityを要求する。したがって
`G7 -> Cdim`や`Bdim7 -> Cdim`は、root motionだけが一致しても`Cdim`を
tonicized targetにしない。互換プロファイル`Python019`の旧判定は変更しない。

### Diminished-seventh alternatives

完全減七には一つの機能を固定せず、前後関係に応じて次の候補を別々に生成する。

- `rootless_dominant_ninth`: `V7(b9)`のroot省略形。`bIIIdim7 -> V`では
  `vii°7/V`と`II7(b9, omit root) -> V`の両方を保持する。
- `passing_diminished`: `IIIm7 -> bIIIdim7 -> IIm7`のような半音下行、
  または`I -> Idim7 -> IIm7`で想定できる内声の半音進行。
- `common_tone_diminished`: `I -> Idim7 -> I`のように共通音を残して
  元の和音へ戻る刺繍的な進行。同じ候補に、ジャズ／ポピュラー理論で検索しやすい
  `auxiliary_diminished` familyも付ける。
- `tonic_substitute`: global tonicを音集合に含み、`IIm`へ進む減七の弱い
  tonic代理候補。

`I -> Idim7 -> IIm7`では、rootless double-dominant、内声のpassing
diminished、tonic substituteが同時に成立し得る。声部が表示されないコード
シンボルだけではpassingを確定できないため、その候補には
`split_voice_leading`も付け、実根音が半音進行する場合より低く採点する。
将来MIDI/voicingを入力したときに候補を作り直すのではなく、既存候補へ観測証拠を
加えて順位を更新できる設計とする。

### Modal interchange

major tonicを基準として、次の代表的な借用を候補化する。

- parallel minor: `Im/Im7`, `IIm7b5`, `bIIIM/bIIIM7`, `IVm/IVm7`, `Vm/Vm7`, `bVIM/bVIM7`, `bVII7`
- Phrygian: `bIIM/bIIM7`
- Mixolydian: `bVIIM/bVIIM7`
- Lydian: `#IVm7b5`

借用元は `HarmonicSource`、進行内の役割は `HarmonicRole` に分ける。たとえば `bVIM7` は `source=parallel_minor` と `role=subdominant` を同時に持てる。

`bVII7`は、modal-interchange候補へSDm属性をまとめて付けず、次を独立候補にする。

- modal/parallel-minor colour
- `role=subdominant`, `source=subdominant_minor`のSDm候補
- tonicへ進む場合のbackdoor dominant候補

同じ表示でもSDmとbackdoorを別のtop-k状態にすることで、後続のMIDIや
voice-leading証拠を別々に加点できる。

### Neapolitan

`bIIM/bIIM7` がglobal Vを準備するとき、Phrygianのmodal-interchange候補とは別にNeapolitan predominant候補を作る。直接Iへ進む場合も弱いNeapolitan/Phrygian cadence候補を残す。

`bII7`はこの規則へ入れない。minor seventhを持つdominant-quality chordは、通常のtritone-substitute判定を優先する。

### Degree-specific ranking

汎用`chromatic_approach`と、より具体的なmodal/function候補が同時に成立するとき、
次の遷移証拠を使う。

- `bVI -> V`: `subdominant_minor`候補をgenericな半音接近より上げる。
- plain `bII -> I`: Neapolitan候補を1位、Phrygian候補を次位にし、
  genericな同型半音接近も下位候補として残す。
- `bII7 -> I`: 上記のplain-major規則ではなく、tritone substituteを優先する。

### Chromatic mediant

majorまたはmajor-seventh chordがIから長短3度離れ、特にIと直接隣接するとき候補化する。

```text
I <-> bIIIM7
I <-> IIIM7
I <-> bVIM7
I <-> VIM7
```

`bIIIM7`や`bVIM7`ではparallel-minor候補と共存する。

### Chromatic approach

非ダイアトニックなコードが、同じ構造の次コードまたはglobal tonicへ半音で接近するとき、機能和声とは別の線的候補を作る。

```text
F#m7 -> Fm7 -> Em7
DbM7 -> CM7
```

これは長いconstant-structure区間の確定ではなく、隣接する2イベントから得られる局所的な根音／voice-leading候補である。

### Half-diminished common-tone neighbor

half-diminished chordのrootが半音下行してmajor-seventh chordへ進むと、3音を保持する
`common_tone_neighbor`候補を作る。targetはglobal tonicに限定しない。さらに同根・同品質の
反復または転回を見通し、区間全体を同じ装飾和音として扱う。

```text
global G: C#m7b5/G -> C#m7b5 -> Cmaj7
function:      CT          CT        IVmaj7
```

### Suspended dominantとvoice-leading品質ゲート

`Dm7/G`のようにfunctional bassから見て`{2,5,10}`を持ち3度を欠くslash chordは、
独立した`suspended_dominant`候補として`V9sus4`を生成する。後続が同じrootの
unsuspended dominantなら`suspension_to_dominant`遷移証拠を加える。
この候補へ通常コード側のmodal-interchange属性をコピーしない。

同じ`9sus4`／`7sus4(b9)` hybridが3和音以上続き、functional rootが同方向の
短3度周期を作る場合は、進行全体を`constant_structure`として扱う。各コードの
局所的なdominant／modal候補は残すが、1-bestではnon-functionalな`CS`を優先する。
系列検出後の`F#/G# -> Gb/Ab`のような正規化では、綴りではなくpitch classで
member identityを維持する。

```text
Eb/F -> Gb/Ab -> A/B -> C/D -> Eb/F -> G
```

augmented upper structureが直前から保持される例では、bass motionだけから得た
dominant／tritone-substituteを確定しない。

```text
Caug -> Caug/F# -> FM7
```

この場合、split-voice-leading候補を上げ、競合する機能候補へ
`voice_leading_required`を付ける。機能候補は削除せず、MIDI/voicing観測がない
段階の過剰なtop-1断定だけを抑える。

## k-bestでの扱い

各コード層には次のような状態が入る。

```text
PrimaryDegree             表示上のglobal degreeだけを持つ中立候補
ContextualHarmony         通常コードの文脈解釈
FunctionalHybrid          slash / Blackadderの構造解釈
```

enharmonic spellingやslash bassを省略した表示は意味候補ではないため、top-kの枠を消費しない。

隣接候補が同じtemporary keyを選んだ場合は、次の組合せにtransition evidenceを加える。

- related `ii -> V`
- `V/subV/leading-tone -> local I`
- `Neapolitan -> V`
- secondary `V -> VI/bVI`
- alternate-key pairと、同じlocal tonal stateの継続
- constant-structure member同士の系列継続
- suspended dominantから同rootのunsuspended dominant

したがって `k=5` は内部候補を256個返す指定ではなく、異なる意味を持つ完成経路の上位5件を返す指定である。

## 現在の制限

- 長い区間からkeyそのものを自由探索するモデルはまだない。現在は保守的な
  pair templateからlocal stateを生成し、Viterbiがその継続を選ぶ。
- 拍節、duration、melody、bass line、voicingはまだscoreに入らない。
- modal tonicizationと本格的なmodulationの区別は、現在は `TonalScope::Tonicization` に留める。

将来MIDI等の観測が加わった場合も、候補生成とk-best選択を分離したまま、transitionまたはspan factorへ証拠を追加する。
