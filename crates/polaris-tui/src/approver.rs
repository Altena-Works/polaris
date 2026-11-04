//! The `Approver` that runs inside the TUI: draws a modal over the current
//! frame and blocks on a single keypress. Generic over `KeyReader` so
//! tests can feed keys without a real terminal.

use std::cell::RefCell;
use std::time::{Duration, Instant};

use polaris_core::approval::{Approver, Decision};
use ratatui::Terminal;
use ratatui::backend::Backend;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use zeroize::Zeroizing;

use crate::render::render_approval_modal;

/// Pasted text stays in a zeroizing owner until the dedicated input consumes it.
pub enum SecretInputEvent {
    Key(KeyCode),
    Paste(Zeroizing<String>),
}

/// Abstracts "block until the next key comes in" so `TuiApprover` can be
/// tested without a real terminal.
pub trait KeyReader {
    fn read_key(&mut self) -> std::io::Result<KeyCode>;

    /// Drops the events already waiting, so a key typed before a prompt opened
    /// cannot answer it. The default drops nothing, which fits a reader with
    /// no queue of its own, such as a scripted one.
    fn discard_pending(&mut self) -> std::io::Result<()> {
        Ok(())
    }

    fn read_secret_key(&mut self) -> std::io::Result<KeyCode> {
        self.read_key()
    }

    fn read_secret_input(&mut self) -> std::io::Result<SecretInputEvent> {
        self.read_secret_key().map(SecretInputEvent::Key)
    }
}

/// The real, blocking reader used outside tests.
pub struct CrosstermKeyReader;

impl KeyReader for CrosstermKeyReader {
    fn read_key(&mut self) -> std::io::Result<KeyCode> {
        loop {
            if let ratatui::crossterm::event::Event::Key(k) = crate::terminal::read_event()?
                && k.kind == KeyEventKind::Press
            {
                return Ok(k.code);
            }
        }
    }

    fn discard_pending(&mut self) -> std::io::Result<()> {
        discard_waiting_events()
    }

    fn read_secret_key(&mut self) -> std::io::Result<KeyCode> {
        loop {
            if let ratatui::crossterm::event::Event::Key(key) = crate::terminal::read_event()?
                && let Some(code) = secret_key_code(key)
            {
                return Ok(code);
            }
        }
    }

    fn read_secret_input(&mut self) -> std::io::Result<SecretInputEvent> {
        loop {
            if let Some(input) = secret_input_event(crate::terminal::read_event()?) {
                return Ok(input);
            }
        }
    }
}

/// Reads and drops every event already waiting. It reads through `read_event`,
/// so the on-screen selection still sees the events it drops.
pub(crate) fn discard_waiting_events() -> std::io::Result<()> {
    while ratatui::crossterm::event::poll(Duration::ZERO)? {
        crate::terminal::read_event()?;
    }
    Ok(())
}

fn secret_input_event(event: ratatui::crossterm::event::Event) -> Option<SecretInputEvent> {
    match event {
        ratatui::crossterm::event::Event::Key(key) => {
            secret_key_code(key).map(SecretInputEvent::Key)
        }
        ratatui::crossterm::event::Event::Paste(text) => {
            Some(SecretInputEvent::Paste(Zeroizing::new(text)))
        }
        _ => None,
    }
}

fn secret_key_code(key: KeyEvent) -> Option<KeyCode> {
    if key.kind != KeyEventKind::Press {
        return None;
    }
    if key.modifiers.contains(KeyModifiers::CONTROL) && matches!(key.code, KeyCode::Char('c' | 'C'))
    {
        Some(KeyCode::Esc)
    } else {
        Some(key.code)
    }
}

/// `terminal` is shared via `RefCell`, not held exclusively, because the
/// main loop's agent turn now redraws the screen concurrently with
/// `agent::run` (see `lib.rs`'s `tokio::select!` around the turn) —
/// something that borrows the same terminal to animate the status row
/// while `agent::run` is in flight, including while it's inside a call to
/// `ask` here. Both sides only ever borrow it for the duration of one
/// synchronous `draw` call, never across an `.await`, so runtime aliasing
/// never actually occurs even though the type allows it.
pub struct TuiApprover<'a, B: Backend, R: KeyReader> {
    pub terminal: &'a RefCell<Terminal<B>>,
    pub reader: &'a mut R,
}

/// How long after the modal is drawn a `y` or `n` still does not count. A key
/// typed before the modal appeared can reach the reader just after it does,
/// and must not answer for someone who has not seen the question.
pub(crate) const ANSWER_GUARD: Duration = Duration::from_millis(300);

impl<'a, B: Backend, R: KeyReader> Approver for TuiApprover<'a, B, R> {
    fn ask(&mut self, reason: &str) -> Decision {
        self.ask_at(reason, Instant::now)
    }
}

impl<B: Backend, R: KeyReader> TuiApprover<'_, B, R> {
    /// `ask` with the clock passed in, so a test can cross `ANSWER_GUARD`
    /// without waiting for it.
    fn ask_at(&mut self, reason: &str, now: impl Fn() -> Instant) -> Decision {
        // A draw failure or a read failure both fall through to Deny — an
        // approval gate that silently allows on I/O trouble is not a gate.
        if self
            .terminal
            .borrow_mut()
            .draw(|f| render_approval_modal(f, reason))
            .is_err()
        {
            return Decision::Deny;
        }
        // A `y` typed before the modal appeared is still waiting in the
        // terminal's input and would read as the answer. Drop what is already
        // there; `ANSWER_GUARD` below covers what is still on its way.
        if self.reader.discard_pending().is_err() {
            return Decision::Deny;
        }
        let drawn = now();

        loop {
            let key = self.reader.read_key();
            let too_early = now().duration_since(drawn) < ANSWER_GUARD;
            match key {
                Ok(KeyCode::Char('y')) | Ok(KeyCode::Char('Y')) if !too_early => {
                    return Decision::Allow;
                }
                Ok(KeyCode::Char('n')) | Ok(KeyCode::Char('N')) if !too_early => {
                    return Decision::Deny;
                }
                Ok(_) => continue,
                Err(_) => return Decision::Deny,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use std::cell::Cell;
    use std::collections::VecDeque;
    use std::rc::Rc;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    /// One read: how long the key takes to reach the approver, then the key
    /// (`None`: the read fails).
    type Step = (Duration, Option<KeyCode>);

    fn key(after: Duration, c: char) -> Step {
        (after, Some(KeyCode::Char(c)))
    }

    /// The terminal's input on a fake clock that moves by each key's delay as
    /// the key is read. `waiting` was typed before the prompt opened, so
    /// `discard_pending` removes it; `arriving` comes after.
    struct TypedKeys {
        clock: Rc<Cell<Instant>>,
        waiting: VecDeque<Step>,
        arriving: VecDeque<Step>,
        drain_fails: bool,
    }

    impl KeyReader for TypedKeys {
        fn read_key(&mut self) -> std::io::Result<KeyCode> {
            let (after, key) = self
                .waiting
                .pop_front()
                .or_else(|| self.arriving.pop_front())
                .expect("the prompt read past the end of the script");
            self.clock.set(self.clock.get() + after);
            key.ok_or_else(|| std::io::Error::other("read failed"))
        }

        fn discard_pending(&mut self) -> std::io::Result<()> {
            if self.drain_fails {
                return Err(std::io::Error::other("drain failed"));
            }
            self.waiting.clear();
            Ok(())
        }
    }

    /// One prompt, answered from `waiting` then `arriving` keys.
    fn ask(waiting: &[Step], arriving: &[Step], drain_fails: bool) -> Decision {
        let terminal = RefCell::new(Terminal::new(TestBackend::new(40, 10)).expect("terminal"));
        let clock = Rc::new(Cell::new(Instant::now()));
        let mut reader = TypedKeys {
            clock: Rc::clone(&clock),
            waiting: waiting.iter().copied().collect(),
            arriving: arriving.iter().copied().collect(),
            drain_fails,
        };
        let mut approver = TuiApprover {
            terminal: &terminal,
            reader: &mut reader,
        };
        approver.ask_at("writing to src/main.rs", || clock.get())
    }

    #[test]
    fn secret_input_ctrl_c_cancels_without_changing_an_ordinary_c() {
        assert_eq!(
            secret_key_code(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)),
            Some(KeyCode::Esc)
        );
        assert_eq!(
            secret_key_code(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE)),
            Some(KeyCode::Char('c'))
        );
        let mut release = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        release.kind = KeyEventKind::Release;
        assert_eq!(secret_key_code(release), None);
    }

    #[test]
    fn secret_paste_is_owned_by_a_zeroizing_event() {
        let event = ratatui::crossterm::event::Event::Paste("synthetic-paste".to_owned());
        let Some(SecretInputEvent::Paste(value)) = secret_input_event(event) else {
            panic!("paste must reach the dedicated input");
        };
        assert_eq!(value.as_str(), "synthetic-paste");
        assert!(matches!(
            secret_input_event(ratatui::crossterm::event::Event::Key(KeyEvent::new(
                KeyCode::Char('c'),
                KeyModifiers::CONTROL
            ))),
            Some(SecretInputEvent::Key(KeyCode::Esc))
        ));
    }

    #[test]
    fn y_allows() {
        assert_eq!(ask(&[], &[key(ANSWER_GUARD, 'y')], false), Decision::Allow);
    }

    #[test]
    fn n_denies() {
        // The y behind it is never read: the n already answered.
        let script = [key(ANSWER_GUARD, 'n'), key(ms(0), 'y')];
        assert_eq!(ask(&[], &script, false), Decision::Deny);
    }

    #[test]
    fn an_unrecognized_key_is_ignored_until_y_or_n_comes() {
        let script = [
            (ANSWER_GUARD, Some(KeyCode::Char('x'))),
            (ms(0), Some(KeyCode::Up)),
            key(ms(0), 'y'),
        ];
        assert_eq!(ask(&[], &script, false), Decision::Allow);
    }

    #[test]
    fn a_y_waiting_before_the_prompt_opened_does_not_approve() {
        // A backlog slow enough to be read after the guard has passed: only
        // dropping it keeps its y from answering. The n is the real answer.
        let waiting = [key(ANSWER_GUARD * 2, 'y')];
        let arriving = [key(ANSWER_GUARD, 'n')];
        assert_eq!(ask(&waiting, &arriving, false), Decision::Deny);
    }

    #[test]
    fn a_y_inside_the_guard_period_does_not_approve() {
        // The y one tick short of the guard is ignored; the n a tick later is
        // the first key that counts.
        let script = [key(ANSWER_GUARD - ms(1), 'y'), key(ms(1), 'n')];
        assert_eq!(ask(&[], &script, false), Decision::Deny);
    }

    #[test]
    fn an_n_inside_the_guard_period_is_ignored_too() {
        let script = [key(ms(1), 'n'), key(ANSWER_GUARD, 'y')];
        assert_eq!(ask(&[], &script, false), Decision::Allow);
    }

    #[test]
    fn a_read_error_denies() {
        assert_eq!(ask(&[], &[(ms(0), None)], false), Decision::Deny);
    }

    #[test]
    fn a_failed_drain_denies_without_waiting_for_a_key() {
        assert_eq!(ask(&[], &[key(ANSWER_GUARD, 'y')], true), Decision::Deny);
    }
}
