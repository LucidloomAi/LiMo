//! The helper's side of the channel, with no Windows code: what it does with
//! each message from the service, and the [`Display`] that draws.
//!
//! The helper runs in a signed in session, started by the service. It
//! reports what only a program inside the session can see (how long since
//! the last input, the window in front, full screen, whether a program of
//! the session plays sound, whether a program keeps the screen awake), and
//! shows what the service orders through a [`Display`]. It decides nothing.
//!
//! **Sound: whether, never what.** The helper learns only whether sound is
//! coming out of a program in its session. It reads each audio session's
//! loudness meter, a number Windows keeps from 0.0 for silence to 1.0, and
//! compares it with zero ([`audio_session_plays`]). No sound is captured,
//! recorded, analysed or kept, and nothing about which program plays it, or
//! what it is, is sent to the service.

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

/// What draws, in the helper. It has two implementations in the Windows
/// modules: the drawn display (`win/drawn.rs`), the lock screen and time
/// left display drawn to the agreed design, which the helper uses; and the
/// plain display (`win/display.rs`), which takes its place at once if the
/// drawn one cannot start or fails ([`Chooser`]). Each must do exactly this:
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
///   while, beside the time left.
/// - **[`Display::answer`]** shows the answer to a way out it sent.
/// - **[`Display::pump`]**, called about four times a second on the same
///   thread, keeps the lock on top, counts down and lets notices fade.
/// - **[`Display::input`]**, called on the same thread when the display's
///   own windows or keyboard hook say the child did something, takes it.
///
/// It never shows or keeps a window title, the name of a document or a web
/// address: the service sends none. The only program name it shows is the
/// one a notice carries, of a program LiMo closed: the description in the
/// program's own file, or else that file's name.
pub trait Display {
    /// Puts up what the service orders now, and says what is on the screen.
    fn show(&mut self, now: &ShowNow<'_>) -> Shown;
    /// Shows that a program was closed, and why; `program` is its name, when
    /// the notice is about one program.
    fn notice(&mut self, text: &str, program: Option<&str>);
    /// Shows the answer to a way out, by the number the [`Outbox`] gave it.
    fn answer(&mut self, ask: u32, answer: &Answer);
    /// Keeps what is shown on top and up to date.
    fn pump(&mut self);
    /// Takes the clicks and keys that came since the last call. A display
    /// with nothing to take does nothing.
    fn input(&mut self) {}
}

/// A display that can find it can no longer be trusted to show what it is
/// told: the drawn one, whose windows, pictures or keyboard hook may fail.
pub trait MayFail {
    /// Why it failed, once it has; None while it works.
    fn failure(&self) -> Option<String>;
}

/// The display the helper uses: the drawn one, and in its place, at once,
/// the plain one, if the drawn one cannot start, or fails, or does not
/// cover every monitor with a lock it was ordered to show. A lock must
/// always show. The service is told, for its log, once.
///
/// When it falls back, the plain display is made and shown what was last
/// ordered before the drawn one is dropped, so a lock is never off the
/// screen between the two.
pub struct Chooser<D, P, F> {
    drawn: Option<D>,
    plain: Option<P>,
    make_plain: F,
    to_service: Sender<FromHelper>,
    last: Option<(u64, Option<String>, Show)>,
}

impl<D, P, F> std::fmt::Debug for Chooser<D, P, F> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Chooser").field("drawn", &self.drawn.is_some()).field("plain", &self.plain.is_some()).finish()
    }
}

/// Why a lock the drawn display was ordered to show is taken over by the
/// plain one, when it gave no reason of its own.
pub const LOCK_NOT_COVERED: &str = "the drawn lock did not cover every monitor";

impl<D, P, F> Chooser<D, P, F>
where
    D: Display + MayFail,
    P: Display,
    F: FnMut(Outbox) -> P,
{
    /// The drawn display, as it was made; the plain one at once if it could
    /// not be.
    pub fn drawn(made: Result<D, String>, to_service: Sender<FromHelper>, make_plain: F) -> Chooser<D, P, F> {
        let mut chooser = Chooser { drawn: None, plain: None, make_plain, to_service, last: None };
        match made {
            Ok(drawn) => chooser.drawn = Some(drawn),
            Err(why) => {
                chooser.fall_back(&format!("the drawn display could not start: {why}"));
            }
        }
        chooser
    }

    /// The plain display from the start, for this reason.
    pub fn plain(why: &str, to_service: Sender<FromHelper>, make_plain: F) -> Chooser<D, P, F> {
        let mut chooser = Chooser { drawn: None, plain: None, make_plain, to_service, last: None };
        chooser.fall_back(why);
        chooser
    }

    /// True while the drawn display is in use.
    pub fn is_drawn(&self) -> bool {
        self.drawn.is_some()
    }

    /// The plain display takes over: made, shown what was last ordered, and
    /// only then is the drawn one dropped. The service is told why. What the
    /// plain display says it shows, when it was shown something.
    fn fall_back(&mut self, why: &str) -> Option<Shown> {
        if self.plain.is_some() {
            return None;
        }
        let mut plain = (self.make_plain)(Outbox::new(self.to_service.clone()));
        let shown = self
            .last
            .as_ref()
            .map(|(seq, child_name, show)| plain.show(&ShowNow { seq: *seq, child_name: child_name.as_deref(), show }));
        self.plain = Some(plain);
        self.drawn = None;
        let _ = self.to_service.send(FromHelper::ShowsPlain { why: why.to_string() });
        shown
    }

    /// Falls back when the drawn display says it failed.
    fn check(&mut self) {
        if let Some(why) = self.drawn.as_ref().and_then(MayFail::failure) {
            self.fall_back(&why);
        }
    }

    /// Falls back while a show is being put up, and says what the plain
    /// display shows of it.
    fn fall_back_now(&mut self, why: &str, now: &ShowNow<'_>) -> Shown {
        match self.fall_back(why) {
            Some(shown) => shown,
            None => self.plain.as_mut().map(|plain| plain.show(now)).unwrap_or_default(),
        }
    }
}

impl<D, P, F> Display for Chooser<D, P, F>
where
    D: Display + MayFail,
    P: Display,
    F: FnMut(Outbox) -> P,
{
    fn show(&mut self, now: &ShowNow<'_>) -> Shown {
        self.last = Some((now.seq, now.child_name.map(str::to_string), now.show.clone()));
        if let Some(drawn) = self.drawn.as_mut() {
            let shown = drawn.show(now);
            let uncovered = matches!(now.show, Show::Locked { .. }) && !shown.lock_on_screen;
            return match drawn.failure() {
                Some(why) => self.fall_back_now(&why, now),
                None if uncovered => self.fall_back_now(LOCK_NOT_COVERED, now),
                None => shown,
            };
        }
        match self.plain.as_mut() {
            Some(plain) => plain.show(now),
            None => Shown::default(),
        }
    }

    fn notice(&mut self, text: &str, program: Option<&str>) {
        if let Some(drawn) = self.drawn.as_mut() {
            drawn.notice(text, program);
            self.check();
        }
        if self.drawn.is_none()
            && let Some(plain) = self.plain.as_mut()
        {
            plain.notice(text, program);
        }
    }

    fn answer(&mut self, ask: u32, answer: &Answer) {
        match (self.drawn.as_mut(), self.plain.as_mut()) {
            (Some(drawn), _) => {
                drawn.answer(ask, answer);
                self.check();
            }
            (None, Some(plain)) => plain.answer(ask, answer),
            (None, None) => {}
        }
    }

    fn pump(&mut self) {
        match (self.drawn.as_mut(), self.plain.as_mut()) {
            (Some(drawn), _) => {
                drawn.pump();
                self.check();
            }
            (None, Some(plain)) => plain.pump(),
            (None, None) => {}
        }
    }

    fn input(&mut self) {
        match (self.drawn.as_mut(), self.plain.as_mut()) {
            (Some(drawn), _) => {
                drawn.input();
                self.check();
            }
            (None, Some(plain)) => plain.input(),
            (None, None) => {}
        }
    }
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

/// A report of what the helper sees in its session: `playing_sound` when a
/// program of the session plays sound, `screen_kept_awake` when a program
/// asks Windows to keep the screen awake.
pub fn report(
    idle_ms: u64,
    front_pid: Option<u32>,
    own_pid: u32,
    full_screen: bool,
    playing_sound: bool,
    screen_kept_awake: bool,
) -> FromHelper {
    // The service also knows the helper's own process; saying nothing in
    // front for it here saves it the look.
    let front_pid = front_pid.filter(|pid| *pid != own_pid && *pid != 0);
    FromHelper::Report { idle_ms, front_pid, full_screen, playing_sound, screen_kept_awake }
}

/// One audio session on a sound device, as the helper read it: only what
/// decides whether it counts. Each is `None` when Windows would not say.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct AudioSessionSeen {
    /// True when at least one of its streams is running.
    pub active: Option<bool>,
    /// True when it is Windows' own system sounds session.
    pub system_sounds: Option<bool>,
    /// The Windows session of the process that made it.
    pub windows_session: Option<u32>,
    /// Its loudness meter: 0.0 for silence, up to 1.0.
    pub peak: Option<f32>,
}

/// True when an audio session counts as this session playing sound: it is
/// active, it is not the system sounds session, its process is in the
/// helper's own Windows session (`own_session`), and its loudness meter is
/// above zero. A stream that is open but silent, such as a paused video,
/// has a meter of zero and does not count. Anything Windows would not say
/// counts as no.
///
/// The system sounds session never counts. It carries Windows' own short
/// sounds (a notification's chime, an error's ding) for every program at
/// once, not something a child plays, and it belongs to no one program of
/// the session.
pub fn audio_session_plays(seen: &AudioSessionSeen, own_session: u32) -> bool {
    seen.active == Some(true)
        && seen.system_sounds == Some(false)
        && seen.windows_session == Some(own_session)
        && seen.peak.is_some_and(|peak| peak > 0.0)
}

/// How long sound heard counts as still playing, in milliseconds. The
/// helper reads the meters once a second, and a meter says the loudest
/// moment of only the last few milliseconds: a pause between two sentences
/// of a video can read as silence. Within this time of the last sound heard,
/// the session still plays sound; longer, and it does not, so a paused
/// video stops counting this long after its sound stopped. It also covers
/// a read that is late or stuck: what was heard goes stale here too.
pub const SOUND_HELD_MS: u64 = 3_000;

/// True when sound was last heard `heard_ms` and that is no more than
/// [`SOUND_HELD_MS`] before `now_ms`, both counted from the same moment.
/// `None`: never heard.
pub fn heard_lately(heard_ms: Option<u64>, now_ms: u64) -> bool {
    heard_ms.is_some_and(|heard| heard <= now_ms && now_ms - heard <= SOUND_HELD_MS)
}

/// The bit of Windows' execution state that says a program asks for the
/// display to stay on (`ES_DISPLAY_REQUIRED`).
pub const DISPLAY_REQUIRED: u32 = 0x0000_0002;

/// True when the whole PC's execution state, as Windows gives it, says a
/// program keeps the screen awake.
pub fn screen_kept_awake(execution_state: u32) -> bool {
    execution_state & DISPLAY_REQUIRED != 0
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
            ToHelper::Notice { text, program } => display.notice(&text, program.as_deref()),
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
        notices: Vec<(String, Option<String>)>,
        answers: Vec<(u32, Answer)>,
        lock_draws: bool,
        pumps: u32,
        inputs: u32,
    }

    impl Display for Paper {
        fn show(&mut self, now: &ShowNow<'_>) -> Shown {
            self.shows.push((now.seq, now.child_name.map(String::from), now.show.clone()));
            Shown {
                lock_on_screen: self.lock_draws && matches!(now.show, Show::Locked { .. }),
                warning_on_screen: matches!(now.show, Show::TimeLeft { warning: Some(_), .. }),
            }
        }
        fn notice(&mut self, text: &str, program: Option<&str>) {
            self.notices.push((text.to_string(), program.map(String::from)));
        }
        fn answer(&mut self, ask: u32, answer: &Answer) {
            self.answers.push((ask, answer.clone()));
        }
        fn pump(&mut self) {
            self.pumps += 1;
        }
        fn input(&mut self) {
            self.inputs += 1;
        }
    }

    /// A drawn display that can be made to fail, and that records into a
    /// log shared with the test.
    struct Drawn {
        log: std::rc::Rc<std::cell::RefCell<Vec<String>>>,
        covers: bool,
        fail_on_pump: bool,
        failed: Option<String>,
    }

    impl Display for Drawn {
        fn show(&mut self, now: &ShowNow<'_>) -> Shown {
            self.log.borrow_mut().push(format!("drawn show {}", now.seq));
            Shown { lock_on_screen: self.covers && matches!(now.show, Show::Locked { .. }), warning_on_screen: false }
        }
        fn notice(&mut self, text: &str, _program: Option<&str>) {
            self.log.borrow_mut().push(format!("drawn notice {text}"));
        }
        fn answer(&mut self, ask: u32, _answer: &Answer) {
            self.log.borrow_mut().push(format!("drawn answer {ask}"));
        }
        fn pump(&mut self) {
            if self.fail_on_pump {
                self.failed = Some("a picture could not be put on the screen".into());
            }
        }
    }

    impl MayFail for Drawn {
        fn failure(&self) -> Option<String> {
            self.failed.clone()
        }
    }

    impl Drop for Drawn {
        fn drop(&mut self) {
            self.log.borrow_mut().push("drawn dropped".into());
        }
    }

    type Log = std::rc::Rc<std::cell::RefCell<Vec<String>>>;

    /// A plain display that records into the shared log.
    struct Plain(Log);

    impl Display for Plain {
        fn show(&mut self, now: &ShowNow<'_>) -> Shown {
            self.0.borrow_mut().push(format!("plain show {}", now.seq));
            Shown { lock_on_screen: matches!(now.show, Show::Locked { .. }), warning_on_screen: false }
        }
        fn notice(&mut self, text: &str, _program: Option<&str>) {
            self.0.borrow_mut().push(format!("plain notice {text}"));
        }
        fn answer(&mut self, _ask: u32, _answer: &Answer) {}
        fn pump(&mut self) {}
    }

    fn told(rx: &mpsc::Receiver<FromHelper>) -> Vec<String> {
        rx.try_iter()
            .filter_map(|m| match m {
                FromHelper::ShowsPlain { why } => Some(why),
                _ => None,
            })
            .collect()
    }

    fn log() -> Log {
        std::rc::Rc::new(std::cell::RefCell::new(Vec::new()))
    }

    fn drawn(log: &Log) -> Drawn {
        drawn_with(log, true, false)
    }

    fn drawn_with(log: &Log, covers: bool, fail_on_pump: bool) -> Drawn {
        Drawn { log: log.clone(), covers, fail_on_pump, failed: None }
    }

    fn show_lock(display: &mut dyn Display, seq: u64) -> Shown {
        let show = lock();
        display.show(&ShowNow { seq, child_name: Some("Asha"), show: &show })
    }

    #[test]
    fn a_healthy_drawn_display_is_used_and_nothing_is_told() {
        let (tx, rx) = mpsc::channel();
        let log = log();
        let plain_log = log.clone();
        let mut chooser = Chooser::drawn(Ok(drawn(&log)), tx, move |_| Plain(plain_log.clone()));
        assert!(show_lock(&mut chooser, 1).lock_on_screen);
        chooser.notice("game.exe was closed.", Some("game.exe"));
        chooser.answer(3, &Answer::Refused { message: "No.".into() });
        chooser.pump();
        chooser.input();
        assert!(chooser.is_drawn());
        assert_eq!(*log.borrow(), vec!["drawn show 1", "drawn notice game.exe was closed.", "drawn answer 3"]);
        assert!(told(&rx).is_empty());
    }

    #[test]
    fn a_drawn_display_that_cannot_start_gives_way_to_the_plain_one_at_once() {
        let (tx, rx) = mpsc::channel();
        let log = log();
        let plain_log = log.clone();
        let mut chooser: Chooser<Drawn, Plain, _> =
            Chooser::drawn(Err("its windows could not be made".into()), tx, move |_| Plain(plain_log.clone()));
        assert!(!chooser.is_drawn());
        assert!(show_lock(&mut chooser, 1).lock_on_screen);
        assert_eq!(*log.borrow(), vec!["plain show 1"]);
        assert_eq!(told(&rx), vec!["the drawn display could not start: its windows could not be made".to_string()]);
    }

    #[test]
    fn a_lock_the_drawn_display_does_not_cover_is_shown_by_the_plain_one_in_the_same_call() {
        let (tx, rx) = mpsc::channel();
        let log = log();
        let plain_log = log.clone();
        let mut chooser = Chooser::drawn(Ok(drawn_with(&log, false, false)), tx, move |_| Plain(plain_log.clone()));
        // The time left is not a lock: the drawn display keeps it.
        let left = Show::TimeLeft { seconds_left: Some(600), counting: true, warning: None, unlocked_until: None };
        chooser.show(&ShowNow { seq: 1, child_name: None, show: &left });
        assert!(chooser.is_drawn());
        // A lock it does not cover: the plain one has it before the call
        // returns, and is made before the drawn one is dropped.
        let shown = show_lock(&mut chooser, 2);
        assert!(shown.lock_on_screen);
        assert!(!chooser.is_drawn());
        assert_eq!(*log.borrow(), vec!["drawn show 1", "drawn show 2", "plain show 2", "drawn dropped"]);
        assert_eq!(told(&rx), vec![LOCK_NOT_COVERED.to_string()]);
        // Told once, however many shows follow.
        show_lock(&mut chooser, 3);
        chooser.notice("game.exe was closed.", None);
        assert!(told(&rx).is_empty());
        assert_eq!(log.borrow().last().map(String::as_str), Some("plain notice game.exe was closed."));
    }

    #[test]
    fn a_drawn_display_that_fails_while_a_lock_is_up_is_replaced_and_the_lock_shown_again() {
        let (tx, rx) = mpsc::channel();
        let log = log();
        let plain_log = log.clone();
        let mut chooser = Chooser::drawn(Ok(drawn_with(&log, true, true)), tx, move |_| Plain(plain_log.clone()));
        assert!(show_lock(&mut chooser, 4).lock_on_screen);
        chooser.pump();
        assert!(!chooser.is_drawn());
        assert_eq!(*log.borrow(), vec!["drawn show 4", "plain show 4", "drawn dropped"]);
        assert_eq!(told(&rx), vec!["a picture could not be put on the screen".to_string()]);
    }

    #[test]
    fn the_settings_can_ask_for_the_plain_display_and_the_service_is_told() {
        let (tx, rx) = mpsc::channel();
        let log = log();
        let plain_log = log.clone();
        let mut chooser: Chooser<Drawn, Plain, _> =
            Chooser::plain("the settings file asks for the plain display", tx, move |_| Plain(plain_log.clone()));
        assert!(!chooser.is_drawn());
        show_lock(&mut chooser, 1);
        assert_eq!(*log.borrow(), vec!["plain show 1"]);
        assert_eq!(told(&rx), vec!["the settings file asks for the plain display".to_string()]);
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
        core.on_message(
            ToHelper::Notice { text: "game.exe is blocked by your family's rules, so LiMo closed it.".into(), program: Some("game.exe".into()) },
            &mut paper,
        );
        core.on_message(ToHelper::Answer { ask: 4, answer: Answer::Refused { message: "No.".into() } }, &mut paper);
        assert_eq!(
            paper.notices,
            vec![("game.exe is blocked by your family's rules, so LiMo closed it.".to_string(), Some("game.exe".to_string()))]
        );
        paper.pump();
        paper.input();
        assert_eq!((paper.pumps, paper.inputs), (1, 1));
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
        assert_eq!(
            report(10, Some(77), 77, false, false, false),
            FromHelper::Report { idle_ms: 10, front_pid: None, full_screen: false, playing_sound: false, screen_kept_awake: false }
        );
        assert_eq!(
            report(10, Some(78), 77, true, true, false),
            FromHelper::Report { idle_ms: 10, front_pid: Some(78), full_screen: true, playing_sound: true, screen_kept_awake: false }
        );
        assert_eq!(
            report(10, Some(0), 77, false, false, true),
            FromHelper::Report { idle_ms: 10, front_pid: None, full_screen: false, playing_sound: false, screen_kept_awake: true }
        );
        assert_eq!(hello(), FromHelper::Hello { protocol: PROTOCOL_VERSION });
    }

    #[test]
    fn an_audio_session_counts_only_when_it_is_heard_in_this_session() {
        let own = 2;
        let playing = AudioSessionSeen { active: Some(true), system_sounds: Some(false), windows_session: Some(own), peak: Some(0.31) };
        assert!(audio_session_plays(&playing, own));
        // The quietest sound still counts: the meter is compared with zero.
        assert!(audio_session_plays(&AudioSessionSeen { peak: Some(0.000_01), ..playing }, own));
        // A paused video: the stream is open and running, and silent.
        assert!(!audio_session_plays(&AudioSessionSeen { peak: Some(0.0), ..playing }, own));
        // A stream stopped, or gone.
        assert!(!audio_session_plays(&AudioSessionSeen { active: Some(false), ..playing }, own));
        // Windows' own sounds, whatever their meter says.
        assert!(!audio_session_plays(&AudioSessionSeen { system_sounds: Some(true), ..playing }, own));
        // A program of another Windows session: another account's, or
        // Windows' own in session 0.
        assert!(!audio_session_plays(&AudioSessionSeen { windows_session: Some(1), ..playing }, own));
        assert!(!audio_session_plays(&AudioSessionSeen { windows_session: Some(0), ..playing }, own));
        // Anything Windows would not say counts as no.
        for unknown in [
            AudioSessionSeen { active: None, ..playing },
            AudioSessionSeen { system_sounds: None, ..playing },
            AudioSessionSeen { windows_session: None, ..playing },
            AudioSessionSeen { peak: None, ..playing },
            AudioSessionSeen { peak: Some(f32::NAN), ..playing },
            AudioSessionSeen { peak: Some(-0.5), ..playing },
            AudioSessionSeen::default(),
        ] {
            assert!(!audio_session_plays(&unknown, own), "{unknown:?}");
        }
    }

    #[test]
    fn sound_heard_counts_for_a_few_seconds_and_then_goes_stale() {
        assert!(!heard_lately(None, 0));
        assert!(!heard_lately(None, 60_000));
        assert!(heard_lately(Some(10_000), 10_000));
        // A moment of silence between two sentences is bridged.
        assert!(heard_lately(Some(10_000), 10_000 + SOUND_HELD_MS));
        // Longer, and it no longer plays: a paused video, or a read that
        // stopped coming.
        assert!(!heard_lately(Some(10_000), 10_001 + SOUND_HELD_MS));
        assert!(!heard_lately(Some(10_000), 3_600_000));
        // A time from after now is not believed.
        assert!(!heard_lately(Some(10_001), 10_000));
        assert_eq!(SOUND_HELD_MS, 3_000);
    }

    #[test]
    fn the_screen_is_kept_awake_only_when_the_display_is_required() {
        // ES_SYSTEM_REQUIRED, ES_DISPLAY_REQUIRED, ES_USER_PRESENT and
        // ES_AWAYMODE_REQUIRED, as Windows numbers them.
        assert!(!screen_kept_awake(0));
        assert!(!screen_kept_awake(0x1));
        assert!(screen_kept_awake(0x2));
        assert!(screen_kept_awake(0x1 | 0x2));
        assert!(!screen_kept_awake(0x4));
        assert!(!screen_kept_awake(0x40));
        assert!(screen_kept_awake(0x8000_0000 | 0x2));
        assert!(!screen_kept_awake(0x8000_0000 | 0x1));
    }
}
