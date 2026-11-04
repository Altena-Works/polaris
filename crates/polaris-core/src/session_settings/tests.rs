//! Session-settings persistence, denial and recovery regression tests.

use super::*;
use crate::{
    conversation_state::{RewindEffectStatusV1, RewindEligibilityV1},
    management_effect::{
        ManagementEffectStateV1, ManagementEffectStoreV1, ManagementOperationIdV1,
        ManagementRewindDispositionV1,
    },
    project_mutation::{ProjectMutationLeaseV1, ProjectMutationOwnerV1},
};
use polaris_provider::Message;
use std::fs;

struct Fixture {
    _root: tempfile::TempDir,
    _mutation: ProjectMutationOwnerV1,
    lease: ProjectMutationLeaseV1,
    effects: ManagementEffectStoreV1,
    session: PersistedSession,
    initial: SavedSessionSettingsV1,
}

impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let project = root.path().join("project");
        let state = project.join(".polaris");
        let data = root.path().join("data");
        fs::create_dir_all(&state).unwrap();
        fs::create_dir(&data).unwrap();
        let mutation =
            ProjectMutationOwnerV1::at_store_root(&state.join("rewind-workspace")).unwrap();
        let lease = mutation.try_mutation().unwrap();
        let effects = ManagementEffectStoreV1::open_at_state_dir(&project, &state, &lease).unwrap();
        let session = PersistedSession::create(
            &data,
            &data.join("memory.sqlite3"),
            project.to_str().unwrap(),
            HistoryMode::Legacy,
            None,
        )
        .unwrap();
        Self {
            _root: root,
            _mutation: mutation,
            lease,
            effects,
            session,
            initial: SavedSessionSettingsV1::new("fake".into(), "model".into(), "auto".into())
                .unwrap(),
        }
    }

    fn owner(&self) -> SessionSettingsOwnerV1<'_> {
        SessionSettingsOwnerV1 {
            effects: &self.effects,
            lease: &self.lease,
        }
    }

    fn apply(&self, command: SessionSettingsCommandV1) -> ConversationSnapshot {
        let operation = ManagementOperationIdV1::random().unwrap();
        let published = self
            .owner()
            .apply(&self.session, &self.initial, command, operation)
            .unwrap();
        let effect = self.effects.effect(operation).unwrap().unwrap();
        assert!(matches!(
            effect.state,
            ManagementEffectStateV1::Completed { .. }
        ));
        published
    }

    fn refuse(&self, command: SessionSettingsCommandV1) {
        let before = self.session.snapshot().unwrap();
        assert!(SessionSettingsOwnerV1::preview(&self.session, &self.initial, command).is_err());
        assert_eq!(self.session.snapshot().unwrap().state, before.state);
    }
}

#[test]
fn m3c_settings_rows_publish_values_and_restore_without_replay() {
    let fixture = Fixture::new();
    let cursor_clone = fixture.session.clone();
    let snapshot = fixture.apply(SessionSettingsCommandV1::Remember { enabled: true });
    assert!(snapshot.state.session_settings.unwrap().remember);
    for mode in [
        ToolMemoryModeV1::History,
        ToolMemoryModeV1::Retrieval,
        ToolMemoryModeV1::Off,
    ] {
        let snapshot = fixture.apply(SessionSettingsCommandV1::ToolMemory { mode });
        assert_eq!(snapshot.state.session_settings.unwrap().tool_memory, mode);
    }
    let pair =
        SavedToolEmbeddingV1::new("http://localhost:8080/v1/".into(), "embed-1".into()).unwrap();
    assert_eq!(pair.url, "http://127.0.0.1:8080/v1/");
    let snapshot = fixture.apply(SessionSettingsCommandV1::EmbeddingEndpoint {
        embedding: Some(pair.clone()),
    });
    assert_eq!(
        snapshot
            .state
            .session_settings
            .unwrap()
            .tool_memory_embedding,
        Some(pair.clone())
    );
    let pair = SavedToolEmbeddingV1 {
        model: "embed-2".into(),
        ..pair
    };
    let snapshot = fixture.apply(SessionSettingsCommandV1::EmbeddingModel {
        embedding: Some(pair.clone()),
    });
    assert_eq!(
        snapshot
            .state
            .session_settings
            .unwrap()
            .tool_memory_embedding,
        Some(pair)
    );
    let revision = fixture
        .session
        .snapshot()
        .unwrap()
        .state
        .session_settings
        .unwrap()
        .revision;
    let snapshot = fixture.apply(SessionSettingsCommandV1::Configure {
        expected_revision: revision,
        provider: "fake".into(),
        model: "next-model".into(),
        effort: "auto".into(),
    });
    assert_eq!(snapshot.state.session_settings.unwrap().model, "next-model");
    fixture.apply(SessionSettingsCommandV1::Draft {
        expected_revision: 0,
        text: "未送信の本文".into(),
    });
    let expected = fixture.apply(SessionSettingsCommandV1::Goal {
        expected_revision: 0,
        operation: GoalOperationV1::Set {
            text: "完了まで進める".into(),
        },
    });
    assert_eq!(expected.raw_offset, 0);
    assert!(expected.events.is_empty());
    assert_eq!(cursor_clone.snapshot().unwrap().state, expected.state);
    let resumed = PersistedSession::open(
        &fixture.session.data_root,
        &fixture.session.database,
        &expected.state.project_id,
        &expected.state.session_id,
    )
    .unwrap();
    assert_eq!(resumed.snapshot().unwrap().state, expected.state);
    assert_eq!(fixture.effects.effects().unwrap().len(), 9);
    cursor_clone
        .append(Message::user("次の入力"), true, None)
        .unwrap();
    assert_eq!(
        fixture.session.snapshot().unwrap().state.generation,
        expected.state.generation + 1
    );
}

#[test]
fn a_saved_compact_threshold_is_read_without_error_and_never_written() {
    let settings =
        SavedSessionSettingsV1::new("fake".into(), "model".into(), "auto".into()).unwrap();
    let mut saved = serde_json::to_value(&settings).unwrap();
    assert!(saved.get("compact_threshold").is_none(), "{saved}");
    for old in [serde_json::json!(null), serde_json::json!(5000)] {
        saved["compact_threshold"] = old;
        let read: SavedSessionSettingsV1 = serde_json::from_value(saved.clone()).unwrap();
        assert_eq!(read, settings, "the old value must not be kept");
        assert!(
            serde_json::to_value(&read)
                .unwrap()
                .get("compact_threshold")
                .is_none()
        );
    }
    saved["compact_thresholds"] = serde_json::json!(5000);
    assert!(serde_json::from_value::<SavedSessionSettingsV1>(saved).is_err());
}

#[test]
fn an_old_state_file_with_a_compact_threshold_loads_and_is_rewritten_without_it() {
    let fixture = Fixture::new();
    fixture.apply(SessionSettingsCommandV1::Remember { enabled: true });
    let path = fixture.session.store.directory().join("state.json");
    let mut state: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    state["session_settings"]["compact_threshold"] = serde_json::json!(5000);
    fs::write(&path, serde_json::to_vec(&state).unwrap()).unwrap();
    let loaded = fixture.session.snapshot().unwrap();
    assert!(loaded.state.session_settings.unwrap().remember);
    fixture.apply(SessionSettingsCommandV1::Remember { enabled: false });
    assert!(
        !fs::read_to_string(&path)
            .unwrap()
            .contains("compact_threshold")
    );
}

#[test]
fn m3c_settings_rows_refuse_cancel_foreign_lease_and_invalid_values() {
    let fixture = Fixture::new();
    let before = fixture.session.snapshot().unwrap();
    let preview = SessionSettingsOwnerV1::preview(
        &fixture.session,
        &fixture.initial,
        SessionSettingsCommandV1::Remember { enabled: true },
    )
    .unwrap();
    let prepared = fixture
        .owner()
        .prepare(
            &fixture.session,
            ManagementOperationIdV1::random().unwrap(),
            &preview,
        )
        .unwrap();
    fixture.owner().deny(&prepared).unwrap();
    assert_eq!(fixture.session.snapshot().unwrap().state, before.state);
    let other = tempfile::tempdir().unwrap();
    let foreign_data = other.path().join("data");
    fs::create_dir(&foreign_data).unwrap();
    let foreign_session = PersistedSession::create(
        &foreign_data,
        &foreign_data.join("memory.sqlite3"),
        other.path().to_str().unwrap(),
        HistoryMode::Legacy,
        None,
    )
    .unwrap();
    let preview = SessionSettingsOwnerV1::preview(
        &foreign_session,
        &fixture.initial,
        SessionSettingsCommandV1::Remember { enabled: true },
    )
    .unwrap();
    let effect_count = fixture.effects.effects().unwrap().len();
    assert!(
        fixture
            .owner()
            .prepare(
                &foreign_session,
                ManagementOperationIdV1::random().unwrap(),
                &preview
            )
            .is_err()
    );
    assert_eq!(foreign_session.snapshot().unwrap().state.generation, 0);
    assert_eq!(fixture.effects.effects().unwrap().len(), effect_count);

    let other_owner =
        ProjectMutationOwnerV1::at_store_root(&other.path().join("rewind-workspace")).unwrap();
    let other_lease = other_owner.try_mutation().unwrap();
    let foreign = SessionSettingsOwnerV1 {
        effects: &fixture.effects,
        lease: &other_lease,
    };
    let preview = SessionSettingsOwnerV1::preview(
        &fixture.session,
        &fixture.initial,
        SessionSettingsCommandV1::ToolMemory {
            mode: ToolMemoryModeV1::History,
        },
    )
    .unwrap();
    assert!(
        foreign
            .prepare(
                &fixture.session,
                ManagementOperationIdV1::random().unwrap(),
                &preview
            )
            .is_err()
    );
    assert_eq!(fixture.session.snapshot().unwrap().state, before.state);
    fixture.refuse(SessionSettingsCommandV1::EmbeddingEndpoint {
        embedding: Some(SavedToolEmbeddingV1 {
            url: "https://example.com/v1".into(),
            model: "embed".into(),
        }),
    });
    fixture.refuse(SessionSettingsCommandV1::EmbeddingModel {
        embedding: Some(SavedToolEmbeddingV1 {
            url: "http://127.0.0.1:8080/v1".into(),
            model: String::new(),
        }),
    });
    fixture.refuse(SessionSettingsCommandV1::Configure {
        expected_revision: 0,
        provider: "another-provider".into(),
        model: "model".into(),
        effort: "auto".into(),
    });
    fixture.refuse(SessionSettingsCommandV1::Configure {
        expected_revision: 0,
        provider: "fake".into(),
        model: "model".into(),
        effort: "high".into(),
    });
}

#[test]
fn m3c_history_initializes_only_proven_empty_sessions() {
    let fixture = Fixture::new();
    let snapshot = fixture.apply(SessionSettingsCommandV1::History {
        mode: HistoryMode::Strict10,
    });
    assert_eq!(snapshot.state.history_mode, HistoryMode::Strict10);
    let snapshot = fixture.apply(SessionSettingsCommandV1::History {
        mode: HistoryMode::Structured10,
    });
    assert_eq!(snapshot.state.history_mode, HistoryMode::Structured10);
    let memory = polaris_memory::MemoryStore::open(&fixture.session.database).unwrap();
    assert_eq!(
        memory.structured_memory_schema_version().unwrap(),
        Some(polaris_memory::structured::STRUCTURED_MEMORY_SCHEMA_VERSION)
    );
    drop(memory);
    fixture
        .session
        .append(Message::user("original history"), true, None)
        .unwrap();
    fixture.refuse(SessionSettingsCommandV1::History {
        mode: HistoryMode::Legacy,
    });
    fixture.session.clear().unwrap();
    fixture.refuse(SessionSettingsCommandV1::History {
        mode: HistoryMode::Legacy,
    });
    let before = fixture.session.snapshot().unwrap();
    let mut direct = before.state.clone();
    direct.history_mode = HistoryMode::Legacy;
    assert!(
        fixture
            .session
            .store
            .lock()
            .unwrap()
            .publish(&before, direct)
            .is_err()
    );
}

#[test]
fn m3c_draft_preserves_text_clears_blank_and_refuses_stale_or_oversize() {
    let fixture = Fixture::new();
    let snapshot = fixture.apply(SessionSettingsCommandV1::Draft {
        expected_revision: 0,
        text: "  原文\n入力  ".into(),
    });
    assert_eq!(snapshot.state.draft.text, "  原文\n入力  ");
    assert!(snapshot.events.is_empty());
    fixture.refuse(SessionSettingsCommandV1::Draft {
        expected_revision: 0,
        text: "stale".into(),
    });
    fixture.refuse(SessionSettingsCommandV1::Draft {
        expected_revision: 1,
        text: "x".repeat(MAX_DRAFT_BYTES + 1),
    });
    let snapshot = fixture.apply(SessionSettingsCommandV1::Draft {
        expected_revision: 1,
        text: " \t\n".into(),
    });
    assert_eq!(snapshot.state.draft.revision, 2);
    assert!(snapshot.state.draft.text.is_empty());
}

#[test]
fn m3c_goal_uses_exact_revision_and_preserves_stop_until_resume() {
    let fixture = Fixture::new();
    let snapshot = fixture.apply(SessionSettingsCommandV1::Goal {
        expected_revision: 0,
        operation: GoalOperationV1::Set {
            text: "first goal".into(),
        },
    });
    assert!(snapshot.state.goal.unwrap().active);
    fixture.refuse(SessionSettingsCommandV1::Goal {
        expected_revision: 0,
        operation: GoalOperationV1::Clear,
    });
    fixture.refuse(SessionSettingsCommandV1::Goal {
        expected_revision: 1,
        operation: GoalOperationV1::Set { text: "   ".into() },
    });
    fixture.apply(SessionSettingsCommandV1::Goal {
        expected_revision: 1,
        operation: GoalOperationV1::Stop,
    });
    let snapshot = fixture.apply(SessionSettingsCommandV1::Goal {
        expected_revision: 2,
        operation: GoalOperationV1::Set {
            text: "next goal".into(),
        },
    });
    assert!(snapshot.state.goal.unwrap().stopped);
    let snapshot = fixture.apply(SessionSettingsCommandV1::Goal {
        expected_revision: 3,
        operation: GoalOperationV1::Resume,
    });
    assert!(!snapshot.state.goal.unwrap().stopped);
    let snapshot = fixture.apply(SessionSettingsCommandV1::Goal {
        expected_revision: 4,
        operation: GoalOperationV1::Clear,
    });
    let goal = snapshot.state.goal.unwrap();
    assert_eq!(goal.revision.get(), 5);
    assert!(!goal.active);
}

#[test]
fn m3c_rewind_restores_captured_goal_and_starts_with_independent_blank_draft() {
    let fixture = Fixture::new();
    fixture.apply(SessionSettingsCommandV1::Remember { enabled: true });
    fixture.apply(SessionSettingsCommandV1::Binding {
        binding: binding("ollama", "qwen3:8b", ""),
    });
    fixture.apply(SessionSettingsCommandV1::Goal {
        expected_revision: 0,
        operation: GoalOperationV1::Set {
            text: "captured goal".into(),
        },
    });
    fixture.apply(SessionSettingsCommandV1::Draft {
        expected_revision: 0,
        text: "independent draft".into(),
    });
    let boundary = fixture
        .session
        .begin_rewind_boundary("a".repeat(32), Vec::new())
        .unwrap();
    fixture
        .session
        .append(Message::user("real input"), true, None)
        .unwrap();
    fixture
        .session
        .append(Message::assistant("answer"), false, None)
        .unwrap();
    fixture
        .session
        .finish_rewind_boundary(
            &boundary.boundary_id,
            "b".repeat(32),
            RewindEligibilityV1::Eligible,
            RewindEffectStatusV1::NoExternalMutation,
        )
        .unwrap();
    fixture.apply(SessionSettingsCommandV1::Goal {
        expected_revision: 1,
        operation: GoalOperationV1::Set {
            text: "later goal".into(),
        },
    });
    fixture.apply(SessionSettingsCommandV1::Binding {
        binding: binding("codex", "gpt-6-astra", "auto"),
    });
    let lock = fixture.session.store.lock().unwrap();
    let branch = lock
        .prepare_rewind_branch(
            &fixture.session.data_root,
            &boundary.boundary_id,
            "00000000-0000-4000-8000-000000000002",
        )
        .unwrap();
    let restored = branch.lock().unwrap().snapshot().unwrap();
    assert_eq!(restored.state.goal.unwrap().text, "captured goal");
    // The child is held with what the conversation was held with at the boundary.
    assert_eq!(
        restored.state.held_with,
        Some(binding("ollama", "qwen3:8b", ""))
    );
    assert_eq!(restored.state.draft, SavedSessionDraftV1::default());
    assert!(restored.state.session_settings.unwrap().remember);
    assert!(restored.events.is_empty());
}

#[test]
fn m3c_publication_refuses_non_session_fields_and_full_preimage_mismatch() {
    let fixture = Fixture::new();
    let before = fixture.session.snapshot().unwrap();
    let hash = content_hash(&serde_json::to_vec(&before.state).unwrap());
    let mut candidate = before.state.clone();
    candidate
        .visible_summary_ids
        .push("unauthorized-summary".into());
    assert!(
        fixture
            .session
            .publish_session_change(before.state.generation, &hash, candidate)
            .is_err()
    );
    assert!(
        fixture
            .session
            .publish_session_change(
                before.state.generation,
                &"a".repeat(64),
                before.state.clone()
            )
            .is_err()
    );
    assert_eq!(fixture.session.snapshot().unwrap().state, before.state);
}

fn binding(provider: &str, model: &str, effort: &str) -> SavedBindingV1 {
    SavedBindingV1::new(provider.into(), model.into(), effort.into(), None).unwrap()
}

#[test]
fn the_binding_is_saved_apart_from_settings_and_follows_each_change() {
    let fixture = Fixture::new();
    let local = binding("ollama", "qwen3:8b", "");
    let operation = ManagementOperationIdV1::random().unwrap();
    let snapshot = fixture
        .owner()
        .apply(
            &fixture.session,
            &fixture.initial,
            SessionSettingsCommandV1::Binding {
                binding: local.clone(),
            },
            operation,
        )
        .unwrap();
    assert_eq!(snapshot.state.held_with, Some(local.clone()));
    // Not a settings change: no settings are created from the launch's, so its
    // flags are not pinned, and nothing else in the marker moves.
    assert_eq!(snapshot.state.session_settings, None);
    assert!(snapshot.events.is_empty());
    // Restorable, so saving it never makes the history before it impossible to
    // rewind across.
    let effect = fixture.effects.effect(operation).unwrap().unwrap();
    assert!(matches!(
        effect.state,
        ManagementEffectStateV1::Completed { .. }
    ));
    assert_eq!(
        effect.request.rewind,
        ManagementRewindDispositionV1::CapturedRestorable
    );

    // The binding it already has is no change.
    fixture.refuse(SessionSettingsCommandV1::Binding {
        binding: local.clone(),
    });

    // A later `/model` replaces it; a settings command does not touch it, and it
    // does not touch the settings the command creates.
    let hosted = binding("codex", "gpt-6-astra", "auto");
    let snapshot = fixture.apply(SessionSettingsCommandV1::Binding {
        binding: hosted.clone(),
    });
    assert_eq!(snapshot.state.held_with, Some(hosted.clone()));
    let snapshot = fixture.apply(SessionSettingsCommandV1::Remember { enabled: true });
    assert_eq!(snapshot.state.held_with, Some(hosted.clone()));
    assert_eq!(snapshot.state.session_settings.unwrap().provider, "fake");

    // It is read back after a restart.
    let resumed = PersistedSession::open(
        &fixture.session.data_root,
        &fixture.session.database,
        &snapshot.state.project_id,
        &snapshot.state.session_id,
    )
    .unwrap();
    assert_eq!(resumed.snapshot().unwrap().state.held_with, Some(hosted));
}

#[test]
fn a_binding_that_could_leak_or_is_malformed_is_refused() {
    let fixture = Fixture::new();
    let good = binding("openai", "m", "auto");
    for bad in [
        SavedBindingV1 {
            provider: String::new(),
            ..good.clone()
        },
        SavedBindingV1 {
            model: " ".into(),
            ..good.clone()
        },
        SavedBindingV1 {
            effort: "loud".into(),
            ..good.clone()
        },
        SavedBindingV1 {
            endpoint: Some(SavedEndpointV1::Custom(
                "https://user:secret@host.example/v1".into(),
            )),
            ..good.clone()
        },
        SavedBindingV1 {
            endpoint: Some(SavedEndpointV1::Custom(
                "http://host.example/v1?key=1".into(),
            )),
            ..good.clone()
        },
        SavedBindingV1 {
            endpoint: Some(SavedEndpointV1::Custom(String::new())),
            ..good.clone()
        },
    ] {
        fixture.refuse(SessionSettingsCommandV1::Binding { binding: bad });
    }
    assert_eq!(fixture.session.snapshot().unwrap().state.held_with, None);
}

#[test]
fn a_state_file_from_before_the_binding_loads_and_one_without_it_is_written_without_the_key() {
    let fixture = Fixture::new();
    fixture.apply(SessionSettingsCommandV1::Remember { enabled: true });
    let path = fixture.session.store.directory().join("state.json");
    let written = |path: &std::path::Path| -> serde_json::Value {
        serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
    };
    // What an older version wrote is what a conversation without the record still
    // is: the key is absent, so an older reader (which refuses unknown keys) can
    // keep reading it.
    assert!(written(&path).get("held_with").is_none());
    let loaded = fixture.session.snapshot().unwrap();
    assert_eq!(loaded.state.held_with, None);
    assert!(loaded.state.session_settings.unwrap().remember);

    // Once recorded, the file has the key; removing it gives the old file back.
    fixture.apply(SessionSettingsCommandV1::Binding {
        binding: binding("ollama", "qwen3:8b", ""),
    });
    let mut state = written(&path);
    assert_eq!(
        state["held_with"],
        serde_json::json!({"provider": "ollama", "model": "qwen3:8b", "effort": ""})
    );
    state.as_object_mut().unwrap().remove("held_with");
    fs::write(&path, serde_json::to_vec(&state).unwrap()).unwrap();
    assert_eq!(fixture.session.snapshot().unwrap().state.held_with, None);

    // The key holds the record and nothing else: an unknown member is refused.
    let mut state = written(&path);
    state["held_with"] =
        serde_json::json!({"provider": "p", "model": "m", "effort": "", "remember": true});
    fs::write(&path, serde_json::to_vec(&state).unwrap()).unwrap();
    assert!(fixture.session.snapshot().is_err());
}

#[test]
fn an_endpoint_is_kept_without_credentials_and_only_the_providers_own_host_is_the_default() {
    use SavedEndpointV1::{Custom, Default};
    for base in [
        None,
        Some("https://api.openai.com/v1"),
        Some("https://api.openai.com"),
        Some("https://API.OPENAI.COM:443/v1/"),
        Some("https://user:secret@api.openai.com/v1?key=abc#fragment"),
    ] {
        assert_eq!(SavedEndpointV1::from_base_url(base), Default, "{base:?}");
    }
    for base in [
        "http://api.openai.com/v1",
        "https://api.openai.com:8443/v1",
        "https://api.openai.com.example.com/v1",
        "https://example.com/v1",
    ] {
        assert!(
            matches!(SavedEndpointV1::from_base_url(Some(base)), Custom(_)),
            "{base}"
        );
    }
    // The same server is the same endpoint, however the URL is spelled.
    let local = Custom("http://127.0.0.1:8080/v1".into());
    for base in [
        "http://127.0.0.1:8080/v1",
        "http://127.0.0.1:8080/v1/",
        "HTTP://127.0.0.1:8080/v1",
    ] {
        assert_eq!(SavedEndpointV1::from_base_url(Some(base)), local, "{base}");
    }
    assert_ne!(
        SavedEndpointV1::from_base_url(Some("http://127.0.0.1:8081/v1")),
        local
    );
    // A key written into the URL never reaches the file.
    let kept = SavedEndpointV1::from_base_url(Some(
        "https://user:secret@llm.example.com:8443/v1/?api-key=abc#frag",
    ));
    assert_eq!(kept, Custom("https://llm.example.com:8443/v1".into()));
    assert!(
        !serde_json::to_string(&kept)
            .unwrap()
            .contains(['@', '?', '#'])
    );
    // A base URL nothing can be sent to still differs from every real one.
    let unreadable = SavedEndpointV1::from_base_url(Some("not a url"));
    assert_eq!(unreadable, Custom("invalid".into()));
    assert!(
        SavedBindingV1::new("openai".into(), "m".into(), "auto".into(), Some(unreadable)).is_ok()
    );
}

#[test]
fn the_endpoint_is_part_of_the_saved_format() {
    let with = |endpoint| {
        serde_json::to_value(
            SavedBindingV1::new("openai".into(), "m".into(), "auto".into(), endpoint).unwrap(),
        )
        .unwrap()
    };
    assert_eq!(
        with(Some(SavedEndpointV1::Default)),
        serde_json::json!({"provider": "openai", "model": "m", "effort": "auto", "endpoint": "default"})
    );
    assert_eq!(
        with(Some(SavedEndpointV1::Custom(
            "http://127.0.0.1:8080/v1".into()
        ))),
        serde_json::json!({
            "provider": "openai", "model": "m", "effort": "auto",
            "endpoint": {"custom": "http://127.0.0.1:8080/v1"}
        })
    );
    // A provider with one place to go has no endpoint key at all.
    assert!(with(None).get("endpoint").is_none());
    let read: SavedBindingV1 =
        serde_json::from_value(serde_json::json!({"provider": "p", "model": "m", "effort": ""}))
            .unwrap();
    assert_eq!(read.endpoint, None);
}
