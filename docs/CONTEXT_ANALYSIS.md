# 文脈解析・複数候補設計

## 1. 目的

この文書は`BehaviorProfile::StrictV1`の文脈解析仕様と、将来のルール蓄積・k-best解析の拡張境界を定義する。

目標は「常に一つの正解を返す」ことではない。各コードで成立し得る解釈を候補として保持し、進行全体で自然な候補列を順位付けし、上位複数経路とその根拠を返すことである。

現在実装済みなのは次の範囲である。

- slash/hybrid chordの複数候補生成
- II–V、完全5度解決、半音解決による文脈score
- 候補layerとtransitionから成る`AnalysisLattice`
- 各終端状態に上位k経路を保持するk-best dynamic programming
- rule id、加点、説明を持つ`ScoreEvidence`
- no-chord、明示boundary、key境界によるsegment制御
- 通常／裏コード／バックドアのdominant relation分類
- 短いapplied cadenceからの局所主音候補（`iiø–V–i`、`iiø–subV–i`）
- major/minorのglobal key推定、segmental転調、原調復帰
- 限定深度の未解決dominant目標とwhole-path reranker

教会旋法等を含む網羅的なmode推定、実時間・拍節の観測、外部ルールファイルの
loaderは将来実装である。

## 2. イベントと文脈境界

### 2.1 二つの出力API

同じ入力に対して用途別に二つのAPIを提供する。

```rust
let compact: Vec<RomanizedChord> = romanizer.annotate_progression(&items);
let aligned: Vec<AnnotatedEvent> = romanizer.annotate_events(&items);
```

- `annotate_progression`: chordだけを返す。旧APIとの互換や単純な変換向け
- `annotate_events`: chord、`NoChord`、`Boundary`を入力順のまま返す。時間列、UI、文脈解析向け

### 2.2 N.C.は既定でtransparent

`N.C.`は休符として使われることがあり、存在だけで和声文脈が終わったとは限らない。そのためStrict V1の既定値は`NoChordPolicy::Transparent`とする。

```text
Dm7, N.C., G7
 ^             II–Vの接続を許す
```

長い空白、セクション境界、解析を明示的に切りたい位置には`ProgressionItem::boundary("long silence")`を入れる。入力元が「N.C.は常に境界」という契約を持つ場合だけ`NoChordPolicy::Break`を指定する。

将来durationを扱う場合も、`N.C.`というラベルだけから長短を推測しない。duration、拍位置、section markerなどの観測情報をイベントmetadataとして受け、呼び出し側またはsegmenterが`Boundary`へ変換する。

### 2.3 key境界

コードごとのtonicが変わった位置では、Strict V1の既定値`KeyBoundaryPolicy::Break`により文脈を区切る。既知の転調を跨いで連続解析する用途では`Continue`を明示する。

将来、key自体を推定候補にする場合は、入力tonicを絶対的な境界ではなく「固定状態」または「強い観測」として扱えるようにする。

## 3. qualityと構成音

qualityはraw文字列だけで判定せず、次の構造へ分解する。

```text
ChordQuality
├── class             major/minor/diminished/augmented/suspended/...
├── seventh           major/minor/diminished/none
├── modifiers[]       degree + alteration + implied/added/altered
├── omissions[]
├── unknown_tokens[]
└── raw               入力表示の保存用
```

構成音、転回形、綴り、dominant/minor/diminished判定は単一の`ChordFormula`から導く。未知tokenを含みformulaを確定できないslash chordは、major triadへfallbackせず`SlashClassification::Indeterminate`とする。

`extension`は単なる「suffixの残り」ではなく、暗黙に積まれる上部構成音を表す。したがって次を区別する。

| 入力 | seventh | ninth |
| --- | --- | --- |
| `Cadd9` | なし | natural 9を追加 |
| `C9` | minor 7th | natural 9を暗黙に含む |
| `C7(b9,#11)` | minor 7th | altered b9のみ。natural 9は含まない |

`C7(b9,#11)`のpitch classは`{0,1,4,6,7,10}`、音名なら`C E G Bb Db F#`である。

## 4. 候補生成から経路選択まで

```mermaid
flowchart LR
    A[Parsed event sequence] --> B[Local candidate generation]
    B --> C[CandidateLayer per chord]
    C --> D[Pairwise transition evidence]
    D --> E[AnalysisLattice]
    E --> F[k-best decoder]
    F --> G[Ranked AnalysisPath list]
```

### 4.1 local candidate

一つのコードから少なくとも次の候補を生成できる。

- primary degree
- inversion
- functional hybrid interpretation
- augmented-over-bassの複数機能候補

異名同音のdegreeと増三和音の対称な回転はnotation metadataであり、harmonic candidateではない。
slashを省いたroot-only表示は`Python019`互換プロファイルだけに残す。
したがってViterbi layerにもk-best pathにも追加しない。明示された非冗長なslash bassは
表示から落とさない。

たとえば`Eaug/D`は形だけではBlackadder系dominantと`Dm7-5(9)`系の両方が成立し得る。`analyze_slash_candidates`は両方を保持し、`analyze_slash_chord`と`RomanizedChord::alter`は互換用の1-best convenience viewとする。全候補は`hybrid_candidates`と`functional_interpretations`から取得する。

### 4.2 lattice

`AnalysisLattice`はコードごとの`CandidateLayer`と、隣接layer間の`CandidateTransition`を持つ。各候補にはemission score、各edgeにはtransition scoreを置く。

```text
total(path) = Σ emission(candidate_t)
            + Σ transition(candidate_t-1, candidate_t)
```

scoreは確率と断定せず、現段階では比較用の重みである。将来統計モデルと統合する場合はlog probabilityへ揃える。

各加減点は`ScoreEvidence`として分解し、少なくとも次を返す。

- 安定した`rule_id`
- scoreへの寄与量
- 人間向け説明

これにより「なぜ候補Aが候補Bより上か」を合計点だけでなく根拠単位で確認できる。

### 4.3 k-best Viterbi

`decode_top_k(k)`は各layerの各終端候補について上位k部分経路を保持し、最後に全終端から上位kを選ぶ。`k=1`は通常のViterbi型1-best、`k>1`は複数の自然な解釈列を返す。

高水準APIの`analyze_top_k_interpretations(k)`はnotation metadataを候補数へ含めず、
和声状態が異なる上位解釈だけを返す。低水準の`analyze_top_k(k)`は格子のpath APIとして残す。

候補ごとの遷移scoreが異なると、途中で低い経路が後段の強い解決によって逆転し得る。そのためlayer全体のglobal beamだけではなく、状態ごとにk経路を保持する。

現在のbuilt-in transition evidenceはII–V、裏コード用related ii、完全5度解決、半音解決に加え、Blackadderの裏コード、通常／secondary dominant、backdoor dominant、half-diminished→dominant、SDm→tonic、`bVI→V`、`bII→I`を持つ。解決先はmajor/minorの安定したqualityに限定し、diminished等をroot motionだけでtonicized targetにしない。完全減七では、written rootだけでなく対称音集合全体からleading-tone解決を判定し、rootless V7(b9)、passing diminished、common-tone/auxiliary diminished、tonic substituteを競合候補として保持する。voice leadingに依存する分離型、偶成和音型、増六解決、および内声型passing diminishedの確定は、MIDI等から観測情報を受け取った段階で加点する。

### 4.4 Blackadderのfactorized state

Blackadderは同じ`{0,2,6,10}`音集合に複数の説明が成立するため、単一の分類enumへ押し込まない。

```text
BlackadderInterpretation
├── structure   dominant ninth / half-diminished / aug7 / whole-tone / aug6 / ...
├── function    dominant / tritone substitute / backdoor / SDm / predominant / ...
├── origin      upper structure / split voice leading / incidental / chord-scale
├── classification
│   ├── role / dominant_relation
│   ├── sources[] / families[]
│   └── global/local tonal perspective
├── effective_root / target_root / scale
└── unresolved_observations[]
```

同じ表示ラベルでも軸が異なれば別のViterbi状態として保持する。たとえば`bVII@ → I`はbackdoor dominantとSDmの両方を持ち、候補固有のtransition scoreで順位を付ける。

### 4.5 共通分類と局所調視点

Blackadder固有の`function`は互換用に残すが、新規処理では次の共通軸を使う。

```text
HarmonicClassification
├── role                 tonic / predominant / dominant / subdominant / non-functional
├── dominant_relation    fifth-related / tritone-substitute / backdoor / leading-tone
├── sources[]            SDm / Lydian dominant / Locrian natural 2 / whole tone
├── families[]           applied cadence / rootless dominant ninth / passing diminished / ...
└── perspective
    ├── global_tonic
    ├── local_tonic / local_tonic_degree
    ├── scope             global / tonicization
    └── mode              major / minor / unknown
```

軸は排他的ではない。たとえばSDm由来のbackdoor候補は、`role=dominant`、
`dominant_relation=backdoor`、`source=subdominant_minor`を同時に持てる。

C調の`Em7-5–A7–Dm7`は全体のローマ数字`IIIm7-5–VI7–IIm7`を保存したまま、
局所D minorから`iiø–V–i`として分類する。`Em7-5–Eb7–Dm7`なら同じD minorを
局所主音とする`iiø–subV–i`であり、`Eb7`の`dominant_relation`は
`tritone_substitute`になる。短い進行だけでは転調と一時主音化を区別し切れないため、
全体調と異なる中心は現段階では`scope=tonicization`として保持する。

### 4.6 長期和声記憶

1次遷移のfunction latticeを巨大な2次・3次Markov状態へ置き換えず、復号後の
構造化状態に`active_key`の継続時間と未解決dominant目標を保持する。
`D7 → Am7 → G`のように装飾的な和音が1個入ってもD7のG目標を維持し、
`D7 → A7 → D7 → G7 → C`では最大2段の目標をstackとして扱う。
さらにpredominant準備を同じlocal keyのdominantへ接続し、完全な終止区間を
`cadential_spans`へ保存する。選択済みsecondary deceptive候補も正式な
`deceptive_arrival`として目標を閉じる。

各イベントのsnapshotと完全パスの解決区間は公開APIと解釈ツリーにも渡る。
通常の隣接V–Iは既存transitionと二重加点せず、遅延解決と入れ子の整合性だけを
whole-path rerankerで比較する。詳細は
[`HARMONIC_MEMORY.md`](HARMONIC_MEMORY.md)を参照する。

## 5. 将来の状態設計

完全な文脈解析では、表示ラベルだけをViterbi状態にしない。少なくとも次を分離したlatent stateを想定する。

```text
HarmonicState
├── global_key / mode
├── local_tonicization
├── active_key_age
├── pending_dominant_targets (bounded stack)
├── scale_degree
├── chord_quality / tensions
├── inversion_or_hybrid_kind
├── functional_role
└── effective_root

NotationMetadata
├── written_symbol / written_upper_root
├── canonical_upper_root
├── enharmonic_alternates
└── display_alternates
```

分離する理由は、同じ表記が異なる機能を持ち、同じ機能が異なる表記を持つためである。たとえば異名同音候補の選択とsecondary dominant判定を一つの文字列labelへ潰すと、調推定とvoice leadingの根拠を別々に採点できない。

追加予定のscore群は次のとおりである。

- emission: 入力シンボルとの一致、構成音、bass、未知token penalty
- tonal: key/mode内の適合、borrowed chord、secondary function
- transition: root motion、common tone、voice leading、tendency tone resolution
- phrase: cadence、sequence、turnaround、section境界
- temporal: duration、強拍、反復、N.C.の長さ
- prior: 楽曲ジャンル、時代、利用者が指定した分析規約

巨大な直積状態をそのまま全列挙せず、候補生成で局所的に絞り、必要に応じてbeam searchまたはfactorized modelを導入する。候補を早期に一つへ確定しないことは維持する。

## 6. 正規化ルールの蓄積

core crateは解析中にWebへアクセスしない。収集した解釈は和声条件へ正規化し、解析器は固定されたversioned rule setだけを読み込む。runtime ruleと解析結果には出典、取得日、ライセンスを保存しない。

推奨するrule recordは次のとおりである。

```text
RuleRecord
├── id                  安定ID
├── version             rule自身の版
├── rule_set_version    配布セットの版
├── explanation         人間向けの短い説明
├── applicability       chord/key/mode/genre/position等の条件
├── features            emission/transitionへ変換する条件
├── weight
├── confidence
├── conflicts_with[]
└── review_status
```

収集した記述は「この条件でこの解釈を支持する証拠」へ変換する。同じ進行に異なる説明があれば上書きせず、別ruleとして共存させる。適用条件が定義できないruleは既定rule setへ入れない。

推奨フローは次のとおりである。

1. 和声上の主張を小さなruleへ正規化する
2. 適用条件と反例をtest fixtureにする
3. 人手review後にversioned rule setへ昇格する
4. golden corpusで順位変動を比較する
5. model/rule set versionを解析結果へ保存する

## 7. 公開結果の考え方

利用者向けには三段階を提供する。

- `RomanizedChord`: 現在の1-bestと全local候補。既存用途向け
- `AnalysisLattice`: 候補とedgeを調査・可視化したい用途向け
- `Vec<AnalysisPath>`: `decode_top_k(k)`で得る進行全体の上位解釈向け
- `analyze_top_k_interpretations(k)`: notation上の重複を除いた利用者向けTop-k

候補のscoreが近い場合は無理に「確定」と表示せず、top-1/top-2の差、適用rule、未確定要因をUI側で示せるようにする。将来は正規化済みconfidenceを追加しても、scoreとconfidenceを同一視しない。

## 8. 残る実装課題

次は意図的に未実装であり、Strict V1の既知の境界である。

- duration、拍位置、section情報を持つ入力event
- major/minor以外を含むmode推定
- MIDI/MusicXML等から`BlackadderObservations`を生成するadapter
- 外部rule setのschemaとloader
- corpusからのweight学習とscore calibration
- k-best結果間の重複除去と等価解釈のgrouping
- 長い曲向けのbeam幅、window、streaming方針

これらを追加しても、`original_symbol`の保存、型付きquality、明示boundary、候補と根拠の保持という現在の契約は変更しない。
