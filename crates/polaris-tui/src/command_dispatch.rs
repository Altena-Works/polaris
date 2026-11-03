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
use polaris_provider::Provider;
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
    session_dependencies: (
        &SavedSessionSettingsV1,
        &dyn Provider,
        Option<&HistoryBackendFactory>,
    ),
    management: (&WorkspaceOwner, &ManagementEffectStoreV1),
    paths: (&Path, &Path),
    embedding: Option<&(String, String)>,
    mcp: (bool, ApprovalPolicy),
    has_pending: bool,
) -> Option<CommandDispatchResult> {
    let (initial, provider, history_factory) = session_dependencies;
    let (workspace, effects) = management;
    let (cwd, state_dir) = paths;
    match action {
        slash::Action::Environment(request) => {
            #[cfg(target_os = "macos")]
            let result = crate::environment_commands::handle(
                request,
                cwd,
                terminal,
                reader,
                workspace,
                effects,
                mcp.1,
                has_pending,
            )
            .await;
            #[cfg(not(target_os = "macos"))]
            let result: Result<Vec<Line<'static>>, String> = {
                let _ = request;
                Err("Environment setup currently supports macOS arm64".into())
            };
            Some(match result {
                Ok(lines) => CommandDispatchResult::success(lines, "Environment result displayed"),
                Err(error) => CommandDispatchResult::error(error),
            })
        }
        slash::Action::SessionControl(request) => Some(
            match session_commands::handle(
                request,
                terminal,
                reader,
                session,
                initial,
                workspace,
                effects,
                provider,
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
        // C35. Linux enters through `WorkspaceCommands` in the terminal loop,
        // which also pins the startup folder. The save itself is the same.
        #[cfg(not(target_os = "linux"))]
        slash::Action::Decided(request) if request.starts_with("add ") => Some(
            match crate::local_actions::decided(
                request.strip_prefix("add ").unwrap_or_default(),
                &crate::now_millis().to_string(),
                terminal,
                reader,
                workspace,
                effects,
                has_pending,
            ) {
                Ok(lines) => {
                    CommandDispatchResult::success(lines, "Decision record result displayed")
                }
                Err(error) => CommandDispatchResult::error(error),
            },
        ),
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

#[cfg(all(test, not(target_os = "linux")))]
mod tests {
    use super::*;
    use polaris_provider::{CompletionRequest, CompletionResponse, ProviderError};
    use ratatui::{backend::TestBackend, crossterm::event::KeyCode};
    use std::os::unix::fs::PermissionsExt;

    struct NoProvider;
    #[async_trait::async_trait]
    impl Provider for NoProvider {
        async fn complete(
            &self,
            _: CompletionRequest,
        ) -> Result<CompletionResponse, ProviderError> {
            panic!("decision commands never call a provider")
        }
    }

    struct Key(KeyCode);
    impl KeyReader for Key {
        fn read_key(&mut self) -> std::io::Result<KeyCode> {
            Ok(self.0)
        }
    }

    /// C35 on a platform whose terminal loop has no `WorkspaceCommands`: the
    /// dispatcher claims `/decided add <statement>` and saves exactly the text
    /// after the first `add `. The bare and `list` forms stay with the display.
    #[tokio::test]
    async fn decided_add_is_claimed_and_saves_the_statement_after_the_first_add() {
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
        let initial =
            SavedSessionSettingsV1::new("fake".into(), "model".into(), "auto".into()).unwrap();
        let terminal = RefCell::new(Terminal::new(TestBackend::new(100, 30)).unwrap());
        let mut session = Session::new();
        let mut dispatch_line = async |line: &str, key: KeyCode, pending: bool| {
            let action = slash::parse(line).expect("a slash command");
            dispatch(
                &action,
                &terminal,
                &mut Key(key),
                &mut session,
                (&initial, &NoProvider, None),
                (&workspace, &effects),
                (&project, &state),
                None,
                (false, ApprovalPolicy::OnRequest),
                pending,
            )
            .await
        };

        let saved = dispatch_line(
            "/decided add add retries to fetch",
            KeyCode::Char('y'),
            false,
        )
        .await
        .expect("the dispatcher claims /decided add");
        assert!(matches!(saved.status, Status::Notice(_)));
        assert!(saved.lines[0].to_string().contains("Decision saved"));
        let entries = polaris_core::decisions::load(&project);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].what, "add retries to fetch");

        let declined = dispatch_line("/decided add not recorded", KeyCode::Esc, false)
            .await
            .expect("a declined save is still claimed");
        assert!(declined.lines[0].to_string().contains("cancelled"));
        let queued = dispatch_line("/decided add queued", KeyCode::Char('y'), true)
            .await
            .expect("a queued-input refusal is still claimed");
        assert!(matches!(queued.status, Status::Error(_)));
        assert_eq!(polaris_core::decisions::load(&project).len(), 1);

        for line in ["/decided", "/decided list", "/decided add"] {
            assert!(
                dispatch_line(line, KeyCode::Char('y'), false)
                    .await
                    .is_none(),
                "{line} stays with the display path"
            );
        }
    }
}
