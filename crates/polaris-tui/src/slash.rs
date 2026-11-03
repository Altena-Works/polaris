//! Slash commands: local, client-side commands recognized when the input
//! buffer starts with `/`. Most dispatch to local owners rather than becoming
//! conversation messages. `Review` expands into a normal model turn;
//! input-admission commands can also request model work through their
//! owners. Local configuration and management commands may persist state.
//!
//! Typing opens an autocomplete popup and submission dispatches the selected
//! command. `COMMANDS` owns the visible catalog; `parse` also accepts the
//! documented aliases. Commands with arguments route to the owner of their
//! session, runtime, workspace, account, memory, or MCP state. `/model`
//! switches the model through `Provider::set_model` on the shared provider. The
//! interactive picker deliberately leaves reasoning effort to automatic
//! routing; fixed noninteractive measurements can still use `--effort`.
//!
//! A removed command is not listed in `COMMANDS` but stays parseable
//! (`REMOVED`, `Action::Removed`, `Action::Compact`): its name answers one
//! line that names its replacement, and starts nothing.
//!
//! A command whose handler exists on one platform only carries the same `cfg`
//! on its `COMMANDS` entry, its `Action` variant, and its `parse` arm. Where the
//! handler is absent the command is not listed and is not recognized, so `/help`
//! and the popup never offer a route that is not there.

pub struct SlashCommand {
    pub name: &'static str,
    pub description: &'static str,
}

pub const COMMANDS: &[SlashCommand] = &[
    SlashCommand {
        name: "env",
        description: "plan or provision a fixed development environment",
    },
    #[cfg(target_os = "linux")]
    SlashCommand {
        name: "setting",
        description: "inspect or confirm Git ignore configuration",
    },
    SlashCommand {
        name: "session",
        description: "inspect or configure the saved conversation",
    },
    SlashCommand {
        name: "goal",
        description: "inspect or update the saved goal",
    },
    SlashCommand {
        name: "tool-memory",
        description: "configure tool-result retention and loopback embedding",
    },
    SlashCommand {
        name: "subagents",
        description: "inspect or wait for an exact child attempt",
    },
    SlashCommand {
        name: "help",
        description: "list available commands",
    },
    SlashCommand {
        name: "status",
        description: "show login status, conversation ID, model, and usage",
    },
    SlashCommand {
        name: "doctor",
        description: "inspect local configuration and runtime availability",
    },
    SlashCommand {
        name: "login",
        description: "register, reauthenticate, select, or label a login",
    },
    SlashCommand {
        name: "memory",
        description: "search or inspect saved project memory",
    },
    SlashCommand {
        name: "decided",
        description: "inspect recorded project decisions",
    },
    #[cfg(target_os = "linux")]
    SlashCommand {
        name: "apply",
        description: "run in an isolated copy, inspect changes, and confirm source apply",
    },
    SlashCommand {
        name: "sync",
        description: "receive, leave, inspect, apply, push, or reconcile shared changes",
    },
    SlashCommand {
        name: "phase",
        description: "show or set the explicit workflow phase",
    },
    SlashCommand {
        name: "workflow",
        description: "show workflow mode or set it to manual, adaptive, or off",
    },
    SlashCommand {
        name: "decider",
        description: "choose a decider and save its API key (takes effect from the next launch)",
    },
    SlashCommand {
        name: "skills",
        description: "browse the skills polaris discovered in this project",
    },
    SlashCommand {
        name: "new",
        description: "start a new chat, keeping this one saved for /resume",
    },
    SlashCommand {
        name: "resume",
        description: "pick a saved conversation to resume",
    },
    SlashCommand {
        name: "clear",
        description: "erase this conversation's saved history",
    },
    SlashCommand {
        name: "init",
        description: "create an AGENTS.md file with instructions for polaris",
    },
    SlashCommand {
        name: "mcp",
        description: "manage MCP servers, catalogs, and credentials",
    },
    SlashCommand {
        name: "model",
        description: "choose what model to use",
    },
    SlashCommand {
        name: "diff",
        description: "show git diff (including untracked files)",
    },
    SlashCommand {
        name: "review",
        description: "ask polaris to review the current changes and find issues",
    },
    SlashCommand {
        name: "permissions",
        description: "choose what polaris is allowed to do for the rest of this session",
    },
    SlashCommand {
        name: "fork",
        description: "copy this conversation into a new one you can branch from",
    },
    SlashCommand {
        name: "export",
        description: "export this conversation as markdown",
    },
    SlashCommand {
        name: "copy",
        description: "copy the last reply to the clipboard",
    },
    SlashCommand {
        name: "logout",
        description: "log out and remove stored ChatGPT credentials",
    },
    SlashCommand {
        name: "quit",
        description: "exit polaris",
    },
];

/// Commands removed from the TUI, each with the one line its name still
/// answers: what replaces it. `parse` keeps recognizing them, with or without
/// arguments, so an old habit gets a pointer instead of "unknown command".
/// None is listed in `COMMANDS`, and typing one starts nothing.
const REMOVED: &[(&str, &str)] = &[
    ("pwd", "/pwd was removed. /status shows the project root."),
    (
        "sandbox",
        "/sandbox was removed. /status shows the sandbox and the approval policy.",
    ),
    (
        "instruction",
        "/instruction was removed. Text typed during a run is queued for the next turn.",
    ),
    ("input", "/input was removed. Esc stops the running turn."),
    (
        "roles",
        "/roles was removed. Role definitions live in agents/<name>/SKILL.md.",
    ),
    (
        "compare",
        "/compare was removed. Run polaris compare <folder> <folder> in a shell.",
    ),
    (
        "run",
        "/run was removed. /subagents lists children; set --max-turns at launch.",
    ),
    (
        "history",
        "/history was removed. Scroll the conversation, or save it with /export.",
    ),
];

/// Commands whose name starts with `prefix` (case-insensitive, without the
/// leading `/`). An empty prefix — a bare `/` just typed — matches every
/// command, which is exactly what should show up as the initial menu.
pub fn matching(prefix: &str) -> Vec<&'static SlashCommand> {
    let prefix = prefix.to_ascii_lowercase();
    COMMANDS
        .iter()
        .filter(|c| c.name.starts_with(prefix.as_str()))
        .collect()
}

pub enum Action {
    Environment(String),
    Help,
    Status,
    Doctor,
    Account(String),
    Memory(String),
    SessionControl(String),
    #[cfg(target_os = "linux")]
    WorkspaceControl(String),
    RuntimeControl(String),
    Decided(String),
    /// Apply is intercepted by the terminal handler; recovery never resends
    /// the source effect and requires the exact displayed operation key.
    Sync(String),
    #[cfg(target_os = "linux")]
    Apply(String),
    /// Empty means display the current phase. A nonempty value is checked by
    /// the session so unknown names are rejected rather than coerced.
    Phase(String),
    /// Empty displays the durable mode. `manual`, `adaptive`, and `off` are
    /// checked by the session before a new configuration is published.
    Workflow(String),
    /// Same interception story as `New`/`Resume` below — the caller
    /// shows a full-screen, browse-only picker over the discovered
    /// skills (needs the terminal, which `apply_slash_action` doesn't
    /// have).
    Skills,
    /// Intercepted by the caller like `Skills`: a full-screen step that saves
    /// the decider key and turns the decider on for the next launch.
    Decider,
    /// Not a local command in the usual sense — the caller intercepts
    /// this before dispatch and rotates to a brand-new session (new id,
    /// new file) instead of running it through `apply_slash_action`,
    /// since that needs to mutate state (`session_path`, `meta_path`,
    /// ...) `apply_slash_action` doesn't own. Listed here anyway so it
    /// still shows up in the popup and `/help`.
    New,
    /// Same interception story as `New` — the caller shows a full-screen
    /// picker (needs the terminal, which `apply_slash_action` doesn't
    /// have) and swaps the active session in place on selection.
    Resume,
    Clear,
    /// Removed: compaction is automatic. Still parsed, with or without the
    /// old `threshold` argument, so that typing it gets an answer instead of
    /// "unknown command". It is not listed in `COMMANDS` and starts nothing.
    Compact,
    /// A command removed from the TUI (see `REMOVED`): the carried text is
    /// the one line its name still answers. Starts nothing and changes
    /// nothing, whatever arguments followed the name.
    Removed(&'static str),
    Init,
    /// Same interception story as `Permissions` — the caller shows a
    /// picker over the fixed model catalog and, on selection, both calls
    /// `provider.set_model(...)` and swaps the display copy
    /// `apply_slash_action` doesn't own.
    Model,
    /// Local and display-only: registration and admission state, read
    /// without connecting to any server and without asking the model.
    Mcp(String),
    Diff,
    /// Not a local command — the caller expands this (plus the carried
    /// extra instructions, if any were typed after the command name) into
    /// a real user message and sends it through the normal model turn
    /// instead of dispatching it via `apply_slash_action`. Listed here
    /// anyway so it still shows up in the popup and `/help`.
    Review(String),
    /// Same interception story as `New`/`Resume` — the caller shows a
    /// picker over `ApprovalPolicy`'s three variants and, on selection,
    /// swaps the `approval_policy` local `apply_slash_action` doesn't own.
    Permissions,
    /// Same interception story as `New` — the caller duplicates the
    /// current session into a new file/id, exactly like `New` except
    /// seeded with the conversation so far instead of starting empty.
    Fork,
    /// The carried text, if any, is the destination path; empty means
    /// "pick a default path". Local — writes a file, never touches the
    /// model.
    Export(String),
    /// Copies the last reply's text to the system clipboard via an OSC 52
    /// escape sequence — no native clipboard crate, works over SSH/tmux the
    /// same way it does locally. Mirrors codex's own `/copy` (and its
    /// `Ctrl+O` hotkey, wired up alongside this in `run()`).
    Copy,
    Logout,
    Quit,
    Unknown(String),
}

impl Action {
    /// Display-only forms that do not require a native mutation owner.
    /// New actions and additional arguments stay closed until classified.
    pub(super) fn allowed_without_mutation_owner(&self) -> bool {
        match self {
            Self::Help
            | Self::Status
            | Self::Doctor
            | Self::Skills
            | Self::Diff
            | Self::Compact
            | Self::Removed(_)
            | Self::Quit
            | Self::Unknown(_) => true,
            Self::Account(value) | Self::Decided(value) | Self::Mcp(value) => {
                matches!(value.as_str(), "" | "list")
            }
            Self::Phase(value) | Self::Workflow(value) => value.is_empty(),
            Self::Memory(value) => matches!(
                value.split_whitespace().next().unwrap_or(""),
                "" | "search" | "get" | "source"
            ),
            Self::SessionControl(value) => matches!(
                value.as_str(),
                "session" | "session current" | "session history" | "goal" | "goal current"
            ),
            Self::RuntimeControl(value) => {
                matches!(value.as_str(), "subagents" | "subagents list")
            }
            Self::Sync(value) => matches!(value.as_str(), "" | "status"),
            _ => false,
        }
    }

    /// A pending durable management effect must be reconciled before these
    /// commands can change session, project, clipboard, or remote state.
    pub fn needs_management_clearance(&self) -> bool {
        match self {
            Self::Phase(value) | Self::Workflow(value) => !value.is_empty(),
            Self::SessionControl(value) => !matches!(
                value.as_str(),
                "session" | "session current" | "session history" | "goal" | "goal current"
            ),
            #[cfg(target_os = "linux")]
            Self::WorkspaceControl(_) => false,
            Self::RuntimeControl(_) => false,
            Self::Sync(value) => {
                // C10 owns composite admission and explicit recovery exceptions.
                if value.split_whitespace().next() == Some("push") {
                    return false;
                }
                value == "apply"
                    || value.starts_with("join ")
                    || matches!(value.as_str(), "pull" | "leave")
            }
            #[cfg(target_os = "linux")]
            Self::Apply(value) => {
                value.starts_with("run ")
                    || value.starts_with("resolve ") && value.ends_with(" allow")
            }
            Self::Account(value) => !value.is_empty() && value != "list",
            Self::Memory(value) => {
                let command = value.split_whitespace().next().unwrap_or("");
                matches!(command, "import" | "index" | "embed" | "forget")
            }
            Self::Decided(value) => !value.is_empty() && value != "list",
            Self::Mcp(value) => {
                let command = value.split_whitespace().next().unwrap_or("");
                matches!(
                    command,
                    "enable"
                        | "disable"
                        | "secret"
                        | "check"
                        | "setup"
                        | "materialize"
                        | "register"
                        | "configure"
                )
            }
            Self::New
            | Self::Resume
            | Self::Clear
            | Self::Init
            | Self::Fork
            | Self::Export(_)
            | Self::Copy
            | Self::Logout
            | Self::Quit => true,
            _ => false,
        }
    }
}

/// The action for a name taken directly from `COMMANDS` (e.g. the
/// currently-highlighted row in the suggestion popup) — every name in
/// `COMMANDS` is by construction one `parse` already recognizes, so this
/// never produces `Unknown`.
pub fn action_for(name: &str) -> Action {
    parse(&format!("/{name}")).expect("COMMANDS names are always valid commands")
}

/// Parses a submitted line as a slash command. Returns `None` when the
/// line doesn't start with `/` at all — the caller then treats it as a
/// normal instruction to send to the model, unchanged from today.
///
/// Only the first whitespace-separated word is matched against a command
/// name; everything after it is trailing text. Argument-bearing commands
/// preserve it for their owner. Simple actions ignore it, so `/status now`,
/// for example, still resolves to `Status` instead of `Unknown`.
pub fn parse(line: &str) -> Option<Action> {
    let rest = line
        .strip_prefix('/')
        .or_else(|| (line == "env" || line.starts_with("env ")).then_some(line))?;
    let trimmed = rest.trim();
    let mut parts = trimmed.splitn(2, char::is_whitespace);
    let name = parts.next().unwrap_or("").to_ascii_lowercase();
    let extra = parts.next().unwrap_or("").trim().to_string();
    if let Some(&(_, notice)) = REMOVED.iter().find(|(removed, _)| *removed == name) {
        return Some(Action::Removed(notice));
    }
    Some(match name.as_str() {
        "env" => Action::Environment(extra),
        "" | "help" => Action::Help,
        "status" => Action::Status,
        "doctor" => Action::Doctor,
        "login" => Action::Account(login_request(&extra)),
        "memory" if extra.starts_with("configure ") => {
            Action::SessionControl(format!("memory {extra}"))
        }
        "memory" => Action::Memory(extra),
        "session" | "goal" | "tool-memory" => {
            Action::SessionControl(format!("{name} {extra}").trim_end().to_owned())
        }
        "subagents" => Action::RuntimeControl(format!("{name} {extra}").trim_end().to_owned()),
        #[cfg(target_os = "linux")]
        "setting" => Action::WorkspaceControl(format!("{name} {extra}").trim_end().to_owned()),
        "decided" => Action::Decided(extra),
        "sync" => Action::Sync(extra),
        #[cfg(target_os = "linux")]
        "apply" => Action::Apply(extra),
        "phase" => Action::Phase(extra),
        "workflow" => Action::Workflow(extra),
        "skills" => Action::Skills,
        "decider" => Action::Decider,
        "new" => Action::New,
        "resume" => Action::Resume,
        "clear" => Action::Clear,
        "compact" => Action::Compact,
        "init" => Action::Init,
        "mcp" => Action::Mcp(extra),
        "model" => Action::Model,
        "diff" => Action::Diff,
        "review" => Action::Review(extra),
        "permissions" => Action::Permissions,
        "fork" => Action::Fork,
        "export" => Action::Export(extra),
        "copy" => Action::Copy,
        "logout" if extra.is_empty() => Action::Logout,
        "logout" => Action::Account(format!("unlink {extra}")),
        "quit" | "exit" | "q" => Action::Quit,
        _ => Action::Unknown(trimmed.to_ascii_lowercase()),
    })
}

fn login_request(extra: &str) -> String {
    let (command, arguments) = extra.split_once(char::is_whitespace).unwrap_or((extra, ""));
    match command {
        "reauth" | "bind" | "label" => extra.to_owned(),
        "use" => format!("bind {arguments}"),
        "new" => format!("login {arguments}").trim_end().to_owned(),
        _ => format!("login {extra}").trim_end().to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn m3c_saved_and_read_routes_keep_arguments_and_mutation_clearance() {
        for command in [
            "/session configure codex gpt-5.6-sol",
            "/goal set finish",
            "/memory configure on",
            "/tool-memory endpoint http://127.0.0.1:8080/v1 embed",
        ] {
            let action = parse(command).unwrap();
            assert!(
                matches!(&action, Action::SessionControl(value) if format!("/{value}") == command)
            );
            assert!(action.needs_management_clearance());
        }
        for command in ["/session current", "/goal", "/subagents wait child attempt"] {
            assert!(!parse(command).unwrap().needs_management_clearance());
        }
        assert!(matches!(parse("/model"), Some(Action::Model)));
    }

    #[test]
    fn subagents_is_the_only_runtime_control_and_only_its_reads_need_no_owner() {
        for command in ["/subagents", "/subagents list"] {
            let action = parse(command).unwrap();
            assert!(matches!(action, Action::RuntimeControl(_)), "{command}");
            assert!(action.allowed_without_mutation_owner(), "{command}");
        }
        for command in [
            "/subagents wait child attempt",
            "/subagents cancel child attempt",
        ] {
            let action = parse(command).unwrap();
            assert!(matches!(action, Action::RuntimeControl(_)), "{command}");
            assert!(!action.allowed_without_mutation_owner(), "{command}");
        }
    }

    /// The eight commands removed from the TUI. Names, argument forms and the
    /// phrase that proves each replacement are written out here instead of
    /// being read back from `REMOVED`, so dropping or rewording an entry fails.
    const REMOVED_TUI_COMMANDS: [(&str, &str, &str); 8] = [
        ("pwd", "/pwd now", "/status shows the project root"),
        ("sandbox", "/sandbox status", "/status shows the sandbox"),
        (
            "instruction",
            "/instruction send do the next thing",
            "queued for the next turn",
        ),
        (
            "input",
            "/input cancel input-id",
            "Esc stops the running turn",
        ),
        (
            "roles",
            "/roles bind worker subgiant",
            "agents/<name>/SKILL.md",
        ),
        ("compare", "/compare a | b", "polaris compare"),
        (
            "run",
            "/run settings max-turns 5",
            "set --max-turns at launch",
        ),
        ("history", "/history 20", "save it with /export"),
    ];

    #[test]
    fn removed_tui_commands_answer_one_line_naming_the_replacement() {
        for (name, with_arguments, replacement) in REMOVED_TUI_COMMANDS {
            for line in [
                format!("/{name}"),
                format!("/{}", name.to_uppercase()),
                with_arguments.to_owned(),
            ] {
                let action = parse(&line).unwrap_or_else(|| panic!("{line} is not a command"));
                let Action::Removed(notice) = &action else {
                    panic!("{line} does not parse as a removed command");
                };
                assert!(
                    notice.starts_with(&format!("/{name} was removed.")),
                    "{line}: {notice}"
                );
                assert!(notice.contains(replacement), "{line}: {notice}");
                assert!(!notice.contains('\n'), "{line}: one line, got {notice:?}");
                assert!(!action.needs_management_clearance(), "{line}");
                assert!(action.allowed_without_mutation_owner(), "{line}");
            }
        }
    }

    /// Columns a notice may take. The status row is one line that does not
    /// wrap, and a new Terminal.app window is 80 columns wide; two stay spare.
    /// Written out here, not read from the render code, so a longer notice fails.
    const NOTICE_COLUMNS: usize = 78;

    #[test]
    fn removed_tui_commands_fit_the_status_row_of_an_80_column_terminal() {
        use unicode_width::UnicodeWidthStr;

        // Every entry of the table, so a ninth removed command is held to it too.
        for &(name, _) in REMOVED {
            let Some(Action::Removed(notice)) = parse(&format!("/{name}")) else {
                panic!("/{name} does not parse as a removed command");
            };
            assert!(
                notice.width() <= NOTICE_COLUMNS,
                "/{name}: {} columns, cut off past {NOTICE_COLUMNS}: {notice}",
                notice.width()
            );
        }
    }

    #[test]
    fn removed_tui_commands_are_not_offered_by_the_catalog_or_the_popup() {
        for (name, _, _) in REMOVED_TUI_COMMANDS {
            assert!(!COMMANDS.iter().any(|c| c.name == name), "/{name} listed");
            for end in 1..=name.len() {
                assert!(
                    matching(&name[..end]).iter().all(|c| c.name != name),
                    "/{name} offered for {}",
                    &name[..end]
                );
            }
            assert!(matching(name).is_empty(), "/{name} offered as typed");
        }
    }

    #[test]
    fn commands_beside_the_removed_ones_are_still_listed_and_parse() {
        for (line, name) in [
            ("/status", "status"),
            ("/status now", "status"),
            ("/subagents list", "subagents"),
            ("/session current", "session"),
            ("/goal", "goal"),
            ("/export notes.md", "export"),
            ("/sync status", "sync"),
        ] {
            assert!(COMMANDS.iter().any(|c| c.name == name), "/{name} unlisted");
            assert!(
                !matches!(parse(line), Some(Action::Removed(_) | Action::Unknown(_))),
                "{line}"
            );
        }
        assert!(matches!(parse("/status now"), Some(Action::Status)));
    }

    #[test]
    fn removed_commands_are_unknown_and_absent_from_completion() {
        for name in [
            "web-search",
            "models",
            "browse",
            "attachment",
            "recipe",
            "handoff",
            "remember",
            "account",
        ] {
            assert!(matching(name).is_empty());
            for arguments in ["", " list", " read src/main.rs", " live"] {
                assert!(matches!(
                    parse(&format!("/{name}{arguments}")),
                    Some(Action::Unknown(_))
                ));
            }
        }
    }

    #[test]
    fn login_and_logout_keep_the_existing_account_operations() {
        for (text, expected) in [
            ("/login", "login"),
            ("/login new label", "login label"),
            ("/login reauth registration", "reauth registration"),
            ("/login use registration", "bind registration"),
            (
                "/login label registration new name",
                "label registration new name",
            ),
            ("/logout registration", "unlink registration"),
        ] {
            assert!(matches!(parse(text), Some(Action::Account(value)) if value == expected));
            assert!(parse(text).unwrap().needs_management_clearance());
        }
        assert!(matches!(parse("/logout"), Some(Action::Logout)));
        assert!(
            matches!(parse("/memory configure on"), Some(Action::SessionControl(value)) if value == "memory configure on")
        );
    }

    #[test]
    fn m2d_sync_routes_read_apply_and_exact_recovery_arguments() {
        for line in ["/sync", "/sync status"] {
            let action = parse(line).unwrap();
            assert!(matches!(&action, Action::Sync(_)));
            assert!(!action.needs_management_clearance());
        }
        assert!(parse("/sync apply").unwrap().needs_management_clearance());
        let action = parse("/sync recover 0123").unwrap();
        assert!(!action.needs_management_clearance());
        assert!(matches!(action, Action::Sync(value) if value == "recover 0123"));
        assert_eq!(matching("syn")[0].name, "sync");
    }

    #[test]
    fn a_bare_slash_matches_every_command() {
        assert_eq!(matching("").len(), COMMANDS.len());
    }

    #[test]
    fn a_prefix_narrows_to_matching_commands_only() {
        let m = matching("cl");
        assert_eq!(m.len(), 1);
        assert_eq!(m[0].name, "clear");
    }

    #[test]
    fn matching_is_case_insensitive() {
        assert_eq!(matching("CL").len(), 1);
    }

    #[test]
    fn a_prefix_matching_nothing_returns_an_empty_list() {
        assert!(matching("zz").is_empty());
    }

    #[test]
    fn text_without_a_leading_slash_is_not_a_command() {
        assert!(parse("clear the table please").is_none());
    }

    #[test]
    fn known_commands_parse_to_their_action() {
        assert!(matches!(parse("/help"), Some(Action::Help)));
        assert!(matches!(parse("/status"), Some(Action::Status)));
        assert!(matches!(parse("/doctor"), Some(Action::Doctor)));
        assert!(matches!(parse("/account list"), Some(Action::Unknown(_))));
        assert!(
            matches!(parse("/memory search a"), Some(Action::Memory(value)) if value == "search a")
        );
        assert!(matches!(parse("/decided list"), Some(Action::Decided(value)) if value == "list"));
        assert!(matches!(parse("/phase"), Some(Action::Phase(value)) if value.is_empty()));
        assert!(matches!(parse("/decider"), Some(Action::Decider)));
        assert!(matches!(action_for("decider"), Action::Decider));
        assert!(
            matches!(parse("/workflow mode adaptive"), Some(Action::Workflow(value)) if value == "mode adaptive")
        );
        assert!(matches!(parse("/skills"), Some(Action::Skills)));
        assert!(matches!(parse("/new"), Some(Action::New)));
        assert!(matches!(parse("/resume"), Some(Action::Resume)));
        assert!(matches!(parse("/clear"), Some(Action::Clear)));
        assert!(matches!(parse("/init"), Some(Action::Init)));
        assert!(matches!(parse("/model"), Some(Action::Model)));
        assert!(matches!(parse("/diff"), Some(Action::Diff)));
        match parse("/review") {
            Some(Action::Review(extra)) => assert!(extra.is_empty()),
            _ => panic!("expected Review"),
        }
        assert!(matches!(parse("/permissions"), Some(Action::Permissions)));
        assert!(matches!(parse("/fork"), Some(Action::Fork)));
        match parse("/export") {
            Some(Action::Export(extra)) => assert!(extra.is_empty()),
            _ => panic!("expected Export"),
        }
        assert!(matches!(parse("/copy"), Some(Action::Copy)));
        assert!(matches!(parse("/logout"), Some(Action::Logout)));
        assert!(matches!(parse("/quit"), Some(Action::Quit)));
        assert!(matches!(parse("/exit"), Some(Action::Quit)));
        assert!(matches!(parse("/q"), Some(Action::Quit)));
    }

    #[test]
    fn export_carries_text_typed_after_the_command_name_as_a_destination_path() {
        match parse("/export notes.md") {
            Some(Action::Export(extra)) => assert_eq!(extra, "notes.md"),
            _ => panic!("expected Export"),
        }
    }

    #[test]
    fn every_command_in_the_list_actually_parses_to_something_other_than_unknown() {
        // Catches the class of bug where a name is added to `COMMANDS` (so
        // it shows up in the popup) but its arm is missing from `parse`'s
        // match — the popup would offer it, but selecting it would fall
        // through to "unknown command".
        for c in COMMANDS {
            let action = parse(&format!("/{}", c.name)).expect("starts with /");
            assert!(
                !matches!(action, Action::Unknown(_)),
                "{} parses as Unknown",
                c.name
            );
        }
    }

    /// T01: these handlers are compiled on Linux only. Anywhere else the
    /// command must be neither listed nor recognized; otherwise `/help` and the
    /// popup offer a route that ends in the unconnected fallback.
    #[test]
    fn linux_only_commands_are_listed_and_recognized_only_on_linux() {
        for name in ["setting", "apply"] {
            let on_linux = cfg!(target_os = "linux");
            let listed = COMMANDS.iter().any(|c| c.name == name);
            let offered = !matching(name).is_empty();
            let recognized = !matches!(parse(&format!("/{name}")), Some(Action::Unknown(_)));
            assert_eq!(listed, on_linux, "/{name} listed");
            assert_eq!(offered, on_linux, "/{name} offered by the popup");
            assert_eq!(recognized, on_linux, "/{name} recognized");
        }
        // Commands with a handler on every platform are never gated away.
        for name in ["decided", "doctor", "help", "sync"] {
            assert_eq!(matching(name).len(), 1, "/{name} offered by the popup");
            assert!(!matches!(
                parse(&format!("/{name}")),
                Some(Action::Unknown(_))
            ));
        }
    }

    #[test]
    fn a_bare_slash_is_help() {
        assert!(matches!(parse("/"), Some(Action::Help)));
    }

    #[test]
    fn action_for_resolves_every_listed_command_without_panicking() {
        for c in COMMANDS {
            let _ = action_for(c.name);
        }
    }

    #[test]
    fn action_for_matches_what_parse_would_give_the_same_name() {
        assert!(matches!(action_for("clear"), Action::Clear));
        assert!(matches!(action_for("quit"), Action::Quit));
    }

    #[test]
    fn parsing_is_case_insensitive() {
        assert!(matches!(parse("/CLEAR"), Some(Action::Clear)));
    }

    #[test]
    fn an_unrecognized_command_reports_its_own_name() {
        match parse("/frobnicate") {
            Some(Action::Unknown(name)) => assert_eq!(name, "frobnicate"),
            _ => panic!("expected Unknown"),
        }
    }

    #[test]
    fn surrounding_whitespace_after_the_slash_is_trimmed() {
        assert!(matches!(parse("/ clear "), Some(Action::Clear)));
    }

    #[test]
    fn compact_is_answered_but_not_offered() {
        for line in ["/compact", "/compact threshold 5000", "/COMPACT now"] {
            let action = parse(line).unwrap();
            assert!(matches!(action, Action::Compact), "{line}");
            assert!(!action.needs_management_clearance(), "{line}");
            assert!(action.allowed_without_mutation_owner(), "{line}");
        }
        assert!(!COMMANDS.iter().any(|c| c.name == "compact"));
        assert!(matching("compact").is_empty());
        assert!(matching("com").iter().all(|c| c.name != "compact"));
    }

    #[test]
    fn effort_is_not_an_interactive_command() {
        assert!(!COMMANDS.iter().any(|command| command.name == "effort"));
        assert!(matches!(parse("/effort"), Some(Action::Unknown(name)) if name == "effort"));
    }

    #[test]
    fn phase_carries_the_requested_name_for_session_validation() {
        assert!(matches!(
            parse("/phase implement"),
            Some(Action::Phase(value)) if value == "implement"
        ));
        assert!(matches!(
            parse("/phase impossible"),
            Some(Action::Phase(value)) if value == "impossible"
        ));
    }

    #[test]
    fn review_carries_text_typed_after_the_command_name() {
        match parse("/review focus on the auth changes") {
            Some(Action::Review(extra)) => assert_eq!(extra, "focus on the auth changes"),
            _ => panic!("expected Review"),
        }
    }

    #[test]
    fn trailing_text_after_a_command_name_is_ignored_for_non_review_commands() {
        // A command shouldn't turn into Unknown just because the user
        // typed something after it — only `/review` reads what follows.
        assert!(matches!(parse("/status now"), Some(Action::Status)));
    }
}
