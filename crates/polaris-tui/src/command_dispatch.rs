//! 保存済みsession、memory、MCPのlocal command実行をまとめる。
//! 入力確定、owner確認、管理操作のclearance、履歴とqueueはcallerが所有する。

use crate::{
    HistoryBackendFactory, active_saved_session_id, approver::KeyReader, mcp_commands,
    memory_commands, render::Status, session_commands, slash,
};
use polaris_core::{
    approval::ApprovalPolicy, management_effect::ManagementEffectStoreV1,
    rewind_workspace::WorkspaceOwner, session::Session, session_settings::SavedSessionSettingsV1,
};
use ratatui::{Terminal, backend::Backend, text::Line};
use std::{cell::RefCell, path::Path};

pub(super) struct CommandDispatchResult {
    pub(super) lines: Vec<Line<'static>>,
    pub(super) status: Status,
    pub(super) settings_changed: bool,
    pub(super) invalidate_mcp_prompt: bool,
}

impl CommandDispatchResult {
    fn success(lines: Vec<Line<'static>>, notice: &'static str) -> Self {
        Self {
            lines,
            status: Status::Notice(notice.into()),
            settings_changed: false,
            invalidate_mcp_prompt: false,
        }
    }

    fn error(error: String) -> Self {
        Self {
            lines: Vec::new(),
            status: Status::Error(error),
            settings_changed: false,
            invalidate_mcp_prompt: false,
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn dispatch<B: Backend, R: KeyReader>(
    action: &slash::Action,
    terminal: &RefCell<Terminal<B>>,
    reader: &mut R,
    session: &mut Session,
    session_dependencies: (&SavedSessionSettingsV1, Option<&HistoryBackendFactory>),
    management: (&WorkspaceOwner, &ManagementEffectStoreV1),
    paths: (&Path, &Path),
    embedding: Option<&(String, String)>,
    mcp: (bool, ApprovalPolicy),
    has_pending: bool,
) -> Option<CommandDispatchResult> {
    let (initial, history_factory) = session_dependencies;
    let (workspace, effects) = management;
    let (cwd, state_dir) = paths;
    match action {
        slash::Action::MemoryConfigure(request) => Some(
            match session_commands::handle(
                request,
                terminal,
                reader,
                session,
                initial,
                workspace,
                effects,
                history_factory,
                has_pending,
            ) {
                Ok(result) => CommandDispatchResult {
                    lines: result.lines,
                    status: Status::Notice("Session operation result displayed".into()),
                    settings_changed: result.settings_changed,
                    invalidate_mcp_prompt: false,
                },
                Err(error) => CommandDispatchResult::error(error),
            },
        ),
        slash::Action::Memory(request) => {
            let id = match active_saved_session_id(session) {
                Ok(id) => id,
                Err(error) => return Some(CommandDispatchResult::error(error)),
            };
            Some(
                match memory_commands::handle(
                    request,
                    terminal,
                    reader,
                    workspace,
                    effects,
                    cwd,
                    state_dir,
                    id.as_deref(),
                    embedding.cloned(),
                    has_pending,
                )
                .await
                {
                    Ok(lines) => {
                        CommandDispatchResult::success(lines, "Memory operation result displayed")
                    }
                    Err(error) => CommandDispatchResult::error(error),
                },
            )
        }
        slash::Action::Mcp(request) if request.split_whitespace().next() != Some("prompt") => Some(
            match mcp_commands::handle(
                request,
                terminal,
                reader,
                (workspace, effects),
                cwd,
                mcp,
                has_pending,
            )
            .await
            {
                Ok(lines) => {
                    let mut result =
                        CommandDispatchResult::success(lines, "MCP operation result displayed");
                    result.invalidate_mcp_prompt = matches!(
                        request.split_whitespace().next(),
                        Some(
                            "enable"
                                | "disable"
                                | "check"
                                | "setup"
                                | "register"
                                | "materialize"
                                | "secret"
                        )
                    );
                    result
                }
                Err(error) => CommandDispatchResult::error(error),
            },
        ),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use polaris_core::{conversation_state::HistoryMode, session_store::PersistedSession};
    use ratatui::{backend::TestBackend, crossterm::event::KeyCode};
    use std::os::unix::fs::PermissionsExt;

    struct Key(KeyCode);
    impl KeyReader for Key {
        fn read_key(&mut self) -> std::io::Result<KeyCode> {
            Ok(self.0)
        }
    }

    /// `/memory configure` is the one session setting the TUI still saves. The
    /// dispatcher claims it, reports a committed change only after the person
    /// confirms, refuses it while input is queued, and leaves removed names to
    /// the display path.
    #[tokio::test]
    async fn memory_configure_is_claimed_and_removed_names_are_not() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().canonicalize().unwrap();
        let project = root.join("project");
        let state = root.join("state");
        std::fs::create_dir(&project).unwrap();
        std::fs::create_dir(&state).unwrap();
        std::fs::set_permissions(&state, std::fs::Permissions::from_mode(0o700)).unwrap();
        let workspace = WorkspaceOwner::open(
            &project,
            &state.join("rewind-workspace"),
            Default::default(),
        )
        .unwrap();
        let lease = workspace.try_mutation().unwrap();
        let effects = ManagementEffectStoreV1::open_at_state_dir(&project, &state, &lease).unwrap();
        drop(lease);
        let persisted = PersistedSession::create(
            &root,
            &state.join("memory.sqlite3"),
            project.to_str().unwrap(),
            HistoryMode::Legacy,
            None,
        )
        .unwrap();
        let mut session = Session::new();
        session.attach(persisted.clone()).unwrap();
        let initial =
            SavedSessionSettingsV1::new("fake".into(), "model".into(), "auto".into()).unwrap();
        let terminal = RefCell::new(Terminal::new(TestBackend::new(100, 30)).unwrap());
        let remembered = || {
            persisted
                .snapshot()
                .unwrap()
                .state
                .session_settings
                .is_some_and(|saved| saved.remember)
        };
        let mut dispatch_line = async |line: &str, key: KeyCode, pending: bool| {
            let action = slash::parse(line).expect("a slash command");
            dispatch(
                &action,
                &terminal,
                &mut Key(key),
                &mut session,
                (&initial, None),
                (&workspace, &effects),
                (&project, &state),
                None,
                (false, ApprovalPolicy::OnRequest),
                pending,
            )
            .await
        };

        let queued = dispatch_line("/memory configure on", KeyCode::Char('y'), true)
            .await
            .expect("a queued-input refusal is still claimed");
        assert!(matches!(queued.status, Status::Error(_)));
        assert!(!queued.settings_changed);
        let declined = dispatch_line("/memory configure on", KeyCode::Esc, false)
            .await
            .expect("a declined save is still claimed");
        assert!(declined.lines[0].to_string().contains("cancelled"));
        assert!(!declined.settings_changed);
        assert!(!remembered());

        let saved = dispatch_line("/memory configure on", KeyCode::Char('y'), false)
            .await
            .expect("the dispatcher claims /memory configure");
        assert!(matches!(saved.status, Status::Notice(_)));
        assert!(saved.settings_changed);
        assert!(remembered());

        for line in ["/doctor", "/session configure codex gpt-6-astra", "/status"] {
            assert!(
                dispatch_line(line, KeyCode::Char('y'), false)
                    .await
                    .is_none(),
                "{line} stays with the display path"
            );
        }
        assert!(remembered());
    }
}
