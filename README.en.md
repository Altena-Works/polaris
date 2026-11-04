<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="docs/assets/polaris-banner-white-animated.svg">
    <img src="docs/assets/polaris-banner-ink-animated.svg" alt="polaris — asterisk and wordmark" width="800" height="200">
  </picture>
</p>

<p align="center">
  <img alt="Version v0.3.0-rc Castor" src="https://img.shields.io/badge/v0.3.0--rc-Castor-35505B?style=for-the-badge&labelColor=182D36">
  <img alt="Platform macOS arm64" src="https://img.shields.io/badge/macOS-arm64-35505B?style=for-the-badge&labelColor=182D36&logo=apple&logoColor=white">
  <img alt="Rust 1.96+" src="https://img.shields.io/badge/Rust-1.96%2B-35505B?style=for-the-badge&labelColor=182D36&logo=rust&logoColor=white">
  <img alt="License MIT" src="https://img.shields.io/github/license/Altena-Works/polaris?style=for-the-badge&labelColor=182D36&color=35505B">
  <a href="docs/README.md"><img alt="Docs" src="https://img.shields.io/badge/Docs-Guide-35505B?style=for-the-badge&labelColor=182D36&logo=readthedocs&logoColor=white"></a>
  <img alt="Last commit" src="https://img.shields.io/github/last-commit/Altena-Works/polaris/rc%2Fv0.3.0?style=for-the-badge&labelColor=182D36&color=35505B">
  <a href="https://github.com/sponsors/4ltena"><img alt="Sponsor 4ltena" src="https://img.shields.io/badge/Sponsor-4ltena-35505B?style=for-the-badge&labelColor=182D36&logo=githubsponsors&logoColor=white"></a>
</p>

<p align="center">
  <strong>macOS TUI only · package not yet published</strong> · <a href="CHANGELOG.md">Changelog (Japanese)</a>
</p>

<p align="center">
  <a href="README.md" lang="ja">日本語</a> · <strong>English</strong>
</p>

polaris is a Rust coding agent that selects the context needed for the current task. Its main interfaces are a CLI and TUI, with a separate macOS workspace app. It stores workflow state and assembles instructions and skills for the active phase. By default it sends each turn only that turn plus recalled memory.

v0.3.0-rc targets the macOS arm64 TUI. GUI and other-platform sources remain in the repository. See the [system overview](docs/system-overview.md), [release scope](docs/releases/v0.3.0.md), and [command reference](docs/command-reference.md) (Japanese). No RC tag, release, or package has been published.

<p><a href="docs/releases/v0.3.0.md"><img src="docs/assets/versions/polaris-v0.3.0-starchart.gif" alt="v0.3.0 Castor star chart" width="100%"></a></p>

<h2><img src="docs/assets/readme/get-started.svg" alt="Using this development checkout" width="100%"></h2>

Start from a project directory. Starting from your home directory currently fails helper preparation with `RuntimeDenied`; see [startup troubleshooting](docs/usage.md#起動時のトラブル).

On macOS arm64, a local preview package can be assembled with a compiled polaris CLI. Once obtained, install it with one command, `sudo installer -pkg polaris-cli-preview-0.3.0-rc-arm64.pkg -target /`, then launch the TUI with `polaris`. Rust is not needed on the user's machine. This package is currently unsigned and unpublished; a real-machine installation check is still pending. See the [macOS guide](apps/macos/README.md) for assembly and verification details.

Building from source requires Rust 1.96.0. To use ChatGPT authentication:

```sh
cargo build --release -p polaris-cli
target/release/polaris login
POLARIS_PROVIDER=codex target/release/polaris
```

With no prompt argument, polaris starts the interactive TUI; `-p` runs one prompt. Without an explicit provider, an available OpenAI-compatible API key selects `openai`; saved ChatGPT authentication selects `codex` when no API key is available. With neither, interactive startup shows the login screen. The example explicitly selects `codex`. Use `target/release/polaris` when testing from source to distinguish it from another installed version.

Use `--phase` to choose the starting phase:

```sh
POLARIS_PROVIDER=codex target/release/polaris --phase specify -p "Define acceptance criteria for an equipment lending service"
```

The API-key `openai` provider currently uses Chat Completions and does not support Astra tool calling, which requires the Responses API. See the [usage guide](docs/usage.md) for authentication, configuration, and resuming conversations.

<h2><img src="docs/assets/readme/features.svg" alt="Core features" width="100%"></h2>

- **Stateful workflow:** Persist the phase and active task, then assemble the instructions and skills needed for each turn.
- **Conversation continuity:** Separate unrelated work into Sidechats; resume, fork, or rewind a conversation and its workspace files from the TUI.
- **Tools and delegation:** Use files, Git, MCP, and subagents through a shared execution core with connection and permission checks.
- **Memory and retrieval:** Preserve source material while retrieving older relevant context and large tool results when needed.
- **Selectable typed decision backends:** Retain Jev and SemIf while adding fixed Clef/Clef-flash profiles and explicit System One compatible connections under one decision contract. Disabled by default. Capacity tiers are recommendations and never change the selected backend automatically. No Clef profile has production acceptance yet, so normal Clef execution is not available. See [configuration and current limits](docs/usage.md#型付き判断器の設定) (Japanese).

The macOS CLI preview package contains no optional resources. The GUI installer has a manifest-verified download path for llama.cpp, the embedding runtime, and multilingual-e5-small/base under `~/.polaris`; Rust is optional. Qwen3.5-4B is a candidate for the SemIf classifier, but its pinned distribution artifacts and acceptance checks are incomplete, so it is not automatically downloaded. Signed distribution assets and real-device acceptance are still missing. See the [macOS guide](apps/macos/README.md) for the verified scope.

<h2><img src="docs/assets/readme/benchmark.svg" alt="Terminal-Bench v4.0 comparison with stock Codex" width="100%"></h2>

On 2026-11-04 we ran 7 Terminal-Bench v4.0 tasks once each at `gpt-6.1-sol` with `xhigh` effort, comparing polaris (turn-only) with Codex 0.160.0.

| | polaris (turn-only) | Codex 0.160.0 |
| --- | ---: | ---: |
| Tests passed | 99.2 to 99.3% | 92.9 to 93.0% |
| Tasks fully passed | 5 to 6 of 7 | 3 to 4 of 7 |
| Cost per task (API list rate, not a plan charge) | $0.377 | $0.362 |
| Input tokens per task | 705,922 | 794,133 |
| Requests per task | 24.1 | 20.6 |

Limits to read it by:

- Each task ran once, so differences of this size may be run-to-run variation.
- polaris cost 4% more.
- The ranges come from one test that depends on timing and flips between gradings.
- A benchmark task is a single user turn, where turn-only drops nothing, so the pass-rate gap cannot be credited to turn-only. Its benefit in long conversations is a separate measurement: over 18 turns, input fell from 841,289 to 96,215, against an older polaris, not Codex.

For the v0.2.0 result, see the [method and results](docs/preimplementation-evaluation.md#v0120とcodex-01540の途中結果) (Japanese).

<h2><img src="docs/assets/readme/docs.svg" alt="Documentation" width="100%"></h2>

Start with the [documentation index](docs/README.md). Detailed documents below are in Japanese.

| Document | Contents |
| --- | --- |
| [Documentation index](docs/README.md) | Current guides, implementation references, and historical measurements. |
| [Usage guide](docs/usage.md) | Authentication, startup troubleshooting, configuration, envkit, and conversations. |
| [Command reference](docs/command-reference.md) | CLI/TUI commands and their output. |
| [System overview](docs/system-overview.md) | Execution, storage, isolation, and crate roles. |
| [v0.3.0-rc](docs/releases/v0.3.0.md) | Distribution scope, candidate checksums, verification, and limits. |


<h2><img src="docs/assets/readme/development.svg" alt="Development and verification" width="100%"></h2>

```sh
cargo test --locked --offline --workspace
cargo clippy --locked --offline --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

<h2><img src="docs/assets/readme/references.svg" alt="Reference repository" width="100%"></h2>

- [openai/codex](https://github.com/openai/codex): the baseline for comparing CLI and TUI behavior, and a reference for parts of the design.

<h2><img src="docs/assets/readme/research.svg" alt="Research incorporated" width="100%"></h2>

This section lists the elements polaris takes from research papers, grouped by status. Planned and candidate items are not in the code yet. polaris has not re-measured the papers' results. Papers checked as evidence for the turn-only design are collected at the end of this section.

**Implemented**

- **Fact admission check:** Turn-only memory (the [default](docs/usage.md#ビルドと一発実行), Japanese) keeps a ledger that compresses each turn into one line. Of the decisions written in ledger lines, only those whose checkable terms all appear in that turn's source text, and that the user stated or approved, are carried with every request as facts. No model makes this judgment. Each recall checks that the supporting source text is still in the log and marks the fact unsupported if it is gone. Source: [Assay](https://arjmandi.github.io/assay/) (Arjmandi, 2026).
- **Lexical search:** Skill search, MCP tool preselection, and the turn-only index pick the entries closest to the input by term matching. Source: BM25 (Robertson et al., Okapi at TREC-3, 1994).
- **Rank fusion:** Merge the lexical and embedding rankings into one by summing 1/(60 + rank). Used by memory search and the turn-only index. Source: [RRF](https://doi.org/10.1145/1571941.1572114) (Cormack et al., 2009).

**Planned: next stages of turn-only memory**

- **Topic segments and gists:** Split the conversation where the topic changes, and keep a gist and the source position for each segment. Retrieve gists first and open the source text only when needed. Sources: [SeCom](https://arxiv.org/abs/2502.05589) (Pan et al., 2025), [ReadAgent](https://arxiv.org/abs/2402.09727) (Lee et al., 2024).
- **Facts with validity periods:** Store one claim per fact, with when it became valid and which fact replaced it. Old facts are never deleted. Sources: [Zep](https://arxiv.org/abs/2501.13956) (Rasmussen et al., 2025), [Chronos](https://arxiv.org/abs/2603.16862) (Sen et al., 2026).
- **Incremental synopsis:** Grow the short synopsis carried with every request by deltas instead of rewriting it whole, because repeated rewrites are reported to erode details (context collapse). Source: [ACE](https://arxiv.org/abs/2510.04618) (Zhang et al., 2025).
- **Links between memories** (optional stage): Connect memories with links that carry the relation's type and a reason. Source: [A-MEM](https://arxiv.org/abs/2502.12110) (Xu et al., 2025).
- **Update and temporal measurement:** Measure each stage with questions about fact updates and time, following the paper's question types. Source: [LongMemEval](https://arxiv.org/abs/2410.10813) (Wu et al., 2024).

**Planned: v2.0.0 and later**

- **Self-improving harness:** polaris itself runs the measure–diagnose–fix loop that people run today. It mines failure patterns from execution traces, proposes a small fix for each pattern, and keeps only proposals that pass regression tests without degrading tasks held out from the improvement. Source: [Self-Harness](https://arxiv.org/abs/2606.09498) (Zhang et al., 2026).
- **Model for proposals:** Generate improvement proposals with a cheaper model and spend the budget on the task-solving agent, since the ability to propose useful updates is reported to depend little on model strength. Source: [Harness Updating Is Not Harness Benefit](https://arxiv.org/abs/2605.30621) (Lin et al., 2026).

**Candidate**

- **Suppressing repeatedly retrieved chunks:** Lower a chunk's rank in later turns according to how often it has been quoted, so that chunks matching every query give way to the evidence hidden behind them. No model is called. Whether to adopt it will be decided by measuring it against no suppression. Source: [AMIE](https://arxiv.org/abs/2610.34977) (Hu et al., 2026).

<details>
<summary>Papers checked as evidence for the turn-only design (29)</summary>

For each of the five turn-only design decisions, the papers below support it, or limit or contradict it. Each was checked against its arXiv page for title, authors, and claims. None measured coding-agent tasks, and none measured Japanese. Most recall studies use models of 8B parameters or fewer.

- **Keep exact strings (names, paths, identifiers) in a dictionary outside the model**
  - Support: Models with a fixed-size state (SSMs and linear attention) trail attention-based models at recalling and copying exact strings from the context ([Repeat After Me](https://arxiv.org/abs/2402.01032), [Zoology](https://arxiv.org/abs/2312.04927), [Based](https://arxiv.org/abs/2402.18668), [Gated Delta Networks](https://arxiv.org/abs/2412.06464), [Waleffe et al.](https://arxiv.org/abs/2406.07887), [RULER](https://arxiv.org/abs/2404.06654)). In nested key-value lookups that GPT-4 failed, [MemGPT](https://arxiv.org/abs/2310.08560), which re-reads from external memory, was unaffected. Index keys that keep the original sentence retrieve better than keys made of extracted phrases alone ([LongMemEval](https://arxiv.org/abs/2410.10813)). Adding an exact memory to the linear-attention state realizes the same idea inside the model, but it requires training ([HOLA](https://arxiv.org/abs/2607.02303)).
  - Limits: A single short string can be recalled almost exactly within the trained length ([Stuffed Mamba](https://arxiv.org/abs/2410.07145)). Reading the context twice narrows the gap but does not close it ([Just read twice](https://arxiv.org/abs/2407.05483)). Mixing in a few attention layers narrows the gap substantially (Zoology, Waleffe et al.).
- **Replace verbatim history with one ledger line per turn and a direction line that sums up where the conversation is heading**
  - Support: Performance drops when an instruction is split across turns and recovers when it is consolidated ([LLMs Get Lost In Multi-Turn Conversation](https://arxiv.org/abs/2505.06120)). Input length alone hurts, even with perfect retrieval ([Context Length Alone Hurts](https://arxiv.org/abs/2510.05381)). Accuracy drops when relevant information sits in the middle of the context ([Lost in the Middle](https://arxiv.org/abs/2307.03172)). Claimed context lengths do not match the lengths at which performance holds (RULER). Reading a long history whole lowers accuracy compared with passing only the evidence (LongMemEval, [Zep](https://arxiv.org/abs/2501.13956)). In long search trajectories, summarization and truncation improved accuracy ([Context Rot](https://arxiv.org/abs/2606.29718)). Repeatedly rewriting a short memory from the previous memory and the new dialogue is the closest precedent for the direction line ([Recursively Summarizing](https://arxiv.org/abs/2308.15022)).
  - Limits and counterevidence: Iterative compression weakened the influence of recent interactions and increased execution instability ([Min et al.](https://arxiv.org/abs/2608.06503), counterevidence). Replacing past responses with a fixed string hurt code tasks ([What if LLMs Ate Their Words](https://arxiv.org/abs/2609.05882)). Rewriting multi-turn input into a fully specified instruction recovers performance, but simple summaries and Mem0 barely do ([Intent Mismatch](https://arxiv.org/abs/2602.07338)). On LoCoMo, the verbatim full history was the most accurate, and the advantage of [Mem0](https://arxiv.org/abs/2504.19413) lay in lower cost and latency. Assistant-side details are easily lost in compression (Zep). Truncation that keeps only the latest three turns helped about as much as summarization (Context Rot).
- **Retrieve source chunks lexically and by embedding, with citations**
  - Support: BM25 beats dense retrieval on questions about rare entities, while the opposite holds on Natural Questions ([Simple Entity-Centric Questions](https://arxiv.org/abs/2109.08535)). Storing raw rounds as values and adding extracted facts to the keys worked best (LongMemEval). Query-conditioned extraction outperforms summaries made without the query ([Jha et al.](https://arxiv.org/abs/2407.08892), [LongLLMLingua](https://arxiv.org/abs/2310.06839)). Citations in answers are more accurate when the input is narrowed than when the full text is passed ([Summary of a Haystack](https://arxiv.org/abs/2407.01370)). When summaries and source passages share one candidate pool, both get retrieved ([RAPTOR](https://arxiv.org/abs/2401.18059)). polaris indexes old ledger lines alongside chunks in the same way.
  - Limits: None of the checked papers compares BM25–embedding fusion itself. Dense retrieval beats BM25 on natural-language questions (LongMemEval, LongLLMLingua). When the input is a poor search key, retrieval backfires (Recursively Summarizing). All evaluations use English QA, dialogue, or documents, with no code or paths.
- **Admit a fact only after a deterministic check**
  - Support: In memory systems, hallucinations arise and accumulate at the model-driven extraction and update stages ([HaluMem](https://arxiv.org/abs/2511.03506)). SSM errors take the form of strings close to the right answer (Waleffe et al.). Errors of this form can be caught by exact string matching. Citations have been scored by matching extracted document IDs against the answer key without a model judge (Summary of a Haystack).
  - Limits: Extracting candidates and comparing them with existing memories (Mem0), or invalidating contradicted facts (Zep), is similar, but a model makes the decisions. None of the checked papers evaluates deterministic fact admission.
- **Keep the source text re-readable instead of relying on summaries alone**
  - Support: Lossy summaries alone scored far below approaches that can retrieve the full history (MemGPT, Zep). Gists alone trail the full text, but re-reading one or two source pages brings accuracy level with it ([ReadAgent](https://arxiv.org/abs/2402.09727)). Recursive summarization loses relevant information within a few steps ([Walking Down the Memory Maze](https://arxiv.org/abs/2310.05029)). Replacing source text with summaries or facts lowered QA accuracy (LongMemEval).
  - Limits: MemGPT's gap is against summaries alone, not the full context. When a conversation fits the context window, passing it whole scored about the same (Zep). Simple truncation sometimes matched summarization (Context Rot).

</details>

<h2><img src="docs/assets/readme/license.svg" alt="License" width="100%"></h2>

Released under the [MIT License](LICENSE). See the [TLS fixture notes](crates/polaris-http/src/fixtures/README.md) for the test material's source and license.
