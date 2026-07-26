# 設計文書

このディレクトリには実装から独立して参照したい設計判断をまとめています。

- [`DESIGN.md`](DESIGN.md): Python 0.1.9の分析、Rust移植方針、Strict V1の確定仕様、Phase 0–5の結果
- [`CONTEXT_ANALYSIS.md`](CONTEXT_ANALYSIS.md): N.C.と境界、複数候補、候補lattice、k-best復号、拡張可能なrule set
- [`KEY_INFERENCE.md`](KEY_INFERENCE.md): global/local keyと機能解析の統合
- [`MODULATION.md`](MODULATION.md): 転調確認、ピボット、dominant bridge、Top-k統合
- [`HARMONIC_MEMORY.md`](HARMONIC_MEMORY.md): 長距離のdominant目標、限定深度のtonicization stack、semi-Markov継続時間
- [`INTERPRETATION_TREE.md`](INTERPRETATION_TREE.md): UI向け解釈ツリーと条件付き再計算

コードを読む場合は次の順序が分かりやすいです。

1. `chord-romanizer-rs/src/domain/`: 音名、コードAST、quality、degree
2. `chord-romanizer-rs/src/notation/`: 入力parseと出力render
3. `chord-romanizer-rs/src/theory/`: 綴りとChordFormula
4. `chord-romanizer-rs/src/analysis/interpreter.rs`: 一つのslash chordからlocal候補を作る
5. `chord-romanizer-rs/src/analysis/context.rs`: 進行上のneighborと決定的metadataを作る
6. `chord-romanizer-rs/src/analysis/lattice.rs`: 候補をgraph化して上位経路を復号する
7. `chord-romanizer-rs/src/analysis/key.rs`: global/local keyと機能候補を統合する
8. `chord-romanizer-rs/src/analysis/modulation.rs`: 終止から転調区間とピボットを推定する
9. `chord-romanizer-rs/src/analysis/memory.rs`: 長距離の未解決目標を追跡し完全パスを再採点する
10. `chord-romanizer-rs/src/analysis/tree.rs`: Top-kをUI向けprefix treeへ折り畳む
11. `chord-romanizer-rs/src/romanizer.rs`: 上記を公開結果へまとめる

`analysis/`の三ファイルは、local candidate generation → deterministic context → ranked path decodingという順に責務を分けています。候補を途中で一つへ潰さないことが、この構成で最も重要な設計上の制約です。
