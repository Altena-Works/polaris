//! 会話のtranscript表示だけを所有する。
//! Session、provider、永続化、入力admissionはcallerに残す。

use crate::{render, selection};
use ratatui::text::Line;
use std::collections::HashMap;

pub(super) struct ConversationViewState {
    pub(super) history: Vec<render::HistoryLine>,
    pub(super) history_wrap_cache: render::HistoryWrapCache,
    printed_messages: usize,
    printed_local_lines: usize,
    pub(super) scroll_offset: usize,
    pub(super) selection: Option<selection::Selection>,
    #[cfg(target_os = "macos")]
    pub(super) file_click: crate::file_reveal::FileClick,
    activity_rows: AgentActivityRows,
    /// The footer's workflow stage and subagent summary, kept from the same
    /// live events as `activity_rows`.
    pub(super) footer: render::FooterLive,
}

impl ConversationViewState {
    pub(super) fn new(header: &render::HeaderInfo<'_>, width: u16) -> Self {
        Self {
            history: render::header_history_lines(header, width),
            history_wrap_cache: render::HistoryWrapCache::default(),
            printed_messages: 0,
            printed_local_lines: 0,
            scroll_offset: 0,
            selection: None,
            #[cfg(target_os = "macos")]
            file_click: crate::file_reveal::FileClick::default(),
            activity_rows: AgentActivityRows::default(),
            footer: render::FooterLive::default(),
        }
    }

    pub(super) fn append_new_history(
        &mut self,
        session: &polaris_core::session::Session,
        local_lines: &[Line<'static>],
    ) {
        if append_new_history_parts(
            session,
            local_lines,
            &mut self.printed_messages,
            &mut self.printed_local_lines,
            &mut self.history,
        ) {
            self.history_wrap_cache.invalidate();
        }
    }

    pub(super) fn with_temporary_user_input<R>(
        &mut self,
        text: &str,
        draw: impl FnOnce(&[render::HistoryLine], &mut render::HistoryWrapCache) -> R,
    ) -> R {
        let appended = render::history_lines_for(&[polaris_provider::Message::user(text)]);
        self.history_wrap_cache
            .with_appended(&mut self.history, appended, draw)
    }

    pub(super) fn reset(&mut self, header: &render::HeaderInfo<'_>, width: u16) {
        self.history.clear();
        self.history
            .extend(render::header_history_lines(header, width));
        self.history_wrap_cache.invalidate();
        self.printed_messages = 0;
        self.printed_local_lines = 0;
        self.scroll_offset = 0;
        self.selection = None;
        #[cfg(target_os = "macos")]
        self.file_click.cancel();
        self.activity_rows = AgentActivityRows::default();
        // The previous conversation's subagents are not this one's. The
        // workflow stage is read again from the session before each draw.
        self.footer.subagents = render::SubagentSummary::default();
    }

    pub(super) fn append_live_event(&mut self, event: &polaris_core::AgentEvent) {
        self.footer.observe(event);
        let appended = append_live_event_parts(event, &mut self.history, &mut self.activity_rows);
        self.history_wrap_cache.invalidate();
        if self.scroll_offset > 0 {
            self.scroll_offset += appended;
        }
    }

    pub(super) fn push_history_line(&mut self, line: render::HistoryLine) {
        self.history.push(line);
        self.history_wrap_cache.invalidate();
    }

    pub(super) fn extend_text_lines(&mut self, lines: impl IntoIterator<Item = String>) {
        self.history
            .extend(lines.into_iter().map(|line| render::HistoryLine {
                line: Line::from(line),
                shaded: false,
            }));
        self.history_wrap_cache.invalidate();
    }

    pub(super) fn observe_compaction(&mut self, messages_before: usize, messages_after: usize) {
        self.printed_messages = self
            .printed_messages
            .saturating_sub(messages_before.saturating_sub(messages_after));
    }
}

pub(super) fn append_new_history_parts(
    session: &polaris_core::session::Session,
    local_lines: &[Line<'static>],
    printed_messages: &mut usize,
    printed_local_lines: &mut usize,
    history: &mut Vec<render::HistoryLine>,
) -> bool {
    let mut changed = false;
    if *printed_messages > session.messages.len() {
        *printed_messages = 0;
        history.clear();
        changed = true;
    }
    if *printed_local_lines > local_lines.len() {
        *printed_local_lines = 0;
        history.clear();
        changed = true;
    }
    if session.messages.len() > *printed_messages {
        let lines = render::history_lines_for(&session.messages[*printed_messages..]);
        history.extend(lines);
        *printed_messages = session.messages.len();
        changed = true;
    }
    if local_lines.len() > *printed_local_lines {
        let new_lines = &local_lines[*printed_local_lines..];
        history.extend(new_lines.iter().map(|line| render::HistoryLine {
            line: line.clone(),
            shaded: false,
        }));
        *printed_local_lines = local_lines.len();
        changed = true;
    }
    changed
}

#[derive(Default)]
struct AgentActivityRows {
    rows: HashMap<(String, String), usize>,
}

fn append_live_event_parts(
    event: &polaris_core::AgentEvent,
    history: &mut Vec<render::HistoryLine>,
    activity_rows: &mut AgentActivityRows,
) -> usize {
    let key = match event {
        polaris_core::AgentEvent::SpawnStarted {
            display_name,
            agent_type,
            ..
        }
        | polaris_core::AgentEvent::SpawnActivity {
            display_name,
            agent_type,
            ..
        }
        | polaris_core::AgentEvent::SpawnFinished {
            display_name,
            agent_type,
            ..
        } => Some((display_name.clone(), agent_type.clone())),
        _ => None,
    };

    if matches!(event, polaris_core::AgentEvent::SpawnActivity { .. }) {
        if let Some(row) = key
            .as_ref()
            .and_then(|key| activity_rows.rows.get(key))
            .copied()
            && let Some(line) = render::format_event_for_live_print(event)
                .into_iter()
                .next()
            && let Some(slot) = history.get_mut(row)
        {
            *slot = line;
        }
        return 0;
    }

    let before = history.len();
    history.extend(render::format_event_for_live_print(event));
    match event {
        polaris_core::AgentEvent::SpawnStarted { .. } => {
            if let Some(key) = key {
                activity_rows.rows.insert(key, before + 1);
            }
        }
        polaris_core::AgentEvent::SpawnFinished { .. } => {
            if let Some(key) = key {
                activity_rows.rows.remove(&key);
            }
        }
        _ => {}
    }
    history.len() - before
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header<'a>() -> render::HeaderInfo<'a> {
        render::HeaderInfo {
            provider_name: "test",
            model_name: "test",
            usage: Default::default(),
            cwd: "test",
            cwd_short: "test",
            effort_name: "auto",
        }
    }

    #[test]
    fn same_count_conversation_reset_replaces_the_projected_transcript() {
        let mut first = polaris_core::session::Session::default();
        first.push_user("first conversation");
        let mut second = polaris_core::session::Session::default();
        second.push_user("second conversation");
        let mut view = ConversationViewState::new(&header(), 80);
        let first_local = [Line::from("first local line")];
        let second_local = [Line::from("second local line")];

        view.append_new_history(&first, &first_local);
        let _ = view.history_wrap_cache.lines(&view.history, 80);
        view.scroll_offset = 4;
        view.selection = Some(selection::Selection {
            anchor: selection::TextPos { line: 0, col: 0 },
            cursor: selection::TextPos { line: 0, col: 1 },
            dragging: false,
        });
        view.reset(&header(), 80);
        view.append_new_history(&second, &second_local);

        let text = view
            .history_wrap_cache
            .lines(&view.history, 80)
            .iter()
            .map(|line| line.line.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(text.contains("second conversation"));
        assert!(!text.contains("first conversation"));
        assert!(text.contains("second local line"));
        assert!(!text.contains("first local line"));
        assert_eq!(view.printed_messages, second.messages.len());
        assert_eq!(view.printed_local_lines, second_local.len());
        assert_eq!(view.scroll_offset, 0);
        assert!(view.selection.is_none());
        assert_eq!(
            view.history.first().map(|line| line.line.to_string()),
            render::header_history_lines(&header(), 80)
                .first()
                .map(|line| line.line.to_string())
        );
    }

    #[test]
    fn temporary_user_input_does_not_advance_projection_counters() {
        let mut view = ConversationViewState::new(&header(), 80);
        let history_len = view.history.len();

        let result = view.with_temporary_user_input("not accepted", |preview, _| {
            assert_eq!(preview.len(), history_len + 1);
            assert_eq!(preview.last().unwrap().line.to_string(), "not accepted");
            Err::<(), _>("draw failed")
        });

        assert_eq!(result, Err("draw failed"));
        assert_eq!(view.history.len(), history_len);
        assert_eq!(view.printed_messages, 0);
        assert_eq!(view.printed_local_lines, 0);
    }

    #[test]
    fn append_live_event_appends_formatted_lines() {
        let mut view = ConversationViewState::new(&header(), 80);
        view.append_live_event(&polaris_core::AgentEvent::ToolStarted {
            name: "read".to_string(),
            detail: "Cargo.toml".to_string(),
        });
        assert!(view.history.len() > render::header_history_lines(&header(), 80).len());
    }

    #[test]
    fn live_events_feed_the_footer_and_a_reset_forgets_the_old_conversations_subagents() {
        let mut view = ConversationViewState::new(&header(), 80);
        let started = |display_name: &str| polaris_core::AgentEvent::SpawnStarted {
            display_name: display_name.to_string(),
            agent_type: "file-reader".to_string(),
            task: "private".to_string(),
        };
        view.append_live_event(&polaris_core::AgentEvent::WorkflowResolved {
            phase: "implement".to_string(),
            skill_ids: Vec::new(),
            changed_skill_ids: Vec::new(),
            manifest_hash: "hash".to_string(),
        });
        view.append_live_event(&started("Sirius"));
        view.append_live_event(&started("Antares"));
        view.append_live_event(&polaris_core::AgentEvent::SpawnFinished {
            display_name: "Sirius".to_string(),
            agent_type: "file-reader".to_string(),
            ok: true,
        });
        assert_eq!(
            view.footer.workflow,
            Some(polaris_core::workflow::Phase::Implement)
        );
        assert_eq!(
            view.footer.subagents,
            render::SubagentSummary {
                done: 1,
                running: 1,
                failed: 0
            }
        );

        view.reset(&header(), 80);
        assert_eq!(view.footer.subagents, render::SubagentSummary::default());
    }

    #[test]
    fn child_tool_activity_replaces_only_that_agents_single_progress_row() {
        let mut view = ConversationViewState::new(&header(), 80);
        let header_rows = view.history.len();
        for (display_name, agent_type) in [("Sirius", "file-reader"), ("Antares", "file-reader")] {
            view.append_live_event(&polaris_core::AgentEvent::SpawnStarted {
                display_name: display_name.to_string(),
                agent_type: agent_type.to_string(),
                task: "private".to_string(),
            });
        }
        let activity =
            |display_name: &str, name: &str, path: &str| polaris_core::AgentEvent::SpawnActivity {
                display_name: display_name.to_string(),
                agent_type: "file-reader".to_string(),
                name: name.to_string(),
                detail: serde_json::json!({"path": path}).to_string(),
            };
        view.append_live_event(&activity("Sirius", "read", "first.md"));
        view.append_live_event(&activity("Antares", "write", "notes.md"));
        let cached_before_replacement = view
            .history_wrap_cache
            .lines(&view.history, 80)
            .iter()
            .map(|line| line.line.to_string())
            .collect::<Vec<_>>();
        assert!(
            cached_before_replacement
                .iter()
                .any(|line| line == "  → read(first.md)")
        );
        view.append_live_event(&activity("Sirius", "read", "second.md"));

        let history = &view.history[header_rows..];
        assert_eq!(history.len(), 4, "activity must update instead of append");
        assert_eq!(history[0].line.to_string(), "⏺ Sirius (file-reader)");
        assert_eq!(history[1].line.to_string(), "  → read(second.md)");
        assert_eq!(history[2].line.to_string(), "⏺ Antares (file-reader)");
        assert_eq!(history[3].line.to_string(), "  → write(notes.md)");
        let cached_after_replacement = view.history_wrap_cache.lines(&view.history, 80);
        assert!(
            cached_after_replacement
                .iter()
                .any(|line| line.line.to_string() == "  → read(second.md)")
        );
        assert!(
            !cached_after_replacement
                .iter()
                .any(|line| line.line.to_string() == "  → read(first.md)")
        );
    }
}
