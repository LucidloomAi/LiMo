//! The helper's side of the channel, with no Windows code: what it does with
//! each message from the service, and the [`Display`] that draws.
//!
//! The helper runs in a signed in session, started by the service. It
//! reports what only a program inside the session can see (how long since
//! the last input, the window in front, full screen), and shows what the
//! service orders through a [`Display`]. It decides nothing.

#![forbid(unsafe_code)]

use std::sync::mpsc::Sender;

use limo_agent::Show;
use limo_core::{PinAction, WarningCause};

use crate::protocol::{Answer, FromHelper, PROTOCOL_VERSION, Secret, ToHelper};

/// What the service handed over to show, as the display gets it.
#[derive(Debug, Clone, Copy)]
pub struct ShowNow<'a> {
    /// The number of this show; it grows whenever what to show changes.
    pub seq: u64,
    /// The child's name, when the session is a child's.
    pub child_name: Option<&'a str>,
    /// What to show.
    pub show: &'a Show,
}

/// What is on the screen after [`Display::show`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Shown {
    /// True when a lock is up and covers every monitor.
    pub lock_on_screen: bool,
    /// True when the warning in the show handed over is on the screen.
    pub warning_on_screen: bool,
}

/// What draws, in the helper. Its one implementation now, the plain display
/// in the Windows modules (`win/display.rs`), is replaced later by the real
/// lock screen and time left display, drawn to the agreed design; whatever
/// replaces it must do exactly this:
///
/// - **[`Display::show`]** puts up what the service orders, every time it is
///   called, as a level: the same show again changes nothing on the screen
///   (it is called about every five seconds even when nothing changed).
///   - [`Show::Nothing`]: nothing of LiMo on the screen.
///   - [`Show::TimeLeft`]: a small window above every other, which is never
///     activated and never takes the keyboard, with the time left and
///     whether it is being counted; the warning in it, when there is one,
///     for a while; and the end of a guardian's unlock, when there is one.
///   - [`Show::Locked`]: a window over every monitor, above every other
///     window, put back on top a few times a second; the keyboard kept
///     from every other program while it is up; the lock screen's title,
///     message and, when known, when it opens again, and the child's name.
///     The ways out it offers ([`limo_agent::LockScreen`] says which) go to
///     the service through the [`Outbox`] the display was made with, never
///     anywhere else; a typed PIN or code goes as a [`Secret`] and is not
///     kept after it is sent.
///
///   It returns what is on the screen now. It must not say a lock is on the
///   screen unless it covers every monitor: the service disconnects the
///   session when no lock is confirmed within five seconds.
/// - **[`Display::notice`]** shows that a program was closed, and why, for a
///   while, beside the time left (or on the lock).
/// - **[`Display::answer`]** shows the answer to a way out it sent.
/// - **[`Display::pump`]**, called about four times a second on the same
///   thread, keeps the lock on top, counts down and lets notices fade.
///
/// It never shows or keeps a window title, a file name or a web address:
/// the service sends none.
pub trait Display {
    /// Puts up what the service orders now, and says what is on the screen.
    fn show(&mut self, now: &ShowNow<'_>) -> Shown;
    /// Shows that a program was closed, and why.
    fn notice(&mut self, text: &str);
    /// Shows the answer to a way out, by the number the [`Outbox`] gave it.
    fn answer(&mut self, ask: u32, answer: &Answer);
    /// Keeps what is shown on top and up to date.
    fn pump(&mut self);
}

/// Where a display sends the ways out from the lock screen. Each call gives
/// the number the answer will carry.
#[derive(Debug)]
pub struct Outbox {
    to_service: Sender<FromHelper>,
    next_ask: u32,
}

impl Outbox {
    /// An outbox sending to the helper's channel.
    pub fn new(to_service: Sender<FromHelper>) -> Outbox {
        Outbox { to_service, next_ask: 0 }
    }

    fn send(&mut self, message: impl FnOnce(u32) -> FromHelper) -> u32 {
        self.next_ask = self.next_ask.wrapping_add(1);
        let ask = self.next_ask;
        let _ = self.to_service.send(message(ask));
        ask
    }

    /// The child asks for `minutes` more today, with a short reason or
    /// none.
    pub fn ask_for_time(&mut self, minutes: u32, reason: &str) -> u32 {
        let reason = reason.to_string();
        self.send(|ask| FromHelper::AskForTime { ask, minutes, reason })
    }

    /// A one time code, as typed.
    pub fn try_code(&mut self, typed: Secret) -> u32 {
        self.send(|ask| FromHelper::TryCode { ask, code: typed })
    }

    /// A guardian's PIN, as typed, and what it is to do.
    pub fn try_pin(&mut self, typed: Secret, action: PinAction) -> u32 {
        self.send(|ask| FromHelper::TryPin { ask, pin: typed, action })
    }
}

/// What the helper must do after a message from the service.
#[derive(Debug, Default, PartialEq)]
pub struct Reaction {
    /// Messages to send back.
    pub replies: Vec<FromHelper>,
    /// True when the helper must end.
    pub quit: bool,
}

/// The helper's memory of what it told the service.
#[derive(Debug, Default)]
pub struct HelperCore {
    lock_told: Option<u64>,
    warnings_told: Vec<(WarningCause, u32, i64)>,
}

/// The hello that opens the channel.
pub fn hello() -> FromHelper {
    FromHelper::Hello { protocol: PROTOCOL_VERSION }
}

/// A report of what the helper sees in its session.
pub fn report(idle_ms: u64, front_pid: Option<u32>, own_pid: u32, full_screen: bool) -> FromHelper {
    // The service also knows the helper's own process; saying nothing in
    // front for it here saves it the look.
    let front_pid = front_pid.filter(|pid| *pid != own_pid && *pid != 0);
    FromHelper::Report { idle_ms, front_pid, full_screen }
}

impl HelperCore {
    /// A helper that has told the service nothing yet.
    pub fn new() -> HelperCore {
        HelperCore::default()
    }

    /// What to do with one message from the service.
    pub fn on_message(&mut self, message: ToHelper, display: &mut dyn Display) -> Reaction {
        let mut reaction = Reaction::default();
        match message {
            ToHelper::Show { seq, child_name, show } => {
                let shown = display.show(&ShowNow { seq, child_name: child_name.as_deref(), show: &show });
                match &show {
                    Show::Locked { .. } if shown.lock_on_screen => {
                        if self.lock_told != Some(seq) {
                            self.lock_told = Some(seq);
                            reaction.replies.push(FromHelper::LockOnScreen { seq });
                        }
                    }
                    Show::TimeLeft { warning: Some(w), .. } if shown.warning_on_screen => {
                        let key = (w.cause, w.minutes, w.lock_at);
                        if !self.warnings_told.contains(&key) {
                            if self.warnings_told.len() >= 16 {
                                self.warnings_told.remove(0);
                            }
                            self.warnings_told.push(key);
                            reaction.replies.push(FromHelper::WarningShown {
                                cause: w.cause,
                                minutes: w.minutes,
                                lock_at: w.lock_at,
                            });
                        }
                    }
                    _ => {}
                }
            }
            ToHelper::Notice { text } => display.notice(&text),
            ToHelper::Answer { ask, answer } => display.answer(ask, &answer),
            ToHelper::Quit => reaction.quit = true,
        }
        reaction
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use limo_agent::{LockScreen, LockWhy, WarningShown};
    use std::sync::mpsc;

    #[derive(Default)]
    struct Paper {
        shows: Vec<(u64, Option<String>, Show)>,
        notices: Vec<String>,
        answers: Vec<(u32, Answer)>,
        lock_draws: bool,
    }

    impl Display for Paper {
        fn show(&mut self, now: &ShowNow<'_>) -> Shown {
            self.shows.push((now.seq, now.child_name.map(String::from), now.show.clone()));
            Shown {
                lock_on_screen: self.lock_draws && matches!(now.show, Show::Locked { .. }),
                warning_on_screen: matches!(now.show, Show::TimeLeft { warning: Some(_), .. }),
            }
        }
        fn notice(&mut self, text: &str) {
            self.notices.push(text.to_string());
        }
        fn answer(&mut self, ask: u32, answer: &Answer) {
            self.answers.push((ask, answer.clone()));
        }
        fn pump(&mut self) {}
    }

    fn lock() -> Show {
        let why = LockWhy::GuardianLock;
        Show::Locked {
            screen: LockScreen {
                why,
                title: why.title().into(),
                message: why.message().into(),
                until: None,
                can_ask_for_time: false,
                can_use_code: true,
                can_use_pin: true,
                try_wait_until: None,
                latest_request: None,
            },
        }
    }

    #[test]
    fn a_lock_is_confirmed_once_for_each_show_and_only_when_it_is_up() {
        let mut core = HelperCore::new();
        let mut paper = Paper { lock_draws: false, ..Paper::default() };
        let show = |seq| ToHelper::Show { seq, child_name: Some("Asha".into()), show: lock() };
        // Not drawn: nothing is confirmed.
        assert_eq!(core.on_message(show(1), &mut paper), Reaction::default());
        paper.lock_draws = true;
        let first = core.on_message(show(1), &mut paper);
        assert_eq!(first.replies, vec![FromHelper::LockOnScreen { seq: 1 }]);
        // The same show again, as a level: shown again, told once.
        assert!(core.on_message(show(1), &mut paper).replies.is_empty());
        assert_eq!(core.on_message(show(2), &mut paper).replies, vec![FromHelper::LockOnScreen { seq: 2 }]);
        assert_eq!(paper.shows.len(), 4);
        assert_eq!(paper.shows[0].1.as_deref(), Some("Asha"));
    }

    #[test]
    fn a_warning_is_told_once_however_often_it_is_shown() {
        let mut core = HelperCore::new();
        let mut paper = Paper::default();
        let warning = WarningShown {
            cause: WarningCause::TimeRunsOut,
            minutes: 5,
            lock_at: 1_800_000_300,
            message: "5 minutes of screen time left today.".into(),
        };
        let show = |seq| ToHelper::Show {
            seq,
            child_name: None,
            show: Show::TimeLeft { seconds_left: Some(300), counting: true, warning: Some(warning.clone()), unlocked_until: None },
        };
        assert_eq!(
            core.on_message(show(1), &mut paper).replies,
            vec![FromHelper::WarningShown { cause: WarningCause::TimeRunsOut, minutes: 5, lock_at: 1_800_000_300 }]
        );
        assert!(core.on_message(show(2), &mut paper).replies.is_empty());
    }

    #[test]
    fn notices_answers_and_quit_reach_the_display() {
        let mut core = HelperCore::new();
        let mut paper = Paper::default();
        core.on_message(ToHelper::Notice { text: "game.exe was closed.".into() }, &mut paper);
        core.on_message(ToHelper::Answer { ask: 4, answer: Answer::Refused { message: "No.".into() } }, &mut paper);
        assert_eq!(paper.notices, vec!["game.exe was closed.".to_string()]);
        assert_eq!(paper.answers.len(), 1);
        assert!(core.on_message(ToHelper::Quit, &mut paper).quit);
    }

    #[test]
    fn the_outbox_numbers_each_way_out_and_hides_what_was_typed() {
        let (tx, rx) = mpsc::channel();
        let mut outbox = Outbox::new(tx);
        assert_eq!(outbox.ask_for_time(20, "Project"), 1);
        assert_eq!(outbox.try_code(Secret::new("13420715".into())), 2);
        assert_eq!(outbox.try_pin(Secret::new("482915".into()), PinAction::AddMinutes { minutes: 10 }), 3);
        let sent: Vec<FromHelper> = rx.try_iter().collect();
        assert_eq!(sent.len(), 3);
        assert!(!format!("{sent:?}").contains("482915") && !format!("{sent:?}").contains("1342"));
        assert_eq!(report(10, Some(77), 77, false), FromHelper::Report { idle_ms: 10, front_pid: None, full_screen: false });
        assert_eq!(report(10, Some(78), 77, true), FromHelper::Report { idle_ms: 10, front_pid: Some(78), full_screen: true });
        assert_eq!(hello(), FromHelper::Hello { protocol: PROTOCOL_VERSION });
    }
}
