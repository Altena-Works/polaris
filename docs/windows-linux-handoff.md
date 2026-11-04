# Windows・Linux 版の引き継ぎと仕様

別の端末で Windows・Linux 版の実装を続けるための文書である。§0 が引き継ぎで、§1 以降が仕様である。この端末の作業は、§0 を読むことから始める。リポジトリには `AGENTS.md` も `CLAUDE.md` も入っていないので、作業の規則は §0-3 に書く。

状態：案（未承認）。2026-11-04。独立レビューを 2 回受けて直した。判断 D1〜D7（§8）は、メンテナーの答え待ちである。

- 依頼（メンテナー、2026-11-04）：Windows・Linux 版を、Mac の実測と並行して作る。コア部分の実装はなるべく共通とする。
- 方法：読んだだけである。実測が動いているので、cargo・コンパイラ・コンテナは動かしていない。「推測」は、読んで導いた結論を示す。
- macOS 設計：macOS の sandbox と承認の作り直しの草案を指す。非公開のノートにあり、このリポジトリには入っていない。その中の判断 D1〜D7 も答え待ちで、この文書の §8 の D1〜D7 とは別のものである。
- 付録 A：文中の「付録 A の K-LANDLOCK」のような ID は、§11 の付録の項目を指す。

## 0. 引き継ぎ

### 0-1. 今の状態

- 基準の版は `rc/v0.3.0` の `549f550` である。この文書を足すコミットの 1 つ前で、実装の先頭にあたる。
- 本文の `path:行` は `ba86139` の時点のもので、`crates/` からの相対で書く（`scripts/` と `docs/` で始まるものはリポジトリの根からの相対）。読むときは `git show ba86139:crates/<path>` を使う。`549f550` では行がずれた所がある（例：`polaris-cli/src/main.rs` の 3374 行より後は 3 行）。
- macOS arm64 の CLI と TUI は、この版でそろっている。Linux と Windows については、この仕様の作業はまだ始めていない。木の中にある Landlock の閉じ込め、Linux の信号の持ち主、Windows の閲覧の TUI が出発点である（§2-2）。
- 開発用の Mac は性能の実測に使うので、compile と試験はこの端末で行う（§4-1）。
- 検証は macOS arm64 でしか行っていない。
  - `ba86139`：workspace 全体の試験は 4,916 件が通り、失敗 0、無視 52 だった。
  - `549f550`：変えた範囲の確認だけを行った（fmt、clippy、足した 3 件の試験、architecture と filemap の試験）。workspace 全体は回していない。
  - Linux と Windows の compile は確かめていない（§2-2）。

### 0-2. ここでやること

範囲は M0〜M6 である（Linux の P1 と、Windows の中間版）。M7（P2）は macOS 設計の承認とその共通の段を待つので、ここでは始めない。

1. 下の表のとおりに環境を用意し、リポジトリを取得する。最初の build は crate を取得するので、ネットワークが要る。
2. 下の「基準の確認」を 1 回回す。Linux だけの失敗が出るはずで、数は分からない。ここではコードを直さず、失敗した試験の名前を手元に残す。直すのは M1 である（§6-1）。
3. §8 の判断 D1〜D7 をメンテナーに聞く。聞く場所は、この作業を頼まれた場所である。答えが出て仕様が承認されるまで、コミットは作らない。待つ間も、基準の確認と、コミットのいらない測定（§6-1 の F-L1〜F-L8）は進めてよい。D4 では、別の端末があるので、CI を置かない C も選べる。
4. 作業の枝 `feature/windows-linux` を `rc/v0.3.0` から切る。枝の名前は変えない。最初のコミットで、判断の答えと基準の確認の結果を §0-5 に書く。
5. §6-1 の順に進め、段が終わるたびに §0-5 の表を直す。

| 端末 | 用意するもの |
|---|---|
| Linux（Ubuntu 22.04 か 24.04） | rustup、C コンパイラ、cmake、git、python3。例：`sudo apt-get install build-essential cmake git python3` |
| Windows（ネイティブ） | rustup、Visual Studio の C++ Build Tools と Windows SDK、cmake、NASM（PATH に入れる。例：`winget install nasm`）、Git for Windows |
| Windows（WSL2） | WSL2 の Ubuntu 24.04 に、Linux の行と同じもの。プロジェクトは `/mnt/c` ではなく Linux 側のディレクトリに置く（§3-7） |

Rust の版は `rust-toolchain.toml` が 1.96.0 に固定している。rustup が自動で入れる。Landlock の試験は、kernel 5.19 以上の実機か VM で回す。古い kernel やコンテナで落ちたときは、そのことも記録する（§3-6、§4-2）。

```sh
git clone https://github.com/Altena-Works/polaris.git
cd polaris
git switch rc/v0.3.0
```

Windows のネイティブでは、clone に `-c core.autocrlf=false` を付ける。リポジトリに `.gitattributes` がなく、Git for Windows の既定で改行が CRLF に変わると、ファイルをバイト単位で比べる試験が落ちるためである。

基準の確認（Linux と WSL2）：

```sh
cargo fmt --all -- --check
cargo clippy --locked --workspace --exclude polaris-desktop-service --all-targets -- -D warnings
cargo test --locked --workspace --exclude polaris-desktop-service --no-fail-fast
```

- 並列で回したときの既知の失敗 1 件は `polaris-desktop-service` の試験で、このコマンドでは外している（`docs/releases/v0.3.0.md`）。Mac の全体の試験は `RUST_TEST_THREADS=1` で回した。
- Windows のネイティブは今は compile が通らない（§2-2）。`cargo check --locked --workspace --exclude polaris-desktop-service --all-targets` の失敗を記録し、M2 で直す。
- `scripts/verify.sh` は使わない。Mac の手順で、リポジトリに入っていない `AGENTS.md` を更新する段を含む。上の 3 つを個別に回す。

### 0-3. 作業の規則

- push してよいのは作業の枝 `feature/windows-linux` だけである。`rc/v0.3.0` には直接 push しない。`rc/v0.3.0` は Mac の開発用のリポジトリから一方向に書き出しているので、直接足すと Mac からの次の push が早送りにならず拒否され、force push で直せば足したコミットが公開の履歴から消える。
- force は `rc/v0.3.0` には使わない。作業の枝には、メンテナーに確認してから `--force-with-lease` だけを使ってよい（§0-4）。
- push が権限の不足で失敗したら、止めてメンテナーに聞く。fork、Issue、PR を勝手に作らない。
- 公開のリポジトリである。手元のパス、アカウント名、トークン、鍵をコミットに入れない。コミットの作者とメールも公開の履歴に残るので、メンテナーが認めたものを使う。
- 次のものはコミットしない：`AGENTS.md`・`CLAUDE.md`・`GEMINI.md`、`.claude/`・`.codex/`・`.cursor/`、`.project-notes/`、`.superpowers/`、`docs/superpowers/`。
- コミットに AI の共同作者の行を付けない。
- macOS で compile されるコードの挙動を変えない（§3-1 の 5）。Linux だけ、Windows だけの枝に留める。Unix 共通のコードを直すコミットには、そう書く。
- ほかの製品のコードを写さない。polaris は MIT である。使ってよいものと使えないものは §9 にある。
- 依存を足すのは `seccompiler` だけで、Cargo.lock の差分の条件は §4-2 と §9 にある。
- 試験は、壊れたコードで落ちる形にする。否定だけを確かめる試験は、何もしないコードでも通る。
- `docs/architecture.md` と `docs/filemap.md` はソースから作る生成物で、ソースを足すと試験が落ちる。新しいファイルを足したら、`cargo fmt --all` の後に次で作り直す。新しい `.rs` には先頭に `//!` の説明の行が要る。`Cargo.toml` と `rust-toolchain.toml` のほかの新しい `.toml` には、`crates/polaris-core/tests/filemap.rs` の `toml_summary` にパターンを足す。

```sh
UPDATE_ARCHITECTURE=1 cargo test -q -p polaris-core --test architecture
UPDATE_FILEMAP=1 cargo test -q -p polaris-core --test filemap
```

### 0-4. Mac への戻し方

この端末の作業は、作業の枝を push して、次の 3 つをメンテナーに伝えるところまでである。伝えるのは M6 の後か、メンテナーに頼まれたときである。伝えた後は、答えがあるまで作業の枝へ push しない。

- 枝の先頭のコミット（以下 `<渡した先頭>`）
- 枝を切った元の `rc/v0.3.0` のコミット
- 段ごとの確認の結果

Mac 側では、メンテナーが次を行う。

1. 公開のコミットは開発用のリポジトリにないので、まず公開のリポジトリの `feature/windows-linux` を開発用のリポジトリへ fetch する（`git am --3way` には公開側の blob が要る）。次に、枝のコミットを `git format-patch` で取り出し、`git am --3way` で当てる。公開用に文字を置き換えた行と、その前後 3 行の文脈に当たるパッチは、衝突で止まる。開発用のリポジトリの値を残して直す。
2. macOS の確認（`scripts/verify.sh`）を回す。
3. `rc/v0.3.0` へ書き出して push する。ハッシュと時刻は変わる。取り込んだ最後のコミットを、この端末へ伝える。

書き出しの後、この端末では作業木に未コミットの変更がないことを確かめてから、次のどちらかを行う。`<渡した先頭>` は、Mac が取り込んだ最後のコミットを使う。`git merge` はしない。同じ変更がハッシュ違いで 2 重になる。普通の `git rebase` も使わない。Mac でパッチを直すと、同じように 2 重になりうる。

```sh
git fetch origin
git switch feature/windows-linux

# すべて取り込まれたとき
git reset --hard origin/rc/v0.3.0

# <渡した先頭> より後のコミットが手元にあるとき
git rebase --onto origin/rc/v0.3.0 <渡した先頭> feature/windows-linux
```

書き換えた枝の push には force が要る。メンテナーに確認してから、上の `git fetch origin` の直後に次で push する。

```sh
git push --force-with-lease origin feature/windows-linux
```

実測の結果、macOS 設計、過去のノートは、Mac の非公開のノートにある。要るときはメンテナーに頼む。

### 0-5. 進捗

判断（§8）：

| 判断 | 答え | 日付 |
|---|---|---|
| D1 | | |
| D2 | | |
| D3 | | |
| D4 | | |
| D5 | | |
| D6 | | |
| D7 | | |

段：

| 段 | 状態 | 記録 |
|---|---|---|
| 基準の確認（Linux・WSL2） | 未着手 | |
| 基準の確認（Windows） | 未着手 | |
| M0 | 未着手 | |
| M1 | 未着手 | |
| M2 | 未着手 | |
| M3 | 未着手 | |
| M4 | 未着手 | |
| M5 | 未着手 | |
| M6 | 未着手 | |

## 1. 目的と範囲

### 1-1. 目的
- Linux（x86_64・aarch64）と Windows（x86_64）で、TUI と CLI（`polaris`）を使えるようにする。
- 核は 3 つの OS で 1 つにする。核とは、方針（sandbox の方針・秘密の表・承認・プリセット・設定）、道具、会話、保存の形式である。OS の差は、関心ごとに 1 つの境界の裏へ置き、その裏は薄く保つ。
- 安全の保証は、macOS と同じ方針を、各 OS の仕組みで実装する。別の方針は作らない。守れない保証は黙って弱めない。利用者に告げ、その保証に頼る操作を止める。

### 1-2. 対象
| OS | target | 形 | 配る物 |
|---|---|---|---|
| Linux x86_64 | `x86_64-unknown-linux-gnu` | ネイティブの TUI と CLI | tar.gz（§4-4、D5） |
| Linux aarch64 | `aarch64-unknown-linux-gnu` | 同じ | 同じ。ただし、aarch64 の kernel で適合の試験が通ってから配る（§4-2、D5） |
| Windows x86_64 | WSL2 の中は上の Linux 版。ネイティブは `x86_64-pc-windows-msvc` | D1 で決める（推奨は、WSL2 を正規の経路とし、ネイティブは中間版に留める） | WSL2 は Linux の tar.gz。ネイティブは CI で作るだけ（D1 = A のとき） |

### 1-3. 対象外
- macOS の GUI（`apps/macos`）と、デスクトップのサービス（`polaris-desktop-service`・`polaris-fake-service`）。どちらも `polaris-cli` の依存の外にある（`polaris-cli/Cargo.toml:16-26`）。
- macOS だけの機能（§3-3 の表）。Linux と Windows では「この OS では使えない」と出す。
- musl（`polaris-tools/src/native_mount.rs:104-111` が glibc 以外では失敗に倒す）、32 bit、Windows on ARM、WSL1、BSD。
- 実測の間の macOS の挙動の変更。macOS に効く変更は、macOS 設計の段取りで行う。

## 2. 今の姿（読んだ事実）

### 2-1. 量
- OS の cfg は、852 ファイルのうち 269 ファイルに 3,522 個ある。
- 空行と注釈を除いた行を、どの OS が compile するかで分けると次のとおりである（コードの走査）。

| 組み合わせ | 行数 |
|---|---:|
| 3 つの OS | 約 225,600 |
| macOS と Linux（Unix） | 約 178,600 |
| macOS だけ | 約 24,400 |
| Linux だけ（Linux と Windows の組を含む） | 約 11,100 |
| Windows だけ | 約 14,200 |

- Unix の約 17.9 万行の多くは、ディレクトリの記述子からの相対の操作で作った耐久の部品である。耐久の部品とは、途中で落ちても状態を失わない保存と記録の持ち主を指す（会話の保存、管理の効果の記録、プロジェクトの変更の鍵、巻き戻し、MCP の保存など）。例：`polaris-core/src/apply_intent.rs`（約 6,100 行）、`apply_owner.rs`、`rewind_workspace.rs`、`sync/**`、`polaris-core/src/management_effect/store.rs`、`polaris-mcp/src/store.rs`。ba86139 の `crates/` を grep で数えると、`libc::openat(` の呼び出しが 84 か所（注釈を含め、`openat` を含む行は 94）、`libc::mkdirat(` が 37 か所、`libc::flock(` が 12 か所ある。

### 2-2. OS ごとの状態
| 項目 | macOS | Linux | Windows |
|---|---|---|---|
| compile | 通る | 読んだ範囲では妨げがない（推測）。ただし 2026-10-19 にソースの木を復元してから、Linux で compile した記録がない | 通らない。`polaris-tui/src/auth_browser.rs:188` が、Unix だけの `account`（`polaris-tui/src/lib.rs:4-5`）を呼ぶ。`polaris-core/src/project_git.rs:47` も Unix だけの `CommandExt` を門なしで使う。`polaris-cli/examples/cache_quality.rs:866` の試験は `--all-targets` でだけ出る。ほかにもあるはずで、全体は最初の `cargo check` で分かる |
| 子の書き込みの閉じ込め | Seatbelt（`polaris-sandbox/src/macos.rs:84-187`） | Landlock ABI V2（`polaris-sandbox/src/linux.rs:13-17`, `:92-101`, `:116-122`）。V2 の権利だけを扱うので、kernel が新しくても、根の外の切り詰めと、権限・所有者・xattr・時刻の変更は止まらない（§3-6） | なし。どの子も `UnsupportedPlatform`（`polaris-sandbox/src/confine.rs:63-68`, `:284-294`） |
| 子のネットワーク | 規則がないので拒否される | 制限なし（`linux.rs:44-54` はファイルの権利だけを扱う） | — |
| 隔離境界（MCP stdio・隔離実行） | deny default と、読める根の一覧（`macos.rs:34-83`） | 拒否（`confine.rs:252-256`）。そのため MCP の stdio サーバーは起動しない（`polaris-mcp/src/transport.rs:104-140`、推測） | 拒否 |
| 子の FD | `child_fds` の macOS の枝 | `close_range` の CLOEXEC（`polaris-sandbox/src/child_fds.rs:24-38`） | — |
| プロセスの停止 | process group と SIGTERM→SIGKILL（`polaris-sandbox/src/controlled.rs:373-410`, `:541`）。setsid で逃げた子は止まらない（`controlled.rs:1-4`） | 同じ | — |
| 認証情報 | `~/.polaris/auth.json` などを 0600 で原子的に置く（`polaris-auth/src/store.rs:28-60`）。Keychain は Git の同期の認証だけに使う | 同じコード（`cfg(unix)`） | 拒否（`store.rs:19-23`, `:146-170`）。保護 DACL などの NT の部品はあるが、試験と明示の経路でしか使わない（`polaris-auth/src/protection.rs:34-36`） |
| 信号と端末 | crossterm の raw mode | SIGINT・SIGTERM の持ち主、終了コード 130・143、PTY の試験（`polaris-cli/src/main.rs:2461-2468`） | 閲覧の TUI が Linux の端末の守りを使う（`polaris-tui/src/lib.rs:245-345`） |
| TUI | すべて | すべて（`private_state`・`resume_effect`・`signal` は Linux だけ） | 閲覧だけ。`/help`・`/status`・`/quit`（`polaris-tui/src/windows_read_only.rs:318-322`）。会話・`exec`・`sandbox` は拒否（`polaris-cli/src/main.rs:2540-2552`, `:2696-2702`） |
| 承認 | `polaris-core/src/approval.rs` と `polaris-tui/src/approver.rs` に OS の cfg はない。鍵の読み手は OS で分かれるが、どちらも先打ちの鍵を捨てる（`approver.rs:24-41`、`lib.rs:419-434`） | 同じ。ただし秘密の入力が違う。Linux の鍵の読み手（`lib.rs:419-434`）は秘密の入力のための関数を持たず、既定の実装（`approver.rs:34-40`）を通る。そのため `/mcp secret set` などで Ctrl-C が取り消しにならずに 'c' として入り、貼り付けは捨てられる。macOS の読み手は、Ctrl-C を Esc に写し、貼り付けを渡す（`approver.rs:61-77`, `:101-111`）。M1 で直す | 同じ（使う場面がない） |
| CI と配布 | CI はない。配布は macOS の pkg だけ | なし | なし |

### 2-3. 開発用の Mac の道具
- 固定した 1.96.0 の rust-std は `aarch64-apple-darwin` だけである（stable には `x86_64-apple-darwin` と `aarch64-unknown-none` もある）。Linux と Windows の target は、どの toolchain にも入っていない。
- C のクロスコンパイラ、zig、cross、xwin、lld はない。Homebrew の mingw-w64（`x86_64-w64-mingw32-gcc`）はある。
- Docker Desktop は arm64 の Linux コンテナを直接動かす。amd64 は模倣で動く。Rust の image はない。
- `ring`・`libsqlite3-sys`（bundled）・`aws-lc-sys`（cmake）には、target 用の C コンパイラが要る（推測、`Cargo.lock` から）。

## 3. 設計

### 3-1. 原則
1. 方針は値で 1 つにする。`SandboxPolicy`（`polaris-sandbox/src/policy.rs:103-108`）と、macOS 設計が足す欄（秘密の表、保護するファイル、一回だけの拡張）は、OS に依らない値にする。OS の層は、その値を OS の仕組みへ写すだけにする。
2. OS の層は「この機械で何を守れたか」を返す（§3-4 の保証の報告）。核はそれを見て、足りない保証に頼る操作を止める。
3. 境界は cfg で選ぶモジュールにし、OS ごとに同じ関数の形を持たせる。各 target で実装は 1 つなので、trait object は作らない。今の `build_native_command`（`confine.rs:201-294`）と同じ形である。差し替えが要る試験には、既存の trait（`KeyReader` など）を使う。
4. 同じ性質は、同じ試験の本体で確かめる。`polaris-sandbox/tests/conformance.rs`（新）の試験は cfg を持たず、子を起動する所だけが OS で変わる。必須の性質は、成り立つか、操作が拒否されるかのどちらかでなければならない。報告が「守れる」と言う性質は、必ず成り立たなければならない。その機械で確かめられない性質は「該当なし」と記録し、通ったとは数えない。
5. 実測が終わるまで、macOS で compile されるコードの挙動を変えない。共通化のための移動は、実測の後に macOS で `scripts/verify.sh` を回してから統合する。

### 3-2. 関心ごとの境界
| 関心ごと | 境界（今のファイル） | macOS | Linux | Windows（ネイティブ） | 共通に置くもの |
|---|---|---|---|---|---|
| 子の閉じ込め | `polaris-sandbox` の `confine.rs:201-294`、`controlled.rs:93-146` | Seatbelt（`macos.rs`） | Landlock（`linux.rs`）、seccomp（新 `linux/seccomp.rs`）、P2 で起動補助（新 `linux/launch.rs`） | なし（D1 = A）。B なら新 `windows.rs` | `SandboxPolicy`、秘密の表（macOS 設計 M1 の `secret.rs`）、保証の報告（新 `guarantees.rs`）、適合の試験 |
| 認証情報の保存 | `polaris-auth` の `store.rs`、`api_key.rs`、`decision_key.rs` | 0600 のファイル | 同じコード（`cfg(unix)`） | 拒否のまま（A）。B なら保護 DACL の root（`protection.rs:775`） | ファイルの名前と形式、`protection` の登録 |
| プロセスと信号 | `controlled.rs`、`polaris-tui/src/signal.rs`、`polaris-cli` の Ctrl+C | process group、crossterm | 同じ。加えて Linux の信号の持ち主。P2 の完全な階層では PID namespace | B なら job object（KILL_ON_JOB_CLOSE、breakaway なし） | 停止の手順（SIGTERM→SIGKILL の順と待ち時間） |
| 端末とクリップボード | `polaris-tui/src/terminal.rs`、`clipboard.rs:52-105`、`lib.rs:245-345` | crossterm、`pbcopy` | crossterm、OSC 52 を `/dev/tty` へ、Linux の端末の守り | crossterm、OSC 52 を stdout へ、同じ守り（閲覧の TUI） | 変えない |
| ファイルの鍵・権限・身元 | `polaris-auth/src/protection.rs`（Unix の登録と NT の部品の両方がある） | 0600・0700、uid、`flock` | 同じ | NT の部品（DACL、`LockFileEx`、`FileIdInfo`）。B で耐久の部品の土台にする | 耐久の部品の状態遷移と形式 |
| パスとデータの置き場 | `polaris-tools/src/account_home.rs:85-110`（`getpwuid_r`）、`polaris-core/src/project.rs:86-120` | `~/.polaris`、`$TMPDIR` | `~/.polaris`。子の TMPDIR は polaris が作るセッション専用のディレクトリ（§3-6） | B なら `%USERPROFILE%\.polaris`（プロファイルの API から得る。環境変数からは取らない） | `POLARIS_DATA_DIR` の扱い、秘密の表のパスの形 |
| 信頼する実行ファイル | `polaris-core/src/project_git.rs:52`（`/usr/bin/git`）、`polaris-auth/src/login.rs:201-210` | 絶対パス | `git` は `/usr/bin/git`。`xdg-open` は PATH から探し、polaris の環境をそのまま渡している（`login.rs:201-209`）。決めたディレクトリから探し、環境を消して PATH を固定して起動するように直す（§3-6） | B なら Git for Windows を決めた場所から探す | 「PATH から実行ファイルを探さない」という規則。共通のコードの PATH からの起動は macOS 設計に渡す（R16） |
| OS だけの道具 | `polaris-core/src/computer.rs:161-162` ほか（§3-3） | あり | 出さない | 出さない | 出さない理由の文 |

### 3-3. macOS だけのコードから動かすもの・残すもの
動かすもの（OS に依らない形にする）：
- `requires_process_containment`（`policy.rs:581-584` は `cfg(target_os = "macos")`）。Linux の層も読む。
- 子の TMPDIR の決め方（`polaris-cli/src/main.rs:4321-4332`）。関数 1 つにまとめる。macOS は今どおり `$TMPDIR` を使い、結果を変えない。Linux はセッション専用のディレクトリを作る。子へは `polaris-sandbox/src/child_environment.rs:10-24` が親の `TMPDIR` をそのまま渡すので、値はそこで入れる（M3）。
- `polaris common-tests` が使う `network_is_enforced`（`polaris-core/src/common_tests.rs:1224-1233`）。macOS でだけ真を返すので、このままでは M3 の後も、Linux の common-tests は「no-network boundary」を理由に拒否される。保証の報告から作る（M3）。
- 隔離境界で読める実行環境の根。共通のコードの 6 か所に、macOS に合わせた形で書かれている（`/System/Library` を含み、`/lib64`・`/usr/lib64`・`/etc` を含まない。`polaris-core/src/mcp/local.rs:18-21` だけは Linux の枝を持つが、`/bin`・`/usr/bin`・`/usr/lib` だけである）。場所は `mcp/local.rs:18-21`、`mcp/setup.rs:65`、`isolated_run.rs:26`、`verification/checkpoint.rs:2843`、`common_tests.rs:794-804`、`polaris-cli/src/main.rs:671` である。OS の層の関数 1 つにまとめる（M4）。
- macOS 設計の共通の部分。秘密の表と `classify()`（macOS 設計の M1）、方針の組み立て（同 M3）、一回だけの拡張の値（同 M6）、起動時の自己診断の手順（同 M8）である。macOS 設計には次の 4 つを守ってもらう。
  - 表と `SandboxPolicy` に SBPL の文字列を入れない。SBPL の正規表現を作る関数は `macos.rs` に置く。
  - 自己診断の負の対照は 1 回だけ定義する。
  - `/status` の「OS deny」の行は、保証の報告から作る。
  - 秘密の表の場所の行に OS を持たせる（`all`・`macos`・`linux`。§3-9）。`classify()` は 1 つのままにする。
- read ツールの「開いた記述子の本当のパス」（macOS 設計 M5）は、OS ごとの関数 1 つにする。macOS は `fcntl(F_GETPATH)`、Linux は `/proc/self/fd/<n>` の readlink、Windows は `GetFinalPathNameByHandleW`（`protection.rs:316` にある）を使う。

残すもの（macOS だけ）：
| 機能 | 場所 | Linux・Windows での扱い |
|---|---|---|
| Seatbelt の profile と、適用の失敗の判定 | `macos.rs`、`confine.rs:347-370` | OS ごとの層に置き換わる |
| computer ツール（osascript） | `computer.rs:21-22`, `:161-162` | 出さない（`available()` が `None` を返す） |
| envkit（`/env` と environment ツール） | `polaris-envkit/src/lib.rs:204`、`polaris-core/src/agent.rs:1156-1161` | 出さない |
| 読み取り専用の runtime image（hdiutil）、strict10 の履歴の埋め込み、turn-memory の埋め込み | `polaris-core/src/managed_runtime.rs:212`, `:222`、`polaris-cli/src/main.rs:1937-2050` | 「unavailable in this environment」と出す（今のまま） |
| Clef | `polaris-core/src/lib.rs:14-15` | なし |
| Finder での表示（Ctrl-click）と `pbcopy` | `polaris-tui/src/file_reveal.rs:228`、`clipboard.rs:64-92` | Finder の表示はなし。クリップボードは OSC 52 |
| Keychain を使う Git の認証（同期の push） | `polaris-auth/src/git_credentials.rs:657-667`, `:1469-1483` | `UnavailablePlatform`。ファイルによる代わりは作らない（今のまま） |
| pkg の組み立て | `apps/macos/scripts/assemble-cli-pkg.py` | Linux は tar.gz（§4-4） |

Linux で挙動が違う共通のコード（OS だけの機能ではないが、そのままでは Linux で違う結果になる）：
| 場所 | Linux での違い | 扱い |
|---|---|---|
| `bash` ツールの `/bin/sh`（`polaris-tools/src/bash.rs:17-23`） | macOS の `/bin/sh` は bash だが、Debian と Ubuntu では dash なので、モデルの書く bash の構文が Linux でだけ失敗する | Linux では `/bin/bash` があればそれを使う。なければ `/bin/sh` のまま使い、doctor に出す（M3） |
| `polaris decided` の時差（`polaris-cli/src/decided.rs:109-118` の `date -r <秒> +%z`） | BSD の書き方である。GNU の date は `-r` の引数をファイルと読むので、黙って UTC になる | Linux では `/bin/date -d @<秒> +%z` を使う（M1） |
| 空きメモリの読み取り（`polaris-core/src/decision_capacity/observation.rs:25-46`, `:60-102`） | macOS だけで、Linux では不明になる | 今のまま不明と出す |

### 3-4. 保証の報告と、足りないときの止め方
- `polaris-sandbox` に、方針の値を受けて「この機械で守れる保証」を返す関数を置く（新 `guarantees.rs`）。中身は OS の層が埋める。macOS は今の事実を返す。たとえば P2 の前の macOS は「秘密の OS 拒否：なし」を返す。
- 報告には、§3-6 の表のすべての保証を、守れるかどうかと理由つきで載せる。そのうち必須の保証だけが、操作を止める根拠になる。
  - 必須：足りなければ、それに頼る操作を止める。必須の集合は段で決まる（§3-5）。P1 は、書き込みの閉じ込め（中身の書き込み、切り詰め、権限・所有者・xattr・時刻の変更）、コマンドのネットワークの禁止、鍵束と IPC の拒否である。P2 では、秘密の読み取りの拒否、秘密の名前の保護、保護するファイルの書き込みの拒否を足す。
  - 端末への文字の差し込み、端末の装置の読み取り、親の環境変数の読み取りは、macOS でまだ測っていないので、P1 の必須には入れない。Linux では P1 から守り、報告に出す。macOS 設計は M0 の F13 で差し込みを、F12 で環境変数を測る。端末の装置の読み取りも F13 に足すよう頼む（R7）。P2 の必須に入れるかは、その測定の結果で決める。
  - 報告だけ：木の停止の強さ、他のプロセスへのシグナル、他のプロセスのコマンド行が見えるか。これらの害は、止まらない・止められるという形に留まる（推測）。
- 止め方（子を動かす入口はすべて同じ判定を通る。TUI の bash、`polaris -p`、`polaris sandbox`、`polaris common-tests`、write と edit の補助、MCP stdio である）：
  - 起動時の通知、`/status`、`polaris doctor` に、欠けている保証、理由、直し方を出す。
  - 機械の性質で欠ける保証（カーネルが古い、user namespace が使えないなど）は、起動の時に分かる。そのときは、その保証に頼る道具をモデルに渡さない。渡してから拒否すると、同じ失敗が続くためである。macOS には起きない場面なので、macOS の挙動は変わらない。
  - 起動時の自己診断の失敗など、そのほかの不足は macOS 設計 M8 と同じに扱う。TUI は開き、警告を出し、呼び出しは子を起動せずに拒否する。
  - 子を起動する直前にも、同じ判定をもう一度する。足りなければ、子を 0 回起動して拒否する。
  - broker（`POLARIS_SANDBOX_BROKER`、`confine.rs:86-104`）の経路は、protocol v1 が mode と書ける根しか運ばない（`broker.rs:129-140`）。そのため Linux では P1 から、必須の保証を持つ方針の子を broker に渡さず、`NotEnforced` で拒否する（cfg で Linux だけ。macOS は macOS 設計で同じにする）。報告には「broker」と出す。
  - 弱めた実行はしない。外す道は、今ある `--sandbox full-access`（macOS と同じ意味。`/status` に大文字で出す）だけにする。D2 で B を、または D6 で C を選んだときだけ、利用者が明示したときに限って動かす道を足す。
- 起動時の自己診断（macOS 設計 M8）は、報告した保証を負の対照で確かめる。報告と実測が食い違えば、報告を「なし」に倒す。
- 告げる文の例（製品の文は英語である。言い回しは実装で決める）：
  - Landlock がない：`Sandbox unavailable: this Linux kernel lacks Landlock ABI 2 (Linux 5.19 or later with Landlock enabled). Commands and writes are refused; nothing runs unconfined. Run polaris doctor.`
  - WSL2 の kernel が古い：`Sandbox unavailable: this WSL kernel lacks Landlock ABI 2. Run wsl --update in Windows and restart WSL. Commands and writes are refused.`
  - P2 で根に `.git` も `.polaris` もない（D7 = A）：`Commands are unavailable here: this directory has neither .git nor .polaris, so the files that Git and polaris run cannot be protected on this host. Start polaris in the project root, or create one of them. Run polaris doctor.`
  - P2 で user namespace が使えない（D2 = A）：`Commands are unavailable: this host cannot hide secret files from commands (unprivileged user namespaces are blocked). Run polaris doctor for the fix.`
  - loopback の MCP：`This MCP server needs the loopback profile, which Linux cannot limit to loopback. It was not started.`
  - Windows のネイティブ：今の `Windows lacks the required execution and mutation protections. No changes were made.` に、`Use the Linux build inside WSL2 for the full agent.` を足す。

### 3-5. 二段の目標：P1 と P2
| 段 | 方針 | いつ |
|---|---|---|
| P1 | 今の macOS（rc/v0.3.0）と同じ。書き込みは根（プロジェクトの根）と TMPDIR と `/dev/null` だけで、根の外の切り詰めと、権限・所有者・xattr・時刻の変更も止める（D6）。読み取りはどこでも。コマンドのネットワークは禁止。鍵束と IPC には届かない。MCP stdio と隔離実行は隔離境界の中。子の環境は許可の一覧（`polaris-sandbox/src/child_environment.rs:10-24`）。承認は今の Gate | 今から（実測と並行） |
| P2 | macOS 設計と同じ。根は起動したディレクトリ（TMPDIR は例外）。秘密の段階（Never・Ask）を OS でも拒否する（読み取り、rename・削除・link）。設定ファイルと `.git/config`・`.git/hooks` を守る。承認した外への書き込みは一回だけの拡張で通す。ネットワークは常に禁止。manual と auto のプリセット。起動時の自己診断 | macOS 設計をメンテナーが承認し、その共通の段（M1・M3〜M8）ができてから |

- P2 の共通の部分は 1 回だけ作る。OS ごとに作るのは、値を写す層とその試験だけである。
- P1 の間は、どの OS にも秘密の OS 拒否がない（今の macOS も同じ）。そのため P1 では、user namespace が使えない Linux でもコマンドを止めない。D2 の拒否は P2 から効く。ただし D6 で C を選んだときは、P1 でも、利用者が明示しなければコマンドを止める。

### 3-6. Linux：同じ方針をどう守るか
Linux の機械は 2 つの階層に分ける。
- 完全な階層：Landlock ABI V2 以上が使え、権限なしの user namespace で mount ができる。
- 縮小の階層：Landlock は使えるが、user namespace で mount ができない。Ubuntu 23.10 以降は AppArmor で権限なしの user namespace を制限する（既定で効くかは M1 で確かめる）。Docker の既定の seccomp は `mount` を止める（付録 A の K-SECCOMP）。

| 保証 | macOS | Linux（完全） | Linux（縮小） | 守れないとき |
|---|---|---|---|---|
| 書き込みは根と `/dev/null` だけ（中身の書き込み） | `subpath` の許可（`macos.rs:175-184`） | Landlock の path-beneath（今のまま）。切り詰めとメタデータは次の 2 行 | 同じ | V2 がなければ、閉じ込める実行をすべて止める（今の `NotEnforced` と同じ）。起動の時に告げる |
| 根の外の切り詰め | `file-write*` の外なので拒否される（推測） | V3 以上の kernel では Landlock の TRUNCATE も扱い、根と `/dev/null` にだけ許す。V2 の kernel（5.19〜6.1）では seccomp で埋める（補足） | 同じ | — |
| 根の外の権限・所有者・xattr・時刻の変更 | `file-write*` の外なので拒否される（推測。`file-write-mode`・`file-write-owner`・`file-write-xattr`・`file-write-times` は `file-write*` に含まれる） | Landlock はどの ABI でも扱わない。P2 は起動補助が根の外を読み取り専用で張り直す（`EROFS`）。P1 は D6（推奨の A では、seccomp で全体を拒否する） | D6 | D6 |
| コマンドのネットワークの禁止 | 規則がないので拒否される。Unix socket も含む（推測） | seccomp（新）。`socket` は `AF_UNIX` だけを許す。`connect`・`bind`・`listen`・`accept`・`accept4`、宛先つきの `sendto`、`io_uring_*` を拒否する | 同じ | seccomp が使えなければ、閉じ込める実行を止める |
| 端末への文字の差し込み | 未実測（macOS 設計 M0 の F13） | seccomp で `ioctl(TIOCSTI)`・`ioctl(TIOCLINUX)` を拒否する（request は下位 32 bit で比べる。補足） | 同じ | — |
| 端末の装置を読ませない（打った鍵の横取り） | 未実測（macOS 設計 M0 の F13） | 読みの規則を `/` 全体から変える。`/dev` を除いた最上位の各項目と、決めた装置（`/dev/null`・`/dev/zero`・`/dev/full`・`/dev/random`・`/dev/urandom`）だけを許す | 同じ | — |
| 親の環境変数を読ませない | 未実測（macOS 設計 M0 の F12） | Landlock の ptrace の制限で `/proc/<親>/environ` が読めない（推測。M1 で測る） | 同じ | 測って成り立たなければ M3 で塞ぐ |
| 鍵束・SysV IPC・POSIX の message queue（パスを持たないカーネルの物） | deny default の下で、mach-lookup は opendirectoryd しか許さないので、Keychain・KCM・SysV IPC に届かない（`macos.rs:94-95`、推測） | seccomp で `add_key`・`keyctl`・`request_key`、SysV IPC のすべての呼び出し（shm・msg・sem）、`mq_open`・`mq_unlink` を拒否する。あわせて `unshare`・`setns` と、`CLONE_NEW*` つきの `clone` を拒否し、`clone3` には `ENOSYS` を返す（libc は `clone` に戻る）。権限なしの user namespace から届くカーネルの攻撃面も減る | 同じ | seccomp が使えなければ、閉じ込める実行を止める |
| 他のプロセスのコマンド行 | 隔離境界は拒否する（`macos.rs:47`）。通常の子は未実測（macOS 設計 M0 の F12） | P2 は PID namespace で外が見えない。P1 の通常の子は、他のプロセスの `/proc/<pid>/cmdline` と `status` を読める（environ は Landlock の ptrace の制限で読めない、推測）。隔離境界は M4 で決める | P1 と同じ（P2 でも PID namespace がない） | 報告だけ |
| 隔離境界（MCP stdio・隔離実行） | deny default と読める根。FullAccess でもファイルとプロセスの隔離は残し、ネットワークだけを足す（`macos.rs:75-78`） | Landlock の読みの許可の一覧（新）。mode に依らず、読める根の一覧、隔離した HOME と TMPDIR、no-fork の封じ込め（求める方針だけ）を適用し、FullAccess で外すのは IP の socket（`AF_INET`・`AF_INET6`）の規則だけにする。`AF_UNIX` の socket への `connect`・`bind` の拒否と `io_uring` の拒否は、mode に依らず残す（外すと、読める根の外の Unix socket、たとえば `/run/user/<uid>/bus` の session bus を通して、systemd のユーザーの manager に sandbox の外のコマンドを頼める。Landlock は ABI V9 より前はこの connect を止めない）。`socketpair` は許す。`getaddrinfo` が使う `AF_NETLINK` が要るかは M4 で測る。今の `linux.rs:32-34` は FullAccess で何もせずに戻るので、隔離方針はそこを通さない。no-fork の封じ込めは、seccomp で `fork`・`vfork`・`CLONE_THREAD` のない `clone` を拒否し、`clone3` には `ENOSYS` を返す。`/proc`・`/sys`・`/etc` のどこを読ませるかは M4 で決める（例：`/proc` は自分の分だけ、`/etc` は決めたファイルだけ） | 同じ | `loopback` の profile は表せない（Landlock の TCP の規則はポートしか見ない）。そのサーバーは起動しない |
| （P2）秘密の名前の読み取りの拒否 | 名前の正規表現で、開くたびに判定する。場所は問わない | 起動補助が mount namespace の中で覆う（秘密の上に、中身が空で読めない（mode 000）読み取り専用のファイルか tmpfs を重ねる。`cat .env` は macOS と同じく失敗する）。覆う場所は 3 つである。子を起動する時点で、プロジェクトの根と書ける根を走査して見つけた名前。ホームの決まった場所（表の段階 Never・Ask のディレクトリとファイル。§3-9 の Linux と WSL2 の行を含む）。polaris のデータのディレクトリ。ほかの場所（別のプロジェクトの `.env` など）は覆えない | 守れない | 完全な階層でも、ほかの場所は read ツールだけが守る。文書と `/status` に書く。縮小の階層は D2 |
| （P2）秘密の名前の rename・削除・link の拒否 | `file-write-unlink`・`file-link` を拒否する | 覆った点では、rename と削除が `EBUSY`、link が `EXDEV` になる（推測。M1 で測る） | 守れない | D2 |
| （P2）根の中の設定ファイルと `.git/config`・`.git/hooks` の書き込みの拒否、`.polaris` と `.git` の rename と削除の拒否 | literal の拒否。パスがまだなくても効く | `.polaris` と `.git` をそれ自身の上に重ね（rename と削除が `EBUSY` になる、推測）、中の保護するパスを読み取り専用で重ねる。bind は既にあるものにしか重ねられないので、まだないものの扱いは D7（補足） | 守れない（コマンドだけ。write ツールは共通の予測 `polaris-tools/src/predicate.rs` が止める） | D2・D7 |
| （P2）一回だけの書き込みの拡張 | literal の許可（M0 の F6 しだいで親のディレクトリ） | その子の Landlock にだけ規則を足し、読み取り専用の眺めの中でも、その場所を書けるように重ね直す。write と edit の補助（決まったコードが 1 つの変更だけをする）では、既にあるファイルはファイル単位、新しいファイルは親のディレクトリ単位になる（macOS 設計の代わりの案と同じ）。コマンドの `extra_write_paths` は、既にあるファイルと既にあるディレクトリだけを受け付ける（補足）。広すぎないかの判定は、実際に許すディレクトリで行う。覆いと保護は拡張した写しから作るので、拡張が秘密と設定の守りを越えることはない | 補助は同じ（コマンドは D2） | — |
| 子の TMPDIR | `$TMPDIR`（利用者ごと） | polaris が作るセッション専用の 0700 のディレクトリ（`$XDG_RUNTIME_DIR` の下。なければ `/tmp` の下）。終了の時に消す。P2 では `/tmp` にも重ねる | TMPDIR だけ。`/tmp` を決め打ちする道具は書けない | — |
| 木の停止 | process group | P2 は PID namespace。PID 1 が終われば中の全員が止まるので、setsid で逃げた子も止まる（macOS より強い） | process group（同じ限界） | 報告だけ |
| 他のプロセスへのシグナル | 同じ sandbox の中だけ | P2 は PID namespace で外が見えない。P1 は Landlock ABI V6（6.12 以上）があれば範囲を絞る | 6.12 未満では守れない | 報告だけ |
| read ツールの段階分け | 核 | 同じ | 同じ | — |

補足：
- seccomp の BPF は `seccompiler` 0.5（Apache-2.0 OR BSD-3-Clause、純粋な Rust。Codex と Firecracker が使う）で作り、BSD-3-Clause の側で使う。BPF の飛び先を手で書く誤りは、そのまま安全の穴になるためである。
- seccomp の規則の細部は次のとおりである。
  - ioctl の request は下位 32 bit だけで比べる（seccompiler の Dword）。kernel は request を 32 bit に切って読むので、64 bit で比べると、上位の bit を立てた値で TIOCSTI の拒否をすり抜けられる（flatpak の CVE-2019-10063 と同じ形）。
  - x86_64 では、x32 の番号（`nr` に `0x40000000` が立つもの）が同じ `AUDIT_ARCH_X86_64` のまま届くので、すべて拒否する。seccompiler で表せなければ、その判定だけの短い filter を別に入れる。
  - filter は x86_64 と aarch64 で別に作る。aarch64 には `open`・`chmod`・`fork` などがなく、`*at` の形と `clone`・`clone3` だけがある。
- 残る穴：宛先つきの `sendmsg` と `sendmmsg` は、宛先を filter から見られない。そのため、Unix の datagram socket（journald・syslog と、abstract の名前のもの）へは書ける（推測）。Landlock ABI V6（6.12 以上）があれば `Scope::AbstractUnixSocket` で abstract の名前のものを、ABI V9（7.1 以上）があれば `FS_RESOLVE_UNIX` でパスのものを塞ぐ。V6 では `Scope::Signal` も使い、他のプロセスへのシグナルを絞る。塞げない kernel では、この残りを保証の報告と文書に書く。
- 切り詰めとメタデータ。今の ruleset は V2 の権利だけを扱う（`linux.rs:17`, `:44-46`。`:72-77` の注釈も、TRUNCATE を扱わないと書いている）。そのため kernel が新しくても、閉じ込めた子は、根の外のファイルを `truncate(2)` や `O_RDONLY|O_TRUNC` の open で切り詰められる。M3 からは次のようにする。
  - V3 以上の kernel では TRUNCATE も扱い、根と `/dev/null` にだけ許す（`>` の `O_TRUNC` のため）。
  - V2 の kernel では、seccomp で `truncate(2)` と、access mode（`flags & O_ACCMODE`）が `O_WRONLY` でも `O_RDWR` でもない（0 か 3 の）`O_TRUNC` の `open`・`openat` を拒否し、`openat2` には `ENOSYS` を返す（引数が構造体なので、filter から見られないため。`openat2` を使う道具は、`ENOSYS` を受けると `openat` に戻るはずである、推測）。`ftruncate` は書ける記述子にしか効かず、書ける記述子は Landlock が根の中に限るので、許したままにする。V2 で失うのは、根の中でもパスを指定した `truncate(2)` が使えないことである。
  - chmod・chown・setxattr・utime は、どの ABI でも Landlock の対象外である（付録 A の K-LANDLOCK）。根の外だけを止めるには mount namespace の読み取り専用の眺めが要るので、P2 の完全な階層では起動補助がそれを張る。P1 と縮小の階層の止め方は D6 で決める。D6 = A では、seccomp で `chmod` 系・`chown` 系・`*setxattr`・`*removexattr`・`utime` 系を全体で拒否する。名前の形に当たらない新しい呼び出し（6.6 からの `fchmodat2`、6.13 からの `setxattrat`・`removexattrat`、`file_setattr`）も名前で挙げ、filter が知る最大の番号より大きい呼び出しには `ENOSYS` を返す（同じ働きの呼び出しがこの先の kernel で足されても、これで閉じる）。記述子の形（`fchmod` など）も含める。根の外のファイルも読み取りでは開けるので、記述子の形を残すと抜け道になる。`utimensat` だけは、時刻の引数が NULL のとき（今の時刻にするとき）に許す。`touch` を壊さないためで、根の外のファイルの時刻を今にすることだけはできる。この残りは、保証の報告と文書に書く。P2 の完全な階層では、この拒否を外して読み取り専用の眺めに替える。
- `connect` を一律に止めるので、sandbox の中で Unix socket の待ち受けを作る道具（Python 3.14 の multiprocessing の forkserver など）は動かない。macOS も同じはずである（推測。macOS 設計 M0 の F15 と、この仕様の M1 で測る）。
- 今の Linux では `/dev/ptmx` への書き込みが許されていないので、sandbox の中で PTY は作れない（推測）。`/dev` の読み取りを絞っても、この点は変わらない。macOS は `/dev/ptmx` を許している（`macos.rs:100-104`）。M1 で、日常の作業に効くかを測る。
- 読みの規則の最上位の項目は、子を起動する時点で数える。その後に `/` の直下にできた項目は、その子からは読めない（まれである）。
- 隔離境界では、`/` の一覧を許さない。macOS は `/` そのものだけの読み取りを許す（`macos.rs:34-83`）が、Landlock で `/` に規則を置くと、木全体の一覧を許すことになるためである。パスをたどることと `stat` は、Landlock では止まらない。
- 起動補助は、polaris の実行ファイルの内部のモードにする。今の `--confined-apply` と同じ流儀で、置き場も同じ仕組み（`polaris-cli/src/main.rs:3043-3069` の `PreparedConfinedHelper`）を使う。順は次のとおりである。user・mount・PID の namespace を作る → uid と gid を写す → `/` を private にする → 根の外を読み取り専用で張り直す（書ける根、子の TMPDIR、その呼び出しで承認した拡張の場所だけを、書けるまま重ね直す）→ `.polaris` と `.git` をそれ自身の上に重ね、保護するパスを読み取り専用で重ねる（まだないものは次の項）→ 覆いと `/tmp` を重ねる → 新しい `/proc` を張る → Landlock → seccomp（P2 では `mount`・`umount2`・`pivot_root` も拒否する。鍵束・SysV IPC・`unshare`・`setns` は P1 から拒否している）→ exec。覆いの元（mode 000 の空のファイルとディレクトリ）は namespace の中の読み取り専用の tmpfs に置き、子から mode を変えられないようにする。
- まだない保護のパス。読み取り専用の bind は既にあるものにしか重ねられないが、macOS の literal の拒否はパスがまだなくても効く。そのため、`.polaris/` のないプロジェクトや `.git/hooks` のない repository では、重ねるだけでは、コマンドが `.polaris/config.toml`・`.polaris/mcp/**`・`.git/hooks/pre-commit` を作れてしまう。hook は、後でその人の git が sandbox の外で実行する。扱いは D7 で決める。D7 = A では、根に `.git` か `.polaris` があるとき、namespace に入る前に、polaris がまだないものを空で作ってから重ねる。どちらもないときは何も作らない（下の 4 つ目の項）。
  - `.polaris` がなければ空の `.polaris/` を作る。`.polaris/config.toml` は空のファイル、`.polaris/mcp` は空のディレクトリとして作る。`.polaris` のほかの中身は、macOS と同じく書けるままにする。
  - `.git` がディレクトリで `hooks` がなければ、空の `.git/hooks` を作る。`config` がなければ壊れた repository なので、コマンドを止める。
  - `.git` がなければ空の `.git/` を作り、全体を読み取り専用で重ねる（根での `git init` は、macOS と同じく失敗する）。git は空の `.git` を repository と見なさず、上へ探しにいく（推測）。
  - ただし根に `.git` も `.polaris` もないときは何も作らず、コマンドを止めて理由と直し方を出す（§3-4 の文の例）。作ると、そこより下から後で起動したときのプロジェクトの根（`polaris-core/src/project.rs:15-61` の `resolve_root`）が変わり、上のプロジェクトの設定が効かなくなるか、macOS 設計の D1 = B では書ける根が広がるためである。
  - 作ったものは消さない。作業木には空の項目が残り、git に見えるのは空の `.polaris/config.toml` だけのはずである（推測。M7 で確かめる）。
- 一回だけの拡張の粒度。Landlock の path-beneath の規則は、ディレクトリに置くとその下の木全体に効き、新しいファイルを作るには、親のディレクトリに MAKE_REG と WRITE_FILE が要る（推測。付録 A の K-LANDLOCK から）。macOS 設計が親のディレクトリ単位の代わりの案を認めたのは、子が決まったコードの補助で、1 つの変更しかしないからである。そこで、親のディレクトリ単位の拡張は write と edit の補助に限る。コマンドの `extra_write_paths` は、Linux では、既にあるファイル（ファイル単位の WRITE_FILE。V3 以上では TRUNCATE も）と、既にあるディレクトリ（確認画面に「この下のどのファイルも変えられる」と出す）だけを受け付ける。
  - まだないファイルは確認の前に拒否し、親のディレクトリを指定するよう案内する。確認画面には、カーネルが許す範囲をそのまま出す。
  - `/`・$HOME・その祖先・保護するパス・補助と状態のディレクトリに当たるかは、実際に許すディレクトリで判定する。補助が $HOME の直下に新しいファイルを作る承認は、$HOME を開くことになるので拒否する。
- bubblewrap は使わない。理由は 3 つある。サーバーには入っていないことが多い。LGPL の別のプログラムを配ることになる。方針の写し方が 2 か所に分かれる。
- 走査による覆いには限界が 2 つある。1 つは場所で、表に書いた 3 つの場所の外は覆えない。もう 1 つは時点で、子の起動の後にできた秘密の名前は、その子には覆われない。どちらも macOS の「場所を問わず、開くたびに判定する」より弱いので、文書と `/status` に書く。権限なしで名前を場所を問わず拒否する仕組みは、Linux にはない（seccomp はパスを読めず、利用者への通知の方式は検査と使用の間の競合を防げない。付録 A の K-SECCOMP）。
- 走査は `.git/objects` だけを飛ばす。数か時間の上限を超えたら、そのコマンドを止める（覆わずに動かすことはしない）。上限の値は M1 で測ってから決める。
- 子の TMPDIR は P1 で要る。今の Linux では TMPDIR がたいてい空なので、書ける根に TMPDIR が入らず、子は `/tmp` にも書けない（`polaris-cli/src/main.rs:4321-4332`）。そのため `rustc` などの一時ファイルが作れない（推測）。
- `xdg-open` は PATH から探し、polaris の環境をそのまま渡している（`login.rs:201-209`）。書き込める根が PATH にあると（`.venv/bin` を有効にしている、direnv の `PATH_add bin`、`node_modules/.bin` など）、閉じ込めた子が置いた偽物を、閉じ込めていない polaris が実行しうる（推測）。`/usr/bin/xdg-open` はシェルスクリプトで、`gio`・`kde-open`・`xdg-mime` などを受け継いだ PATH から探す（推測。xdg-utils の作りから）ので、xdg-open の場所を決めるだけでは足りない。そこで次のようにする。
  - 決めたディレクトリ（`/usr/bin`、`/usr/local/bin`、`/bin`）にあり、root が持ち、利用者が書けないものだけを使う。
  - 環境を消し（`env_clear`）、PATH を `/usr/bin:/bin` に固定して起動する（`project_git.rs:52-61` と同じやり方）。
  - 渡す変数は、ブラウザを開くのに要る `DISPLAY`・`WAYLAND_DISPLAY`・`XDG_RUNTIME_DIR`・`DBUS_SESSION_BUS_ADDRESS` と、既定のブラウザを選ぶのに要る `HOME`・`XDG_CURRENT_DESKTOP` だけにする（後の 2 つが要るかは推測。M5 で確かめる）。どれも polaris の起動の時に決まり、子は書き換えられない。
- 最低のカーネルは D3 で扱う。Ubuntu 24.04 の既定で縮小の階層になる点は、D2 と D5 で扱う。

### 3-7. WSL2
- WSL2 では Linux 版をそのまま使う。保証は §3-6 の表のとおりで、どちらの階層になるかは機械しだいである。WSL1 は Landlock がないので、閉じ込める実行を止める。
- WSL2 の kernel は WSL の版で違う。5.15 系の kernel は Landlock ABI V1 までで（付録 A の K-LANDLOCK の版の表から。WSL の kernel が手を入れているかは確かめていない）、V2 の床に届かないので、閉じ込める実行をすべて止める。そのとき doctor と起動時の通知は、Windows 側で `wsl --update` を案内する（§3-4 の文の例）。
- WSL2 のために足すことは 3 つある。
  1. Windows のプログラム（`cmd.exe` や `/mnt/c/...exe`）の起動は、WSL の init が Unix socket で Windows 側に頼む。§3-6 の `connect` の拒否は子孫にも効くので、閉じ込めた子からは起動できない（推測。M6 で測る）。
  2. Windows 側のホーム（`/mnt/<drive>/Users/<user>`）の秘密のディレクトリも、P2 では覆いの一覧に入れる（§3-9 の WSL2 の行。子を起動する時点で `/mnt/*/Users/*` を一覧して決める）。read ツールの名前の表は、今も `.ssh` などを場所に関係なく拒否する（`polaris-tools/src/path_policy.rs:60-103`）。§3-9 の WSL2 の行は、read ツールでも使う。
  3. `/mnt/c` の上のプロジェクトでの Landlock と mount の挙動は測っていない。M6 で測るまでは、Linux 側のディレクトリで使うよう文書で案内し、起動の時にも 1 行で知らせる。

### 3-8. Windows ネイティブ
D1 = A（推奨）のとき：
- 今の閲覧版のまま、compile と CI だけを通す（M2）。読んで分かっているのは 3 か所である。`auth_browser.rs:188`、`polaris-core/src/project_git.rs:47`、例の試験 `polaris-cli/examples/cache_quality.rs:866`（Unix の symlink を門なしで使う）。
- 拒否の文で WSL2 を案内する。会話・道具・ログインは使えないままにする。
- 理由：管理者の設定なしで、秘密の読み取りの拒否とネットワークの遮断をネイティブで守る仕組みがない。restricted token の `WRITE_RESTRICTED` は書き込みにしか効かず、読み取りの拒否には別の身元（専用のユーザーか AppContainer）が要る（付録 A の W-OS と CX-W）。

D1 = B または C のときの形（段取りは §6-2）：
- 耐久の部品を NT の部品（`protection.rs` の `WindowsPrivateRoot`・`WindowsPinnedDirectory`・`LockFileEx`・`rename_no_clobber`）へ移す。
- 書き込みは、restricted token、根ごとの capability SID、継承する ACE、job object で閉じ込める。Everyone に書き込みを許すフォルダは、書けるまま残る。
- コマンドと MCP stdio は、B ではネイティブで止め、C では専用のユーザーと WFP の規則で動かす（Codex の elevated と同じ形）。
- 秘密の表に Windows の行を足す（§3-9 の WSL2 の行と同じ種類を、`%APPDATA%` と `%LOCALAPPDATA%` の下で）。
- ツールのパスでは、Windows の別名の形を拒否する。ドライブの後の `:`（代替データストリーム）、末尾の点と空白、装置名（`CON` など）、`\\?\` と `\\.\` で始まる形である。名前の判定は、`GetFinalPathNameByHandleW` で得た、開いた記述子の本当のパスにも行う。

### 3-9. 認証情報の置き場
| OS | 置き場と守り | 子から読ませないこと | 使わないもの |
|---|---|---|---|
| macOS | `~/.polaris/{auth.json, api_key.json, decision_keys.json}`。0600、原子的な置き換え。Keychain は Git の同期の認証だけに使う | P2 で、秘密の段階 Never として OS が拒否する | — |
| Linux | 同じファイルと同じコード（`cfg(unix)`） | P2 の完全な階層で覆う。縮小の階層ではコマンドを止める（D2） | Secret Service。セッションバスの上のすべてのプロセスを信頼する（CVE-2018-19358）うえ、ヘッドレスと WSL では当てにならない。Git の同期の認証は `UnavailablePlatform` のままにする |
| WSL2 | Linux と同じ。WSL の ext4 の上の `~/.polaris` に置く | Linux と同じ | `/mnt/c` の上には置かない |
| Windows ネイティブ（A） | 置かない（ログインは拒否のまま） | — | — |
| Windows ネイティブ（B・C） | `%USERPROFILE%\.polaris` を保護 DACL（そのユーザー・SYSTEM・Administrators だけ、`protection.rs:652-653`）の root にし、同じファイル名で置く | B はコマンドを動かさない。C は専用のユーザーから読めない | Credential Manager（1 件 2,560 バイトの上限、Codex #10353）。DPAPI も v1 では使わない。同じユーザーのプロセスは復号できるので、子への守りにならないためである |

秘密の表の場所の行（OS ごと）。秘密の表（macOS 設計 M1 の `secret.rs`）の行に OS を持たせる（`all`・`macos`・`linux`）。`classify()` は 1 つのままで、build の target の行と `all` の行だけを使う。名前の行（`.env`、`.ssh`、`*.pem` など）は `all` である。場所の行は次のとおりである。

| 種類 | macOS の行（macOS 設計） | Linux の行 | Linux の行のうち、WSL2 のためのもの |
|---|---|---|---|
| ブラウザの Cookie とパスワード | `~/Library/Cookies`、`~/Library/Safari`、`~/Library/Application Support` の下の `Cookies`・`Login Data` | `~/.mozilla/firefox/` の下の `logins.json`・`key4.db`・`cookies.sqlite`。`~/.config/` の `google-chrome`・`chromium`・`BraveSoftware`・`microsoft-edge` の下の `Cookies`・`Login Data` | `/mnt/<drive>/Users/<user>/AppData/` の下の同じ名前（Chrome・Edge・Brave は `Local`、Firefox は `Roaming/Mozilla/Firefox/Profiles`） |
| 鍵束とパスワードの管理 | 名前の行の `keychains`・`.keychain`・`.keychain-db`（今の行。`polaris-tools/src/path_policy.rs:28-36`） | `~/.local/share/keyrings`、`~/.local/share/kwalletd`、`~/.password-store` | `AppData/Roaming/Microsoft/Credentials`、`AppData/Local/Microsoft/Credentials`、`AppData/Roaming/Microsoft/Protect`（DPAPI の鍵） |

- WSL2 のための行は、Linux の行に含める。`/mnt/<drive>/Users/` の下でしか当たらないので、WSL かどうかは判定しない。
- 段階は macOS の行に合わせて Ask にする。
- macOS 設計 M1 の段階の表の試験と、この文書の AC9 に、各 OS の見本のパスを足す。

### 3-10. 承認・設定・プリセット
- Gate、承認の記録、プリセット、設定の鍵は核に置き、3 つの OS で同じにする。macOS 設計の表（A01〜A24）は OS の列を持たない。
- OS の差は、保証の報告を通してだけ表に出る。auto が安全なのは、OS がすべてのコマンドを閉じ込めるからである。必須の保証が足りない機械では、manual でも auto でもその操作を止める。どちらのプリセットでも、黙って動かすことはしない。
- 縮小の階層でも、write と edit の補助は動かす。補助は polaris 自身の決まったコードで、承認された 1 つの変更しかしない。秘密の段階 Never の編集は、核の Gate が子の起動の前に拒否する（macOS 設計の `edit_of_secret_like`）。
- `/status` に「OS guarantees」の行を足し、保証の報告から作る。例：`OS guarantees: writes confined; network denied; terminal injection blocked; secret deny NOT enforced (no user namespaces) - commands refused`。

## 4. ビルドと検証の計画

### 4-1. 実測の間の約束
- 実測の時間を乱さないため、実測の間は開発用の Mac で cargo も docker も動かさない。
- その間の compile と試験は、CI（D4）かメンテナーの別の機械で行う。
- 作業は、`rc/v0.3.0` から切った別の枝と別の作業木で行う。実測に使う固定のクライアントには触れない。
- 確認は段ごとに絞って 1 回ずつ回す。workspace 全体の確認は、統合の時に OS ごとに 1 回だけ回す。

### 4-2. どこで何を確かめるか
| 確かめること | 場所 | 要るもの | 承認 |
|---|---|---|---|
| Linux x86_64 の build と試験 | CI の `ubuntu-24.04`。AC8 の kernel の項は `ubuntu-22.04` でも回す | workflow のファイル | workflow の追加と push（D4） |
| Linux aarch64 の build と、kernel に依らない試験 | 実測の後の開発用の Mac の Docker（arm64 で直接動く）。D4 = B なら CI の arm64 の runner（公開のリポジトリなら使える、推測） | `ubuntu:22.04` に rustup で Rust 1.96.0 と cmake を入れた image（`scripts/ci/linux.Dockerfile`、§4-4）。`~/.cargo/registry` を読み取り専用で入れ、`--network none` と `--offline --locked` で回す。M3 で依存を足した後は、次の行を先に済ませ、承認を得て開発用の Mac で `cargo fetch --locked` を一度回してから使う | image の取得と `cargo fetch` |
| Cargo.lock に依存を足す（M3 の seccompiler） | D4 = A・B では CI の手で起こす job（ネットワークあり、`--locked` なし）。D4 = C ではメンテナーの別の機械 | `polaris-sandbox/Cargo.toml` の Linux の依存に `seccompiler = "=0.5.0"` を足したあと、`cargo fetch` を 1 回回し、Cargo.lock を artifact で受け取る。cargo は既にある版を保ち、足りないものだけを足す。差分が seccompiler の 1 件と checksum だけであること（推測。依存は libc だけのはず）を人が確かめてからコミットする。それ以外が変われば取り込まない | 依存の追加（§9）と同じ承認 |
| Landlock・seccomp・user namespace の本物のカーネルでの試験 | x86_64：CI の VM（Ubuntu のカーネル）と、メンテナーの Linux 機（アーキテクチャは M6 で記録する）。aarch64：次の行で Landlock が使えると分かったときの開発用の Mac の Docker か、D4 = B の arm64 の runner。どちらもなければ、aarch64 の kernel の試験の場所はない | `kernel.apparmor_restrict_unprivileged_userns=0` にした回とそのままの回で、完全と縮小の両方の階層を試す。`dev.tty.legacy_tiocsti=1` にした回も作り、TIOCSTI の対照が成功する場面を必ず作る。V2 の kernel の道は、試験用の差し込み口で V2 の ruleset を選び、新しい kernel の上で試す | CI の中だけの設定 |
| Docker Desktop の linuxkit のカーネル（`7.0.14-linuxkit`） | 開発用の Mac（実測の後） | 次の 3 つを確かめていない。Landlock が有効な LSM に入っているか。Docker の既定の seccomp が `landlock_*` の呼び出しを通すか。`--security-opt seccomp=unconfined` で、user namespace の中の mount ができるか。確かめるまでは、ここでの kernel の試験を閉じ込めの証拠に数えない | — |
| Windows の型の確認 | 開発用の Mac の mingw で `cargo check --target x86_64-pc-windows-gnu`（実測の後） | `rustup target add`。`aws-lc-sys` には NASM か cmake が要るかもしれない（推測）。通らなければ CI で行う | target の追加 |
| Windows の build と試験 | CI の `windows-2025`（MSVC）、またはメンテナーの Windows PC | — | push（D4） |
| WSL2 の受け入れ | メンテナーの Windows PC | WSL2 の Ubuntu 24.04 | メンテナーの操作 |
| macOS が変わっていないこと | 開発用の Mac（実測の後） | `scripts/verify.sh` | — |

- Wine は、Windows 版が起動するかを軽く見る程度にしか使えない。NT の ACL・reparse・`LockFileEx` の証拠にはならない。

### 4-3. CI（案。push にはメンテナーの承認が要る）
- ファイルは `.github/workflows/platforms.yml`（新）である。
- 起動は、作業の枝への push と、手で起こす `workflow_dispatch` にする。
- job は `ubuntu-24.04` の x86_64 と `windows-2025` の 2 つにする。arm64 の Linux の runner が使えるリポジトリなら（D4 = B）、3 つ目に足す。それぞれ次を回す。
  - `cargo fmt --all -- --check`（1 つの job だけ）
  - `cargo clippy --locked --workspace --exclude polaris-desktop-service --all-targets -- -D warnings`
  - `cargo test --locked --workspace --exclude polaris-desktop-service --no-fail-fast`
- デスクトップのサービスを外すのは、対象外の GUI の経路だからである。`scripts/verify.sh:58-65` の既知の失敗 6 件も、すべてそこにある。
- Linux の job では、本物のカーネルの試験（適合の試験、`polaris-cli/tests/linux_signal_cleanup.rs`）も回す。`sudo sysctl` で `kernel.apparmor_restrict_unprivileged_userns=0`（完全な階層）にした回と、`dev.tty.legacy_tiocsti=1`（TIOCSTI の対照）にした回を、それぞれ作る（§4-2）。
- AC8 の kernel の項のために、`ubuntu-22.04` の x86_64 で、tar.gz の `polaris doctor` だけを回す job を置く。
- 依存を足すときだけ、手で起こす lock の job を使う（§4-2）。この job だけは `--locked` を付けずにネットワークから取り、Cargo.lock を artifact で返す。コミットはしない。
- 秘密は使わない。macOS は CI に入れず、開発用の Mac の `scripts/verify.sh` で確かめる。

### 4-4. 配る物
- Linux は `polaris-<版>-<arch>-unknown-linux-gnu.tar.gz` を作る。中身は `polaris`、`LICENSE`、`NOTICE`、README、AppArmor の profile の見本（D5）である。あわせて `SHA256SUMS` を置く。
  - 作る環境は glibc 2.35（Ubuntu 22.04）にする。`scripts/ci/linux.Dockerfile` は、`ubuntu:22.04` に rustup で 1.96.0 を入れる形にする。公式の `rust:1.96` の image は Debian を元にしており、glibc が 2.36 以上なので（推測）使わない。x86_64 は CI でこの image を使って作り、aarch64 は実測の後の開発用の Mac の Docker で同じ image を使って作る（D4 = B なら arm64 の runner）。動くのは glibc 2.35 以上の機械である（推測）。RHEL 9（glibc 2.34）は外れる。
  - aarch64 の tar.gz は、aarch64 の kernel で適合の試験（AC2〜AC6）が通ってから置く（§4-2）。
  - 署名はしない（macOS の rc と同じ）。
  - `polaris-git-askpass` は入れない。使い道の Git の同期の認証が、Linux にはないためである。
  - AppArmor の profile の見本は、root が持ち、利用者が書けないパス（`/usr/local/bin/polaris` など）だけを対象にする。profile は実行ファイルのパスに効くので、利用者が書けるパス（`~/bin` など）を書くと、そこに置かれた任意のものに、権限なしの user namespace を許すことになる。doctor は、実行ファイルのパスの持ち主と書き込みの権限を確かめてから、この見本を案内する。
- 第三者のライセンスの同梱は、macOS の配布物と同じ判断に従う。`seccompiler` の BSD-3-Clause では、配る実行物に著作権の表示が要る。
- Windows は、D1 = A では配らない。WSL2 の利用者は Linux の tar.gz を使う。

## 5. 受け入れ基準（どれも、壊れた実装や何もしない実装で落ちる形にする）
- **AC1（build）**：Linux の x86_64（CI）と aarch64（実測の後の開発用の Mac の Docker か、D4 = B の arm64 の runner）で、§4-3 の fmt・clippy・test が通る（Windows は AC12）。実測の後、macOS の `scripts/verify.sh` が rc/v0.3.0 と同じ結果になる。
- **AC2（書き込み、P1）**：本物のカーネルで、次が成り立つ。根の規則を外す変異と、seccomp の切り詰めとメタデータの規則を外す変異で落ちる。
  - 根の中の書き込みは成功し（対照）、根の外は拒否され、`/dev/null` への `>` は成功する。
  - 根の外のファイルへの `truncate(2)` と、access mode が 0 と 3 の `O_TRUNC` の open が失敗し、中身が変わらない。根の中では成功する（対照）。V3 以上の kernel の道と、試験用の差し込み口で選ぶ V2 の道の両方で確かめる。
  - 根の外のファイルへの `chmod`、読み取りで開いた記述子からの `fchmod`、`setfacl`（入っていれば）、`touch -d` が失敗し、mode・ACL・時刻が変わらない。同じ操作を閉じ込めずに動かすと成功する（対照）。D6 = A の間は、根の中でも失敗することを確かめ、文書の記述と合わせる。時刻の引数が NULL の `utimensat`（`touch` の既定）は成功する。6.6 以上の kernel では `fchmodat2` も失敗し、filter が知る最大の番号より大きい番号の呼び出しは `ENOSYS` を返す。
- **AC3（ネットワークとパスを持たない物）**：
  - 親が 127.0.0.1 に開いた TCP の待ち受けと、親が作った Unix socket の待ち受けへ、閉じ込めた子から接続すると失敗する。同じ子を `full-access` で動かすと成功する（対照）。
  - `socketpair` は成功し（対照）、`io_uring_setup` は失敗する。
  - 閉じ込めた子の `keyctl(KEYCTL_GET_KEYRING_ID, KEY_SPEC_SESSION_KEYRING, 0)`・`shmget`・`mq_open`・`unshare(CLONE_NEWUSER)` が失敗し、閉じ込めない子では成功する（対照）。対照が成り立たない機械（Docker の既定の seccomp の下の `unshare` など）では、その項を「該当なし」と記録する。
  - seccomp の規則を外す変異で落ちる。
- **AC4（端末）**：
  - PTY の中で、閉じ込めた子の `ioctl(TIOCSTI)` が失敗する。同じ PTY で閉じ込めない子は成功する（対照）。カーネルが TIOCSTI を全体で止めていて対照が失敗するときは「該当なし」と記録し、通ったとは数えない。
  - 上位の bit を立てた request（`TIOCSTI | 1 << 32`）の ioctl も失敗する。CI の `dev.tty.legacy_tiocsti=1` の回（§4-3）で、対照が成功する場面を必ず作る。
  - 閉じ込めた子は、TUI の端末（`/dev/pts/N`）も `/dev/tty` も読み取りで開けない。閉じ込めない子は開ける（対照）。
- **AC5（隔離境界）**：
  - 試験用の偽の MCP stdio サーバーが、登録した `read-only` の profile で、Linux でも起動して応答する。
  - 読める根の外は読めず、根の中は読める（対照）。`/proc/<親>/environ` は読めない。
  - `loopback` の profile のサーバーは起動せず、理由を返す。
  - no-fork の封じ込めでは `fork` が失敗し、スレッドは作れる（対照）。
  - `full-access` の profile で登録した偽のサーバーも、読める根の外を読めない（対照：根の中は読める）。親が根の外に作った Unix socket の待ち受けにも接続できない（対照：IP の 127.0.0.1 には接続できる）。隔離方針が FullAccess で何もせずに戻る変異と、FullAccess で `AF_UNIX` の拒否も外す変異で落ちる。
  - 他のプロセスの `/proc/<pid>/cmdline` が読めない。読めるときは、報告にそう出ていることを確かめる。
- **AC6（保証の報告）**：
  - 試験用の差し込み口で Landlock を「なし」にすると、起動時の通知・`/status`・`polaris doctor` に、欠けた保証と理由が出る。write・edit・bash はモデルに渡らない。
  - それでも呼ばれた場合は、子を 0 回起動して拒否する。
  - broker の環境変数があるとき、P1 の方針の子は broker に渡らず、`NotEnforced` で 0 回起動になる（macOS 設計 M2 の試験と同じ形）。
  - 報告を常に「守れる」に固定する変異で落ちる。
- **AC7（周辺）**：書き込める根を PATH の先頭に入れ、そこに偽の `xdg-open`・`gio`・`xdg-mime` を置いても、ログインはどれも起動しない（偽物は起動されると印のファイルを作る）。子の TMPDIR はセッション専用の 0700 のディレクトリで、`rustc` の一時ファイルがそこに作られる。
- **AC8（配る物）**：
  - 利用者空間：tar.gz を `ubuntu:22.04`・`ubuntu:24.04`・`debian:12` のコンテナで展開すると、`polaris --version` が動き、`polaris doctor` が最後まで動いて報告を出す（報告の中身は host の kernel のものである）。x86_64 は CI、aarch64 は実測の後の開発用の Mac の Docker で確かめる。`SHA256SUMS` が一致する。
  - kernel：名前を挙げた機械で、`polaris doctor` の報告が、その kernel の Landlock の ABI と user namespace の実際と一致する。対象は、CI の `ubuntu-22.04` と `ubuntu-24.04` の runner（x86_64）、メンテナーの Linux 機（M6）、メンテナーの Windows PC の WSL2（AC11）である。aarch64 の kernel は、確かめる場所（§4-2）があるときだけ対象にする。
- **AC9（P2、完全な階層）**：
  - macOS 設計の M2・M3・M6 の受け入れの項目を、同じ試験の本体で Linux でも通す。対象は `cat .env`、`mv .env x; cat x`、`ln .env a`、`.env` がない状態での `cp .env.example .env`（対照）、`.polaris/config.toml` と `.git/hooks` への書き込み、承認した外への書き込みが 1 回だけ通ること、2 回目はまた聞くことである。Linux では、`.env` がある間は上書きも拒否される（覆いが読み取り専用のため）ので、対照は `.env` がない状態で取る。
  - §3-9 の Linux と WSL2 の行の見本のパス（`~/.mozilla/firefox/x/logins.json`、`~/.config/google-chrome/Default/Login Data`、`~/.local/share/keyrings/login.keyring`、`/mnt/c/Users/u/AppData/Local/Google/Chrome/User Data/Default/Network/Cookies` など）を、閉じ込めた子が読めない。同じパスを macOS 設計 M1 の段階の表の試験にも足す。
  - `.git` はあるが `.polaris/` も `.git/hooks` もない repository で、`mkdir -p .polaris && echo x > .polaris/config.toml` と `mkdir -p .git/hooks && echo x > .git/hooks/pre-commit` が失敗し、`mv .polaris x` と `mv .git x` も失敗する。`git add`・`git commit` は動く（対照）。D7 = A では、`.git` も `.polaris` もない根で、bash がモデルに渡らず理由が出る。
  - コマンドの `extra_write_paths` に、まだないファイルを指定すると、確認の前に拒否される。既にあるディレクトリを指定すると、確認画面が「この下のどのファイルも変えられる」と言う。
  - setsid で逃げた子が、元のコマンドの終了の後に残らない。
  - 覆いを外す変異で `cat .env` の試験が落ちる。
- **AC10（P2、縮小の階層）**：user namespace を止めた機械で、bash がモデルに渡らず、起動時の通知と `/status` に理由が出る。write ツールは閉じ込めて動く（対照）。bash を渡したままにする変異で落ちる。
- **AC11（WSL2）**：メンテナーの Windows PC の WSL2 で AC2〜AC5 が通る（P2 の後は AC9 も）。閉じ込めた子から `cmd.exe` を起動できない。`uname -r` と Landlock の ABI を記録する。
- **AC12（Windows の中間版）**：
  - Windows の CI で、build と clippy が通る。本番の経路の試験はすべて通る。
  - 本番の経路にない NT の部品の試験で落ちるものは、名前と原因を既知の失敗の一覧（`scripts/verify.sh` と同じ形）に載せる。一覧にない失敗は CI を落とす。
  - 引数のない起動は閲覧の TUI に入る。`polaris -p x` などは `No changes were made` と WSL2 の案内で終わる（`polaris-cli/tests/windows_execution.rs` に足して確かめる）。
- **AC13（文書）**：挙動を変えるコミットで、docs（`usage.md`・`command-reference.md` の Linux と WSL2 の節、OS ごとの使えない機能の表、保証の表）と CHANGELOG を直す。

## 6. 段取り
大きさの目安：S は 1〜2 コミットの配線か小さな修正。M は新しい部品 1 つと、その本物のカーネルでの試験。L は新しい仕組みと、複数の crate の配線と、実機での測定。XL は複数の crate にまたがる移植。

### 6-1. 本線（D1 = A）
| # | 内容 | 大きさ | 依存 | 担当するパス | 確認（門） |
|---|---|---|---|---|---|
| M0 | 検証の置き場と CI | S | D4、push の承認 | `.github/workflows/platforms.yml`（新。手で起こす lock の job を含む）、`scripts/ci/linux.Dockerfile`（新。`ubuntu:22.04` に rustup で 1.96.0 を入れる。aarch64 を開発用の Mac で回すとき、配る物を作るとき、D4 = C のとき）、`docs/testing.md` | 作業の枝で、どの job も最後まで回り、結果（失敗を含む）が記録される。macOS で compile されるコードは変えない |
| M1 | Linux の基準線と実測 | M（ずれが大きければ L） | M0（aarch64 は、実測の後の開発用の Mac の Docker か、D4 = B の arm64 の runner） | Linux だけで compile される枝と、Linux だけの試験。Unix 共通のコードを直したときは、macOS の試験も回してから統合する。`polaris-sandbox/tests/linux_facts.rs`（新）。`polaris-tui/src/lib.rs`（Linux の鍵の読み手の秘密の入力、Linux だけの枝の仕分け）、`polaris-tui/src/resume_effect.rs`、`polaris-cli/src/decided.rs`（Linux の date） | 門は 2 つに分ける。x86_64 の門は、CI での AC1 と、下の F-L1〜F-L8 の結果の表を `linux.rs` の注釈と試験の頭に残すことで、すぐ閉じられる（F-L9 は M6）。aarch64 の門は AC1 で、確かめる場所ができたときに閉じる。遅れても、M3 以降の x86_64 の作業は止めない。x86_64 の門には、さらに 2 つを含める。Linux の鍵の読み手で、Ctrl-C が秘密の入力を取り消し、貼り付けが入る試験と、Linux だけの枝を OS の差し込み口（残す）と製品の論理（macOS の道を基準に、共通の道へ寄せるか消す）に分けた表である。製品の論理の枝は M3 より前に片づける |
| M2 | Windows の中間版 | S | M0。`main.rs` と `polaris-tui/src/lib.rs` の数行は、M1・M3 より先に入れる（下の注） | `polaris-tui/src/auth_browser.rs`、`polaris-core/src/project_git.rs`、`polaris-tui/src/lib.rs`（門の数行）、`polaris-cli/examples/cache_quality.rs`、`polaris-cli/src/main.rs`（拒否の文） | AC12 |
| M3 | Linux の閉じ込めを今の macOS にそろえる（P1） | M | M1。最初に Cargo.lock の段（§4-2） | `polaris-sandbox/src/linux.rs`（読みの規則、V3 以上の TRUNCATE）、`linux/seccomp.rs`（新。ネットワーク、端末、切り詰め、メタデータ（D6）、鍵束と IPC、namespace、x32。x86_64 と aarch64 で別）、`confine.rs` の Linux の枝と broker の拒否、`controlled.rs` の Linux の起動、`child_environment.rs`（子の TMPDIR）、`guarantees.rs`（新）、`lib.rs`、`polaris-sandbox/Cargo.toml`（`seccompiler`）、`polaris-sandbox/tests/conformance.rs`（新）、`polaris-cli/src/main.rs`（TMPDIR・doctor・起動時の通知）、`polaris-core/src/agent.rs`（道具の一覧を作る 1 か所。保証が足りない道具を渡さない）、`polaris-core/src/common_tests.rs`（`network_is_enforced` を報告から作る）、`polaris-tools/src/bash.rs`（Linux の `/bin/bash`）、`polaris-tui/src/status_view.rs` | AC2・AC3・AC4・AC6、AC7 の TMPDIR の項 |
| M4 | Linux の隔離境界（MCP stdio・隔離実行） | M | M3 | `polaris-sandbox/src/linux/isolated.rs`（新。FullAccess でもネットワーク以外の隔離を残す）、`linux.rs:32-34`（隔離方針は早期に戻さない）、`confine.rs:252-256`（拒否を外す）、`policy.rs:581-584`（macOS だけの門を外す）、`polaris-mcp/src/admission.rs`、実行環境の根を返す OS の層の関数（§3-3 の 6 か所をそれに替える）、その試験。呼び手のない `polaris-core/src/sandbox_command.rs`（宣言は `polaris-core/src/lib.rs:119` だけ）は担当に入れない | AC5 |
| M5 | Linux の周辺・配る物・WSL2 の手順 | M | M3、M4 | `polaris-auth/src/login.rs`（環境を消し、PATH を固定した xdg-open）、`scripts/assemble-linux-tarball.py`（新）とその試験、`packaging/linux/apparmor/polaris`（新。root が持つパスだけを対象にする）、`docs/usage.md`・`docs/command-reference.md`・`docs/releases/`、`CHANGELOG.md` | AC7・AC8・AC13 |
| M6 | 実機での受け入れ（P1） | S | M5 | 記録だけ（§0-5 の表） | AC11 の P1 の項と F-L9。macOS 設計 M11 と同じ形で、tmux で一通り操作し、画面の文字を残す。メンテナーの Linux 機では `polaris-core/tests/linux_filesystem_acceptance.rs` も回し、機械のアーキテクチャと kernel を記録する |
| M7 | P2 を Linux へ（秘密の拒否・保護・一回だけの拡張・自己診断） | L | M6。macOS 設計の承認と、その M1・M2（方針の値の部分）・M3・M4・M5・M6・M7・M8 | `polaris-sandbox/src/linux/launch.rs`（新。読み取り専用の眺め、`.polaris` と `.git` の保護、まだない保護のパス（D7）、覆い）、`linux.rs`、`linux/seccomp.rs`（完全な階層ではメタデータの拒否を外す）、`guarantees.rs`、`tests/conformance.rs`（P2 の項目）、`polaris-sandbox/src/secret.rs`（macOS 設計 M1 の表に Linux と WSL2 の行を足す）、`polaris-cli/src/main.rs`（起動補助のモード）、`polaris-tools/src/read.rs`（Linux の記述子のパス） | AC9・AC10、AC11 の P2 の項 |

- 重なるファイルと順序：`polaris-cli/src/main.rs` は M1・M2・M3・M7 が、`polaris-tui/src/lib.rs` は M1 と M2 が触る。M2 の数行（`lib.rs` の門と `main.rs` の拒否の文）を最初に入れ、M1 と M3 はその上に積む。M1 → M3 → M7 は、依存のとおり順に進む。並行してよいのは、この 2 つのファイルに触れない作業だけである。`CHANGELOG.md` と `docs/` はどの段も触る（AC13）ので、統合の時に順に rebase する。M5 の文書は M4 のコードと並行して書いてよいが、入れるのは M4 の後である。
- macOS 設計の M4（承認の profile）と M7（コマンドの追加アクセス）ができていなければ、Linux の P2 ではコマンドの拡張を出さない。
- M1 で測ること（F-L9 だけは M6 で測る）：
  - F-L1：CI とメンテナーの機械のカーネルの Landlock ABI。`FullyEnforced` になるか。
  - F-L2：Docker の既定の seccomp の下で、子が seccomp の filter を入れられるか。
  - F-L3：権限なしで user・mount の namespace を作り、mount できるか（Ubuntu 24.04 の既定、CI、Docker Desktop）。
  - F-L4：覆った点と、それ自身の上に重ねた `.git`・`.polaris` の rename・削除・link の結果（`EBUSY`・`EXDEV` になるか）。mode 000 の覆いに対する `cat`・`git add`・`git status` の結果（追跡されている fixture を覆ったときを含む）。子が覆いを外せないこと。
  - F-L5：Landlock の下で `/proc/<親>/environ` が読めないこと。
  - F-L6：閉じ込めた子の `ioctl(TIOCSTI)` と、端末の装置の読み取り。
  - F-L7：秘密の名前の走査にかかる時間（このリポジトリと、大きな `node_modules` の見本）。
  - F-L8：ネットワークを止めた sandbox の中の日常の作業で、何が壊れるか。対象は `cargo build`・`cargo test`、`git status`・`git diff`・`git commit`、offline の `npm test`、`python -m pytest` である。D6 = A の seccomp の下で、`chmod +x`、`cp -p`、`python3 -m venv`、`cc` でのリンク（GNU ld が出力に実行の bit を付ける処理、推測）、`touch` も測る。
  - F-L9：WSL2 の ext4 と `/mnt/c` の上の Landlock と、Windows のプログラムの起動の拒否（M6 でメンテナーの PC を使う。M1 の門には入れない）。
- 戻し方：各段を 1 コミット（多くても数コミット）にする。問題があれば、その段のコミットを戻す。

### 6-2. D1 で B か C を選んだときに足す段
| # | 内容 | 大きさ | 依存 |
|---|---|---|---|
| N1 | 土台の部品と認証情報（保護 DACL の root、プロファイルのパスの API、ログインのブラウザ） | L | M2 |
| N2 | 耐久の部品の Windows 版（会話の保存と切り替え、管理の効果の記録、プロジェクトの変更の鍵、合成の入場の確認、巻き戻し、MCP の保存）。2026-09-27 の Windows の仕様（非公開のノート。D1 で B か C を選ぶときにメンテナーに頼む）の W3 の約束（private state、パスの結び付け、no-clobber、flush、排他と回復）を引き継ぐ | XL | N1 |
| N3 | 書き込みの閉じ込め（restricted token、根ごとの capability SID、継承する ACE と起動時の残りの回収、job object、handle の一覧）。`windows-sys` に `Win32_System_JobObjects` などの feature を足す | L | N1 |
| N4 | TUI の Unix だけの部品（`command_dispatch` など、`polaris-tui/src/lib.rs:4-53`）を Windows でも動かす。Windows のパスの規則と、Git for Windows の場所 | L | N2、N3 |
| N5 | 実機での受け入れ（メンテナーの Windows PC） | M | N4 |
| N6（C だけ） | 専用のユーザーと WFP の規則（管理者による一度の設定）で、コマンドと MCP stdio を動かす。コンソールへの入力の差し込み（`WriteConsoleInput`）を測ってから開く | XL | N5 |

## 7. 危うさ
| # | 危うさ | 重さ | 手当て |
|---|---|---|---|
| R1 | 2026-10-19 にソースの木を復元した後、macOS だけで 537 ファイルが変わった（+94,631／−39,828）。Linux だけの約 1.1 万行と Windows だけの約 1.4 万行は、型の確認を受けていないので、ずれがあるはずである（推測） | 高（量が読めない） | M1 と M2 を先に行う。CI で毎回確かめる |
| R2 | Ubuntu 23.10 以降の AppArmor、Docker の既定の seccomp、GitHub の runner（推測）では、user namespace で mount できない。P2 では縮小の階層になり、コマンドが止まる | 中（P2 から） | D2・D5。doctor が直し方を出す。CI で両方の階層を試す |
| R3 | Docker Desktop のカーネルで Landlock が有効か分からない。コンテナの試験だけでは、閉じ込めの証拠にならない | 中 | 本物のカーネルの試験は、CI の VM とメンテナーの機械で行う |
| R4 | ネットワークの拒否で、sandbox の中の Unix socket の待ち受け（Python の forkserver、言語サーバー、一部の試験の道具）が動かない | 中 | macOS も同じはずである（推測）。F-L8 と macOS 設計 M0 の F15 で測り、macOS 設計の D7 と一緒に決める |
| R5 | 覆いは子の起動の時点の写しなので、後からできた秘密の名前は覆わない。大きな木では走査が遅い | 中 | 文書に書く。上限を超えたら止める。F-L7 で測ってから、ディレクトリの更新時刻を使う差分の走査を考える |
| R6 | 宛先つきの `sendmsg`・`sendmmsg` で、Unix の datagram socket（abstract の名前のものを含む）へ書ける | 低 | ABI V6 と V9 があれば塞ぐ。文書に書く |
| R7 | macOS の閉じ込めた子の端末の装置の読み取りは、まだ測っていない。Linux は M3 で塞ぐ | 中 | macOS 設計 M0 の F13 に、その測定を足すよう頼む |
| R8 | WSL2 の Windows との連携と、`/mnt/c` の上の Landlock は測っていない | 中 | F-L9。測るまでは Linux 側のディレクトリを案内する |
| R9 | 動く条件が、glibc 2.35 以上、カーネル 5.19 以上、Landlock が有効、になる。RHEL 9、Ubuntu 22.04 の GA カーネル、更新していない WSL2（5.15 系の kernel）は外れる | 中 | D3。doctor と文書で条件を示し、WSL2 では `wsl --update` を案内する |
| R10 | C の依存（`aws-lc-sys` の cmake、Windows では NASM の可能性、`ring`、`libsqlite3-sys`）が、クロスの build を難しくする | 中 | Linux は target ごとのネイティブの image か runner で作る。Windows は CI の MSVC で作る |
| R11 | 並行する macOS の作業（macOS 設計と、`rc/v0.3.0` での別の作業）と、`confine.rs`・`policy.rs`・`polaris-cli/src/main.rs`・`polaris-tui/src/lib.rs` で衝突する | 中 | 新しいファイルに寄せる（`linux/*.rs`、`guarantees.rs`、`conformance.rs`）。共通のファイルの変更は門の数行に留め、統合の前に rebase する |
| R12 | Windows だけの試験 174 個は長く回っていない。本番の経路にない NT の部品の試験が落ちうる | 低（A のとき） | M2 では、本番の拒否の試験を必ず通す。それ以外の失敗は原因を記録し、局所で直せるものだけ直す。D1 = B を選ぶときの材料にする |
| R13 | D1 = A では、ネイティブの Windows を期待する利用者が困る | 中 | 拒否の文と文書で WSL2 を案内する。B は別の仕様で見積もる |
| R14 | 非公開のリポジトリでは、arm64 の Linux の runner が無料で使えないかもしれない（推測）。開発用の Mac の Docker の kernel で Landlock が使えるかも、確かめていない | 中 | aarch64 の build と kernel に依らない試験は、実測の後に開発用の Mac の Docker で行う。aarch64 の kernel の試験の場所ができるまで、aarch64 は配らない（D4・D5） |
| R15 | P2 の Linux の覆いは場所が限られる。プロジェクトの根・書ける根・ホームの決まった場所の外にある秘密の名前（別のプロジェクトの `.env` など）は、完全な階層でもコマンドから読める。macOS は場所を問わず拒否する | 中 | 文書と `/status` に書く。read ツールは場所を問わず守る。覆う場所を足すかは、F-L7 の走査の時間を見てから決める |
| R16 | 共通のコードの、PATH から実行ファイルを探す起動の扱いが決まっていない | 中 | 共通の規則（決めた場所から探す）として macOS 設計に渡す。Linux の date は §3-3 で `/bin/date` にする |
| R17 | D6 = A の seccomp は、根の中でも `chmod +x`・`cp -p`・Python の `shutil.copy` と `python3 -m venv`・GNU ld の実行の bit（推測）などを壊す | 中 | F-L8 で測る。日常の作業が壊れるなら、D6 を B に切り替える。拒否の文に、sandbox が止めたことを出す |
| R18 | D7 = A では、P2 の完全な階層で、`.git` も `.polaris` もない根のコマンドが止まる。作業木には空の項目（`.polaris/config.toml` など）ができる | 中（P2 から） | 文書と起動時の通知で示す。止まる場面が多ければ D7 の C を足す |

## 8. メンテナーに決めてほしいこと

### D1. Windows の形
- A：WSL2 の中で Linux 版を使うのを、Windows の正規の経路にする。ネイティブは今の閲覧版のまま、compile と CI だけを通し、拒否の文で WSL2 を案内する。ネイティブの本格版は、Linux の P1（M6）の後に別の仕様で扱う。
- B：ネイティブで、会話と読み書きのツールを動かす（§6-2 の N1〜N5。XL を含む）。コマンドと MCP stdio はネイティブでは止め、WSL2 を案内する。
- C：B に加えて、管理者による一度の設定（専用のユーザーと WFP）で、ネイティブでもコマンドを動かす（N6）。
- **推奨は A**。核の共有が最も大きい（Linux 版そのものである）。秘密の読み取りの拒否とネットワークの遮断を、管理者の設定なしにネイティブで守る仕組みはない。Claude Code と Cursor も、Windows の sandbox は WSL2 に任せている（付録 A の CC-W・CU）。B の耐久の部品の移植は、2026-09 の Windows の仕様（非公開のノート）の M2 で途中まで進み、復元の後は止まっている。

### D2. P2 で、user namespace が使えない Linux の扱い
- A：コマンドを止める（bash をモデルに渡さない）。read・write・edit・MCP stdio は使える。doctor が直し方（AppArmor の profile か sysctl）を出す。コンテナの中では、外側の隔離を前提に `--sandbox full-access` を選べる（Codex の勧め方と同じ）。
- B：利用者が自分の設定ファイルかフラグで「コマンドからの秘密の OS 拒否なし」を明示したときだけ、書き込みの閉じ込めとネットワークの禁止を付けてコマンドを動かす。プロジェクトの設定からは選べず、`/status` に大文字で出す。
- C：黙って弱めて動かす。依頼の「.env は読み込み不可」が成り立たないので勧めない。
- **推奨は A**。新しい設定を足さずに、macOS 設計の D6 の床（`.env` は読めない）をそのまま守れる。B は、実際に困る人が出てから足しても遅くない。

### D3. Linux の最低のカーネル
- A：今の床（Landlock ABI V2、Linux 5.19）を保つ。V2 と V3 の差（切り詰めの権利 TRUNCATE）は、V2 の kernel（5.19〜6.1。Debian 12 の 6.1 など）でだけ seccomp で埋める（§3-6）。そこで失うのは、根の中でもパスを指定した `truncate(2)` が使えないことだけである。Ubuntu 22.04 の GA カーネル（5.15）と 20.04 では閉じ込める実行を止め、HWE カーネルを案内する。
- B：V1（5.13）まで下げる。sandbox の中では、ディレクトリをまたぐ rename と link が常に失敗する（`linux.rs:13-17` に書かれた理由）。
- C：古いカーネルのために、mount namespace で書き込みを閉じ込める 2 つ目の仕組みを足す。
- D：床を V3（6.2）へ上げる。seccomp で埋める道は要らなくなるが、Debian 12（6.1）では閉じ込める実行を止め、backports のカーネルを案内することになる。
- **推奨は A**。閉じ込めの仕組みを 1 つに保てる。Ubuntu 22.04 も HWE カーネル（6.8）なら使える。V2 の道は、試験用の差し込み口で V2 の ruleset を選べば、新しい kernel の CI でも試せる。

### D4. 実測の間の検証の場所と CI
- A：GitHub Actions を、非公開の開発用のリポジトリに置く（どのリポジトリかも決めてほしい）。workflow のファイルの追加と作業の枝の push には、そのつど承認が要る。x86_64 は CI で確かめる。aarch64 は、実測の後に開発用の Mac の Docker で、build と kernel に依らない試験をする。開発用の Mac の Docker の kernel で Landlock が使えなければ、aarch64 の kernel の試験の場所はなく、aarch64 は配らない。依存を足すときの Cargo.lock は、手で起こす CI の job で作る（§4-2）。
- B：公開のリポジトリ（`Altena-Works/polaris`）に置く。arm64 と Windows の runner を無料で使える（推測）ので、aarch64 の kernel の試験も CI でできる。ただし、公開の push の判断と結びつく。
- C：CI を置かない。メンテナーの Windows PC と Linux 機、実測の後の開発用の Mac を使う。実測の間は compile を確かめられず、Cargo.lock もメンテナーの別の機械で作る。
- **推奨は A**。依頼の「実測中に並行して」を、開発用の Mac に負荷をかけずに満たせる。aarch64 の kernel の試験の場所が要るなら B を選ぶ。

### D5. Linux の配り方
- A：GitHub Releases に、アーキテクチャごとの tar.gz と `SHA256SUMS` を置く。署名はしない。AppArmor の profile の見本を同梱し、入れるかは利用者が決める。見本は、root が持つパス（`/usr/local/bin/polaris` など）だけを対象にする（§4-4）。aarch64 は、aarch64 の kernel で適合の試験が通ってから置く。
- B：A に加えて `.deb` を作り、AppArmor の profile を入れる（対象は `/usr/bin/polaris`）。Ubuntu 24.04 でも、そのまま P2 の完全な階層になる。D6 = A の根の中のメタデータの不便（R17）も、P2 では消える。
- C：公開しない。CI の成果物だけにする。
- **推奨は A**。公開の時期は、別の公開の判断に従う。B は、D2 で困る人が多ければ足す。

### D6. Linux で、根の外の権限・所有者・xattr・時刻の変更をどう止めるか
Landlock はどの ABI でも chmod・chown・setxattr・utime を扱わず、seccomp はパスを見られない。根の外だけを止めるには mount namespace の読み取り専用の眺めが要り、それは P2 の完全な階層の起動補助で張る（§3-6）。決めるのは、P1 と縮小の階層の扱いである。macOS は、根の外ではこれらを拒否する（推測）。
- A：seccomp で、これらの呼び出しを全体で拒否する（根の中でも失敗する）。時刻を今にする `utimensat`（時刻の引数が NULL）だけは許すので、根の外のファイルの時刻を今にすることだけはできる。P2 の完全な階層では、この拒否を外して読み取り専用の眺めに替える。
- B：P1 から、完全な階層では起動補助の読み取り専用の眺めを入れる（M7 の起動補助の一部を M3 へ前倒しする）。縮小の階層は A と同じにする。
- C：止めずに、守れない保証として告げる（報告・`/status`・doctor・文書）。macOS より弱いので、D2 の B と同じく、利用者の設定ファイルかフラグで明示したときだけコマンドを動かす。明示がなければ、P1 でもコマンドを止める。
- **推奨は A**。新しい仕組みを足さずに、P1 から、時刻を今にすることを除いて macOS と同じ保証を守れる（その残りは報告に出す）。代わりに、sandbox の中では `chmod +x`、`cp -p`、Python の `shutil.copy` と `python3 -m venv`、GNU ld が出力に実行の bit を付ける処理（推測）が失敗しうる。F-L8 でこれらを測り、日常の作業が壊れると分かれば B に切り替える（R17）。B に切り替えても、Ubuntu 24.04 の既定（縮小の階層）では A のままである。

### D7. P2 の Linux で、まだない保護のパスをどう守るか
読み取り専用の bind は既にあるものにしか重ねられないが、macOS の literal の拒否は、パスがまだなくても効く（§3-6）。`.polaris/` のないプロジェクトや `.git/hooks` のない repository で、コマンドに `.polaris/config.toml` や `.git/hooks/pre-commit` を作らせないには、次のどれかが要る。
- A：namespace に入る前に、polaris がまだない保護のパスを空で作ってから、読み取り専用で重ねる（§3-6 の補足）。根に `.git` も `.polaris` もないときは作らず、コマンドを止めて理由と直し方を出す。作ると、そこより下から後で起動したときのプロジェクトの根が変わり、上のプロジェクトの設定が効かなくなるか、macOS 設計の D1 = B では書ける根が広がるためである。
- B：根に `.git` も `.polaris` もないときも作る。上のとおり、後の起動が黙って弱くなりうるので勧めない。
- C：A と同じだが、根に `.git` も `.polaris` もないときは止めずに、コマンドの後（PID namespace で中の全員が止まった後）に、コマンドが作った `.git` と `.polaris` を polaris が消し、そう告げる。コマンドの最中は守れず、リンクをたどらない再帰の削除が要る。
- **推奨は A**。新しい仕組みを足さずに、macOS と同じものを守れる。副作用は 2 つあり、文書と起動時の通知で示す（R18）。1 つは、作業木に空の項目ができることである（git に見えるのは空の `.polaris/config.toml` だけのはず、推測）。もう 1 つは、`.git` も `.polaris` もないディレクトリから起動すると、コマンドが止まることである。止まる場面が多ければ C を足す。

## 9. この仕様での判断
- Linux でもデータの置き場は `~/.polaris` にする（XDG の場所には分けない）。秘密の表とパスの組み立てを 1 つに保つためである。
- seccomp のために `seccompiler` 0.5.0 を足す（Linux だけの依存）。この仕様の承認は、この依存の追加と、Cargo.lock を作る段（§4-2。ネットワークのある場所で `cargo fetch` を 1 回回す）の承認を含む。Cargo.lock の差分が seccompiler の 1 件を超えたら、取り込まずにもう一度聞く。
- Linux の起動補助は polaris 自身の内部のモードにし、bubblewrap は使わない（§3-6）。
- Codex・sandbox-runtime・Gemini CLI（Apache-2.0）のコードは写さない。考え方だけを借り、カーネルと Win32 と crate の文書から書く。Claude Code のコード（All rights reserved）は使えない。birdcage（GPL-3.0）と C の libseccomp（LGPL-2.1）は使わない。
- P1 では、閉じ込めた子に新しい session（setsid）を作らない。作ると、`run_confined` で動かす子に端末の Ctrl+C（`-p` のとき）が届かなくなり、polaris が終わった後に子が残りうるためである。端末への差し込みは seccomp で止め、端末の装置の読み取りは Landlock の読みの規則で止める。
- 状態は別々に報告する。`linux_p1`、`linux_p2_full`、`linux_p2_reduced`、`wsl2_p1`、`wsl2_p2`、`windows_native_interim` である。ある状態の成功から、別の状態の成功を推し量らない。

## 10. 後回しの指摘

2 回の独立レビューの指摘のうち、本文に入れなかったものである。実装の中で当たったら、ここから拾う。

- 追跡されている秘密らしいファイル（fixture）を覆ったときの `git status` の扱い。F-L4 で測ってから M7 で決める。
- 呼び手のない `polaris-core/src/sandbox_command.rs` を消すこと。この仕様の目的とは別の片づけである。
- 隔離境界で読ませる `/proc`・`/sys`・`/etc` の正確な一覧。実行環境ごとに要るものが違うので、M4 で偽のサーバーを動かして決める。
- §3-6 の xdg-open の試験。
- `.git` がファイルや symlink のときの扱い。
- 下のディレクトリから起動したときの D7 の扱い。
- `commondir` と submodule の hook。
- D6 を A から B に切り替える判定の基準（測る項目は §6-1 の F-L8 にある）。
- Cargo.lock の差分の条件の細部（§4-2）。
- M4 の担当のファイルの、ほかの段との重なり（§6-1）。
- `.gitattributes` を足して改行を LF に固定すること。木はすべて LF である。Windows の CI（M0・M2）の前に決める。

## 11. 付録 A：参考資料

2026-11-04 に調べた結果である。版で変わるので、実装の前に原典で確かめる。

### K-LANDLOCK

- ABI と kernel：V1 5.13、V2 5.19、V3 6.2、V4 6.7、V5 6.10、V6 6.12、V7 6.15、V8 7.0、V9 7.1（landlock crate 0.4.7 の文書）。
- ABI で増える権利：V2 は REFER（ディレクトリをまたぐ link と rename）、V3 は TRUNCATE、V4 は TCP の bind と connect、V5 は IOCTL_DEV、V6 は abstract の Unix socket とシグナルの scope、V9 は FS_RESOLVE_UNIX。
- FS_RESOLVE_UNIX は、ドメインの外で作られたパスの Unix socket への `connect(2)` と宛先つきの `sendmsg(2)` を止める。
- 止めないもの：`chdir`・`stat`・`flock`・`chmod`・`chown`・`setxattr`・`utime`・`fcntl`・`access`。Landlock を掛ける前に開いたファイルと、`/proc/<pid>/fd` から届く pipe と socket も止めない。
- 許した木の中の名前を拒否する規則はない（推測）。WRITE_FILE・READ_FILE・TRUNCATE はファイルの権利で、MAKE_REG・REMOVE_FILE・REFER はディレクトリの中身の権利である。新しいファイルを作るには親のディレクトリの規則が要る（推測）。
- ドメインは子孫へ受け継がれ、外せない。閉じ込めたプロセスが ptrace できるのは、同じドメインの下の相手だけである。
- Ubuntu の kernel：20.04 は GA 5.4・HWE 5.15、22.04 は GA 5.15・HWE 6.8、24.04 は GA 6.8・HWE 6.17 と 7.0。
- 既定で有効な配布：Ubuntu（20.04 LTS）、Fedora（35）、Debian（Sid、5.18.16-1 以降）、Arch、openSUSE、RHEL（9.6.0）、CentOS Stream、Amazon Linux、Alpine、NixOS、WSL2（linux-msft-wsl-5.15.57.1）。
- 出典：[Landlock（kernel の文書）](https://docs.kernel.org/userspace-api/landlock.html)、[landlock crate の ABI](https://docs.rs/landlock/latest/landlock/enum.ABI.html)、[AccessFs](https://docs.rs/landlock/latest/landlock/enum.AccessFs.html)、[Scope](https://docs.rs/landlock/latest/landlock/enum.Scope.html)、[配布の一覧](https://landlock.io/integrations/)、[Ubuntu の kernel の周期](https://ubuntu.com/kernel/lifecycle)。

### K-SECCOMP

- seccomp は sandbox そのものではない。BPF はポインタをたどれないので、パスの引数では絞れない。`PR_SET_NO_NEW_PRIVS` か `CAP_SYS_ADMIN` が要り、子は filter を受け継ぐ。`SECCOMP_RET_USER_NOTIF` は引数の写しを取らず、検査と使用の間の競合を防げない。
- Docker の既定の profile は許可の一覧で、namespace の作成、`mount`・`umount2`・`pivot_root`・`setns`、鍵束の呼び出しを止める。
- 出典：[Seccomp BPF（kernel の文書）](https://docs.kernel.org/userspace-api/seccomp_filter.html)、[Docker の seccomp profile](https://docs.docker.com/engine/security/seccomp/)。

### W-OS

- `CreateRestrictedToken`：`WRITE_RESTRICTED` の restricting SID は書き込みにしか効かない。それ以外の restricted token は、通常の検査と restricting SID の検査の両方を通す必要がある。
- AppContainer：ファイル、ネットワーク、資格情報、装置、プロセス、ウィンドウを隔離する。
- Job object：`CreateProcess` の子は既定で job に入る。`KILL_ON_JOB_CLOSE` は、最後のハンドルが閉じると全員を止める。
- WFP：`ALE_USER_ID` でローカルのユーザーを、`ALE_PACKAGE_ID` で AppContainer の SID を条件にできる。
- DPAPI：同じ資格情報のユーザーは復号できる。Credential Manager の generic credential は 1 件 5×512 バイトまでである。
- `WriteConsoleInput` は、入力のハンドルに `GENERIC_WRITE` があれば入力バッファへ書ける。TUI とコンソールを共有する子は、打鍵を差し込める（推測）。
- 出典：[CreateRestrictedToken](https://learn.microsoft.com/en-us/windows/win32/api/securitybaseapi/nf-securitybaseapi-createrestrictedtoken)、[AppContainer](https://learn.microsoft.com/en-us/windows/win32/secauthz/appcontainer-isolation)、[Job Objects](https://learn.microsoft.com/en-us/windows/win32/procthread/job-objects)、[WFP の条件](https://learn.microsoft.com/en-us/windows/win32/fwp/filtering-condition-identifiers-)、[DPAPI](https://learn.microsoft.com/en-us/windows/win32/api/dpapi/nf-dpapi-cryptprotectdata)、[CREDENTIALW](https://learn.microsoft.com/en-us/windows/win32/api/wincred/ns-wincred-credentialw)。

### CX-W

- Codex の Windows sandbox は、MXC を勧める。古い手段は 2 つある。`elevated` は管理者の設定で、専用のユーザー、ファイルの権限の境界、ファイアウォールの規則を使う。`unelevated` は restricted token と ACL を使い、ネットワークの隔離は弱い。
- `WRITE_RESTRICTED` の token は書き込みにしか restricting SID を見ないので、`unelevated` では読み取りの拒否を守れない。読み取りの制限には `elevated` が要る。
- Everyone が書けるフォルダは、書けるまま残る（推測）。
- 拒否する読み取りの glob は、子を起動する時に解決する。その後にできたファイルは読める。
- 出典：[Codex の Windows sandbox](https://learn.chatgpt.com/docs/windows/windows-sandbox)、[Codex の権限の profile](https://learn.chatgpt.com/codex/permissions)。

### CC-W

- Claude Code はネイティブの Windows ではコマンドを sandbox なしで動かす。Windows で sandbox を使うには WSL2 の中で動かす。WSL1 には対応しない。
- WSL2 の Windows のプログラム（`cmd.exe`、`/mnt/c/` の下の実行ファイル）の起動は、Unix socket で Windows 側へ頼むので、sandbox の Unix socket の設定に従う。
- 出典：[Claude Code の Sandboxing](https://code.claude.com/docs/en/sandboxing)。

### CU

- Cursor の Linux の sandbox は Landlock と seccomp を組み合わせる。Windows では Linux の sandbox を WSL2 の中で動かす。
- 出典：[Cursor の agent sandbox の記事](https://cursor.com/blog/agent-sandboxing)。
