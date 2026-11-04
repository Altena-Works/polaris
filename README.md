<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="docs/assets/polaris-banner-white-animated.svg">
    <img src="docs/assets/polaris-banner-ink-animated.svg" alt="polaris — アスタリスクとワードマーク" width="800" height="200">
  </picture>
</p>

<p align="center">
  <img alt="Version v0.3.0-rc Castor" src="https://img.shields.io/badge/v0.3.0--rc-Castor-35505B?style=for-the-badge&labelColor=182D36">
  <img alt="Platform macOS arm64" src="https://img.shields.io/badge/macOS-arm64-35505B?style=for-the-badge&labelColor=182D36&logo=apple&logoColor=white">
  <img alt="Rust 1.96+" src="https://img.shields.io/badge/Rust-1.96%2B-35505B?style=for-the-badge&labelColor=182D36&logo=rust&logoColor=white">
  <img alt="License MIT" src="https://img.shields.io/github/license/Altena-Works/polaris?style=for-the-badge&labelColor=182D36&color=35505B">
  <a href="https://github.com/sponsors/4ltena"><img alt="Sponsor 4ltena" src="https://img.shields.io/badge/Sponsor-4ltena-35505B?style=for-the-badge&labelColor=182D36&logo=githubsponsors&logoColor=white"></a>
</p>

<p align="center">
  <strong>macOS TUI限定 · パッケージ未公開</strong> · <a href="CHANGELOG.md">変更履歴</a>
</p>

<p align="center">
  <strong>日本語</strong> · <a href="README.en.md" lang="en">English</a>
</p>

polarisは、作業に必要な文脈を選んでモデルへ渡すRust製のコーディングエージェントである。CLIとTUIを主な操作経路とし、macOS向けの作業画面も備える。workflowの段階を状態として保存し、その段階に必要な指示・skillを組み立てる。既定では、各ターンの要求にそのターンと思い出した記憶だけを送る。

v0.3.0-rcはmacOS arm64のTUIを配布対象とする。GUI・他OSのソースは保持するが、この版の配布対象には含めない。会話・巻戻し・MCP・subagentの操作は[利用ガイド](docs/usage.md)と[コマンド参照](docs/command-reference.md)、構成と今回の提供範囲は[システム構成図](docs/system-overview.md)と[リリース資料](docs/releases/v0.3.0.md)を参照。RCパッケージは未公開である。

## 暫定版の利用方法

起動前に作業対象のプロジェクトへ移動する。ホームディレクトリからの起動は、現在`RuntimeDenied`で停止する。[起動時のトラブル](docs/usage.md#起動時のトラブル)を参照。

macOS arm64では、ビルド済みCLIを同梱したローカル検証用パッケージを作成できる。パッケージを取得した後の導入は`sudo installer -pkg polaris-cli-preview-0.3.0-rc-arm64.pkg -target /`の1コマンドで、以後は`polaris`で起動する。利用者側にRustは不要。現時点では未署名・未公開であり、実機へのインストール受入も未完了。組立て方法と検証範囲は[macOSの説明](apps/macos/README.md)を参照。

このcheckoutをソースから実行する場合はRust 1.96.0を用意する。ChatGPT認証を使う例は次のとおり。

```sh
cargo build --release -p polaris-cli
target/release/polaris login
POLARIS_PROVIDER=codex target/release/polaris
```

引数を省略すると対話TUI、`-p`を指定すると一発実行になる。プロバイダ未指定時は、OpenAI互換APIキーがあれば`openai`、APIキーがなく保存済みChatGPT認証があれば`codex`を選ぶ。いずれもなければ対話起動時にログイン画面を開く。例では`codex`を明示している。ソースから試す場合は`target/release/polaris`を指定し、導入済みの別版と区別する。

段階を指定して始める場合は、`--phase`を使う。

```sh
POLARIS_PROVIDER=codex target/release/polaris --phase specify -p "機材貸出サービスの受入条件を整理する"
```

APIキー用の`openai`プロバイダはChat Completions形式のため、Responses APIが必要なAstraのtool callingには未対応である。認証・設定・会話の再開方法は[利用ガイド](docs/usage.md)にまとめる。

## 主な機能

- **状態で管理するworkflow**：段階、作業、必要なskillを記録し、毎ターンの文脈を組み立てる。
- **会話の保存と分岐**：Sidechatで別文脈を分け、保存済み会話を再開・分岐できる。TUIでは対象のユーザー入力より前へ会話と作業ファイルを巻き戻せる。
- **ツールと委譲**：ファイル・Git・MCPの操作とsubagentへの委譲を共通の実行基盤で扱う。MCPの接続条件と操作権限を検査する。
- **記憶と取得**：原文を保持し、直近のターンと過去の関連情報を必要に応じて取り出す。大きなツール結果は参照から再取得できる。
- **選べる型付き判断器**：Jev・SemIfを維持し、Clef/Clef-flashの固定profileとSystem One互換接続を共通の判断契約で扱う。既定は無効。容量帯は推薦の目安で、backendを自動変更しない。Clefの本番受入profileはまだ0件であり、通常実行は未提供である。[設定と現在の制約](docs/usage.md#型付き判断器の設定)を参照。

macOSのCLI専用プレビューパッケージには追加資源を含めない。GUIインストーラには固定manifestからllama.cpp、埋め込み実行環境、multilingual-e5-small/baseを検証して`~/.polaris`へ取得する経路があり、Rustは任意選択である。Qwen系はSemIf用のQwen3.5-4Bを候補としているが、固定資源の登録と受入が未完了で、自動取得しない。署名済み配布資源と実機受入も未完了である。検証済み範囲は[macOSデスクトップ](apps/macos/README.md)に記載する。

## Terminal-Bench v4.0での純正Codexとの比較

2026-11-04に、Terminal-Bench v4.0の7課題を各1回、`gpt-6.1-sol`・`xhigh`で測った。polaris（turn-only）とCodex 0.160.0の比較である。

| | polaris（turn-only） | Codex 0.160.0 |
| --- | ---: | ---: |
| テスト合格率 | 99.2〜99.3% | 92.9〜93.0% |
| 全合格した課題 | 7課題中5〜6 | 7課題中3〜4 |
| 1課題あたりの費用（API単価での換算。プランの請求額ではない） | $0.377 | $0.362 |
| 1課題あたりの入力トークン | 705,922 | 794,133 |
| 1課題あたりの要求数 | 24.1 | 20.6 |

読むときの限界は次のとおり。

- 各課題を1回しか走らせていないので、この程度の差は実行ごとの揺れかもしれない。
- polarisの費用は4%高い。
- 範囲で示した値は、時間に依存する1つのテストの結果が採点のたびに入れ替わるためである。
- ベンチマークの課題は1回のユーザーターンで終わる。そこではturn-onlyは何も落とさないので、合格率の差をturn-onlyのせいとは言えない。長い会話での効果は別の測定で、18ターンの会話では入力が841,289から96,215になった。ただし比べた相手はCodexでなく、以前のpolarisである。

v0.2.0の測定は[測定条件と結果](docs/preimplementation-evaluation.md#v0120とcodex-01540の途中結果)を参照。

## 文書

目的別の入口は[文書一覧](docs/README.md)を参照。以下の実測文書は対象版と測定条件を確認して読む。

| 文書 | 内容 |
| --- | --- |
| [文書一覧](docs/README.md) | 現行ガイド、実装資料、過去の測定を目的別に整理。 |
| [利用ガイド](docs/usage.md) | 認証、起動トラブル、設定、envkit、会話操作。 |
| [コマンド参照](docs/command-reference.md) | CLI/TUIコマンドの役割と出力。 |
| [システム構成図](docs/system-overview.md) | 実行、保存、隔離、各crateの役割。 |
| [v0.3.0-rc](docs/releases/v0.3.0.md) | 配布範囲、候補checksum、検証結果と限界。 |


## 開発と検証

```sh
cargo test --locked --offline --workspace
cargo clippy --locked --offline --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

## 参考にしているリポジトリ

- [openai/codex](https://github.com/openai/codex)：CLI・TUIの操作と挙動を比べる基準であり、一部の設計の参考にしている。

## 取り入れた研究

polarisが設計に取り入れた論文の要素を、状態ごとに示す。「計画」と「候補」は、まだコードに無い。論文の結果は、polarisでは測り直していない。turn-onlyの設計の根拠として確かめた論文は、節の末尾にまとめる。

**実装済み**

- **確定事項の検査**：turn-onlyの記憶（[既定](docs/usage.md#ビルドと一発実行)）は、各ターンを1行に圧縮した台帳を持つ。台帳の行に書かれた決定のうち、照合できる語がすべてそのターンの原文にあり、利用者がそれを言ったか承認したものだけを、確定事項として毎回の要求に載せる。判定にモデルは使わない。思い出すたびに根拠の原文がまだ残っているかを確かめ、消えていれば根拠が無いと示す。出典：[Assay](https://arjmandi.github.io/assay/)（Arjmandi, 2026）。
- **語による検索**：skillの検索、MCPの工具の絞り込み、turn-onlyの索引で、入力との語の一致から近いものを選ぶ。出典：BM25（Robertson et al., Okapi at TREC-3, 1994）。
- **順位の統合**：語と埋め込みで得た2つの順位を、1/(60+順位)の和で1つにまとめる。記憶の検索とturn-onlyの索引で使う。出典：[RRF](https://doi.org/10.1145/1571941.1572114)（Cormack et al., 2009）。

**計画：turn-onlyの記憶の次の段階**

- **話題の区切りと要旨**：話題が変わるところで会話を区切り、区切りごとに要旨と原文の位置を持つ。先に要旨を引き、要るときだけ原文を開く。出典：[SeCom](https://arxiv.org/abs/2502.05589)（Pan et al., 2025）、[ReadAgent](https://arxiv.org/abs/2402.09727)（Lee et al., 2024）。
- **有効期間つきの事実**：事実は1件に1つの主張とし、有効になった時点と、どの事実に置き換えられたかを持たせる。古い事実は消さない。出典：[Zep](https://arxiv.org/abs/2501.13956)（Rasmussen et al., 2025）、[Chronos](https://arxiv.org/abs/2603.16862)（Sen et al., 2026）。
- **あらすじの差分更新**：毎回載せる短いあらすじを、まるごと書き直さずに差分で積む。書き直しを重ねると細部が失われる（context collapse）という報告による。出典：[ACE](https://arxiv.org/abs/2510.04618)（Zhang et al., 2025）。
- **記憶どうしのリンク**（任意の段階）：関係の種類と理由をつけて、記憶を互いに結ぶ。出典：[A-MEM](https://arxiv.org/abs/2502.12110)（Xu et al., 2025）。
- **更新と時系列の測定**：各段階を、事実の更新と時系列を問う問いで測る。問いの種類は論文の分類に倣う。出典：[LongMemEval](https://arxiv.org/abs/2410.10813)（Wu et al., 2024）。

**計画：v2.0.0以降**

- **ハーネスの自己改良**：今は人が回している測定・原因の調査・修正の繰り返しを、polaris自身が回す。実行記録から失敗の型を見つけ、型ごとに小さな修正案を出し、改良に使っていない課題を悪くしない案だけを回帰試験で残す。出典：[Self-Harness](https://arxiv.org/abs/2606.09498)（Zhang et al., 2026）。
- **改良案を作るモデル**：改良案は安いモデルで作り、費用は課題を解く側に回す。改良案を作る力はモデルの強さにあまり依らないという報告による。出典：[Harness Updating Is Not Harness Benefit](https://arxiv.org/abs/2605.30621)（Lin et al., 2026）。

**候補**

- **繰り返し引かれるチャンクの抑制**：引用に載った回数に応じて、次のターンからそのチャンクの順位を下げる。どの問いにも当たるチャンクを下げ、その陰に隠れた根拠を上位に出すためである。モデルは呼ばない。抑えない場合と比べて測り、採るかどうかを決める。出典：[AMIE](https://arxiv.org/abs/2610.34977)（Hu et al., 2026）。

<details>
<summary>turn-onlyの設計の根拠として確かめた論文（29編）</summary>

turn-onlyの5つの設計判断ごとに、支持する研究と、限定または反証する研究を挙げる。どの論文も、arXivのページで題・著者・主張を確かめた。コーディングエージェントの課題で測った論文も、日本語で測った論文も無い。想起の研究の多くは、8B以下のモデルを対象とする。

- **正確な文字列（名前・パス・識別子）を、モデルの外の辞書に置く**
  - 支持：固定サイズの状態を持つモデル（SSM・線形注意）は、文脈の中の文字列を正確に思い出したり写したりする課題で、attentionを使うモデルに劣る（[Repeat After Me](https://arxiv.org/abs/2402.01032)、[Zoology](https://arxiv.org/abs/2312.04927)、[Based](https://arxiv.org/abs/2402.18668)、[Gated Delta Networks](https://arxiv.org/abs/2412.06464)、[Waleffe et al.](https://arxiv.org/abs/2406.07887)、[RULER](https://arxiv.org/abs/2404.06654)）。入れ子の鍵の検索でGPT-4が正答できなくなる条件でも、外の記憶から引き直す[MemGPT](https://arxiv.org/abs/2310.08560)は影響を受けなかった。索引の鍵を抜き出した語句だけにするより、元の文も持たせるほうがよく引ける（[LongMemEval](https://arxiv.org/abs/2410.10813)）。線形注意の状態に正確な記憶を足す方式は、同じ考え方をモデルの中で実現する。ただし学習が要る（[HOLA](https://arxiv.org/abs/2607.02303)）。
  - 限定：短い文字列1つなら、学習した長さの中ではほぼ正確に引ける（[Stuffed Mamba](https://arxiv.org/abs/2410.07145)）。文脈を2回読ませると差は縮むが、残る（[Just read twice](https://arxiv.org/abs/2407.05483)）。attentionの層を少し混ぜると、差は大きく縮む（Zoology、Waleffe et al.）。
- **履歴を逐語で持たず、1ターン1行の台帳と、会話の方向をまとめた1行（方向の行）にする**
  - 支持：指示を複数のターンに分けると性能が落ち、1つにまとめ直すと戻る（[LLMs Get Lost In Multi-Turn Conversation](https://arxiv.org/abs/2505.06120)）。入力が長いだけで、検索が完全でも性能が落ちる（[Context Length Alone Hurts](https://arxiv.org/abs/2510.05381)）。関連する情報が文脈の中ほどにあると、精度が落ちる（[Lost in the Middle](https://arxiv.org/abs/2307.03172)）。公称の文脈長と、性能を保てる長さは一致しない（RULER）。長い履歴を丸ごと読ませると、根拠だけを渡す場合より正答率が下がる（LongMemEval、[Zep](https://arxiv.org/abs/2501.13956)）。長い検索の軌跡では、要約や切り詰めが精度を上げた（[Context Rot](https://arxiv.org/abs/2606.29718)）。前の記憶と新しい対話から短い記憶を書き直し続ける方式が、方向の行に最も近い先例である（[Recursively Summarizing](https://arxiv.org/abs/2308.15022)）。
  - 限定・反証：反復する圧縮は直近のやり取りの影響を弱め、実行の不安定を増やした（[Min et al.](https://arxiv.org/abs/2608.06503)、反証）。過去の応答を固定の文に置き換えると、コードの課題では悪化した（[What if LLMs Ate Their Words](https://arxiv.org/abs/2609.05882)）。複数ターンの入力を完全な指示に書き直すと戻るが、単純な要約やMem0ではほとんど戻らない（[Intent Mismatch](https://arxiv.org/abs/2602.07338)）。LoCoMoでは逐語の全履歴が最も正確で、[Mem0](https://arxiv.org/abs/2504.19413)の利点は費用と遅延の削減にあった。アシスタント側の細部は、圧縮で落ちやすい（Zep）。直近3ターンだけを残す切り詰めも、要約と同程度に効いた（Context Rot）。
- **語と埋め込みで原文のチャンクを引き、出典をつける**
  - 支持：まれな実体を問う質問ではBM25が密ベクトル検索に勝ち、Natural Questionsでは逆になる（[Simple Entity-Centric Questions](https://arxiv.org/abs/2109.08535)）。生のやり取りを値として持ち、抜き出した事実を鍵に足す構成が最もよかった（LongMemEval）。問いに合わせて抜き出す圧縮は、問いと無関係に作った要約を上回る（[Jha et al.](https://arxiv.org/abs/2407.08892)、[LongLLMLingua](https://arxiv.org/abs/2310.06839)）。出典を挙げる回答は、全文を渡すより、絞って渡すほうが引用が正確になる（[Summary of a Haystack](https://arxiv.org/abs/2407.01370)）。要約と原文を同じ候補に入れて引くと、要約も原文も引かれる（[RAPTOR](https://arxiv.org/abs/2401.18059)）。polarisが台帳の古い行をチャンクと同じ索引に入れるのも、同じ形である。
  - 限定：BM25と埋め込みの融合そのものを比べた論文は、確かめた範囲に無い。自然文の質問では、密ベクトル検索がBM25を上回る（LongMemEval、LongLLMLingua）。入力が検索の鍵に向かないと、検索は逆効果になる（Recursively Summarizing）。評価はどれも英語の質疑・対話・文書で、コードやパスを含まない。
- **決定的な検査を通った事実だけを、確定事項に入れる**
  - 支持：記憶の方式では、モデルによる抽出と更新の段階で幻覚が生じ、積み重なる（[HaluMem](https://arxiv.org/abs/2511.03506)）。SSMの誤りは、正解に近い文字列の形で出る（Waleffe et al.）。この形の誤りは、文字列の完全一致で見つけられる。モデルの判定を使わずに、抜き出した文書IDを正解と照合して引用を採点した先例がある（Summary of a Haystack）。
  - 限定：候補を抜き出して既存の記憶と照らす流れ（Mem0）や、矛盾した事実を失効させる仕組み（Zep）は近いが、判断はモデルが行う。決定的な検査で事実を確定する方式を評価した論文は、確かめた範囲に無い。
- **要約だけに頼らず、原文を引き直せるようにする**
  - 支持：損失のある要約だけを見る方式は、全履歴を引ける方式より大きく劣った（MemGPT、Zep）。要旨だけでは全文より劣るが、原文を1〜2ページ引き直すと全文と同程度になった（[ReadAgent](https://arxiv.org/abs/2402.09727)）。再帰的な要約では、関連する情報が数段で失われる（[Walking Down the Memory Maze](https://arxiv.org/abs/2310.05029)）。要約や事実で原文を置き換えると、質疑の正答率が下がった（LongMemEval）。
  - 限定：MemGPTの差は、全文脈でなく要約だけとの比較である。会話が文脈に収まるなら、全会話をそのまま渡しても同程度だった（Zep）。単純な切り詰めが要約と同程度に効いた例もある（Context Rot）。

</details>

## ライセンス

[MIT License](LICENSE)で公開する。TLSテスト素材の出典と許諾は[fixtureの説明](crates/polaris-http/src/fixtures/README.md)を参照。
