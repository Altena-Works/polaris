//! `/memory configure` saves the remember and history-mode settings of the open conversation.
//! It uses the conversation owner and the shared project lease.

use crate::{HistoryBackendFactory, account, approver::KeyReader, render};
use polaris_core::{
    conversation_state::HistoryMode,
    management_effect::{ManagementEffectStoreV1, ManagementOperationIdV1},
    rewind_workspace::WorkspaceOwner,
    session::Session,
    session_settings::{SavedSessionSettingsV1, SessionSettingsCommandV1, SessionSettingsOwnerV1},
};
use ratatui::{Terminal, backend::Backend, text::Line};
use std::cell::RefCell;

pub(crate) struct CommandResult {
    pub lines: Vec<Line<'static>>,
    pub settings_changed: bool,
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn handle<B: Backend, R: KeyReader>(
    request: &str,
    terminal: &RefCell<Terminal<B>>,
    reader: &mut R,
    session: &mut Session,
    initial: &SavedSessionSettingsV1,
    workspace: &WorkspaceOwner,
    effects: &ManagementEffectStoreV1,
    history_factory: Option<&HistoryBackendFactory>,
    has_pending: bool,
) -> Result<CommandResult, String> {
    let request = request.trim().trim_start_matches('/');
    let persistence = session
        .persistence
        .as_ref()
        .ok_or("No saved conversation")?
        .clone();
    if std::fs::symlink_metadata(persistence.store.directory().join("raw.jsonl"))
        .map_err(|e| e.to_string())?
        .len()
        > 16 * 1024 * 1024
    {
        return Err("Conversation source text exceeds the 16 MiB read limit".into());
    }
    let snapshot = persistence.snapshot().map_err(|e| e.to_string())?;
    if has_pending {
        return Err("Cannot change session settings while input is queued".into());
    }
    session.check_persistence().map_err(|e| e.to_string())?;
    let command = parse(request)?;
    let history_mode = match &command {
        SessionSettingsCommandV1::History { mode } => Some(*mode),
        _ => None,
    };
    let preview = SessionSettingsOwnerV1::preview(&persistence, initial, command)
        .map_err(|e| e.to_string())?;
    let lease = workspace
        .try_mutation()
        .map_err(|_| "Cannot change session settings while an operation is active")?;
    workspace
        .require_quiescent_with_lease(&lease)
        .map_err(|_| "An unresolved rewind operation exists")?;
    polaris_core::sync::composite::C10CompositeBlockerV1::require_clear_with_mutation(
        workspace.project_root(),
        effects.state_dir(),
        &lease,
    )
    .map_err(|error| error.to_string())?;
    let mut staged = session.clone();
    if let Some(mode) = history_mode {
        crate::ensure_history_backend(&mut staged, mode, history_factory)?;
    }
    let owner = SessionSettingsOwnerV1 {
        effects,
        lease: &lease,
    };
    let prepared = owner
        .prepare(
            &persistence,
            ManagementOperationIdV1::random().map_err(|e| e.to_string())?,
            &preview,
        )
        .map_err(|e| e.to_string())?;
    if !account::confirm(
        terminal,
        reader,
        "Save session settings",
        preview.session_id(),
        &format!("{request}\nSave in this conversation; effective from the next send."),
    ) {
        owner.deny(&prepared).map_err(|e| e.to_string())?;
        return Ok(CommandResult {
            lines: vec![line("Session settings change cancelled")],
            settings_changed: false,
        });
    }
    let published = owner
        .execute(&persistence, &prepared)
        .map_err(|e| e.to_string())?;
    if let Some(mode) = history_mode {
        if mode == HistoryMode::Legacy {
            session.strict_history = None;
            session.summary_usage = None;
        } else {
            session.strict_history = staged.strict_history;
            session.summary_usage = staged.summary_usage;
        }
    }
    Ok(CommandResult {
        lines: vec![line(format!(
            "Saved {} (conversation generation {}).",
            preview.inventory(),
            published.state.generation
        ))],
        settings_changed: published.state.session_settings != snapshot.state.session_settings,
    })
}

fn parse(request: &str) -> Result<SessionSettingsCommandV1, String> {
    match request.split_whitespace().collect::<Vec<_>>().as_slice() {
        ["memory", "configure", "on"] => Ok(SessionSettingsCommandV1::Remember { enabled: true }),
        ["memory", "configure", "off"] => Ok(SessionSettingsCommandV1::Remember { enabled: false }),
        ["memory", "configure", mode] => Ok(SessionSettingsCommandV1::History {
            mode: parse_history(mode)?,
        }),
        _ => Err("Specify /memory configure on|off|legacy|strict10|structured10".into()),
    }
}

fn parse_history(value: &str) -> Result<HistoryMode, String> {
    match value {
        "legacy" => Ok(HistoryMode::Legacy),
        "strict10" => Ok(HistoryMode::Strict10),
        "structured10" => Ok(HistoryMode::Structured10),
        _ => Err("Specify history mode as legacy|strict10|structured10".into()),
    }
}

fn line(text: impl AsRef<str>) -> Line<'static> {
    Line::from(render::sanitize(text.as_ref()))
}

#[cfg(test)]
mod tests;
