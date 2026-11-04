//! TUI session settings and history controls without provider requests.

use super::*;
use polaris_core::{
    conversation_memory::{
        EmbeddingModel, StrictEmbedder, StrictHistory, StrictSummaryProvider, SummaryRequest,
    },
    management_effect::ManagementEffectStateV1,
    session_store::PersistedSession,
};
use polaris_provider::{CompletionRequest, CompletionResponse, Message, ProviderError};
use ratatui::{backend::TestBackend, crossterm::event::KeyCode};
use std::{collections::VecDeque, future::Future, io, pin::Pin, sync::Arc};

struct CurrentProvider;
#[async_trait::async_trait]
impl Provider for CurrentProvider {
    async fn complete(&self, _: CompletionRequest) -> Result<CompletionResponse, ProviderError> {
        panic!("settings never call a provider")
    }
    fn model(&self) -> Option<String> {
        Some("gpt-5.6-sol".into())
    }
    fn set_model(&self, _: &str) {
        panic!("root owns activation after commit")
    }
}

struct Keys(VecDeque<KeyCode>);
impl KeyReader for Keys {
    fn read_key(&mut self) -> io::Result<KeyCode> {
        self.0.pop_front().ok_or_else(|| io::Error::other("no key"))
    }
}

struct Fixture {
    _root: tempfile::TempDir,
    workspace: WorkspaceOwner,
    effects: ManagementEffectStoreV1,
    session: Session,
    initial: SavedSessionSettingsV1,
    terminal: RefCell<Terminal<TestBackend>>,
}
impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let project = root.path().join("project");
        let state = root.path().join("state");
        std::fs::create_dir(&project).unwrap();
        std::fs::create_dir(&state).unwrap();
        std::fs::set_permissions(
            &state,
            <std::fs::Permissions as std::os::unix::fs::PermissionsExt>::from_mode(0o700),
        )
        .unwrap();
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
            root.path(),
            &state.join("memory.sqlite3"),
            project.to_str().unwrap(),
            HistoryMode::Legacy,
            None,
        )
        .unwrap();
        let mut session = Session::new();
        session.attach(persisted).unwrap();
        Self {
            _root: root,
            workspace,
            effects,
            session,
            initial: SavedSessionSettingsV1::new(
                "codex".into(),
                "gpt-5.6-sol".into(),
                "auto".into(),
            )
            .unwrap(),
            terminal: RefCell::new(Terminal::new(TestBackend::new(100, 24)).unwrap()),
        }
    }
    fn call(
        &mut self,
        request: &str,
        key: KeyCode,
        pending: bool,
        factory: Option<&HistoryBackendFactory>,
    ) -> Result<Vec<Line<'static>>, String> {
        self.call_result(request, key, pending, factory)
            .map(|result| result.lines)
    }
    fn call_result(
        &mut self,
        request: &str,
        key: KeyCode,
        pending: bool,
        factory: Option<&HistoryBackendFactory>,
    ) -> Result<CommandResult, String> {
        handle(
            request,
            &self.terminal,
            &mut Keys(VecDeque::from([key])),
            &mut self.session,
            &self.initial,
            &self.workspace,
            &self.effects,
            &CurrentProvider,
            factory,
            pending,
        )
    }
    fn snapshot(&self) -> ConversationSnapshot {
        self.session
            .persistence
            .as_ref()
            .unwrap()
            .snapshot()
            .unwrap()
    }
    fn refuse(&mut self, request: &str) {
        let before = self.snapshot().state;
        assert!(
            self.call(request, KeyCode::Char('y'), false, None).is_err(),
            "{request}"
        );
        assert_eq!(self.snapshot().state, before, "{request}");
    }
}

#[test]
fn only_committed_settings_changes_request_binding_refresh() {
    let mut fixture = Fixture::new();
    let before = fixture.snapshot().state;
    for request in ["session", "session current", "session history", "goal"] {
        let result = fixture
            .call_result(request, KeyCode::Esc, true, None)
            .unwrap();
        assert!(!result.settings_changed, "{request}");
        assert!(!result.lines.is_empty(), "{request}");
    }
    let cancelled = fixture
        .call_result("memory configure on", KeyCode::Esc, false, None)
        .unwrap();
    assert!(!cancelled.settings_changed);
    assert_eq!(fixture.snapshot().state, before);
    let committed = fixture
        .call_result("memory configure on", KeyCode::Char('y'), false, None)
        .unwrap();
    assert!(committed.settings_changed);
    assert!(fixture.snapshot().state.session_settings.unwrap().remember);
    let goal = fixture
        .call_result("goal set finish", KeyCode::Char('y'), false, None)
        .unwrap();
    assert!(!goal.settings_changed);
    assert!(fixture.snapshot().state.goal.unwrap().active);
}

#[test]
fn m3c_helper_settings_goal_rows_save_normal_values_and_refuse_unique_inputs() {
    let mut fixture = Fixture::new();
    for request in [
        "memory configure on",
        "memory configure off",
        "tool-memory history",
        "tool-memory retrieval",
        "tool-memory off",
        "tool-memory endpoint http://localhost:8080/v1 embed-1",
        "tool-memory model embed-2",
        "session configure codex gpt-6-astra auto",
        "goal set 実装を完了する",
        "goal stop",
        "goal resume",
        "goal clear",
    ] {
        fixture
            .call(request, KeyCode::Char('y'), false, None)
            .unwrap();
    }
    let state = fixture.snapshot().state;
    let saved = state.session_settings.unwrap();
    assert!(!saved.remember);
    assert_eq!(saved.tool_memory, ToolMemoryModeV1::Off);
    assert_eq!(saved.tool_memory_embedding.unwrap().model, "embed-2");
    assert_eq!(saved.model, "gpt-6-astra");
    assert_eq!(CurrentProvider.model().as_deref(), Some("gpt-5.6-sol"));
    assert!(!state.goal.unwrap().active);
    for request in [
        "memory configure unsupported",
        "tool-memory external",
        "tool-memory endpoint https://example.com/v1 model",
        "tool-memory model",
        "compact threshold 1000",
        "compact threshold off",
        "session configure other-provider gpt-6-astra auto",
        "session configure codex unavailable-model auto",
        "session configure codex gpt-6-astra high",
        "goal set",
    ] {
        fixture.refuse(request);
    }
    let before = fixture.snapshot().state;
    assert!(
        fixture
            .call("memory configure on", KeyCode::Char('y'), true, None)
            .is_err()
    );
    assert_eq!(fixture.snapshot().state, before);
    fixture
        .call("tool-memory endpoint off", KeyCode::Char('y'), false, None)
        .unwrap();
    fixture.refuse("tool-memory model embed-3");
    assert!(
        fixture
            .effects
            .effects()
            .unwrap()
            .iter()
            .all(|effect| matches!(effect.state, ManagementEffectStateV1::Completed { .. }))
    );
}

#[test]
fn saved_settings_written_with_a_compact_threshold_still_load_and_set_no_threshold() {
    use crate::session_activation::{
        MemoryActivation, apply_compaction_and_memory, read_saved_activation,
    };
    let mut fixture = Fixture::new();
    for request in ["memory configure on", "memory configure off"] {
        fixture
            .call(request, KeyCode::Char('y'), false, None)
            .unwrap();
    }
    // The file as it was written before the threshold was removed.
    let path = fixture
        .session
        .persistence
        .as_ref()
        .unwrap()
        .store
        .directory()
        .join("state.json");
    let mut state: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    state["session_settings"]["compact_threshold"] = serde_json::json!(5000);
    std::fs::write(&path, serde_json::to_vec(&state).unwrap()).unwrap();

    assert!(
        read_saved_activation(&fixture.session, &fixture.initial, "gpt-5.6-sol", false)
            .expect("the old file loads")
            .is_some()
    );
    let settings = fixture.snapshot().state.session_settings.unwrap();
    let mut session = fixture.session.clone();
    let any = std::path::Path::new(".");
    apply_compaction_and_memory(
        &mut session,
        any,
        any,
        any,
        &MemoryActivation::from_saved(&settings),
    )
    .unwrap();
    assert_eq!(session.compaction_threshold, None);
}

/// The process a conversation is opened in: it started on `codex`, and
/// `endpoint` stands in for where the local servers would be.
async fn reopen(
    fixture: &Fixture,
    process: &mut crate::session_activation::fixtures::Process,
    endpoint: &str,
) -> (Option<String>, SavedSessionSettingsV1) {
    use crate::{
        model_menu::LocalEndpoints,
        session_activation::{apply_saved_binding, read_saved_activation},
    };
    let started = SavedSessionSettingsV1::new(
        process.provider_name.clone(),
        process.model_name.clone(),
        "auto".into(),
    )
    .unwrap();
    let activation = read_saved_activation(&fixture.session, &started, &process.model_name, true)
        .unwrap()
        .expect("a saved conversation");
    let endpoints = LocalEndpoints {
        ollama: endpoint,
        lm_studio: endpoint,
    };
    let mut effective = started;
    let needs = apply_saved_binding(
        &activation,
        true,
        &mut effective,
        &mut process.binding(&endpoints),
    )
    .await;
    (needs, effective)
}

#[tokio::test]
async fn resuming_a_conversation_saved_with_a_local_model_selects_it_before_the_first_turn() {
    use crate::session_activation::fixtures::{Process, closed_endpoint, fake_ollama};
    let mut fixture = Fixture::new();
    // Its first saved setting records the binding the conversation was held with.
    fixture.initial =
        SavedSessionSettingsV1::new("ollama".into(), "qwen3:8b".into(), String::new()).unwrap();
    fixture
        .call("memory configure off", KeyCode::Char('y'), false, None)
        .unwrap();
    let recorded = fixture.snapshot().state.session_settings.unwrap();
    assert_eq!(
        (recorded.provider.as_str(), recorded.model.as_str()),
        ("ollama", "qwen3:8b")
    );

    // A process that started on the hosted provider and can reach the server.
    let endpoint = fake_ollama(&["qwen3:8b"], true).await;
    let mut process = Process::started_on("codex", "gpt-6.1-sol");
    let (needs, effective) = reopen(&fixture, &mut process, &endpoint).await;
    assert_eq!(needs, None);
    assert_eq!(process.provider_name, "ollama");
    assert_eq!(process.model_name, "qwen3:8b");
    assert_eq!(process.provider.model().as_deref(), Some("qwen3:8b"));
    assert_eq!(effective, recorded);

    // One that cannot reach it keeps its binding and is told what is needed.
    let closed = closed_endpoint().await;
    let mut process = Process::started_on("codex", "gpt-6.1-sol");
    let (needs, _) = reopen(&fixture, &mut process, &closed).await;
    let line = needs.expect("the conversation cannot be served here");
    assert!(
        line.contains("ollama") && line.contains("qwen3:8b"),
        "{line}"
    );
    assert_eq!(process.provider_name, "codex");
    assert!(Arc::ptr_eq(&process.provider, &process.startup));
}

#[tokio::test]
async fn a_conversation_saved_with_the_hosted_model_opens_on_it_in_a_hosted_process() {
    use crate::session_activation::fixtures::{Process, closed_endpoint};
    let mut fixture = Fixture::new();
    fixture
        .call(
            "session configure codex gpt-6-astra auto",
            KeyCode::Char('y'),
            false,
            None,
        )
        .unwrap();
    let mut process = Process::started_on("codex", "gpt-6.1-sol");
    let (needs, _) = reopen(&fixture, &mut process, &closed_endpoint().await).await;
    assert_eq!(needs, None);
    assert_eq!(process.provider_name, "codex");
    assert_eq!(process.model_name, "gpt-6-astra");
    assert_eq!(process.provider.model().as_deref(), Some("gpt-6-astra"));
}

/// What `/model` and every send do: save the binding in use as the one the
/// chat is held with. No settings command is involved.
fn record(
    fixture: &Fixture,
    (provider, model): (&str, &str),
    endpoint: Option<&polaris_core::session_settings::SavedEndpointV1>,
) {
    crate::binding_record::record(
        (
            &fixture.session,
            &fixture.initial,
            &fixture.workspace,
            &fixture.effects,
        ),
        provider,
        model,
        None,
        endpoint,
    )
    .unwrap();
}

#[tokio::test]
async fn a_chat_held_with_a_model_chosen_by_model_is_reopened_on_it_or_sends_nothing() {
    use crate::session_activation::fixtures::{Process, closed_endpoint, fake_ollama};
    let fixture = Fixture::new();
    record(&fixture, ("ollama", "qwen3:8b"), None);
    // No settings were ever saved; the record is all the chat has.
    assert_eq!(fixture.snapshot().state.session_settings, None);

    // A hosted process that can reach the server opens it on the local model.
    let endpoint = fake_ollama(&["qwen3:8b"], true).await;
    let mut process = Process::started_on("codex", "gpt-6.1-sol");
    let (needs, _) = reopen(&fixture, &mut process, &endpoint).await;
    assert_eq!(needs, None);
    assert_eq!(process.provider_name, "ollama");
    assert_eq!(process.model_name, "qwen3:8b");
    assert_eq!(process.provider.model().as_deref(), Some("qwen3:8b"));

    // One that cannot keeps its hosted provider, and the send gate then sends
    // nothing: the input is back in the box and the line is shown.
    let closed = closed_endpoint().await;
    let mut process = Process::started_on("codex", "gpt-6.1-sol");
    let (needs, _) = reopen(&fixture, &mut process, &closed).await;
    let line = needs.expect("the chat cannot be served here");
    assert!(
        line.contains("ollama") && line.contains("qwen3:8b"),
        "{line}"
    );
    assert_eq!(process.provider_name, "codex");
    assert!(Arc::ptr_eq(&process.provider, &process.startup));
    let mut buffer = String::new();
    let mut cursor = 0;
    let mut status = crate::render::Status::Idle;
    crate::refuse_before_send(
        &line,
        "fix the parser".into(),
        14,
        false,
        &mut buffer,
        &mut cursor,
        &mut VecDeque::new(),
        &mut false,
        &mut status,
    );
    assert_eq!((buffer.as_str(), cursor), ("fix the parser", 14));
    assert!(matches!(status, crate::render::Status::Error(shown) if shown == line));
}

#[tokio::test]
async fn a_chat_with_saved_settings_and_a_record_opens_on_the_record() {
    use crate::session_activation::fixtures::{Process, fake_ollama};
    let mut fixture = Fixture::new();
    // The settings were confirmed while the hosted model was in use ...
    fixture
        .call(
            "session configure codex gpt-6-astra auto",
            KeyCode::Char('y'),
            false,
            None,
        )
        .unwrap();
    // ... and `/model` moved the chat to a local model afterwards.
    record(&fixture, ("ollama", "qwen3:8b"), None);
    let state = fixture.snapshot().state;
    assert_eq!(state.session_settings.unwrap().model, "gpt-6-astra");
    assert_eq!(state.held_with.unwrap().model, "qwen3:8b");

    let endpoint = fake_ollama(&["qwen3:8b"], true).await;
    let mut process = Process::started_on("codex", "gpt-6.1-sol");
    assert_eq!(reopen(&fixture, &mut process, &endpoint).await.0, None);
    assert_eq!(process.provider_name, "ollama");
    assert_eq!(process.model_name, "qwen3:8b");
}

#[tokio::test]
async fn the_record_follows_a_later_model_choice_when_the_chat_is_reopened() {
    use crate::session_activation::fixtures::{Process, fake_ollama};
    let fixture = Fixture::new();
    let endpoint = fake_ollama(&["qwen3:8b"], true).await;
    record(&fixture, ("ollama", "qwen3:8b"), None);
    let mut process = Process::started_on("codex", "gpt-6.1-sol");
    assert_eq!(reopen(&fixture, &mut process, &endpoint).await.0, None);
    assert_eq!(process.provider_name, "ollama");

    // `/model` moved the chat back to a hosted model. The server is still up,
    // but the chat is no longer held with it.
    record(&fixture, ("codex", "gpt-6-astra"), None);
    let mut process = Process::started_on("codex", "gpt-6.1-sol");
    assert_eq!(reopen(&fixture, &mut process, &endpoint).await.0, None);
    assert_eq!(process.provider_name, "codex");
    assert_eq!(process.model_name, "gpt-6-astra");
    assert!(!process.bound_local_endpoint());
}

#[tokio::test]
async fn a_chat_held_through_a_local_openai_compatible_endpoint_is_not_served_by_the_default_one() {
    use crate::session_activation::fixtures::{Process, closed_endpoint};
    use polaris_core::session_settings::SavedEndpointV1;
    let fixture = Fixture::new();
    let local = SavedEndpointV1::from_base_url(Some("http://127.0.0.1:8080/v1"));
    record(&fixture, ("openai", "local-model"), Some(&local));
    let closed = closed_endpoint().await;

    let mut process =
        Process::started_on("openai", "local-model").sending_to(SavedEndpointV1::Default);
    let line = reopen(&fixture, &mut process, &closed)
        .await
        .0
        .expect("api.openai.com is not where this chat was held");
    assert!(line.contains("http://127.0.0.1:8080/v1"), "{line}");

    let mut process = Process::started_on("openai", "local-model").sending_to(local);
    assert_eq!(reopen(&fixture, &mut process, &closed).await.0, None);
}

#[test]
fn m3c_helper_cancel_records_denial_and_persistence_error_refuses_changes() {
    let mut fixture = Fixture::new();
    let before = fixture.snapshot().state;
    let lines = fixture
        .call("memory configure on", KeyCode::Esc, false, None)
        .unwrap();
    assert!(
        lines
            .iter()
            .any(|line| line.to_string().contains("cancelled"))
    );
    assert_eq!(fixture.snapshot().state, before);
    assert!(matches!(
        fixture.effects.effects().unwrap()[0].state,
        ManagementEffectStateV1::Denied { .. }
    ));
    fixture.session.persistence_error = Some("saved append failure".into());
    fixture.refuse("memory configure on");
}

type StrictFuture<'a, T> = Pin<Box<dyn Future<Output = io::Result<T>> + Send + 'a>>;
struct NoInference {
    metadata: EmbeddingModel,
}
impl StrictSummaryProvider for NoInference {
    fn summarize<'a>(&'a self, _: SummaryRequest) -> StrictFuture<'a, String> {
        panic!("history initialization never summarizes")
    }
}
impl StrictEmbedder for NoInference {
    fn metadata(&self) -> &EmbeddingModel {
        &self.metadata
    }
    fn embed_passage<'a>(&'a self, _: &'a str) -> StrictFuture<'a, Vec<f32>> {
        panic!("history initialization never embeds")
    }
    fn embed_query<'a>(&'a self, _: &'a str) -> StrictFuture<'a, Vec<Vec<f32>>> {
        panic!("history initialization never retrieves")
    }
}

#[test]
fn m3c_helper_history_requires_backend_and_initializes_only_empty_conversations() {
    let mut fixture = Fixture::new();
    fixture.refuse("session history strict10");
    assert!(fixture.session.strict_history.is_none());
    let backend = Arc::new(NoInference {
        metadata: EmbeddingModel {
            model: "fixture".into(),
            revision: "v1".into(),
            dimension: 2,
        },
    });
    let history = Arc::new(StrictHistory::new(backend.clone(), backend));
    let factory: HistoryBackendFactory =
        Arc::new(move || Ok((history.clone(), Default::default())));
    let before = fixture.snapshot().state;
    fixture
        .call(
            "session history strict10",
            KeyCode::Esc,
            false,
            Some(&factory),
        )
        .unwrap();
    assert_eq!(fixture.snapshot().state, before);
    assert!(fixture.session.strict_history.is_none());
    fixture
        .call(
            "session history strict10",
            KeyCode::Char('y'),
            false,
            Some(&factory),
        )
        .unwrap();
    assert_eq!(fixture.snapshot().state.history_mode, HistoryMode::Strict10);
    assert!(fixture.session.strict_history.is_some());
    fixture
        .call(
            "memory configure structured10",
            KeyCode::Char('y'),
            false,
            Some(&factory),
        )
        .unwrap();
    assert_eq!(
        fixture.snapshot().state.history_mode,
        HistoryMode::Structured10
    );
    fixture
        .session
        .persistence
        .as_ref()
        .unwrap()
        .append(Message::user("actual history"), true, None)
        .unwrap();
    fixture.refuse("session history legacy");
}

#[test]
fn m3c_helper_read_views_work_while_input_is_queued_and_the_transcript_page_is_gone() {
    let mut fixture = Fixture::new();
    assert!(
        fixture
            .call("session current", KeyCode::Esc, true, None)
            .unwrap()
            .iter()
            .any(|line| line.to_string().contains("gpt-5.6-sol"))
    );
    assert!(
        fixture
            .call("goal current", KeyCode::Esc, true, None)
            .unwrap()[0]
            .to_string()
            .contains("No saved goal")
    );
    // `/history` was removed from the TUI: no transcript page is served.
    for request in ["history", "history 0", "history 20"] {
        fixture.refuse(request);
    }
    assert!(fixture.effects.effects().unwrap().is_empty());
}
