# 解釈ツリーUI

`analyze_interpretation_tree` は、global keyと機能解析のTop-kを共有prefixでまとめた
UI向けのtrieを返す。

```text
C major
└─ C: tonic
   └─ E7: dominant in A minor
      ├─ F: secondary deceptive target in A minor
      └─ F: global subdominant in C major
```

平坦なTop-kでは同じ先頭部分が候補ごとに繰り返される。ツリーでは「どの位置から
解釈が分かれたか」と「その分岐を選ぶと後続がどう変わるか」を直接表示できる。

## Python API

```python
from chord_romanizer import Romanizer

analyzer = Romanizer.strict()
tree = analyzer.analyze_interpretation_tree(
    ["Em7", "A7", "Dm7", "G7", "Cmaj7"],
    k=5,
)

for key_root in tree.roots:
    print(key_root.global_key, key_root.score_delta_from_best)
    for node in key_root.children:
        print(node.input_symbol, node.label, node.role, node.local_key)
```

global keyが既知なら固定できる。

```python
tree = analyzer.analyze_interpretation_tree(
    progression,
    global_key="C",
    global_mode="major",
    k=5,
)
```

## 推奨表示

global keyを第1階層、`chord_index`を横方向の列、`children`を縦方向の分岐として
表示する。ノードの短いラベルには次を使える。

- `input_symbol`: 入力コード
- `label`: global key基準のローマ数字
- `role`: tonic / predominant / dominant / subdominant / non-functional
- `active_key`: global keyまたは終止で確立した転調先
- `local_key`, `scope`, `local_degree`: active key内の局所調からの解釈
- `is_pivot`: 旧調と新調の両方で読むピボット位置
- `is_modulation_confirmation`: 新調の確認終止におけるtonic到達
- `score_delta_from_best`: 1位の完全パスからの差
- `best_rank`: そのbranchを通る最上位path

詳細パネルでは次を表示する。

- `emission_score`: その候補単独の点
- `transition_score`: 親候補との接続の点
- `step_score`: 上の2つの合計
- `cumulative_score`: その位置までの機能パス合計。key scoreは含まない
- `evidence`: このノードとincoming edgeに属する規則
- `harmonic_classifications`, `blackadder`: 詳細な理論分類

`supporting_path_ranks`は、そのノードを通る返却pathの1始まりrankである。
`top_k_support_ratio`と`is_top_k_consensus`は、返却されたTop-kの中での一致だけを
表す。統計的確率や理論的な確定度として表示してはいけない。UI上では
「Top 5中4候補」のように表示するのが安全である。

`consensus_node_ids`は全返却pathで一致した先頭prefixである。ここに含まれる
ノードは強調できるが、表示名は「Top-k一致」などにし、「確定」とはしない。

## 分岐の選択と再計算

各key rootとコードノードは`condition`を持つ。コードノードのconditionには、
rule-set version、入力進行と解析profileのfingerprint、global key、rootから
そのノードまでのcandidate prefixが入っている。

```python
selected = tree.roots[0].children[1]

conditioned_tree = analyzer.analyze_interpretation_tree(
    progression,
    k=5,
    condition=selected.condition,
)
```

これは以前のTop-kから該当pathを抽出する処理ではない。選択prefixを固定した上で
完全な候補ラティスから子孫を再度k-best探索する。そのため、最初のTop-kでは下位
だった後続候補も、選択後のTop-kへ入れる。

`condition`とglobal key引数は同時に指定しない。condition自身がglobal keyを保持
するためである。

転調候補ではcandidate IDにversioned tonal-state suffixが付く。同じローマ数字・
機能候補でも、tonicizationのままの枝とmodulationへ昇格した枝は別ノードになる。
suffixは内部識別子として扱い、UIの表示文字列には`label`、`active_key`、
`scope`を使用する。

長期和声記憶は各ノードの`pending_resolutions`、`pending_predominant`、
`resolved_resolution_sources`、`resolved_cadence_predominant_sources`へ投影する。
UIはsource eventと現在eventを結ぶdominant arc、および
predominant–dominant–resolutionの3点cadence arcを、完全パスを再joinせず描ける。

## 永続化と更新

`TreeCondition`自身が`rule_set_version`と`progression_fingerprint`を保持する。
UIが選択状態を保存する場合は、入力表示用データとconditionを一緒に保存する。

```text
入力コード進行
TreeCondition
```

規則更新や入力変更はcandidate IDを照合する前に検出される。返却値は
`condition_applied=true`, `condition_satisfied=false`となり、panicや別候補への
暗黙の置換は行わない。UIはこの状態で保存済み選択を解除し、新しいtreeを表示する。

## ツリーに含めないもの

N.C.や明示boundaryは候補ノードを作らないが、`event_index`は元入力位置を維持する。
`chord_index`はコードだけの連番なので、描画列にはこちらを使える。N.C.の長さや
MIDI由来のphrase境界が追加されても、この2種類のindexの意味は変わらない。
