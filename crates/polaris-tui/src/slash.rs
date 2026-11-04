//! Slash commands: local, client-side commands recognized when the input
//! buffer starts with `/`. Most dispatch to local owners rather than becoming
//! conversation messages. Input-admission commands can also request model work
//! through their owners. Local configuration and management commands may
//! persist state.
//!
//! Typing opens an autocomplete popup and submission dispatches the selected
//! command. `COMMANDS` owns the visible catalog; `parse` also accepts `/exit`
//! and `/q` as aliases of `/quit`. Commands with arguments route to the owner
//! of their session, workspace, account, memory, or MCP state. `/model`
//! switches the model through `Provider::set_model` on the shared provider. The
//! interactive picker deliberately leaves reasoning effort to automatic
//! routing; fixed noninteractive measurements can still use `--effort`.
//!
//! A removed command is not listed in `COMMANDS` but stays parseable (`REMOVED`,
//! `REMOVED_FORMS`, `Action::Removed`): its name answers one line that names
//! where to go instead, and starts nothing. `/new fork` and `/login decider`
//! are the former `/fork` and `/decider`.

pub struct SlashCommand {
    pub name: &'static str,
    pub description: &'static str,
}

pub const COMMANDS: &[SlashCommand] = &[
    SlashCommand {
        name: "model",
        description: "choose what model to use",
    },
    SlashCommand {
        name: "new",
        description: "start a new chat (/new fork branches this one)",
    },
    SlashCommand {
        name: "resume",
        description: "pick a saved conversation to resume",
    },
    SlashCommand {
        name: "status",
        description: "show login status, conversation ID, model, and usage",
    },
    SlashCommand {
        name: "diff",
        description: "show git diff (including untracked files)",
    },
    SlashCommand {
        name: "permissions",
        description: "choose what polaris is allowed to do for the rest of this session",
    },
    SlashCommand {
        name: "mcp",
        description: "manage MCP servers, catalogs, and credentials",
    },
    SlashCommand {
        name: "login",
        description: "add, switch, re-authorize or label an account; /login decider",
    },
    SlashCommand {
        name: "logout",
        description: "log out and remove stored ChatGPT credentials",
    },
    SlashCommand {
        name: "memory",
        description: "search or inspect saved project memory",
    },
    SlashCommand {
        name: "sync",
        description: "receive, leave, inspect, apply, push, or reconcile shared changes",
    },
    SlashCommand {
        name: "export",
        description: "export this conversation as markdown",
    },
    SlashCommand {
        name: "init",
        description: "create an AGENTS.md file with instructions for polaris",
    },
    SlashCommand {
        name: "help",
        description: "list the commands and keys",
    },
    SlashCommand {
        name: "quit",
        description: "exit polaris",
    },
];

/// The keys `/help` lists after the commands: what to press, what it does.
pub const KEYS: &[(&str, &str)] = &[
    ("Esc", "during a run: stop it"),
    (
        "Esc Esc",
        "at an empty prompt: rewind to an earlier message",
    ),
    ("Ctrl+C", "during a run: stop it; twice at the prompt: exit"),
    ("Ctrl+O", "copy the last reply (drag to copy anything else)"),
    ("PageUp / PageDown", "scroll the conversation"),
];

/// Commands removed from the TUI, each with the one line its name still
/// answers: where to go instead. `parse` keeps recognizing them, with or
/// without arguments and in any case, so an old habit gets a pointer instead
/// of "unknown command". None is listed in `COMMANDS`, and typing one starts
/// nothing.
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
    (
        "input",
        "/input was removed. Press Esc twice to stop the running turn.",
    ),
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
        "/run was removed. Press Esc twice to stop a run; set --max-turns at launch.",
    ),
    (
        "history",
        "/history was removed. Scroll the conversation, or save it with /export.",
    ),
    (
        "env",
        "/env was removed. In a shell: polaris env set|plan|status|recover.",
    ),
    (
        "setting",
        "/setting was removed. In a shell: polaris setting git.",
    ),
    (
        "session",
        "/session was removed. /status shows the session; /model sets the model.",
    ),
    ("goal", "/goal was removed. State the goal in your message."),
    (
        "tool-memory",
        "/tool-memory was removed. Start polaris with --tool-memory <mode>.",
    ),
    (
        "subagents",
        "/subagents was removed. Press Esc twice to stop the run and its subagents.",
    ),
    ("doctor", "/doctor was removed. In a shell: polaris doctor."),
    (
        "decided",
        "/decided was removed. In a shell: polaris decided [sentence].",
    ),
    (
        "apply",
        "/apply was removed. Ask polaris to make the change, then check it with /diff.",
    ),
    (
        "phase",
        "/phase was removed. Start polaris with --phase <name> (new chats only).",
    ),
    (
        "workflow",
        "/workflow was removed. Use polaris --resume <ID> --workflow-mode <mode>.",
    ),
    ("decider", "/decider was removed. Use /login decider."),
    (
        "skills",
        "/skills was removed. /status lists the skills in use for this phase.",
    ),
    (
        "clear",
        "/clear was removed. /new starts fresh and keeps this chat in /resume.",
    ),
    (
        "review",
        "/review was removed. Ask polaris in plain words to review your changes.",
    ),
    ("fork", "/fork was removed. Use /new fork."),
    (
        "copy",
        "/copy was removed. Press Ctrl+O to copy the last reply.",
    ),
    (
        "compact",
        "Compaction is automatic. Local models: set context_window in config.toml.",
    ),
];

/// Forms of a listed command that were removed, as (command, form, line): the
/// word after the command name decides, in any case, and what follows it is
/// ignored. The rest of the command keeps working.
const REMOVED_FORMS: &[(&str, &str, &str)] = &[
    (
        "mcp",
        "configure",
        "/mcp configure was removed. Use /mcp setup, or polaris mcp configure.",
    ),
    (
        "mcp",
        "delete",
        "/mcp delete was removed. Use /mcp disable <server> or /mcp secret delete.",
    ),
    (
        "sync",
        "join",
        "/sync join was removed. In a shell: polaris sync join.",
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
    Help,
    Status,
    /// `/login` in all its forms (including `/login list`) and `/logout <id>`.
    Account(String),
    Memory(String),
    /// `/memory configure …`, carried whole: saves the remember or history-mode
    /// setting of the open conversation.
    MemoryConfigure(String),
    /// Apply is intercepted by the terminal handler; recovery never resends
    /// the source effect and requires the exact displayed operation key.
    Sync(String),
    /// `/login decider`. Intercepted by the caller like `New`: a full-screen
    /// step that saves the decider key and turns the decider on for the next
    /// launch.
    Decider,
    /// Not a local command in the usual sense — the caller intercepts
    /// this before dispatch and rotates to a brand-new session (new id,
    /// new file) instead of running it through `apply_slash_action`,
    /// since that needs to mutate state (`session_path`, `meta_path`,
    /// ...) `apply_slash_action` doesn't own. Listed here anyway so it
    /// still shows up in the popup and `/help`.
    New,
    /// `/new fork`. Same interception story as `New` — the caller duplicates
    /// the current session into a new file/id, exactly like `New` except
    /// seeded with the conversation so far instead of starting empty.
    Fork,
    /// Same interception story as `New` — the caller shows a full-screen
    /// picker (needs the terminal, which `apply_slash_action` doesn't
    /// have) and swaps the active session in place on selection.
    Resume,
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
    /// Same interception story as `New`/`Resume` — the caller shows a
    /// picker over `ApprovalPolicy`'s three variants and, on selection,
    /// swaps the `approval_policy` local `apply_slash_action` doesn't own.
    Permissions,
    /// The carried text, if any, is the destination path; empty means
    /// "pick a default path". Local — writes a file, never touches the
    /// model.
    Export(String),
    /// Copies the last reply's text, or a finished mouse selection, to the
    /// system clipboard. Only `Ctrl+O` and the selection produce it: `parse`
    /// never does, because `/copy` is removed.
    Copy,
    Logout,
    Quit,
    /// A command, or a form of one, that was removed from the TUI (see
    /// `REMOVED` and `REMOVED_FORMS`): the carried text is the one line it
    /// still answers. Starts nothing and changes nothing, whatever arguments
    /// followed.
    Removed(&'static str),
    Unknown(String),
}

impl Action {
    /// Display-only forms that do not require a native mutation owner.
    /// New actions and additional arguments stay closed until classified.
    pub(super) fn allowed_without_mutation_owner(&self) -> bool {
        match self {
            Self::Help
            | Self::Status
            | Self::Diff
            | Self::Removed(_)
            | Self::Quit
            | Self::Unknown(_) => true,
            Self::Account(value) | Self::Mcp(value) => matches!(value.as_str(), "" | "list"),
            Self::Memory(value) => matches!(
                value.split_whitespace().next().unwrap_or(""),
                "" | "search" | "get" | "source"
            ),
            Self::Sync(value) => matches!(value.as_str(), "" | "status"),
            _ => false,
        }
    }

    /// A pending durable management effect must be reconciled before these
    /// commands can change session, project, clipboard, or remote state.
    pub fn needs_management_clearance(&self) -> bool {
        match self {
            Self::MemoryConfigure(_) => true,
            Self::Sync(value) => {
                // C10 owns composite admission and explicit recovery exceptions.
                if value.split_whitespace().next() == Some("push") {
                    return false;
                }
                value == "apply" || matches!(value.as_str(), "pull" | "leave")
            }
            Self::Account(value) => !value.is_empty() && value != "list",
            Self::Memory(value) => {
                let command = value.split_whitespace().next().unwrap_or("");
                matches!(command, "import" | "index" | "embed" | "forget")
            }
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
                )
            }
            Self::New
            | Self::Resume
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
    let trimmed = line.strip_prefix('/')?.trim();
    let mut parts = trimmed.splitn(2, char::is_whitespace);
    let name = parts.next().unwrap_or("").to_ascii_lowercase();
    let extra = parts.next().unwrap_or("").trim().to_string();
    let form = extra
        .split_whitespace()
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    if let Some(&(_, notice)) = REMOVED.iter().find(|(removed, _)| *removed == name) {
        return Some(Action::Removed(notice));
    }
    if let Some(&(_, _, notice)) = REMOVED_FORMS
        .iter()
        .find(|(command, removed, _)| *command == name && *removed == form)
    {
        return Some(Action::Removed(notice));
    }
    Some(match name.as_str() {
        "" | "help" => Action::Help,
        "status" => Action::Status,
        "login" => login_request(&extra),
        "memory" if extra.starts_with("configure ") => {
            Action::MemoryConfigure(format!("memory {extra}"))
        }
        "memory" => Action::Memory(extra),
        "sync" => Action::Sync(extra),
        "new" if form == "fork" => Action::Fork,
        "new" => Action::New,
        "resume" => Action::Resume,
        "init" => Action::Init,
        "mcp" => Action::Mcp(extra),
        "model" => Action::Model,
        "diff" => Action::Diff,
        "permissions" => Action::Permissions,
        "export" => Action::Export(extra),
        "logout" if extra.is_empty() => Action::Logout,
        "logout" => Action::Account(format!("unlink {extra}")),
        "quit" | "exit" | "q" => Action::Quit,
        _ => Action::Unknown(trimmed.to_ascii_lowercase()),
    })
}

fn login_request(extra: &str) -> Action {
    let (command, arguments) = extra.split_once(char::is_whitespace).unwrap_or((extra, ""));
    Action::Account(match command {
        "reauth" | "bind" | "label" => extra.to_owned(),
        "use" => format!("bind {arguments}"),
        "new" => format!("login {arguments}").trim_end().to_owned(),
        "list" if arguments.is_empty() => "list".to_owned(),
        _ if extra.eq_ignore_ascii_case("decider") => return Action::Decider,
        _ => format!("login {extra}").trim_end().to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use unicode_width::UnicodeWidthStr;

    /// The list `/help`, the popup and completion show, written out here
    /// instead of read back from `COMMANDS`, so adding, dropping or
    /// reordering a name fails.
    const LISTED: [&str; 15] = [
        "model",
        "new",
        "resume",
        "status",
        "diff",
        "permissions",
        "mcp",
        "login",
        "logout",
        "memory",
        "sync",
        "export",
        "init",
        "help",
        "quit",
    ];

    /// Columns a notice may take. The status row is one line that does not
    /// wrap, and a new Terminal.app window is 80 columns wide; two stay spare.
    /// Written out here, not read from the render code, so a longer notice fails.
    const NOTICE_COLUMNS: usize = 78;

    #[test]
    fn the_list_is_exactly_the_fifteen_in_order() {
        let names: Vec<_> = COMMANDS.iter().map(|command| command.name).collect();
        assert_eq!(names, LISTED);
        assert!(COMMANDS.iter().all(|c| !c.description.is_empty()));
        // The popup and completion offer the same fifteen, in the same order.
        let offered: Vec<_> = matching("").iter().map(|command| command.name).collect();
        assert_eq!(offered, LISTED);
    }

    #[test]
    fn every_listed_command_parses_to_a_live_action() {
        // Catches a name added to `COMMANDS` (so the popup offers it) whose arm
        // is missing from `parse`: selecting it would fall to "unknown command"
        // or to a removed notice.
        for name in LISTED {
            let action = parse(&format!("/{name}")).expect("starts with /");
            assert!(
                !matches!(action, Action::Unknown(_) | Action::Removed(_)),
                "/{name} is listed but does not run"
            );
            let _ = action_for(name);
        }
    }

    #[test]
    fn the_list_names_the_forms_that_moved_and_the_help_names_the_keys() {
        let description = |name: &str| {
            COMMANDS
                .iter()
                .find(|command| command.name == name)
                .unwrap()
                .description
        };
        assert!(description("new").contains("/new fork"));
        assert!(description("login").contains("/login decider"));
        assert!(!description("sync").contains("join"));
        let keys: Vec<_> = KEYS.iter().map(|(keys, _)| *keys).collect();
        assert_eq!(
            keys,
            ["Esc", "Esc Esc", "Ctrl+C", "Ctrl+O", "PageUp / PageDown"]
        );
        assert!(KEYS.iter().all(|(_, what)| !what.is_empty()));
    }

    /// Every removed command: its name, a form that carries arguments, and
    /// the phrase that proves where the answer sends the person. All written
    /// out here instead of being read back from `REMOVED`, so dropping or
    /// rewording an entry fails.
    const REMOVED_COMMANDS: [(&str, &str, &str); 25] = [
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
            "Press Esc twice to stop the running turn",
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
            "Press Esc twice to stop a run; set --max-turns at launch",
        ),
        ("history", "/history 20", "save it with /export"),
        ("env", "/env set node --path app", "polaris env"),
        ("setting", "/setting git", "polaris setting git"),
        (
            "session",
            "/session configure codex gpt-6-astra auto",
            "/status shows the session; /model sets the model",
        ),
        (
            "goal",
            "/goal set finish the task",
            "State the goal in your message",
        ),
        (
            "tool-memory",
            "/tool-memory endpoint http://127.0.0.1:8080/v1 embed",
            "--tool-memory <mode>",
        ),
        (
            "subagents",
            "/subagents cancel child attempt",
            "Press Esc twice to stop the run and its subagents",
        ),
        ("doctor", "/doctor now", "polaris doctor"),
        ("decided", "/decided add keep the ledger", "polaris decided"),
        (
            "apply",
            "/apply run change the title",
            "check it with /diff",
        ),
        ("phase", "/phase implement", "--phase <name>"),
        (
            "workflow",
            "/workflow mode adaptive",
            "polaris --resume <ID> --workflow-mode <mode>",
        ),
        ("decider", "/decider now", "Use /login decider"),
        ("skills", "/skills list", "/status lists the skills"),
        (
            "clear",
            "/clear now",
            "/new starts fresh and keeps this chat in /resume",
        ),
        (
            "review",
            "/review focus on the auth changes",
            "Ask polaris in plain words",
        ),
        ("fork", "/fork now", "Use /new fork"),
        ("copy", "/copy now", "Press Ctrl+O to copy the last reply"),
    ];

    /// Forms of listed commands that were removed, same shape.
    const REMOVED_FORM_LINES: [(&str, &str, &str); 3] = [
        (
            "/mcp configure",
            "/mcp configure unity --preset unity-local-v2",
            "polaris mcp configure",
        ),
        (
            "/mcp delete",
            "/mcp delete some-server",
            "/mcp secret delete",
        ),
        (
            "/sync join",
            "/sync join https://example.com/x share invite 9999999999",
            "polaris sync join",
        ),
    ];

    fn assert_one_removed_line(line: &str, name: &str, target: &str) {
        let action = parse(line).unwrap_or_else(|| panic!("{line} is not a command"));
        let Action::Removed(notice) = &action else {
            panic!("{line} does not parse as a removed command");
        };
        assert!(
            notice.starts_with(&format!("{name} was removed.")),
            "{line}: {notice}"
        );
        // The phrase ends where a word ends, so `polaris env` does not pass for
        // a line that names `polaris environment`.
        let end = notice
            .find(target)
            .map(|at| at + target.len())
            .unwrap_or_else(|| panic!("{line}: {target:?} is not in {notice}"));
        assert!(
            !notice[end..].starts_with(|c: char| c.is_alphanumeric() || c == '-' || c == '_'),
            "{line}: {target:?} runs on in {notice}"
        );
        assert!(!notice.contains('\n'), "{line}: one line, got {notice:?}");
        assert!(
            notice.width() <= NOTICE_COLUMNS,
            "{line}: {} columns, cut off past {NOTICE_COLUMNS}: {notice}",
            notice.width()
        );
        assert!(!action.needs_management_clearance(), "{line}");
        assert!(action.allowed_without_mutation_owner(), "{line}");
    }

    #[test]
    fn removed_commands_answer_one_line_naming_where_to_go() {
        for (name, with_arguments, target) in REMOVED_COMMANDS {
            for line in [
                format!("/{name}"),
                format!("/{}", name.to_uppercase()),
                with_arguments.to_owned(),
                with_arguments.to_uppercase(),
            ] {
                assert_one_removed_line(&line, &format!("/{name}"), target);
            }
        }
    }

    #[test]
    fn compaction_is_automatic_and_the_answer_names_the_config_key() {
        for line in ["/compact", "/COMPACT", "/compact threshold 5000"] {
            let Some(Action::Removed(notice)) = parse(line) else {
                panic!("{line} does not parse as a removed command");
            };
            assert_eq!(
                notice,
                "Compaction is automatic. Local models: set context_window in config.toml."
            );
            assert!(notice.width() <= NOTICE_COLUMNS);
        }
    }

    #[test]
    fn removed_forms_of_listed_commands_answer_in_any_case_and_leave_the_rest_alone() {
        for (name, with_arguments, target) in REMOVED_FORM_LINES {
            for line in [
                name.to_owned(),
                name.to_uppercase(),
                with_arguments.to_owned(),
                with_arguments.to_uppercase(),
            ] {
                assert_one_removed_line(&line, name, target);
            }
        }
        // The command around the removed form is untouched.
        for (line, name) in [
            ("/mcp", "mcp"),
            ("/mcp list", "mcp"),
            ("/mcp setup", "mcp"),
            ("/mcp enable a", "mcp"),
            ("/mcp secret delete reference", "mcp"),
            ("/sync", "sync"),
            ("/sync pull", "sync"),
            ("/sync recover 0123", "sync"),
        ] {
            assert!(
                !matches!(parse(line), Some(Action::Removed(_) | Action::Unknown(_))),
                "{line}"
            );
            assert!(COMMANDS.iter().any(|c| c.name == name), "/{name} unlisted");
        }
    }

    #[test]
    fn removed_names_are_not_offered_by_the_list_or_the_popup() {
        let names = REMOVED_COMMANDS
            .iter()
            .map(|(name, _, _)| *name)
            .chain(["compact"]);
        for name in names {
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
    fn every_notice_the_table_holds_fits_the_status_row_of_an_80_column_terminal() {
        // Every entry of both tables, so one more removed command is held to it too.
        for &(name, notice) in REMOVED {
            assert!(
                notice.width() <= NOTICE_COLUMNS,
                "/{name}: {} columns: {notice}",
                notice.width()
            );
        }
        for &(command, form, notice) in REMOVED_FORMS {
            assert!(
                notice.width() <= NOTICE_COLUMNS,
                "/{command} {form}: {} columns: {notice}",
                notice.width()
            );
        }
        // The tests above name every entry, so the tables cannot grow unnoticed.
        assert_eq!(REMOVED.len(), REMOVED_COMMANDS.len() + 1);
        assert_eq!(REMOVED_FORMS.len(), REMOVED_FORM_LINES.len());
    }

    #[test]
    fn removed_commands_that_were_never_here_are_unknown_and_absent_from_completion() {
        for name in [
            "web-search",
            "models",
            "browse",
            "attachment",
            "recipe",
            "handoff",
            "remember",
            "account",
            "effort",
        ] {
            assert!(matching(name).is_empty());
            for arguments in ["", " list", " read src/main.rs", " live"] {
                assert!(matches!(
                    parse(&format!("/{name}{arguments}")),
                    Some(Action::Unknown(_))
                ));
            }
        }
        assert!(matches!(parse("/effort"), Some(Action::Unknown(name)) if name == "effort"));
    }

    #[test]
    fn new_fork_is_the_old_fork_and_login_decider_is_the_old_decider() {
        assert!(matches!(parse("/new"), Some(Action::New)));
        assert!(matches!(parse("/new fork"), Some(Action::Fork)));
        assert!(matches!(parse("/NEW FORK"), Some(Action::Fork)));
        assert!(matches!(parse("/new   fork now"), Some(Action::Fork)));
        // Anything else after `/new` still starts an empty chat.
        assert!(matches!(parse("/new forked"), Some(Action::New)));
        assert!(matches!(parse("/new something"), Some(Action::New)));
        assert!(Action::Fork.needs_management_clearance());

        assert!(matches!(parse("/login decider"), Some(Action::Decider)));
        assert!(matches!(parse("/LOGIN Decider"), Some(Action::Decider)));
        assert!(matches!(action_for("login"), Action::Account(value) if value == "login"));
        assert!(!Action::Decider.needs_management_clearance());
        // Only the bare word: a longer label is still a new login.
        assert!(
            matches!(parse("/login decider two"), Some(Action::Account(value)) if value == "login decider two")
        );
    }

    #[test]
    fn login_and_logout_keep_the_existing_account_operations() {
        for (text, expected) in [
            ("/login", "login"),
            ("/login new label", "login label"),
            ("/login new", "login"),
            ("/login a label", "login a label"),
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
    }

    #[test]
    fn login_list_lists_the_accounts_instead_of_registering_one_named_list() {
        let action = parse("/login list").unwrap();
        assert!(matches!(&action, Action::Account(value) if value == "list"));
        assert!(!action.needs_management_clearance());
        assert!(action.allowed_without_mutation_owner());
        // A label that merely starts with the word is still a new login.
        assert!(
            matches!(parse("/login list of work"), Some(Action::Account(value)) if value == "login list of work")
        );
    }

    #[test]
    fn memory_configure_keeps_its_arguments_and_mutation_clearance() {
        for command in [
            "/memory configure on",
            "/memory configure off",
            "/memory configure strict10",
        ] {
            let action = parse(command).unwrap();
            assert!(
                matches!(&action, Action::MemoryConfigure(value) if format!("/{value}") == command)
            );
            assert!(action.needs_management_clearance());
            assert!(!action.allowed_without_mutation_owner());
        }
        assert!(
            matches!(parse("/memory search a"), Some(Action::Memory(value)) if value == "search a")
        );
        assert!(
            !parse("/memory search a")
                .unwrap()
                .needs_management_clearance()
        );
        for command in [
            "/memory import a.jsonl",
            "/memory embed s i",
            "/memory index s",
            "/memory forget s",
        ] {
            assert!(parse(command).unwrap().needs_management_clearance());
        }
        assert!(matches!(parse("/model"), Some(Action::Model)));
    }

    #[test]
    fn m2d_sync_routes_read_apply_and_exact_recovery_arguments() {
        for line in ["/sync", "/sync status"] {
            let action = parse(line).unwrap();
            assert!(matches!(&action, Action::Sync(_)));
            assert!(!action.needs_management_clearance());
        }
        for line in ["/sync apply", "/sync pull", "/sync leave"] {
            assert!(parse(line).unwrap().needs_management_clearance(), "{line}");
        }
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
        let m = matching("re");
        assert_eq!(m.len(), 1);
        assert_eq!(m[0].name, "resume");
        // A prefix of a removed name offers nothing.
        assert!(matching("cl").is_empty());
        assert!(matching("doc").is_empty());
    }

    #[test]
    fn matching_is_case_insensitive() {
        assert_eq!(matching("RE").len(), 1);
    }

    #[test]
    fn a_prefix_matching_nothing_returns_an_empty_list() {
        assert!(matching("zz").is_empty());
    }

    #[test]
    fn text_without_a_leading_slash_is_not_a_command() {
        assert!(parse("clear the table please").is_none());
        // The retired bare `env …` form is an ordinary message now.
        assert!(parse("env set node").is_none());
        assert!(parse("env").is_none());
    }

    #[test]
    fn known_commands_parse_to_their_action() {
        assert!(matches!(parse("/help"), Some(Action::Help)));
        assert!(matches!(parse("/status"), Some(Action::Status)));
        assert!(matches!(parse("/account list"), Some(Action::Unknown(_))));
        assert!(matches!(parse("/new"), Some(Action::New)));
        assert!(matches!(parse("/resume"), Some(Action::Resume)));
        assert!(matches!(parse("/init"), Some(Action::Init)));
        assert!(matches!(parse("/model"), Some(Action::Model)));
        assert!(matches!(parse("/diff"), Some(Action::Diff)));
        assert!(matches!(parse("/permissions"), Some(Action::Permissions)));
        match parse("/export") {
            Some(Action::Export(extra)) => assert!(extra.is_empty()),
            _ => panic!("expected Export"),
        }
        assert!(matches!(parse("/logout"), Some(Action::Logout)));
        assert!(matches!(parse("/quit"), Some(Action::Quit)));
        assert!(matches!(parse("/exit"), Some(Action::Quit)));
        assert!(matches!(parse("/q"), Some(Action::Quit)));
        // `Copy` belongs to Ctrl+O and the mouse selection, not to a command.
        assert!(!matches!(parse("/copy"), Some(Action::Copy)));
    }

    #[test]
    fn export_carries_text_typed_after_the_command_name_as_a_destination_path() {
        match parse("/export notes.md") {
            Some(Action::Export(extra)) => assert_eq!(extra, "notes.md"),
            _ => panic!("expected Export"),
        }
    }

    #[test]
    fn a_bare_slash_is_help() {
        assert!(matches!(parse("/"), Some(Action::Help)));
    }

    #[test]
    fn action_for_matches_what_parse_would_give_the_same_name() {
        assert!(matches!(action_for("resume"), Action::Resume));
        assert!(matches!(action_for("quit"), Action::Quit));
    }

    #[test]
    fn parsing_is_case_insensitive() {
        assert!(matches!(parse("/RESUME"), Some(Action::Resume)));
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
        assert!(matches!(parse("/ resume "), Some(Action::Resume)));
    }

    #[test]
    fn trailing_text_after_a_command_name_is_ignored_for_simple_commands() {
        // A command shouldn't turn into Unknown just because the user
        // typed something after it.
        assert!(matches!(parse("/status now"), Some(Action::Status)));
        assert!(matches!(parse("/diff now"), Some(Action::Diff)));
    }
}
