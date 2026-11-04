//! 保存会話の設定とプロセス所有サービスを実行中のセッションへ適用する。

use std::path::Path;
use std::sync::Arc;

#[cfg(unix)]
use polaris_core::local_execution::LocalRouter;
use polaris_core::session::Session;
#[cfg(unix)]
use polaris_core::session_settings::{SavedSessionSettingsV1, ToolMemoryModeV1};
use polaris_core::tool_memory::RetentionMode;
use polaris_provider::Provider;
#[cfg(unix)]
use polaris_provider::UsageMeter;

#[cfg(unix)]
use crate::model_menu::{self, LocalEndpoints, LocalMiss};
#[cfg(unix)]
use crate::{draft::DraftAutosave, input};
use crate::{memory, persist, tool_memory};

#[cfg(unix)]
pub(super) struct SavedActivationRead {
    settings: SavedSessionSettingsV1,
    /// False when the conversation never saved settings: `settings` is then
    /// only the live binding, and there is nothing to follow.
    recorded: bool,
    draft: Option<String>,
}

#[cfg(unix)]
pub(super) fn read_saved_activation(
    session: &Session,
    initial_settings: &SavedSessionSettingsV1,
    current_model: &str,
    switched: bool,
) -> Result<Option<SavedActivationRead>, String> {
    let Some(saved) = &session.persistence else {
        return Ok(None);
    };
    let snapshot = saved
        .snapshot()
        .map_err(|error| format!("Cannot restore conversation settings: {error}"))?;
    let recorded = snapshot.state.session_settings.is_some();
    let settings = snapshot.state.session_settings.unwrap_or_else(|| {
        let mut initial = initial_settings.clone();
        initial.model = current_model.to_owned();
        initial
    });
    let draft = switched.then_some(snapshot.state.draft.text);
    Ok(Some(SavedActivationRead {
        settings,
        recorded,
        draft,
    }))
}

/// The provider and model the run uses, which `/model` also edits, and what
/// this process can still switch to. Borrowed from `run`; none of it is
/// conversation state.
#[cfg(unix)]
pub(super) struct ModelBinding<'a> {
    pub(super) provider: &'a mut Arc<dyn Provider>,
    pub(super) provider_name: &'a mut String,
    pub(super) model_name: &'a mut String,
    pub(super) effort_name: &'a mut String,
    /// What the process started on, which `/model`'s hosted rows switch back to.
    pub(super) startup_provider: &'a Arc<dyn Provider>,
    pub(super) startup_name: &'a str,
    pub(super) startup_effort: &'a str,
    pub(super) usage_meter: &'a UsageMeter,
    pub(super) local_router: &'a LocalRouter,
    pub(super) endpoints: &'a LocalEndpoints<'a>,
}

/// Whether a change of conversation opens one. Opening selects what the
/// conversation was saved with and decides afresh whether this process can
/// serve it, so the line saying it cannot is dropped here.
///
/// `carried` is a fork or rewind child of the conversation that was open. It
/// copies the parent's record, which `/model` never rewrites, so selecting that
/// record would undo the person's choice: it continues the binding in use, and
/// a parent that could not be served leaves its child unserved. Anything else
/// that changes the conversation, such as `/resume`, opens it. `switched` is
/// false for a refresh of the same conversation, which opens nothing.
#[cfg(unix)]
pub(super) fn opens_conversation(
    switched: bool,
    carried: bool,
    unserved: &mut Option<String>,
) -> bool {
    let open = switched && !carried;
    if open {
        *unserved = None;
    }
    open
}

/// Selects the provider and model a conversation was saved with, when this
/// process can serve them: a local model through the same selection and bind
/// as `/model`'s `Local model`, a hosted one when it is the provider the
/// process started on. When it cannot, the binding stays as it was and the
/// result is the one line saying what the conversation needs. The caller then
/// sends nothing: a conversation held with a local model must not reach a
/// hosted provider unless the person chooses that in `/model`.
///
/// `open` marks a conversation opened from elsewhere (see
/// `opens_conversation`). Otherwise, for a refresh or a fork or rewind child,
/// only a changed saved model on the provider already in use is applied; a
/// switch to another provider that the person made with `/model` stands.
#[cfg(unix)]
pub(super) async fn apply_saved_binding(
    activation: &SavedActivationRead,
    open: bool,
    effective_settings: &mut SavedSessionSettingsV1,
    live: &mut ModelBinding<'_>,
) -> Option<String> {
    effective_settings.clone_from(&activation.settings);
    let saved = &activation.settings;
    if !activation.recorded
        || (*live.provider_name == saved.provider && *live.model_name == saved.model)
    {
        return None;
    }
    if let Some((runtime, endpoint)) = live.endpoints.runtime_for(&saved.provider) {
        if !open {
            return None;
        }
        return match model_menu::bind_saved_local_model(
            runtime,
            endpoint,
            &saved.model,
            live.usage_meter,
            live.local_router,
        )
        .await
        {
            Ok(bound) => {
                *live.provider = bound;
                live.provider_name.clone_from(&saved.provider);
                live.model_name.clone_from(&saved.model);
                live.effort_name.clear();
                None
            }
            Err(miss) => Some(needs_local(saved, miss)),
        };
    }
    let on_startup_provider = *live.provider_name == live.startup_name;
    if saved.provider == live.startup_name && (open || on_startup_provider) {
        live.startup_provider.set_model(&saved.model);
        if live.startup_provider.model().as_deref() == Some(saved.model.as_str()) {
            if !on_startup_provider {
                live.provider_name.clone_from(&saved.provider);
                live.effort_name.clear();
                live.effort_name.push_str(live.startup_effort);
            }
            *live.provider = live.startup_provider.clone();
            live.model_name.clone_from(&saved.model);
            return None;
        }
    }
    open.then(|| needs_restart(saved))
}

#[cfg(unix)]
fn needs_local(saved: &SavedSessionSettingsV1, miss: LocalMiss) -> String {
    let what = format!("This chat needs {} ({})", saved.provider, saved.model);
    match miss {
        LocalMiss::NotReachable => format!(
            "{what}, but its server is not reachable. Start it, then choose the model with /model (Local model)."
        ),
        LocalMiss::NotListed => format!(
            "{what}, but its server does not list that model. Add it, then choose it with /model (Local model)."
        ),
        LocalMiss::Unusable(why) => {
            format!("{what}, but it cannot be used here ({why}). Choose another model with /model.")
        }
    }
}

#[cfg(unix)]
fn needs_restart(saved: &SavedSessionSettingsV1) -> String {
    let how = match saved.provider.as_str() {
        "llamacpp" => format!("POLARIS_PROVIDER=llamacpp and --model {}", saved.model),
        provider => format!("POLARIS_PROVIDER={provider}"),
    };
    format!(
        "This chat needs {} ({}). Restart with {how}, or choose a model with /model.",
        saved.provider, saved.model
    )
}

#[cfg(unix)]
pub(super) struct SavedDraftTarget<'a> {
    pub(super) input_buffer: &'a mut String,
    pub(super) input_cursor: &'a mut usize,
    pub(super) input_history: &'a mut input::History,
    pub(super) autosave: &'a mut DraftAutosave,
}

#[cfg(unix)]
pub(super) fn apply_saved_draft(activation: SavedActivationRead, target: SavedDraftTarget<'_>) {
    let Some(draft) = activation.draft else {
        return;
    };
    *target.input_buffer = draft;
    *target.input_cursor = target.input_buffer.len();
    *target.input_history = input::History::new();
    target.autosave.restored(target.input_buffer);
}

pub(super) struct ProcessServices<'a> {
    pub(super) decider: &'a Option<Arc<dyn polaris_provider::decision::Decider>>,
    pub(super) tier_decider: &'a Option<Arc<dyn polaris_core::tier::TierDecider>>,
    pub(super) light_model: &'a Option<Arc<dyn Provider>>,
    pub(super) approval_auto: polaris_core::approval::AutoApproval,
    pub(super) setting: &'a Arc<polaris_core::recipe::tool::SettingTool>,
    pub(super) computer: &'a Option<Arc<polaris_core::computer::ComputerTool>>,
    /// Makes the `turn-only` memory with the writer and embedder the process
    /// chose. Every session the TUI builds gets it here, so a session that
    /// replaces another (`/new`, `/resume`, rewind, sidechat) keeps them.
    pub(super) turn_memory: &'a Option<polaris_core::turn_memory::Factory>,
}

pub(super) fn attach_process_services(session: &mut Session, services: ProcessServices<'_>) {
    session.decider = services.decider.clone();
    session.tier_decider = services.tier_decider.clone();
    session.light_model = services.light_model.clone();
    session.approval_auto = services.approval_auto;
    session.setting = Some(services.setting.clone());
    session.computer = services.computer.clone();
    session.turn_memory_factory = services.turn_memory.clone();
}

pub(super) struct MemoryActivation {
    tool_mode: Option<RetentionMode>,
    embedding: Option<(String, String)>,
    remember: bool,
}

impl MemoryActivation {
    pub(super) fn unverified_model_allocation(&self) -> bool {
        self.tool_mode.is_some() && self.embedding.is_some()
    }

    #[cfg(unix)]
    pub(super) fn from_saved(settings: &SavedSessionSettingsV1) -> Self {
        let tool_mode = match settings.tool_memory {
            ToolMemoryModeV1::Off => None,
            ToolMemoryModeV1::History => Some(RetentionMode::History),
            ToolMemoryModeV1::Retrieval => Some(RetentionMode::Retrieval),
        };
        Self {
            tool_mode,
            embedding: settings
                .tool_memory_embedding
                .as_ref()
                .map(|pair| (pair.url.clone(), pair.model.clone())),
            remember: settings.remember,
        }
    }

    #[cfg(not(unix))]
    pub(super) fn from_runtime(
        tool_mode: Option<RetentionMode>,
        embedding: Option<(String, String)>,
        remember: bool,
    ) -> Self {
        Self {
            tool_mode,
            embedding,
            remember,
        }
    }

    #[cfg(unix)]
    pub(super) fn tool_memory_enabled(&self) -> bool {
        self.tool_mode.is_some()
    }

    #[cfg(unix)]
    pub(super) fn embedding(&self) -> Option<&(String, String)> {
        self.embedding.as_ref()
    }
}

pub(super) fn apply_tool_memory(
    session: &mut Session,
    project: &Path,
    state_dir: &Path,
    session_path: &Path,
    active_path: Option<&Path>,
    settings: &MemoryActivation,
) -> Result<bool, String> {
    if active_path != Some(session_path)
        || (settings.tool_mode.is_some()
            && session.tool_memory.is_none()
            && memory::saved_project_matches(session, project, session_path))
    {
        configure_saved_tool_memory(
            session,
            project,
            state_dir,
            session_path,
            settings.tool_mode,
            settings.embedding.clone(),
        )
        .map_err(|error| format!("Cannot prepare tool memory: {error}"))?;
        return Ok(true);
    }
    Ok(false)
}

pub(super) fn apply_compaction_and_memory(
    session: &mut Session,
    project: &Path,
    state_dir: &Path,
    session_path: &Path,
    settings: &MemoryActivation,
) -> Result<(), String> {
    if settings.remember {
        memory::configure_saved_memory(session, project, state_dir, session_path)
            .map_err(|error| format!("Cannot prepare history storage: {error}"))?;
    } else if session.persistence.is_some() {
        session.before_compact = None;
    } else if session.persistence.is_none()
        && session.before_compact.is_none()
        && let Some(id) = session_path.file_stem().and_then(|value| value.to_str())
    {
        // Without this the legacy log is rewritten over the summary and
        // the compacted prefix is gone from memory and disk at once.
        memory::configure_archive(session, project, state_dir, id)
            .map_err(|error| format!("Cannot archive history before compaction: {error}"))?;
    }
    Ok(())
}

// Ancestors are read-only and must form an explicit, bounded same-project chain.
pub(super) const MAX_TOOL_MEMORY_ORIGINS: usize = 64;

pub(super) fn saved_tool_memory_namespace(
    project: &Path,
    path: &Path,
) -> std::io::Result<(String, Vec<String>)> {
    let id = path
        .file_stem()
        .and_then(|s| s.to_str())
        .filter(|s| {
            !s.is_empty()
                && s.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        })
        .ok_or_else(|| std::io::Error::other("invalid session id"))?
        .to_owned();
    let project = polaris_core::project::resolve_root(project);
    let mut seen = std::collections::HashSet::from([id.clone()]);
    let mut origins = Vec::new();
    let mut cursor = path.to_path_buf();
    while let Some(origin) = persist::read_tool_memory_origin(&cursor.with_extension("meta.json"))?
    {
        if origins.len() >= MAX_TOOL_MEMORY_ORIGINS || !seen.insert(origin.clone()) {
            return Err(std::io::Error::other(
                "Memory lineage contains a cycle or exceeds the limit",
            ));
        }
        let source = path.with_file_name(format!("{origin}.jsonl"));
        let source_meta =
            persist::read_meta(&source.with_extension("meta.json")).ok_or_else(|| {
                std::io::Error::other("Cannot verify memory source session provenance")
            })?;
        if !source.is_file()
            || !Path::new(&source_meta.cwd).is_absolute()
            || polaris_core::project::resolve_root(Path::new(&source_meta.cwd)) != project
        {
            return Err(std::io::Error::other(
                "The memory source session is missing or belongs to a different project",
            ));
        }
        origins.push(origin);
        cursor = source;
    }
    Ok((id, origins))
}

pub(super) fn configure_saved_tool_memory(
    session: &mut Session,
    project: &Path,
    state_dir: &Path,
    session_path: &Path,
    mode: Option<RetentionMode>,
    embedding: Option<(String, String)>,
) -> std::io::Result<()> {
    session.tool_memory = None;
    if let Some(mode) = mode
        && memory::saved_project_matches(session, project, session_path)
    {
        if let Some(persistence) = session.persistence.as_ref() {
            let snapshot = persistence.snapshot()?;
            let mut origins = snapshot
                .state
                .ancestors
                .iter()
                .map(|ancestor| ancestor.session_id.clone())
                .collect::<Vec<_>>();
            origins.sort();
            origins.dedup();
            if origins.len() > MAX_TOOL_MEMORY_ORIGINS {
                return Err(std::io::Error::other("Memory lineage exceeds the limit"));
            }
            if let Some(branch) = persistence.store.rewind_branch_metadata()? {
                if !origins.contains(&branch.origin_session_id) {
                    return Err(std::io::Error::other(
                        "Rewind fork memory provenance does not match saved lineage",
                    ));
                }
                tool_memory::configure_tool_memory_with_origin_allowlist(
                    session,
                    project,
                    state_dir,
                    &snapshot.state.session_id,
                    &origins,
                    &branch.visible_memory_raw_object_ids,
                    mode,
                    embedding,
                )?;
            } else {
                tool_memory::configure_tool_memory_with_origins(
                    session,
                    project,
                    state_dir,
                    &snapshot.state.session_id,
                    &origins,
                    mode,
                    embedding,
                )?;
            }
            return Ok(());
        }
        let (id, origins) = saved_tool_memory_namespace(project, session_path)?;
        tool_memory::configure_tool_memory_with_origins(
            session, project, state_dir, &id, &origins, mode, embedding,
        )?;
    }
    Ok(())
}

/// Stand-ins for binding tests: a process that started on a hosted provider,
/// and a loopback server that answers Ollama's two metadata routes.
#[cfg(all(test, unix))]
pub(crate) mod fixtures {
    use super::*;
    use polaris_provider::{CompletionRequest, CompletionResponse, ProviderError};
    use std::sync::Mutex;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    /// Switches model the way codex and openai do.
    pub(crate) struct Hosted(Mutex<String>);
    impl Hosted {
        pub(crate) fn shared(model: &str) -> Arc<dyn Provider> {
            Arc::new(Self(Mutex::new(model.to_owned())))
        }
    }
    #[async_trait::async_trait]
    impl Provider for Hosted {
        async fn complete(
            &self,
            _: CompletionRequest,
        ) -> Result<CompletionResponse, ProviderError> {
            panic!("choosing a binding never sends")
        }
        fn model(&self) -> Option<String> {
            Some(self.0.lock().unwrap().clone())
        }
        fn set_model(&self, model: &str) {
            *self.0.lock().unwrap() = model.to_owned();
        }
    }

    /// What `run` holds for the binding, as a process started on `name`.
    pub(crate) struct Process {
        pub(crate) startup: Arc<dyn Provider>,
        startup_name: String,
        pub(crate) provider: Arc<dyn Provider>,
        pub(crate) provider_name: String,
        pub(crate) model_name: String,
        pub(crate) effort_name: String,
        usage: UsageMeter,
        router: LocalRouter,
    }

    impl Process {
        pub(crate) fn started_on(name: &str, model: &str) -> Self {
            let startup = Hosted::shared(model);
            Self {
                provider: startup.clone(),
                startup,
                startup_name: name.to_owned(),
                provider_name: name.to_owned(),
                model_name: model.to_owned(),
                effort_name: "high".to_owned(),
                usage: UsageMeter::default(),
                router: LocalRouter::new(1),
            }
        }

        pub(crate) fn binding<'a>(
            &'a mut self,
            endpoints: &'a LocalEndpoints<'a>,
        ) -> ModelBinding<'a> {
            ModelBinding {
                provider: &mut self.provider,
                provider_name: &mut self.provider_name,
                model_name: &mut self.model_name,
                effort_name: &mut self.effort_name,
                startup_provider: &self.startup,
                startup_name: &self.startup_name,
                startup_effort: "high",
                usage_meter: &self.usage,
                local_router: &self.router,
                endpoints,
            }
        }

        pub(crate) fn bound_local_endpoint(&self) -> bool {
            self.router.has_bound_endpoints()
        }
    }

    /// An Ollama-shaped server on a free loopback port; returns its endpoint.
    pub(crate) async fn fake_ollama(models: &'static [&'static str], tools: bool) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}/", listener.local_addr().unwrap());
        tokio::spawn(async move {
            while let Ok((mut stream, _)) = listener.accept().await {
                tokio::spawn(async move {
                    let mut request = Vec::new();
                    let mut chunk = [0u8; 1024];
                    let (head_end, body_len) = loop {
                        let read = stream.read(&mut chunk).await.unwrap_or(0);
                        if read == 0 {
                            return;
                        }
                        request.extend_from_slice(&chunk[..read]);
                        if let Some(end) = request.windows(4).position(|w| w == b"\r\n\r\n") {
                            let head = String::from_utf8_lossy(&request[..end]).to_lowercase();
                            let body_len = head
                                .lines()
                                .find_map(|line| line.strip_prefix("content-length:"))
                                .and_then(|value| value.trim().parse::<usize>().ok())
                                .unwrap_or(0);
                            break (end + 4, body_len);
                        }
                    };
                    while request.len() < head_end + body_len {
                        let read = stream.read(&mut chunk).await.unwrap_or(0);
                        if read == 0 {
                            break;
                        }
                        request.extend_from_slice(&chunk[..read]);
                    }
                    let request_line = String::from_utf8_lossy(&request)
                        .lines()
                        .next()
                        .unwrap_or_default()
                        .to_owned();
                    let body = if request_line.starts_with("GET /api/tags ") {
                        serde_json::json!({"models": models.iter().map(|name| serde_json::json!({"name": name})).collect::<Vec<_>>()})
                    } else if request_line.starts_with("POST /api/show ") {
                        serde_json::json!({"capabilities": if tools { vec!["completion", "tools"] } else { vec!["completion"] }})
                    } else {
                        serde_json::json!({})
                    }
                    .to_string();
                    let response = format!(
                        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    let _ = stream.write_all(response.as_bytes()).await;
                    let _ = stream.shutdown().await;
                });
            }
        });
        endpoint
    }

    /// An endpoint nothing listens on.
    pub(crate) async fn closed_endpoint() -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        format!("http://{}/", listener.local_addr().unwrap())
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::fixtures::{Process, closed_endpoint, fake_ollama};
    use super::*;

    fn saved(provider: &str, model: &str) -> SavedActivationRead {
        SavedActivationRead {
            settings: SavedSessionSettingsV1::new(provider.into(), model.into(), "auto".into())
                .unwrap(),
            recorded: true,
            draft: None,
        }
    }

    async fn apply(
        process: &mut Process,
        activation: &SavedActivationRead,
        open: bool,
        endpoint: &str,
    ) -> Option<String> {
        let endpoints = LocalEndpoints {
            ollama: endpoint,
            lm_studio: endpoint,
        };
        let mut effective = activation.settings.clone();
        apply_saved_binding(
            activation,
            open,
            &mut effective,
            &mut process.binding(&endpoints),
        )
        .await
    }

    /// A conversation opened from elsewhere, as `/resume` does.
    async fn open(
        process: &mut Process,
        activation: &SavedActivationRead,
        endpoint: &str,
    ) -> Option<String> {
        apply(process, activation, true, endpoint).await
    }

    #[tokio::test]
    async fn a_saved_local_model_is_selected_through_the_local_router() {
        let endpoint = fake_ollama(&["qwen3:8b"], true).await;
        let mut process = Process::started_on("codex", "gpt-6.1-sol");
        let startup = process.startup.clone();
        let needs = open(&mut process, &saved("ollama", "qwen3:8b"), &endpoint).await;
        assert_eq!(needs, None);
        assert_eq!(process.provider_name, "ollama");
        assert_eq!(process.model_name, "qwen3:8b");
        assert_eq!(process.provider.model().as_deref(), Some("qwen3:8b"));
        assert!(!Arc::ptr_eq(&process.provider, &startup));
        assert!(
            process.effort_name.is_empty(),
            "a local model has no effort"
        );
        assert!(process.bound_local_endpoint());
        // Choosing a local model never retunes the hosted provider.
        assert_eq!(startup.model().as_deref(), Some("gpt-6.1-sol"));
    }

    #[tokio::test]
    async fn a_saved_local_model_this_process_cannot_serve_leaves_the_binding_and_says_what_it_needs()
     {
        let closed = closed_endpoint().await;
        let without_model = fake_ollama(&["other:1b"], true).await;
        let without_tools = fake_ollama(&["qwen3:8b"], false).await;
        for (endpoint, reason) in [
            (closed, "not reachable"),
            (without_model, "does not list that model"),
            (without_tools, "tool support"),
        ] {
            let mut process = Process::started_on("codex", "gpt-6.1-sol");
            let startup = process.startup.clone();
            let line = open(&mut process, &saved("ollama", "qwen3:8b"), &endpoint)
                .await
                .expect("the conversation reports what it needs");
            for expected in ["ollama", "qwen3:8b", "/model", reason] {
                assert!(line.contains(expected), "{expected} missing from {line}");
            }
            // Still on the hosted provider the process started on, unchanged.
            assert_eq!(process.provider_name, "codex");
            assert_eq!(process.model_name, "gpt-6.1-sol");
            assert!(Arc::ptr_eq(&process.provider, &startup));
            assert_eq!(startup.model().as_deref(), Some("gpt-6.1-sol"));
            assert!(!process.bound_local_endpoint());
        }
    }

    #[tokio::test]
    async fn a_saved_hosted_model_on_the_started_provider_is_applied() {
        let closed = closed_endpoint().await;
        let mut process = Process::started_on("codex", "gpt-6.1-sol");
        let needs = open(&mut process, &saved("codex", "gpt-6-astra"), &closed).await;
        assert_eq!(needs, None);
        assert_eq!(process.provider_name, "codex");
        assert_eq!(process.model_name, "gpt-6-astra");
        assert_eq!(process.provider.model().as_deref(), Some("gpt-6-astra"));

        // After `/model` moved to a local model, opening another conversation
        // that was held with the hosted one (`/resume`) switches back.
        let endpoint = fake_ollama(&["qwen3:8b"], true).await;
        let mut process = Process::started_on("codex", "gpt-6.1-sol");
        assert_eq!(
            open(&mut process, &saved("ollama", "qwen3:8b"), &endpoint).await,
            None
        );
        let needs = open(&mut process, &saved("codex", "gpt-6-astra"), &closed).await;
        assert_eq!(needs, None);
        assert_eq!(process.provider_name, "codex");
        assert!(Arc::ptr_eq(&process.provider, &process.startup));
        assert_eq!(process.provider.model().as_deref(), Some("gpt-6-astra"));
        assert_eq!(process.effort_name, "high");
    }

    #[tokio::test]
    async fn a_conversation_held_with_another_backend_is_not_switched_to() {
        let closed = closed_endpoint().await;
        for (provider, model, how) in [
            ("openai", "gpt-6-astra", "POLARIS_PROVIDER=openai"),
            (
                "llamacpp",
                "gemma-4b",
                "POLARIS_PROVIDER=llamacpp and --model gemma-4b",
            ),
        ] {
            let mut process = Process::started_on("codex", "gpt-6.1-sol");
            let startup = process.startup.clone();
            let line = open(&mut process, &saved(provider, model), &closed)
                .await
                .expect("it cannot be served here");
            for expected in [provider, model, how, "/model"] {
                assert!(line.contains(expected), "{expected} missing from {line}");
            }
            assert_eq!(process.provider_name, "codex");
            assert!(Arc::ptr_eq(&process.provider, &startup));
            assert_eq!(startup.model().as_deref(), Some("gpt-6.1-sol"));
        }
    }

    #[tokio::test]
    async fn a_conversation_that_saved_nothing_follows_the_live_binding() {
        let closed = closed_endpoint().await;
        let mut process = Process::started_on("codex", "gpt-6.1-sol");
        let mut activation = saved("ollama", "qwen3:8b");
        activation.recorded = false;
        assert_eq!(open(&mut process, &activation, &closed).await, None);
        assert_eq!(process.provider_name, "codex");
        assert_eq!(process.model_name, "gpt-6.1-sol");
    }

    #[tokio::test]
    async fn after_opening_a_switch_made_with_model_stands() {
        let closed = closed_endpoint().await;
        let mut process = Process::started_on("codex", "gpt-6.1-sol");
        let activation = saved("ollama", "qwen3:8b");
        let endpoints = LocalEndpoints {
            ollama: &closed,
            lm_studio: &closed,
        };
        let mut effective = activation.settings.clone();
        let needs = apply_saved_binding(
            &activation,
            false,
            &mut effective,
            &mut process.binding(&endpoints),
        )
        .await;
        assert_eq!(needs, None);
        assert_eq!(process.provider_name, "codex");
        assert_eq!(process.model_name, "gpt-6.1-sol");
        // The saved settings still become the effective ones.
        assert_eq!(effective, activation.settings);
    }

    #[tokio::test]
    async fn a_fork_or_rewind_child_continues_the_binding_in_use_not_its_copied_record() {
        let endpoint = fake_ollama(&["qwen3:8b"], true).await;
        let mut process = Process::started_on("codex", "gpt-6.1-sol");
        let startup = process.startup.clone();
        // The chat was recorded with the hosted model; `/model` then moved it to
        // a local one, which the record does not show.
        assert_eq!(
            open(&mut process, &saved("ollama", "qwen3:8b"), &endpoint).await,
            None
        );
        let local = process.provider.clone();
        let copied = saved("codex", "gpt-6.1-sol");

        // The child copies that record. Selecting it would send the local
        // model's history to the hosted provider without anyone choosing it.
        assert_eq!(apply(&mut process, &copied, false, &endpoint).await, None);
        assert_eq!(process.provider_name, "ollama");
        assert_eq!(process.model_name, "qwen3:8b");
        assert!(Arc::ptr_eq(&process.provider, &local));
        assert_eq!(startup.model().as_deref(), Some("gpt-6.1-sol"));

        // Opening the same record from elsewhere (`/resume`) does follow it.
        assert_eq!(open(&mut process, &copied, &endpoint).await, None);
        assert_eq!(process.provider_name, "codex");
        assert!(Arc::ptr_eq(&process.provider, &startup));
    }

    #[test]
    fn only_a_change_to_another_conversation_opens_one() {
        let line = || Some("This chat needs ollama (qwen3:8b).".to_owned());
        // Startup and `/resume`: it opens, deciding afresh whether it is served.
        let mut unserved = line();
        assert!(opens_conversation(true, false, &mut unserved));
        assert_eq!(unserved, None);
        // A fork or rewind child continues what its parent was left with.
        let mut unserved = line();
        assert!(!opens_conversation(true, true, &mut unserved));
        assert_eq!(unserved, line());
        // A refresh of the same conversation opens nothing.
        let mut unserved = line();
        assert!(!opens_conversation(false, false, &mut unserved));
        assert_eq!(unserved, line());
    }

    #[tokio::test]
    async fn a_child_of_a_chat_this_process_cannot_serve_stays_unserved_when_its_server_comes_up() {
        let mut process = Process::started_on("codex", "gpt-6.1-sol");
        let startup = process.startup.clone();
        let record = saved("ollama", "qwen3:8b");

        // Opened while its server is down: it is shown, and nothing is sent.
        let mut unserved = None;
        let open_now = opens_conversation(true, false, &mut unserved);
        let closed = closed_endpoint().await;
        unserved = apply(&mut process, &record, open_now, &closed).await;
        assert!(unserved.is_some());

        // The server starts and the chat is forked (a rewind is the same). The
        // child copies the record, but it is no more servable than its parent.
        let endpoint = fake_ollama(&["qwen3:8b"], true).await;
        let open_child = opens_conversation(true, true, &mut unserved);
        assert_eq!(
            apply(&mut process, &record, open_child, &endpoint).await,
            None
        );
        assert!(unserved.is_some(), "only `/model` clears it");
        assert_eq!(process.provider_name, "codex");
        assert!(Arc::ptr_eq(&process.provider, &startup));
        assert!(!process.bound_local_endpoint());

        // Opening the chat from elsewhere decides afresh, and now it is served.
        let open_again = opens_conversation(true, false, &mut unserved);
        assert_eq!(unserved, None);
        assert_eq!(
            apply(&mut process, &record, open_again, &endpoint).await,
            None
        );
        assert_eq!(process.provider_name, "ollama");
        assert!(process.bound_local_endpoint());
    }
}
