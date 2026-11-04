//! The binding a conversation is held with, saved beside it by the host.
//!
//! Reopening the conversation then does not depend on a settings command.
use crate::draft::DraftOwners;
use polaris_core::{
    management_effect::ManagementOperationIdV1,
    rewind_workspace::ErrorKind,
    session_settings::{
        SavedBindingV1, SavedEndpointV1, SessionSettingsCommandV1, SessionSettingsError,
        SessionSettingsOwnerV1,
    },
};

/// The effort a run is held with: the one the launch fixed (`auto` when it did
/// not), and none for a local model, which has no effort to choose.
pub(crate) fn effort(provider: &str, fixed: Option<&str>) -> String {
    if crate::is_local_provider(provider) {
        String::new()
    } else {
        fixed.unwrap_or("auto").to_owned()
    }
}

/// The binding the run holds now, as the record keeps it. Only the `openai`
/// provider reaches different places, so only it carries `endpoint`, which is
/// where this process sends it.
fn live(
    provider: &str,
    model: &str,
    fixed_effort: Option<&str>,
    endpoint: Option<&SavedEndpointV1>,
) -> Result<SavedBindingV1, String> {
    SavedBindingV1::new(
        provider.to_owned(),
        model.to_owned(),
        effort(provider, fixed_effort),
        endpoint.filter(|_| provider == "openai").cloned(),
    )
    .map_err(|error| error.to_string())
}

/// Saves the binding the run holds now as the one the conversation is held
/// with, unless it already is, and returns it. `/model` calls it as soon as a
/// model is chosen, and every send calls it first: the save that failed or
/// never happened is made before anything goes to the model. A send that opens
/// a sidechat creates it with the returned binding, because this cannot write
/// once admission holds the project.
pub(crate) fn record(
    owners: DraftOwners<'_>,
    provider: &str,
    model: &str,
    fixed_effort: Option<&str>,
    endpoint: Option<&SavedEndpointV1>,
) -> Result<SavedBindingV1, String> {
    let binding = live(provider, model, fixed_effort, endpoint)?;
    save(owners, &binding)?;
    Ok(binding)
}

/// Saves `binding` as the one the conversation is held with, unless it already
/// is. It is not a settings change: nothing is confirmed, as with a draft, and
/// `session_settings` stays as it is, so launch flags are not pinned into the
/// conversation. A conversation that is not saved has nowhere to keep it.
fn save(owners: DraftOwners<'_>, binding: &SavedBindingV1) -> Result<(), String> {
    let (session, initial, workspace, effects) = owners;
    let Some(saved) = session.persistence.as_ref() else {
        return Ok(());
    };
    session.check_persistence().map_err(|e| e.to_string())?;
    let snapshot = saved.snapshot().map_err(|e| e.to_string())?;
    if snapshot.state.held_with.as_ref() == Some(binding) {
        return Ok(());
    }
    let lease = workspace.try_mutation().map_err(|error| {
        if error.kind == ErrorKind::Busy {
            "another operation is using this project".to_owned()
        } else {
            error.to_string()
        }
    })?;
    polaris_core::sync::composite::C10CompositeBlockerV1::require_clear_with_mutation(
        workspace.project_root(),
        effects.state_dir(),
        &lease,
    )
    .map_err(|error| format!("a management operation is unresolved: {error}"))?;
    let owner = SessionSettingsOwnerV1 {
        effects,
        lease: &lease,
    };
    match owner.apply(
        saved,
        initial,
        SessionSettingsCommandV1::Binding {
            binding: binding.clone(),
        },
        ManagementOperationIdV1::random().map_err(|e| e.to_string())?,
    ) {
        // Another owner saved the same binding between the check and the write.
        Ok(_) | Err(SessionSettingsError::NoChange) => Ok(()),
        Err(error) => Err(error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use polaris_core::{
        conversation_state::HistoryMode,
        management_effect::{ManagementEffectStateV1, ManagementEffectStoreV1},
        rewind_workspace::WorkspaceOwner,
        session::Session,
        session_settings::{SavedSessionSettingsV1, ToolMemoryModeV1},
        session_store::PersistedSession,
    };

    /// A saved conversation with its owners, the way `run()` holds them.
    struct Fixture {
        _root: tempfile::TempDir,
        data: std::path::PathBuf,
        state: std::path::PathBuf,
        project_id: String,
        workspace: WorkspaceOwner,
        effects: ManagementEffectStoreV1,
        session: Session,
        saved: PersistedSession,
        /// The launch: it fixes `--remember` and `--tool-memory`, which the
        /// record must not copy into the conversation.
        launch: SavedSessionSettingsV1,
    }

    impl Fixture {
        fn new() -> Self {
            use std::os::unix::fs::PermissionsExt;
            let temporary = tempfile::tempdir().unwrap();
            let root = temporary.path().canonicalize().unwrap();
            let project = root.join("project");
            let data = root.join("data");
            let state = root.join("state");
            for path in [&project, &data, &state] {
                std::fs::create_dir(path).unwrap();
            }
            std::fs::set_permissions(&state, std::fs::Permissions::from_mode(0o700)).unwrap();
            let workspace = WorkspaceOwner::open(
                &project,
                &state.join("rewind-workspace"),
                Default::default(),
            )
            .unwrap();
            let lease = workspace.try_mutation().unwrap();
            let effects =
                ManagementEffectStoreV1::open_at_state_dir(&project, &state, &lease).unwrap();
            drop(lease);
            let project_id = project.to_str().unwrap().to_owned();
            let saved = PersistedSession::create(
                &data,
                &data.join("memory.sqlite3"),
                &project_id,
                HistoryMode::Legacy,
                None,
            )
            .unwrap();
            let mut session = Session::default();
            session.attach(saved.clone()).unwrap();
            let mut launch =
                SavedSessionSettingsV1::new("codex".into(), "gpt-6.1-sol".into(), "auto".into())
                    .unwrap();
            launch.remember = true;
            launch.tool_memory = ToolMemoryModeV1::Retrieval;
            Self {
                _root: temporary,
                data,
                state,
                project_id,
                workspace,
                effects,
                session,
                saved,
                launch,
            }
        }

        fn save(&self, binding: &SavedBindingV1) -> Result<(), String> {
            save(
                (&self.session, &self.launch, &self.workspace, &self.effects),
                binding,
            )
        }

        fn held(&self) -> Option<SavedBindingV1> {
            self.saved.snapshot().unwrap().state.held_with
        }
    }

    fn binding(provider: &str, model: &str) -> SavedBindingV1 {
        live(provider, model, None, None).unwrap()
    }

    #[test]
    fn the_effort_is_empty_for_a_local_model_and_otherwise_what_the_launch_fixed() {
        for local in ["ollama", "lmstudio", "llamacpp"] {
            assert_eq!(effort(local, Some("high")), "");
        }
        assert_eq!(effort("codex", None), "auto");
        assert_eq!(effort("openai", Some("high")), "high");
    }

    #[test]
    fn only_the_openai_provider_carries_an_endpoint() {
        let local = SavedEndpointV1::Custom("http://127.0.0.1:8080/v1".into());
        assert_eq!(
            live("openai", "m", None, Some(&local)).unwrap().endpoint,
            Some(local.clone())
        );
        // A hosted codex or a local model has one place to go.
        for provider in ["codex", "ollama", "llamacpp"] {
            assert_eq!(
                live(provider, "m", None, Some(&local)).unwrap().endpoint,
                None
            );
        }
        assert!(live("openai", "", None, None).is_err());
    }

    #[test]
    fn the_record_is_written_when_missing_and_follows_a_later_change() {
        let fixture = Fixture::new();
        assert_eq!(fixture.held(), None);

        // `/model` chose a local model; no settings command was ever confirmed.
        let local = binding("ollama", "qwen3:8b");
        fixture.save(&local).unwrap();
        assert_eq!(fixture.held(), Some(local.clone()));
        let generation = fixture.saved.snapshot().unwrap().state.generation;
        let effects = fixture.effects.effects().unwrap().len();

        // The same binding again writes nothing.
        fixture.save(&local).unwrap();
        assert_eq!(
            fixture.saved.snapshot().unwrap().state.generation,
            generation
        );
        assert_eq!(fixture.effects.effects().unwrap().len(), effects);

        // A later `/model` replaces it, and each write is one completed effect.
        let hosted = binding("codex", "gpt-6-astra");
        fixture.save(&hosted).unwrap();
        assert_eq!(fixture.held(), Some(hosted));
        let all = fixture.effects.effects().unwrap();
        assert_eq!(all.len(), effects + 1);
        assert!(
            all.iter()
                .all(|effect| matches!(effect.state, ManagementEffectStateV1::Completed { .. }))
        );
    }

    #[test]
    fn launch_flags_are_not_pinned_into_the_conversation_by_the_record() {
        let fixture = Fixture::new();
        assert!(fixture.launch.remember);
        fixture.save(&binding("codex", "gpt-6.1-sol")).unwrap();
        fixture.save(&binding("ollama", "qwen3:8b")).unwrap();
        // The record is written, and only the record.
        assert_eq!(fixture.held(), Some(binding("ollama", "qwen3:8b")));
        let state = fixture.saved.snapshot().unwrap().state;
        assert_eq!(state.session_settings, None);
        assert!(
            !std::fs::read_to_string(fixture.saved.store.directory().join("state.json"))
                .unwrap()
                .contains("remember")
        );
    }

    #[test]
    fn the_record_survives_reopening_and_never_enters_raw_history() {
        let fixture = Fixture::new();
        let custom = SavedEndpointV1::from_base_url(Some("http://127.0.0.1:8080/v1"));
        let held = live("openai", "local-model", Some("high"), Some(&custom)).unwrap();
        fixture.save(&held).unwrap();
        let id = fixture.saved.snapshot().unwrap().state.session_id;
        let reopened = PersistedSession::open(
            &fixture.data,
            &fixture.data.join("memory.sqlite3"),
            &fixture.project_id,
            &id,
        )
        .unwrap()
        .snapshot()
        .unwrap();
        assert_eq!(reopened.state.held_with, Some(held));
        assert!(reopened.events.is_empty());
    }

    #[test]
    fn a_busy_project_or_an_unresolved_operation_refuses_and_changes_nothing() {
        let fixture = Fixture::new();
        let before = fixture.saved.snapshot().unwrap().state;
        let other_owner = fixture.workspace.try_mutation().unwrap();
        let error = fixture.save(&binding("ollama", "qwen3:8b")).unwrap_err();
        assert!(error.contains("using this project"), "{error}");
        drop(other_owner);
        assert_eq!(fixture.saved.snapshot().unwrap().state, before);

        // A damaged C10 intent file cannot be proven clear.
        use std::os::unix::fs::PermissionsExt;
        let intents = fixture.state.join("sync").join("remote-intents-v2");
        for directory in [
            fixture.state.join("sync"),
            intents.clone(),
            intents.join("archive"),
        ] {
            std::fs::create_dir(&directory).unwrap();
            std::fs::set_permissions(directory, std::fs::Permissions::from_mode(0o700)).unwrap();
        }
        for (path, bytes) in [
            (intents.join("owner.lock"), b"".as_slice()),
            (
                intents.join(format!("{}.json", "a".repeat(64))),
                b"{torn".as_slice(),
            ),
        ] {
            std::fs::write(&path, bytes).unwrap();
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
        }
        assert!(fixture.save(&binding("ollama", "qwen3:8b")).is_err());
        assert_eq!(fixture.saved.snapshot().unwrap().state, before);
    }

    #[test]
    fn a_binding_already_recorded_needs_no_lease_even_while_another_operation_holds_the_project() {
        let fixture = Fixture::new();
        let local = binding("ollama", "qwen3:8b");
        fixture.save(&local).unwrap();
        let other_owner = fixture.workspace.try_mutation().unwrap();
        // Every send checks the record; only a change needs the project.
        fixture.save(&local).expect("there is nothing to write");
        assert!(fixture.save(&binding("codex", "gpt-6-astra")).is_err());
        drop(other_owner);
        assert_eq!(fixture.held(), Some(local));
    }

    #[test]
    fn a_conversation_that_is_not_saved_has_nothing_to_record_into() {
        let fixture = Fixture::new();
        let session = Session::default();
        save(
            (
                &session,
                &fixture.launch,
                &fixture.workspace,
                &fixture.effects,
            ),
            &binding("ollama", "qwen3:8b"),
        )
        .unwrap();
        assert_eq!(fixture.held(), None);
    }
}
