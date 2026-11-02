//! The reasoning effort of the root's next request, picked from the turn so
//! far.
//!
//! An experiment, off unless `POLARIS_EFFORT_PLAN` names a lower effort
//! (`medium`, `high` or `xhigh`; never below the model's floor). Only the
//! reading that opens a turn goes lower, and only once it has gone on for two
//! requests in a row: every switch of effort loses the server's cache of the
//! whole history so far (measured 2026-11-01: lowering after every passing
//! check cut output by a quarter and gave most of it back as uncached input).
//! From the first edit on, the turn stays at its own effort. The tool list
//! and the system prompt do not change.
//!
//! Decided from the message history alone; nothing here calls a model.

use polaris_provider::{Message, Provider, Role};

/// Lowest first, as `--effort` accepts them.
pub const EFFORTS: [&str; 5] = ["low", "medium", "high", "xhigh", "max"];

/// The lowest effort a model is run at. luna, terra and sol run from
/// `medium` to `max`; astra is chosen for work judged to need it, so it runs
/// from `high`. Other models have none.
pub fn floor(model: &str) -> Option<&'static str> {
    match model {
        "gpt-6-luna" | "gpt-6.1-terra" | "gpt-6.1-sol" => Some("medium"),
        "gpt-6-astra" => Some("high"),
        _ => None,
    }
}

/// `effort` raised to `model`'s floor; the flag says whether it was.
pub fn at_least_floor(model: &str, effort: &str) -> (String, bool) {
    let rank = |name: &str| EFFORTS.iter().position(|known| *known == name);
    match floor(model) {
        Some(floor) if rank(effort) < rank(floor) => (floor.to_owned(), true),
        _ => (effort.to_owned(), false),
    }
}

/// What the next request is about to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// The turn's first request, or the one after a harness message.
    Plan,
    /// Reading before anything was changed.
    Explore,
    /// Following an edit, a failed command or a tool other than read and bash.
    Implement,
    /// Following commands that passed, after an edit.
    Verify,
}

/// The lower effort `POLARIS_EFFORT_PLAN` names; `None` leaves every request
/// at the session's effort.
pub fn lower_from_env() -> Option<String> {
    std::env::var("POLARIS_EFFORT_PLAN")
        .ok()
        .filter(|value| EFFORTS.contains(&value.as_str()))
}

pub fn step(messages: &[Message]) -> Step {
    let start = messages
        .iter()
        .rposition(|m| m.role == Role::User && !crate::quality::is_harness(&m.content))
        .unwrap_or(0);
    let turn = &messages[start..];
    let Some(last) = turn
        .iter()
        .rposition(|m| m.role == Role::Assistant && !m.tool_calls.is_empty())
    else {
        return Step::Plan;
    };
    let after = &turn[last + 1..];
    if after.iter().any(|m| m.role == Role::User) {
        return Step::Plan;
    }
    let edits = |m: &Message| m.tool_calls.iter().any(changes_files);
    let batch = &turn[last];
    let failed = after.iter().any(|m| {
        m.role == Role::Tool && m.content.starts_with("the command failed with exit code")
    });
    // Only `read` counts as reading. A command's output is usually something
    // the model is about to interpret and decide on (measured 2026-11-01: a
    // plan judged infeasible at the lower effort right after an analysis
    // script, and the judgement held), so it keeps the turn's effort.
    let reads_only = batch.tool_calls.iter().all(|c| c.name == "read");
    if edits(batch) || failed || !reads_only {
        Step::Implement
    } else if turn[..last].iter().any(edits) {
        Step::Verify
    } else {
        Step::Explore
    }
}

/// Whether a call changes files: `write`, `edit`, or a command that writes
/// (a redirect, `tee`, an in-place edit, a heredoc, moving or removing files).
// ponytail: a pattern over the command text, not a parse of the shell; a write
// it misses only lets later reading go lower again.
pub fn changes_files(call: &polaris_provider::ToolCall) -> bool {
    match call.name.as_str() {
        "write" | "edit" => true,
        "bash" => call
            .arguments
            .get("command")
            .and_then(|command| command.as_str())
            .is_some_and(writes),
        _ => false,
    }
}

fn writes(command: &str) -> bool {
    let words = [
        "tee ",
        "sed -i",
        "perl -i",
        "<<",
        "patch ",
        "git apply",
        "mv ",
        "cp ",
        "rm ",
        "mkdir ",
        "touch ",
        "truncate ",
        ".write_text(",
        ".write(",
    ];
    if words.iter().any(|word| command.contains(word)) {
        return true;
    }
    // `>` or `>>` into a file, but not `2>&1`, `>&2` or into /dev/null.
    let bytes = command.as_bytes();
    bytes.iter().enumerate().any(|(i, b)| {
        *b == b'>'
            && !matches!(
                i.checked_sub(1).map(|j| bytes[j]),
                Some(b'0'..=b'9' | b'-' | b'=' | b'>')
            )
            && {
                let rest = command[i + 1..].trim_start_matches('>').trim_start();
                !rest.starts_with('&') && !rest.starts_with("/dev/null") && !rest.is_empty()
            }
    })
}

/// Whether the next request goes lower: it is exploring, and so was the one
/// before it. A single reading step is not worth a switch and its cache loss.
pub fn lowered(messages: &[Message]) -> bool {
    if step(messages) != Step::Explore {
        return false;
    }
    let Some(last) = messages
        .iter()
        .rposition(|m| m.role == Role::Assistant && !m.tool_calls.is_empty())
    else {
        return false;
    };
    step(&messages[..last]) == Step::Explore
}

/// The effort of the next request: `lower` (raised to `model`'s floor) when
/// [`lowered`], otherwise `base`. Never above `base`, and nothing changes when
/// `base` is unknown (the provider's own default), since `lower` might then
/// raise it.
pub fn effort_for(
    lowered: bool,
    base: Option<&str>,
    lower: &str,
    model: Option<&str>,
) -> Option<String> {
    let rank = |effort: &str| EFFORTS.iter().position(|e| *e == effort);
    let lower = match model {
        Some(model) => at_least_floor(model, lower).0,
        None => lower.to_owned(),
    };
    match (lowered, base.and_then(rank), rank(&lower)) {
        (true, Some(base_rank), Some(lower_rank)) if lower_rank < base_rank => Some(lower),
        _ => base.map(str::to_owned),
    }
}

/// Holds the effort the turn started with and puts it back when the loop
/// ends, however it ends, so the next turn does not start from a lowered one.
pub struct Restore<'a> {
    provider: &'a dyn Provider,
    base: Option<String>,
}

impl<'a> Restore<'a> {
    pub fn new(provider: &'a dyn Provider) -> Self {
        Self {
            base: provider.effort(),
            provider,
        }
    }

    pub fn base(&self) -> Option<&str> {
        self.base.as_deref()
    }
}

impl Drop for Restore<'_> {
    fn drop(&mut self) {
        self.provider.set_effort(self.base.as_deref());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use polaris_provider::ToolCall;

    fn calls(names: &[&str]) -> Message {
        Message::assistant_with_tool_calls(
            "",
            names
                .iter()
                .enumerate()
                .map(|(i, name)| ToolCall {
                    id: format!("c{i}"),
                    name: (*name).into(),
                    arguments: "{}".into(),
                })
                .collect(),
        )
    }

    fn result(text: &str) -> Message {
        Message::tool_result("c0", text)
    }

    #[test]
    fn the_steps_follow_reading_editing_and_checking() {
        let mut turn = vec![Message::user("fix the bug")];
        assert_eq!(step(&turn), Step::Plan);
        turn.extend([calls(&["read"]), result("fn main() {}")]);
        assert_eq!(step(&turn), Step::Explore);
        turn.extend([calls(&["bash"]), result("ls output")]);
        assert_eq!(
            step(&turn),
            Step::Implement,
            "a command's output is interpreted at full effort"
        );
        turn.extend([calls(&["read"]), result("more")]);
        assert_eq!(step(&turn), Step::Explore);
        turn.extend([calls(&["edit"]), result("ok")]);
        assert_eq!(step(&turn), Step::Implement);
        turn.extend([
            calls(&["bash"]),
            result("the command failed with exit code 1"),
        ]);
        assert_eq!(
            step(&turn),
            Step::Implement,
            "a failure is fixed at full effort"
        );
        turn.extend([calls(&["bash"]), result("12 passed")]);
        assert_eq!(
            step(&turn),
            Step::Implement,
            "a command's output keeps full effort"
        );
        turn.extend([calls(&["read"]), result("checked")]);
        assert_eq!(step(&turn), Step::Verify);
        turn.push(Message::user("[harness] check the requirements"));
        assert_eq!(
            step(&turn),
            Step::Plan,
            "a harness message is answered at full effort"
        );
        turn.extend([calls(&["spawn"]), result("done")]);
        assert_eq!(step(&turn), Step::Implement, "other tools keep full effort");
    }

    #[test]
    fn only_a_second_reading_step_before_any_edit_goes_lower() {
        let mut turn = vec![Message::user("fix the bug")];
        assert!(!lowered(&turn), "the first request plans");
        turn.extend([calls(&["read"]), result("a")]);
        assert!(!lowered(&turn), "one reading step is not worth a switch");
        turn.extend([calls(&["read"]), result("b")]);
        assert!(lowered(&turn), "two in a row are");
        turn.extend([calls(&["bash"]), result("analysis")]);
        assert!(!lowered(&turn), "a command's output is not lowered");
        turn.extend([calls(&["read"]), result("c")]);
        assert!(
            !lowered(&turn),
            "after a command, reading starts counting again"
        );
        turn.extend([calls(&["read"]), result("c2")]);
        assert!(lowered(&turn));
        turn.extend([calls(&["edit"]), result("ok")]);
        assert!(!lowered(&turn), "an edit returns to full effort");
        turn.extend([calls(&["bash"]), result("12 passed")]);
        turn.extend([calls(&["read"]), result("d")]);
        assert!(
            !lowered(&turn),
            "after the first edit the turn stays at full effort"
        );
    }

    fn bash(command: &str) -> Message {
        Message::assistant_with_tool_calls(
            "",
            vec![ToolCall {
                id: "c0".into(),
                name: "bash".into(),
                arguments: serde_json::json!({ "command": command }),
            }],
        )
    }

    #[test]
    fn a_command_that_writes_counts_as_an_edit() {
        for command in [
            "cat > app/x.py <<'EOF'\nprint(1)\nEOF",
            "echo hi >> notes.txt",
            "sed -i '' 's/a/b/' f.py",
            "python3 -c \"open('f','w').write('x')\"",
            "printf x | tee out.txt",
        ] {
            assert!(changes_files(&bash(command).tool_calls[0]), "{command}");
        }
        for command in [
            "ls -la app",
            "pytest -q 2>&1 | tail -5",
            "grep -n foo src/*.py >/dev/null && echo ok",
            "cargo test 2>&1",
        ] {
            assert!(!changes_files(&bash(command).tool_calls[0]), "{command}");
        }
        // After a command that wrote, reading stays at full effort.
        let turn = vec![
            Message::user("fix it"),
            bash("cat > a.py <<'EOF'\nx=1\nEOF"),
            result("ok"),
            calls(&["read"]),
            result("x=1"),
            calls(&["read"]),
            result("x=1"),
        ];
        assert!(!lowered(&turn));
    }

    #[test]
    fn a_new_user_turn_starts_over() {
        let turn = vec![
            Message::user("first"),
            calls(&["edit"]),
            result("ok"),
            Message::assistant("done"),
            Message::user("now read this"),
            calls(&["read"]),
            result("text"),
        ];
        assert_eq!(
            step(&turn),
            Step::Explore,
            "the earlier turn's edit does not count"
        );
    }

    #[test]
    fn lower_only_when_lowered_never_above_the_session_nor_below_the_floor() {
        let sol = Some("gpt-6.1-sol");
        assert_eq!(
            effort_for(true, Some("xhigh"), "medium", sol).as_deref(),
            Some("medium")
        );
        assert_eq!(
            effort_for(false, Some("xhigh"), "medium", sol).as_deref(),
            Some("xhigh")
        );
        assert_eq!(
            effort_for(true, Some("medium"), "high", sol).as_deref(),
            Some("medium")
        );
        assert_eq!(
            effort_for(true, None, "high", sol),
            None,
            "an unknown default is left alone"
        );
        assert_eq!(
            effort_for(true, Some("xhigh"), "low", sol).as_deref(),
            Some("medium"),
            "sol's floor"
        );
        assert_eq!(
            effort_for(true, Some("xhigh"), "medium", Some("gpt-6-astra")).as_deref(),
            Some("high"),
            "astra's floor"
        );
    }

    #[test]
    fn each_model_has_its_range() {
        for model in ["gpt-6-luna", "gpt-6.1-terra", "gpt-6.1-sol"] {
            assert_eq!(at_least_floor(model, "low"), ("medium".into(), true));
            assert_eq!(at_least_floor(model, "medium"), ("medium".into(), false));
            assert_eq!(at_least_floor(model, "max"), ("max".into(), false));
        }
        assert_eq!(
            at_least_floor("gpt-6-astra", "medium"),
            ("high".into(), true)
        );
        assert_eq!(at_least_floor("gpt-6-astra", "max"), ("max".into(), false));
        assert_eq!(at_least_floor("other", "low"), ("low".into(), false));
    }
}
