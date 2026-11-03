//! An unsent draft belongs to its conversation and never enters raw history.
use polaris_core::{
    management_effect::{ManagementEffectStoreV1, ManagementOperationIdV1},
    rewind_workspace::{ErrorKind, WorkspaceOwner},
    session::Session,
    session_settings::{SavedSessionSettingsV1, SessionSettingsCommandV1, SessionSettingsOwnerV1},
};
use std::time::{Duration, Instant};

/// How long the composer must stay unchanged before its text is stored.
const AUTOSAVE_DELAY: Duration = Duration::from_millis(350);

/// The conversation, its settings, and the owners a draft write needs.
pub(crate) type DraftOwners<'a> = (
    &'a Session,
    &'a SavedSessionSettingsV1,
    &'a WorkspaceOwner,
    &'a ManagementEffectStoreV1,
);

enum Outcome {
    Stored,
    /// Another owner holds the project, or a management operation is
    /// unresolved. Nothing is wrong with the draft; it stays in the composer.
    Deferred(String),
}

pub(crate) fn save(
    session: &Session,
    initial: &SavedSessionSettingsV1,
    workspace: &WorkspaceOwner,
    effects: &ManagementEffectStoreV1,
    text: &str,
) -> Result<(), String> {
    match store(session, initial, workspace, effects, text)? {
        Outcome::Stored => Ok(()),
        Outcome::Deferred(reason) => Err(reason),
    }
}

fn store(
    session: &Session,
    initial: &SavedSessionSettingsV1,
    workspace: &WorkspaceOwner,
    effects: &ManagementEffectStoreV1,
    text: &str,
) -> Result<Outcome, String> {
    if crate::is_secret_command(text) {
        return Err("Secrets cannot be saved in drafts".into());
    }
    // A local command is not an unsent model input. Preserve the previous
    // draft while the user operates a picker or types a management command.
    if text.trim_start().starts_with('/') {
        return Ok(Outcome::Stored);
    }
    session.check_persistence().map_err(|e| e.to_string())?;
    let saved = session
        .persistence
        .as_ref()
        .ok_or("Draft storage requires a v2 conversation")?;
    let snapshot = saved.snapshot().map_err(|e| e.to_string())?;
    let text = if text.trim().is_empty() { "" } else { text };
    if snapshot.state.draft.text == text {
        return Ok(Outcome::Stored);
    }
    let lease = match workspace.try_mutation() {
        Ok(lease) => lease,
        Err(error) if error.kind == ErrorKind::Busy => {
            return Ok(Outcome::Deferred(error.to_string()));
        }
        Err(error) => return Err(error.to_string()),
    };
    if let Err(error) =
        polaris_core::sync::composite::C10CompositeBlockerV1::require_clear_with_mutation(
            workspace.project_root(),
            effects.state_dir(),
            &lease,
        )
    {
        return Ok(Outcome::Deferred(error.to_string()));
    }
    let owner = SessionSettingsOwnerV1 {
        effects,
        lease: &lease,
    };
    owner
        .apply(
            saved,
            initial,
            SessionSettingsCommandV1::Draft {
                expected_revision: snapshot.state.draft.revision,
                text: text.to_owned(),
            },
            ManagementOperationIdV1::random().map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
    Ok(Outcome::Stored)
}

/// Tracks the composer between key presses so an idle terminal stores the
/// unsent text without waiting for the process to exit. Closing the terminal
/// or switching conversations then loses at most the last quiet period.
pub(crate) struct DraftAutosave {
    /// What the composer showed at the last tick.
    candidate: String,
    /// What was last stored, or restored from storage, for it.
    saved: String,
    changed_at: Instant,
}

impl DraftAutosave {
    pub(crate) fn new() -> Self {
        Self {
            candidate: String::new(),
            saved: String::new(),
            changed_at: Instant::now(),
        }
    }

    /// The composer now shows exactly what storage holds.
    pub(crate) fn restored(&mut self, text: &str) {
        self.candidate = text.to_owned();
        self.saved.clone_from(&self.candidate);
        self.changed_at = Instant::now();
    }

    /// The text was sent; nothing is waiting to be stored.
    pub(crate) fn sent(&mut self) {
        self.candidate.clear();
        self.saved.clear();
    }

    /// One idle tick. `pending` is the unsent text and `event_ready` says a
    /// key is already waiting, in which case the write is left for a quieter
    /// moment. An error is a storage fault; waiting for another owner is not.
    pub(crate) fn tick(
        &mut self,
        owners: DraftOwners<'_>,
        pending: &str,
        event_ready: bool,
    ) -> Result<(), String> {
        self.tick_at(Instant::now(), owners, pending, event_ready)
    }

    fn tick_at(
        &mut self,
        now: Instant,
        (session, initial, workspace, effects): DraftOwners<'_>,
        pending: &str,
        event_ready: bool,
    ) -> Result<(), String> {
        // A secret command is never a draft, and is not copied here either.
        if session.persistence.is_none() || crate::is_secret_command(pending) {
            return Ok(());
        }
        if pending != self.candidate {
            self.candidate = pending.to_owned();
            self.changed_at = now;
        }
        if event_ready
            || self.candidate == self.saved
            || now.saturating_duration_since(self.changed_at) < AUTOSAVE_DELAY
        {
            return Ok(());
        }
        match store(session, initial, workspace, effects, &self.candidate)? {
            Outcome::Stored => self.saved.clone_from(&self.candidate),
            // Try again after the next quiet period.
            Outcome::Deferred(_) => self.changed_at = now,
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use polaris_core::{conversation_state::HistoryMode, session_store::PersistedSession};

    #[test]
    fn m3c_tui_draft_restores_exact_text_clears_blank_and_keeps_commands_out() {
        let root = tempfile::tempdir().unwrap();
        let project = root.path().join("project");
        let data = root.path().join("data");
        let state = root.path().join("state");
        for path in [&project, &data, &state] {
            std::fs::create_dir(path).unwrap();
        }
        std::fs::create_dir(project.join(".polaris")).unwrap();
        use std::os::unix::fs::PermissionsExt;
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
        let project_id = project.canonicalize().unwrap().to_str().unwrap().to_owned();
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
        let initial =
            SavedSessionSettingsV1::new("fake".into(), "model".into(), "auto".into()).unwrap();
        save(&session, &initial, &workspace, &effects, "未送信\nそのまま").unwrap();
        let id = saved.snapshot().unwrap().state.session_id;
        let reopened =
            PersistedSession::open(&data, &data.join("memory.sqlite3"), &project_id, &id).unwrap();
        assert_eq!(
            reopened.snapshot().unwrap().state.draft.text,
            "未送信\nそのまま"
        );
        assert!(reopened.snapshot().unwrap().events.is_empty());
        save(&session, &initial, &workspace, &effects, "/status").unwrap();
        assert_eq!(
            saved.snapshot().unwrap().state.draft.text,
            "未送信\nそのまま"
        );
        save(&session, &initial, &workspace, &effects, " \n\t").unwrap();
        assert!(saved.snapshot().unwrap().state.draft.text.is_empty());
        assert!(
            save(
                &session,
                &initial,
                &workspace,
                &effects,
                "/mcp secret set s k secret"
            )
            .is_err()
        );
        assert!(saved.snapshot().unwrap().state.draft.text.is_empty());

        // Terminal-only management cannot authorize writes when a C10 file is
        // torn. The actual draft owner must check under its newly acquired EX.
        let torn = tear_c10_intents(&state);
        let before = saved.snapshot().unwrap();
        let cursor = effects.cursor().unwrap();
        assert!(
            save(
                &session,
                &initial,
                &workspace,
                &effects,
                "must stay unsaved"
            )
            .is_err()
        );
        assert_eq!(saved.snapshot().unwrap().state, before.state);
        assert_eq!(effects.cursor().unwrap(), cursor);
        assert_eq!(std::fs::read(torn).unwrap(), b"{torn");
    }

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
        initial: SavedSessionSettingsV1,
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
            Self {
                _root: temporary,
                data,
                state,
                project_id,
                workspace,
                effects,
                session,
                saved,
                initial: SavedSessionSettingsV1::new("fake".into(), "model".into(), "auto".into())
                    .unwrap(),
            }
        }

        fn owners(&self) -> DraftOwners<'_> {
            (&self.session, &self.initial, &self.workspace, &self.effects)
        }

        fn stored(&self) -> (String, u64) {
            let draft = self.saved.snapshot().unwrap().state.draft;
            (draft.text, draft.revision)
        }
    }

    const QUIET: Duration = AUTOSAVE_DELAY;
    const TICK: Duration = Duration::from_millis(1);

    #[test]
    fn d17_autosave_stores_the_draft_once_the_composer_has_been_quiet() {
        let fixture = Fixture::new();
        let mut autosave = DraftAutosave::new();
        let t0 = Instant::now();
        autosave
            .tick_at(t0, fixture.owners(), "draft A", false)
            .unwrap();
        autosave
            .tick_at(t0 + QUIET - TICK, fixture.owners(), "draft A", false)
            .unwrap();
        assert_eq!(fixture.stored().0, "", "stored before the quiet period");
        autosave
            .tick_at(t0 + QUIET, fixture.owners(), "draft A", false)
            .unwrap();
        let stored = fixture.stored();
        assert_eq!(stored.0, "draft A");
        assert!(stored.1 > 0);
        // The same text is not written again.
        autosave
            .tick_at(t0 + QUIET * 10, fixture.owners(), "draft A", false)
            .unwrap();
        assert_eq!(fixture.stored(), stored);
    }

    #[test]
    fn d17_each_edit_restarts_the_quiet_period_and_a_waiting_key_defers_the_write() {
        let fixture = Fixture::new();
        let mut autosave = DraftAutosave::new();
        let t0 = Instant::now();
        autosave.tick_at(t0, fixture.owners(), "a", false).unwrap();
        // Longer than the quiet period after the first edit, but not after
        // the second one.
        let t1 = t0 + QUIET + TICK;
        autosave.tick_at(t1, fixture.owners(), "ab", false).unwrap();
        autosave
            .tick_at(t1 + QUIET - TICK, fixture.owners(), "ab", false)
            .unwrap();
        assert_eq!(fixture.stored().0, "");
        autosave
            .tick_at(t1 + QUIET, fixture.owners(), "ab", true)
            .unwrap();
        assert_eq!(fixture.stored().0, "", "a key is already waiting");
        autosave
            .tick_at(t1 + QUIET, fixture.owners(), "ab", false)
            .unwrap();
        assert_eq!(fixture.stored().0, "ab");
    }

    #[test]
    fn d17_blank_composer_clears_the_draft_but_commands_and_secrets_leave_it_alone() {
        let fixture = Fixture::new();
        let mut autosave = DraftAutosave::new();
        let mut now = Instant::now();
        let mut settle = |autosave: &mut DraftAutosave, text: &str| {
            autosave
                .tick_at(now, fixture.owners(), text, false)
                .unwrap();
            now += QUIET;
            autosave
                .tick_at(now, fixture.owners(), text, false)
                .unwrap();
        };
        settle(&mut autosave, "kept");
        assert_eq!(fixture.stored().0, "kept");
        settle(&mut autosave, "/status");
        assert_eq!(fixture.stored().0, "kept", "a command is not a draft");
        // Typing the masked-input command is not a storage fault, and the
        // text never reaches the tracker.
        settle(&mut autosave, "/mcp secret set");
        assert_eq!(fixture.stored().0, "kept");
        assert_eq!(autosave.candidate, "/status");
        settle(&mut autosave, "");
        assert_eq!(fixture.stored().0, "", "blank input clears the draft");
    }

    #[test]
    fn d17_autosave_waits_for_another_owner_instead_of_ending_the_session() {
        let fixture = Fixture::new();
        let mut autosave = DraftAutosave::new();
        let t0 = Instant::now();
        let other_owner = fixture.workspace.try_mutation().unwrap();
        autosave
            .tick_at(t0, fixture.owners(), "waiting", false)
            .unwrap();
        autosave
            .tick_at(t0 + QUIET, fixture.owners(), "waiting", false)
            .expect("a busy project is not a storage fault");
        assert_eq!(fixture.stored().0, "");
        drop(other_owner);
        // The retry is a further quiet period away, then it stores.
        autosave
            .tick_at(t0 + QUIET * 2 - TICK, fixture.owners(), "waiting", false)
            .unwrap();
        assert_eq!(fixture.stored().0, "");
        autosave
            .tick_at(t0 + QUIET * 2, fixture.owners(), "waiting", false)
            .unwrap();
        assert_eq!(fixture.stored().0, "waiting");
    }

    #[test]
    fn d17_autosave_waits_while_a_management_operation_is_unresolved() {
        let fixture = Fixture::new();
        let torn = tear_c10_intents(&fixture.state);
        let mut autosave = DraftAutosave::new();
        let t0 = Instant::now();
        autosave
            .tick_at(t0, fixture.owners(), "waiting", false)
            .unwrap();
        autosave
            .tick_at(t0 + QUIET, fixture.owners(), "waiting", false)
            .expect("an unresolved operation is not a storage fault");
        assert_eq!(fixture.stored().0, "");
        assert!(
            save(
                &fixture.session,
                &fixture.initial,
                &fixture.workspace,
                &fixture.effects,
                "waiting"
            )
            .is_err(),
            "the exit-time save still refuses"
        );
        std::fs::remove_file(torn).unwrap();
        autosave
            .tick_at(t0 + QUIET * 2, fixture.owners(), "waiting", false)
            .unwrap();
        assert_eq!(fixture.stored().0, "waiting");
    }

    #[test]
    fn d17_a_conversation_without_saved_storage_is_never_tracked() {
        let fixture = Fixture::new();
        let session = Session::default();
        let mut autosave = DraftAutosave::new();
        let t0 = Instant::now();
        for at in [t0, t0 + QUIET * 2] {
            autosave
                .tick_at(
                    at,
                    (
                        &session,
                        &fixture.initial,
                        &fixture.workspace,
                        &fixture.effects,
                    ),
                    "unsaved conversation",
                    false,
                )
                .unwrap();
        }
        assert!(autosave.candidate.is_empty() && autosave.saved.is_empty());
        assert_eq!(fixture.stored().0, "");
    }

    #[test]
    fn d17_restart_and_switch_restore_the_stored_draft_into_the_composer() {
        use crate::input::History;
        use crate::session_activation::{
            SavedDraftTarget, apply_saved_draft, read_saved_activation,
        };

        let fixture = Fixture::new();
        let mut autosave = DraftAutosave::new();
        let t0 = Instant::now();
        let text = "未送信\nそのまま";
        autosave.tick_at(t0, fixture.owners(), text, false).unwrap();
        autosave
            .tick_at(t0 + QUIET, fixture.owners(), text, false)
            .unwrap();

        // A new process opens the conversation again.
        let id = fixture.saved.snapshot().unwrap().state.session_id;
        let reopened = PersistedSession::open(
            &fixture.data,
            &fixture.data.join("memory.sqlite3"),
            &fixture.project_id,
            &id,
        )
        .unwrap();
        let mut restarted = Session::default();
        restarted.attach(reopened).unwrap();

        let mut buffer = "typed before the switch".to_owned();
        let mut cursor = 3;
        let mut history = History::new();
        history.record("older input");
        assert!(history.older("typed before the switch").is_some());
        let mut resumed = DraftAutosave::new();
        let activation = read_saved_activation(&restarted, &fixture.initial, "model", true)
            .unwrap()
            .expect("a saved conversation has an activation");
        apply_saved_draft(
            activation,
            SavedDraftTarget {
                input_buffer: &mut buffer,
                input_cursor: &mut cursor,
                input_history: &mut history,
                autosave: &mut resumed,
            },
        );
        assert_eq!(buffer, text);
        assert_eq!(cursor, text.len());
        assert!(history.newer().is_none(), "history navigation is reset");
        // What was restored is what storage holds, so nothing is rewritten.
        assert_eq!(
            (resumed.candidate.as_str(), resumed.saved.as_str()),
            (text, text)
        );
        let stored = fixture.stored();
        resumed
            .tick_at(
                t0 + QUIET * 10,
                (
                    &restarted,
                    &fixture.initial,
                    &fixture.workspace,
                    &fixture.effects,
                ),
                &buffer,
                false,
            )
            .unwrap();
        assert_eq!(fixture.stored(), stored);

        // A settings refresh of the same conversation keeps what is typed.
        let mut typing = "typing now".to_owned();
        let mut typing_cursor = typing.len();
        let refresh = read_saved_activation(&restarted, &fixture.initial, "model", false)
            .unwrap()
            .unwrap();
        apply_saved_draft(
            refresh,
            SavedDraftTarget {
                input_buffer: &mut typing,
                input_cursor: &mut typing_cursor,
                input_history: &mut History::new(),
                autosave: &mut DraftAutosave::new(),
            },
        );
        assert_eq!(typing, "typing now");
    }

    /// A damaged C10 intent file: the blocker cannot be proven clear, so a draft
    /// write must wait. Returns the file, so a test can clear the condition.
    fn tear_c10_intents(state: &std::path::Path) -> std::path::PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let sync = state.join("sync");
        let intents = sync.join("remote-intents-v2");
        for directory in [&sync, &intents, &intents.join("archive")] {
            std::fs::create_dir(directory).unwrap();
            std::fs::set_permissions(directory, std::fs::Permissions::from_mode(0o700)).unwrap();
        }
        let torn = intents.join(format!("{}.json", "a".repeat(64)));
        for (path, bytes) in [
            (intents.join("owner.lock"), b"".as_slice()),
            (torn.clone(), b"{torn".as_slice()),
        ] {
            std::fs::write(&path, bytes).unwrap();
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
        }
        torn
    }
}
