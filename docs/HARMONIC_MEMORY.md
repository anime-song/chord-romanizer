# 長期和声記憶

## なぜ2次・3次Markovにしないか

Viterbi復号そのものは「1つ前のコードしか見られない」方式ではない。状態に何を
保持するかで、より長い依存関係を扱える。コード列そのものを2個、3個と状態へ
入れると候補数が急増し、どの記憶が順位へ効いたのかも説明しにくくなる。

本実装は次の3層に分ける。

1. function latticeが局所候補と1次遷移をTop-k復号する
2. segmental key-state beamがactive keyと転調区間を選ぶ
3. whole-path memoryが未解決の機能目標を追跡して再採点する

3層目は選択済みの機能候補を上書きしない。異なるlocal keyや機能を持つTop-k
パスごとに、その解釈から期待される解決先を追跡する。

## 状態

各`KeyedPathSelection`は次を返す。

- `key_region_age_chords`: 現在の`active_key`が継続している和音数
- `pending_resolutions`: そのイベント後にも残る未解決目標
- `resolved_resolution_sources`: そのイベントで解決した元イベント番号
- `pending_predominant`: dominantを待っているpredominant/subdominant
- `resolved_cadence_predominant_sources`: そのイベントで完結した終止の開始番号

`PendingResolution`は、開始イベント、目標調、dominant relation、間に入った
和音数、入れ子深度を持つ。スタック深度は2、許容する介在和音は2個に制限する。
これは`V/V/V → V/V → V → I`程度の入れ子を扱いながら、古い目標が後の
フレーズへ混入することを防ぐためである。

完全パスの`harmonic_resolutions`には、次の解決種別を保存する。

- `tonic_arrival`: major/minorの安定した目標和音へ到着
- `dominant_chain_link`: 目標rootが次のdominantとして到着
- `root_arrival`: rootは到着したが、コード記号だけでは安定したtonicと言えない
- `deceptive_arrival`: 選択済みsecondary deceptive候補による代理到着

完全な`Predominant → Dominant → Resolution`は`cadential_spans`へ保存する。
dominantの前と解決の前に何和音介在したかを別々に保持するため、UIは3点を結ぶ
終止arcとして表示できる。

## 更新順序

各コードでは次の順で状態を更新する。

1. 直前との間に明示的境界があればpending目標を消す
2. スタック最上位の目標が現在のroot、または選択済みdeceptive候補で解決するか調べる
3. 現在のコードがdominantなら新しい目標を開く
4. predominant/subdominantなら同じlocal keyのdominantを待つ準備状態を開く
5. 既存目標の介在和音数を増やし、window外を失効させる
6. 更新後のsnapshotをUI用selectionへ保存する

「解決してから開く」ため、`D7 → G7`の`G7`はD7の目標を解決すると同時に、
Cへの次の目標を開ける。境界はpending目標だけを消し、転調モデルが確定済みの
`active_key`は維持する。短い`N.C.`は通常は境界にしないが、
`NoChordPolicy::Break`では境界として扱う。

## スコア

通常の隣接`V–I`はfunction latticeですでに採点されるため、
`memory_score`では二重加点しない。次だけをwhole-path evidenceにする。

- 1～2個の介在和音を越えて、選択済み目標へ実際に到着した
- predominantが1～2個の介在和音を越えて同じ調のdominantへ到着した
- 深度2の内側の目標を解決し、外側の目標を保持できた
- 目標が短いwindow内に到着せず失効した（小さな減点）

したがって合計点は次になる。

```text
total_score =
    function_score + key_score + modulation_score + memory_score
```

`memory_score`は確率ではない。すべての寄与は`builtin.memory.*`の
`ScoreEvidence`として監査できる。

`Dm7 → G7 → C`のような隣接する完全終止は、既存latticeですでに
predominant–dominantとdominant–tonicの両方を採点しているため、
`CadentialSpan.score=0`で記録だけを行う。

## key regionの継続時間

転調区間には`duration_chords`を保存する。区間の終了は後続のkey-state選択で
初めて確定するため、継続時間は区間全体を選んだ後に採点するsemi-Markov
potentialである。

- 2和音だけのregionは短いtonicizationの可能性が高いため小さく減点する
- 3和音は中立
- 4～6和音は上限付きで少し加点する

長さだけで転調を確定せず、確認終止、pivot、active-key適合と併用する。

## 今後MIDIで追加するもの

現在はコード記号のイベント数だけを使う。MIDI/MusicXMLを統合すると、
実時間のduration、拍節、導音の実音、bass、声部進行、フレーズ境界を同じ
状態更新とevidenceへ追加できる。公開型はその観測追加を前提に分離してある。
