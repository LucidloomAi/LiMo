//! The lock screen: a small machine with a picture.
//!
//! It is told what to show (the engine's [`LockScreen`], the child's name,
//! the time and the PC's clock), keeps what the child is doing on it (the
//! way out that is open, the minutes picked, the reason typed, the digits
//! typed, the PIN's choice, the message under a field), takes clicks, keys,
//! the passing of time and the answers to its ways out, and gives back the
//! way out to send, if any. It draws itself at any size, and the same
//! layout says what is under a point.

mod layout;

use std::fmt;

use limo_agent::{LockScreen, LockWhy};
use limo_core::{
    FREE_WRONG_GUESSES, MAX_ASK_MINUTES, MAX_PIN_DIGITS, MAX_REASON_CHARS, MIN_TYPED_PIN_DIGITS, PinAction,
    RequestAnswer, RequestView, TimeRequest, TryOutcome,
};
use zeroize::{Zeroize, Zeroizing};

use crate::icons::Icon;
use crate::paint::{Painter, paper};
use crate::picture::{Picture, Size};
use crate::scene::Scene;
use crate::theme::{CORAL, INK, INK_SOFT, Rgba, TEAL};
use crate::words::{self, PcClock};

use layout::{Digits, RequestLine, Right, Row, View};
pub use layout::lock_scale;

/// The digits of a one time code.
pub const CODE_DIGITS: usize = 8;
/// The longest a way out waits for its answer, in seconds, before the
/// button can be used again.
pub const ANSWER_WAIT_SECS: i64 = 30;
/// How long a right code or PIN is said on the screen, in seconds, if the
/// lock is not taken down first.
pub const OPENED_SECS: i64 = 20;
/// The minutes a child can ask for, as the chips offer them.
pub const ASK_CHOICES: [u32; 4] = [15, 30, 45, 60];

/// A way out of the lock.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Way {
    /// Ask a guardian for more time.
    Ask,
    /// Type a one time code a guardian reads out.
    Code,
    /// A guardian types the PIN.
    Pin,
}

/// What is open on the right half of the card.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Panel {
    /// The ways out, or the words when there are none.
    Ways,
    /// Asking for more time.
    Ask,
    /// Typing a one time code.
    Code,
    /// A guardian typing the PIN.
    Pin,
}

/// What a right PIN is to give, as the chips offer it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PinChoice {
    /// Add 15 minutes.
    Add15,
    /// Add 30 minutes.
    Add30,
    /// Add an hour.
    Add60,
    /// Keep the PC open for an hour.
    OpenHour,
    /// Keep the PC open for two hours.
    OpenTwoHours,
    /// Keep the PC open for the rest of today.
    OpenToday,
}

impl PinChoice {
    /// Every choice, in the order of the chips.
    pub const ALL: [PinChoice; 6] =
        [PinChoice::Add15, PinChoice::Add30, PinChoice::Add60, PinChoice::OpenHour, PinChoice::OpenTwoHours, PinChoice::OpenToday];

    /// What the PIN is to do, from `now`, on the PC's clock.
    pub fn action(self, now: i64, pc: PcClock) -> PinAction {
        match self {
            PinChoice::Add15 => PinAction::AddMinutes { minutes: 15 },
            PinChoice::Add30 => PinAction::AddMinutes { minutes: 30 },
            PinChoice::Add60 => PinAction::AddMinutes { minutes: 60 },
            PinChoice::OpenHour => PinAction::UnlockUntil { until: now.saturating_add(3600) },
            PinChoice::OpenTwoHours => PinAction::UnlockUntil { until: now.saturating_add(7200) },
            PinChoice::OpenToday => PinAction::UnlockUntil { until: words::end_of_day(now, pc).max(now.saturating_add(60)) },
        }
    }

    /// The chip's words.
    pub(crate) fn label(self) -> &'static str {
        match self {
            PinChoice::Add15 => "15 min",
            PinChoice::Add30 => "30 min",
            PinChoice::Add60 | PinChoice::OpenHour => "1 hour",
            PinChoice::OpenTwoHours => "2 hours",
            PinChoice::OpenToday => "Today",
        }
    }
}

/// Something on the lock screen that can be clicked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Target {
    /// A row that opens a way out.
    Way(Way),
    /// Back to the ways out.
    Back,
    /// A chip of minutes to ask for.
    Minutes(u32),
    /// The field for the reason.
    Reason,
    /// Send the request.
    Send,
    /// A digit key.
    Digit(u8),
    /// The key that removes the last digit.
    Delete,
    /// Use the code typed.
    UseCode,
    /// A chip of what the PIN gives.
    Choice(PinChoice),
    /// Use the PIN typed.
    Unlock,
}

/// A key the child pressed. The helper turns the keyboard's own events into
/// these; nothing else typed goes anywhere.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    /// A character, as typed.
    Char(char),
    /// Remove the last character or digit.
    Backspace,
    /// Use what is typed.
    Enter,
    /// Back to the ways out.
    Escape,
}

/// The answer to a way out that was sent, as the service gives it.
#[derive(Debug, Clone, PartialEq)]
pub enum Answer {
    /// The request for more time was made.
    Asked {
        /// The request.
        request: TimeRequest,
    },
    /// The code or the PIN was tried.
    Tried {
        /// What became of it.
        outcome: TryOutcome,
    },
    /// It was not tried, for this reason, in plain words.
    Refused {
        /// Why.
        message: String,
    },
}

/// What happens to the lock screen.
#[derive(Debug, Clone, PartialEq)]
pub enum Input {
    /// A click at a point, in pixels of the main screen's picture as last
    /// drawn.
    Click {
        /// Pixels from the left.
        x: f32,
        /// Pixels from the top.
        y: f32,
    },
    /// A key.
    Key(Key),
    /// Time passed: it is now this, unix seconds.
    Time {
        /// Unix seconds.
        now: i64,
    },
    /// The answer to the way out that was sent.
    Answer(Answer),
}

/// A PIN or a code as it was typed, on its way to the service. Its `Debug`
/// hides it, and it is wiped from memory when dropped.
pub struct Typed(Zeroizing<String>);

impl Typed {
    /// What was typed, for the one call that needs it.
    pub fn expose(&self) -> &str {
        &self.0
    }

    /// What was typed, moved out without a copy, to hand straight to
    /// something that wipes it in turn (the helper's channel does).
    pub fn take(mut self) -> String {
        std::mem::take(&mut *self.0)
    }
}

impl fmt::Debug for Typed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Typed(hidden)")
    }
}

impl PartialEq for Typed {
    fn eq(&self, other: &Typed) -> bool {
        *self.0 == *other.0
    }
}

/// A way out the child chose: the helper sends it to the service.
#[derive(Debug, PartialEq)]
pub enum WayOut {
    /// Ask for more time today.
    AskForTime {
        /// The minutes asked for.
        minutes: u32,
        /// The child's reason, possibly empty, at most 200 characters.
        reason: String,
    },
    /// Try a one time code.
    TryCode {
        /// The code, 8 digits.
        code: Typed,
    },
    /// Try the guardian PIN.
    TryPin {
        /// The PIN, 4 to 8 digits.
        pin: Typed,
        /// What it is to do.
        action: PinAction,
    },
}

/// What the service last said to show.
struct Told {
    screen: LockScreen,
    child_name: Option<String>,
    pc: PcClock,
}

/// A way out sent and not yet answered.
#[derive(Debug, Clone, Copy)]
struct Pending {
    way: Way,
    since: i64,
    choice: PinChoice,
}

/// A right code or PIN, said until the lock goes.
#[derive(Debug, Clone)]
struct Opened {
    title: String,
    since: i64,
}

/// A picture drawn, with what it shows.
struct Drawn {
    size: Size,
    view: View,
    scene: Scene<Target>,
    picture: Option<Picture>,
}

/// The lock screen. See the module's documentation.
pub struct Lock {
    told: Option<Told>,
    now: i64,
    panel: Panel,
    minutes: u32,
    reason: String,
    typed: Zeroizing<String>,
    choice: PinChoice,
    message: Option<String>,
    pending: Option<Pending>,
    asked: Option<TimeRequest>,
    wait_until: Option<i64>,
    opened: Option<Opened>,
    size: Size,
    painter: Painter,
    /// The paper and its dots, for each size drawn lately.
    papers: Vec<Picture>,
    main: Option<Drawn>,
    /// The other screens, one for each size drawn lately.
    others: Vec<Drawn>,
    nothing: Picture,
}

/// How many sizes of the other screens, and of the paper, are kept: a PC
/// rarely has more monitors than this.
const SIZES_KEPT: usize = 6;

impl fmt::Debug for Lock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Lock")
            .field("why", &self.told.as_ref().map(|t| t.screen.why))
            .field("panel", &self.panel)
            .field("minutes", &self.minutes)
            .field("reason_chars", &self.reason.chars().count())
            .field("typed", &"hidden")
            .field("choice", &self.choice)
            .field("message", &self.message)
            .field("waiting_for_answer", &self.pending.map(|p| p.way))
            .field("wait_until", &self.wait_until)
            .field("opened", &self.opened.as_ref().map(|o| &o.title))
            .field("size", &self.size)
            .finish_non_exhaustive()
    }
}

impl Default for Lock {
    fn default() -> Lock {
        Lock::new()
    }
}

fn fresh_digits() -> Zeroizing<String> {
    Zeroizing::new(String::with_capacity(CODE_DIGITS))
}

impl Lock {
    /// A lock screen with nothing told yet. It draws the reason "LiMo
    /// cannot check the time" until it is told what to show.
    pub fn new() -> Lock {
        Lock {
            told: None,
            now: 0,
            panel: Panel::Ways,
            minutes: 30,
            reason: String::new(),
            typed: fresh_digits(),
            choice: PinChoice::Add30,
            message: None,
            pending: None,
            asked: None,
            wait_until: None,
            opened: None,
            size: Size::new(1366, 768),
            painter: Painter::new(),
            papers: Vec::new(),
            main: None,
            others: Vec::new(),
            nothing: Picture::clear(Size::new(1, 1)),
        }
    }

    /// What to show, as the service orders it: called whenever a show
    /// comes, the same one again included. It keeps what the child is doing,
    /// unless the way out that is open is no longer offered.
    pub fn show(&mut self, screen: &LockScreen, child_name: Option<&str>, now: i64, pc: PcClock) {
        self.now = now;
        // The engine's wait, when it gives one, is the one in force; a wait
        // learned from an answer only bridges the moment until it comes.
        self.wait_until = None;
        if let (Some(asked), Some(latest)) = (&self.asked, &screen.latest_request)
            && asked.id == latest.request.id
        {
            self.asked = None;
        }
        self.told = Some(Told { screen: screen.clone(), child_name: child_name.map(str::to_string), pc });
        self.close_panel_not_offered();
    }

    /// Forgets everything the child did on the screen: for when the lock is
    /// taken down. The next lock starts with the ways out.
    pub fn reset(&mut self) {
        self.wipe_typed();
        self.panel = Panel::Ways;
        self.minutes = 30;
        self.reason.clear();
        self.choice = PinChoice::Add30;
        self.message = None;
        self.pending = None;
        self.asked = None;
        self.wait_until = None;
        self.opened = None;
    }

    /// One thing that happened. Returns the way out to send, if the child
    /// chose one. At most one is ever waiting for its answer.
    pub fn input(&mut self, input: Input) -> Option<WayOut> {
        match input {
            Input::Click { x, y } => {
                let target = self.target_at(x, y)?;
                self.act(target)
            }
            Input::Key(key) => self.key(key),
            Input::Time { now } => {
                self.tick(now);
                None
            }
            Input::Answer(answer) => {
                self.answer(answer);
                None
            }
        }
    }

    /// The panel open now.
    pub fn panel(&self) -> Panel {
        self.panel
    }

    /// The message under the field, if there is one.
    pub fn message(&self) -> Option<&str> {
        self.message.as_deref()
    }

    /// The way out waiting for its answer, if one is.
    pub fn waiting_for(&self) -> Option<Way> {
        self.pending.map(|p| p.way)
    }

    /// How many digits are typed.
    pub fn typed_digits(&self) -> usize {
        self.typed.chars().count()
    }

    /// The reason typed so far.
    pub fn reason(&self) -> &str {
        &self.reason
    }

    /// The minutes picked to ask for.
    pub fn minutes(&self) -> u32 {
        self.minutes
    }

    /// What the PIN is to give.
    pub fn pin_choice(&self) -> PinChoice {
        self.choice
    }

    /// True when a right code or PIN is being said.
    pub fn opened(&self) -> bool {
        self.opened.is_some()
    }

    /// The ways out offered now, in their order on the screen.
    pub fn ways(&self) -> Vec<Way> {
        let Some(told) = &self.told else {
            return Vec::new();
        };
        let s = &told.screen;
        [(Way::Ask, s.can_ask_for_time), (Way::Code, s.can_use_code), (Way::Pin, s.can_use_pin)]
            .into_iter()
            .filter(|(_, on)| *on)
            .map(|(w, _)| w)
            .collect()
    }

    /// The wait after wrong tries, while it is in force.
    fn wait(&self) -> Option<i64> {
        let engine = self.told.as_ref().and_then(|t| t.screen.try_wait_until);
        engine.into_iter().chain(self.wait_until).max().filter(|&w| w > self.now)
    }

    /// The newest request, as the screen should say it.
    fn request(&self) -> Option<RequestView> {
        let latest = self.told.as_ref().and_then(|t| t.screen.latest_request.clone());
        match (&self.asked, latest) {
            (Some(asked), latest) if latest.as_ref().is_none_or(|l| l.request.id != asked.id) => Some(RequestView {
                request: asked.clone(),
                answer: RequestAnswer::Waiting,
                answered_at: None,
                reached_master: false,
            }),
            (_, latest) => latest,
        }
    }

    fn request_waiting(&self) -> bool {
        self.request().is_some_and(|r| r.answer == RequestAnswer::Waiting)
    }

    fn offered(&self, way: Way) -> bool {
        self.ways().contains(&way)
    }

    /// Closes the open panel when its way out is no longer offered, or must
    /// wait.
    fn close_panel_not_offered(&mut self) {
        let keep = match self.panel {
            Panel::Ways => true,
            Panel::Ask => self.offered(Way::Ask) && !self.request_waiting(),
            Panel::Code => self.offered(Way::Code) && self.wait().is_none(),
            Panel::Pin => self.offered(Way::Pin) && self.wait().is_none(),
        };
        if !keep || self.opened.is_some() {
            self.back();
        }
    }

    fn wipe_typed(&mut self) {
        if !self.typed.is_empty() {
            self.typed.zeroize();
            // What was drawn of it goes too.
            self.forget_main();
        }
    }

    /// Drops the main picture, wiping its pixels.
    fn forget_main(&mut self) {
        if let Some(mut drawn) = self.main.take()
            && let Some(picture) = drawn.picture.as_mut()
        {
            picture.wipe();
        }
    }

    fn back(&mut self) {
        self.wipe_typed();
        self.panel = Panel::Ways;
        self.message = None;
    }

    fn open(&mut self, way: Way) {
        if !self.offered(way) || self.opened.is_some() {
            return;
        }
        match way {
            Way::Ask if self.request_waiting() => return,
            Way::Code | Way::Pin if self.wait().is_some() => return,
            _ => {}
        }
        self.wipe_typed();
        self.message = None;
        self.panel = match way {
            Way::Ask => Panel::Ask,
            Way::Code => Panel::Code,
            Way::Pin => Panel::Pin,
        };
    }

    fn act(&mut self, target: Target) -> Option<WayOut> {
        match target {
            Target::Way(way) if self.panel == Panel::Ways => self.open(way),
            Target::Back => self.back(),
            Target::Minutes(m) if self.panel == Panel::Ask && ASK_CHOICES.contains(&m) => self.minutes = m,
            Target::Send if self.panel == Panel::Ask => return self.send_request(),
            Target::Digit(d) if matches!(self.panel, Panel::Code | Panel::Pin) => self.digit(d),
            Target::Delete if matches!(self.panel, Panel::Code | Panel::Pin) => self.delete(),
            Target::UseCode if self.panel == Panel::Code => return self.send_code(),
            Target::Choice(c) if self.panel == Panel::Pin => self.choice = c,
            Target::Unlock if self.panel == Panel::Pin => return self.send_pin(),
            _ => {}
        }
        None
    }

    fn key(&mut self, key: Key) -> Option<WayOut> {
        match (self.panel, key) {
            (Panel::Ways, _) => None,
            (_, Key::Escape) => {
                self.back();
                None
            }
            (Panel::Ask, Key::Char(c)) => {
                if !c.is_control() && self.reason.chars().count() < MAX_REASON_CHARS as usize {
                    self.reason.push(c);
                }
                None
            }
            (Panel::Ask, Key::Backspace) => {
                self.reason.pop();
                None
            }
            (Panel::Ask, Key::Enter) => self.send_request(),
            (_, Key::Char(c)) => {
                if let Some(d) = c.to_digit(10).filter(|_| c.is_ascii_digit()) {
                    self.digit(d as u8);
                }
                None
            }
            (_, Key::Backspace) => {
                self.delete();
                None
            }
            (Panel::Code, Key::Enter) => self.send_code(),
            (Panel::Pin, Key::Enter) => self.send_pin(),
        }
    }

    fn digit(&mut self, d: u8) {
        let most = if self.panel == Panel::Code { CODE_DIGITS } else { MAX_PIN_DIGITS as usize };
        if d <= 9 && self.typed.len() < most {
            self.typed.push(char::from(b'0' + d));
            self.message = None;
        }
    }

    fn delete(&mut self) {
        if self.typed.pop().is_some() {
            // The removed digit was in the drawn picture.
            self.forget_main();
        }
        self.message = None;
    }

    fn send_request(&mut self) -> Option<WayOut> {
        if self.pending.is_some() || !self.offered(Way::Ask) {
            return None;
        }
        let minutes = self.minutes.clamp(1, MAX_ASK_MINUTES);
        let reason: String = self.reason.trim().chars().take(MAX_REASON_CHARS as usize).collect();
        self.pending = Some(Pending { way: Way::Ask, since: self.now, choice: self.choice });
        self.message = None;
        Some(WayOut::AskForTime { minutes, reason })
    }

    fn send_code(&mut self) -> Option<WayOut> {
        if self.pending.is_some() || !self.offered(Way::Code) {
            return None;
        }
        if self.typed.len() < CODE_DIGITS {
            self.message = Some("A code has 8 digits.".to_string());
            return None;
        }
        let code = Typed(std::mem::replace(&mut self.typed, fresh_digits()));
        self.forget_main();
        self.pending = Some(Pending { way: Way::Code, since: self.now, choice: self.choice });
        self.message = None;
        Some(WayOut::TryCode { code })
    }

    fn send_pin(&mut self) -> Option<WayOut> {
        if self.pending.is_some() || !self.offered(Way::Pin) {
            return None;
        }
        if self.typed.len() < MIN_TYPED_PIN_DIGITS as usize {
            self.message = Some("A PIN has 6 to 8 digits.".to_string());
            return None;
        }
        let pc = self.pc();
        let action = self.choice.action(self.now, pc);
        let pin = Typed(std::mem::replace(&mut self.typed, fresh_digits()));
        self.forget_main();
        self.pending = Some(Pending { way: Way::Pin, since: self.now, choice: self.choice });
        self.message = None;
        Some(WayOut::TryPin { pin, action })
    }

    fn pc(&self) -> PcClock {
        self.told.as_ref().map(|t| t.pc).unwrap_or_default()
    }

    fn tick(&mut self, now: i64) {
        self.now = now;
        if let Some(p) = self.pending
            && now.saturating_sub(p.since) >= ANSWER_WAIT_SECS
        {
            self.pending = None;
            self.message = Some("LiMo did not answer. Try again.".to_string());
        }
        if let Some(o) = &self.opened
            && now.saturating_sub(o.since) >= OPENED_SECS
        {
            self.opened = None;
        }
        self.close_panel_not_offered();
    }

    fn answer(&mut self, answer: Answer) {
        let Some(pending) = self.pending.take() else {
            // Nothing was waiting for it.
            return;
        };
        let what = if pending.way == Way::Code { "code" } else { "PIN" };
        match answer {
            Answer::Asked { request } => {
                self.asked = Some(request);
                self.reason.clear();
                self.minutes = 30;
                self.back();
            }
            Answer::Refused { message } => {
                self.message = Some(message);
            }
            Answer::Tried { outcome } => match outcome {
                TryOutcome::Added { added } => {
                    let minutes = u32::try_from(added.secs.div_ceil(60)).unwrap_or(u32::MAX);
                    self.open_with(format!("{} added.", capitalised(&words::long_minutes(minutes))));
                }
                TryOutcome::Unlocked { until } => {
                    let pc = self.pc();
                    let title = match pending.choice {
                        PinChoice::OpenHour if pending.way == Way::Pin => "Open for 1 hour.".to_string(),
                        PinChoice::OpenTwoHours if pending.way == Way::Pin => "Open for 2 hours.".to_string(),
                        PinChoice::OpenToday if pending.way == Way::Pin => "Open for the rest of today.".to_string(),
                        _ => format!("Open until {}.", words::until_in_sentence(self.now, until, pc)),
                    };
                    self.open_with(title);
                }
                TryOutcome::ReleaseConfirmed => self.open_with("That was right.".to_string()),
                TryOutcome::Wrong { wrong_guesses, wait_until } => {
                    self.wipe_typed();
                    match wait_until.filter(|&w| w > self.now) {
                        Some(w) => {
                            self.wait_until = Some(w);
                            self.back();
                        }
                        None => {
                            let left = FREE_WRONG_GUESSES.saturating_sub(wrong_guesses);
                            self.message = Some(match left {
                                0 => format!("That {what} is not right."),
                                1 => format!("That {what} is not right. 1 more try before a wait."),
                                n => format!("That {what} is not right. {n} more tries before a wait."),
                            });
                        }
                    }
                }
                TryOutcome::MustWait { until } => {
                    self.wipe_typed();
                    self.wait_until = Some(until);
                    self.back();
                }
                TryOutcome::Malformed => {
                    self.message = Some(
                        if pending.way == Way::Code { "A code has 8 digits." } else { "A PIN has 6 to 8 digits." }.to_string(),
                    );
                }
                TryOutcome::AlreadyUsed => self.message = Some("That code was already used today.".to_string()),
                TryOutcome::NotSet => {
                    self.message = Some(
                        if pending.way == Way::Code {
                            "Codes are not set up on this PC yet."
                        } else {
                            "A guardian PIN is not set up yet."
                        }
                        .to_string(),
                    );
                }
            },
        }
        self.close_panel_not_offered();
    }

    fn open_with(&mut self, title: String) {
        self.wipe_typed();
        self.opened = Some(Opened { title, since: self.now });
        self.back();
    }

    /// What is under a point, in pixels of the main screen's picture as
    /// last drawn (or of 1366 by 768 before the first draw).
    pub fn target_at(&mut self, x: f32, y: f32) -> Option<Target> {
        if !(x.is_finite() && y.is_finite()) {
            return None;
        }
        let k = lock_scale(self.size);
        let size = self.size;
        self.lay_out(size, false);
        let drawn = self.main.as_ref()?;
        drawn.scene.hit(x / k, y / k)
    }

    /// Every piece of text that does not fit the room it was given on the
    /// main screen and on another screen at this size, in words; empty when
    /// all fits. For tests, here and in a helper.
    pub fn overflows(&mut self, size: Size) -> Vec<String> {
        let size = size.kept();
        let mut out = Vec::new();
        for other in [false, true] {
            self.lay_out(size, other);
            let drawn = if other { self.others.iter().find(|d| d.size == size) } else { self.main.as_ref() };
            if let Some(d) = drawn {
                for fit in d.scene.fits.iter().filter(|f| !f.room.holds(&f.used)) {
                    out.push(format!("{}: {:?} is outside {:?}", fit.what, fit.used, fit.room));
                }
            }
        }
        out
    }

    /// The main screen at this size: the card, with everything on it.
    pub fn draw(&mut self, size: Size) -> &Picture {
        let size = size.kept();
        self.size = size;
        self.paint(size, false);
        match &self.main {
            Some(Drawn { picture: Some(p), .. }) => p,
            _ => &self.nothing,
        }
    }

    /// Any other screen attached to the PC, at this size: it only says to
    /// look at the main one.
    pub fn draw_other(&mut self, size: Size) -> &Picture {
        let size = size.kept();
        self.paint(size, true);
        match self.others.iter().find(|d| d.size == size) {
            Some(Drawn { picture: Some(p), .. }) => p,
            _ => &self.nothing,
        }
    }

    /// Lays the screen out unless the same view at the same size is laid
    /// out already.
    fn lay_out(&mut self, size: Size, other: bool) {
        let view = self.view(other);
        let current = if other { self.others.iter().find(|d| d.size == size) } else { self.main.as_ref() };
        if current.is_some_and(|d| d.size == size && d.view == view) {
            return;
        }
        let k = lock_scale(size);
        let scene = layout::lay_out(&view, &mut self.painter.text, size, k);
        let drawn = Drawn { size, view, scene, picture: None };
        if other {
            self.others.retain(|d| d.size != size);
            if self.others.len() >= SIZES_KEPT {
                self.others.remove(0);
            }
            self.others.push(drawn);
        } else {
            self.forget_main();
            self.main = Some(drawn);
        }
    }

    fn paint(&mut self, size: Size, other: bool) {
        self.lay_out(size, other);
        let k = lock_scale(size);
        let drawn = if other { self.others.iter_mut().find(|d| d.size == size) } else { self.main.as_mut() };
        let Some(drawn) = drawn else {
            return;
        };
        if drawn.picture.is_some() {
            return;
        }
        // The paper and its dots, kept for each size: copied, then drawn on.
        let mut picture = match self.papers.iter().find(|p| p.size() == size) {
            Some(p) => p.clone(),
            None => {
                let mut blank = Picture::clear(size);
                if let Some(mut canvas) = blank.canvas() {
                    paper(&mut canvas, k);
                }
                if self.papers.len() >= SIZES_KEPT {
                    self.papers.remove(0);
                }
                self.papers.push(blank.clone());
                blank
            }
        };
        if let Some(mut canvas) = picture.canvas() {
            self.painter.paint(&mut canvas, &drawn.scene.ops, k);
        }
        drawn.picture = Some(picture);
    }

    /// Everything the picture shows, in words, for the time now.
    fn view(&self, other: bool) -> View {
        let pc = self.pc();
        let now = self.now;
        let clock = words::time_of_day(now, pc);
        let date = words::date_line(now, pc);
        if other {
            return View::other(clock, date);
        }
        let told = self.told.as_ref();
        let why = told.map(|t| t.screen.why).unwrap_or(LockWhy::CannotCheck);
        let (icon, stamp, stamp_color) = match &self.opened {
            Some(_) => (Icon::Unlock, "Open", TEAL),
            None => (
                why_icon(why),
                if why == LockWhy::WaitingForTime { "One moment" } else { "Locked" },
                if why == LockWhy::WaitingForTime { INK_SOFT } else { CORAL },
            ),
        };
        let eyebrow = match told.and_then(|t| t.child_name.as_deref()).map(str::trim).filter(|n| !n.is_empty()) {
            Some(name) => format!("{name} \u{b7} this PC"),
            None => "This PC".to_string(),
        };
        let (title, detail) = match (&self.opened, told) {
            (Some(o), _) => (o.title.clone(), "The PC opens now.".to_string()),
            (None, Some(t)) => (words::typographic(&t.screen.title), words::typographic(&t.screen.message)),
            (None, None) => (why.title().to_string(), words::typographic(why.message())),
        };
        let until = match (&self.opened, told) {
            (None, Some(t)) => match (t.screen.until, why) {
                (Some(at), _) => Some((words::opens_again(now, at, pc), false)),
                (None, LockWhy::GuardianLock) => Some(("When a guardian unlocks it".to_string(), true)),
                (None, LockWhy::HeldByAnotherDevice) => Some(("When that device is back home".to_string(), true)),
                (None, _) => None,
            },
            _ => None,
        };
        let right = self.right(why, now, pc);
        View { other: false, clock, date, icon, stamp, stamp_color, eyebrow, title, detail, until, right }
    }

    fn right(&self, why: LockWhy, now: i64, pc: PcClock) -> Right {
        if self.opened.is_some() {
            return Right::Nothing {
                icon: Icon::Check,
                bold: "That was right.".to_string(),
                text: "This screen goes away in a moment.".to_string(),
            };
        }
        let waiting = self.pending.map(|p| p.way);
        match self.panel {
            Panel::Ask => {
                return Right::Ask {
                    minutes: self.minutes,
                    reason: self.reason.clone(),
                    message: self.message.clone(),
                    waiting: waiting == Some(Way::Ask),
                };
            }
            Panel::Code => {
                return Right::Code {
                    digits: Digits::of(&self.typed),
                    message: self.message.clone(),
                    waiting: waiting == Some(Way::Code),
                };
            }
            Panel::Pin => {
                return Right::Pin {
                    count: self.typed.len(),
                    choice: self.choice,
                    message: self.message.clone(),
                    waiting: waiting == Some(Way::Pin),
                };
            }
            Panel::Ways => {}
        }
        let ways = self.ways();
        if ways.is_empty() {
            let (bold, text) = nothing_to_do(why);
            return Right::Nothing { icon: Icon::Info, bold: bold.to_string(), text: text.to_string() };
        }
        let request = self.request().map(|r| request_line(&r, pc));
        let wait = self.wait();
        let request_waiting = self.request_waiting();
        let rows = ways
            .into_iter()
            .filter(|w| !(*w == Way::Ask && request_waiting))
            .map(|way| {
                let off = way != Way::Ask && wait.is_some();
                let (icon, title, sub) = match way {
                    Way::Ask => (Icon::Hand, "Ask for more time", "A guardian sees your request in LiMo"),
                    Way::Code => (Icon::Code, "Type a one time code", "A guardian who is away can read you 8 digits"),
                    Way::Pin => (Icon::Pin, "Guardian PIN", "For a guardian who is at this PC"),
                };
                let sub = match wait {
                    Some(w) if off => format!("Wait until {}", words::until_in_sentence(now, w, pc)),
                    _ => sub.to_string(),
                };
                Row { way, icon, title, sub, off }
            })
            .collect();
        let wait_words = wait.map(|w| {
            let at = words::until_in_sentence(now, w, pc);
            if at.starts_with(|c: char| c.is_ascii_digit()) {
                format!("Too many wrong tries. The next try is at {at}.")
            } else {
                format!("Too many wrong tries. The next try is {at}.")
            }
        });
        let also = !(request.is_some() && wait_words.is_some());
        Right::Ways { request, wait: wait_words, rows, also }
    }
}

/// The picture for each reason.
fn why_icon(why: LockWhy) -> Icon {
    match why {
        LockWhy::BlockedHours => Icon::Moon,
        LockWhy::NoTimeToday => Icon::Calendar,
        LockWhy::TimeUsedUp => Icon::Hourglass,
        LockWhy::WaitingForTime => Icon::Sync,
        LockWhy::HeldByAnotherDevice => Icon::Away,
        LockWhy::GuardianLock => Icon::Lock,
        LockWhy::AccountNotLinked => Icon::Account,
        LockWhy::CannotCheck => Icon::Alert,
        LockWhy::ProgramBlocked => Icon::Block,
    }
}

/// What the right half says when no way out is offered.
fn nothing_to_do(why: LockWhy) -> (&'static str, &'static str) {
    match why {
        LockWhy::WaitingForTime => ("Nothing to do here.", "The PC opens by itself."),
        LockWhy::AccountNotLinked => {
            ("A guardian can set it up.", "In LiMo on this PC, a guardian says whose account this is, or that it has no limit.")
        }
        LockWhy::CannotCheck => ("Ask a guardian.", "A guardian can look at LiMo on this PC to see what is wrong."),
        LockWhy::GuardianLock => ("A guardian can unlock it.", "From LiMo on their phone, or in LiMo on this PC."),
        LockWhy::BlockedHours => ("Nothing to do here for now.", "The PC opens again by itself."),
        LockWhy::ProgramBlocked => ("Nothing to do here.", "This program is blocked by your family\u{2019}s rules."),
        LockWhy::TimeUsedUp | LockWhy::NoTimeToday | LockWhy::HeldByAnotherDevice => {
            ("Ask a guardian.", "A guardian can give more time from LiMo on their phone.")
        }
    }
}

/// "You asked for 30 min at 9:32 pm", and how it stands.
fn request_line(r: &RequestView, pc: PcClock) -> RequestLine {
    let text = format!(
        "You asked for {} at {}",
        words::minutes(r.request.minutes),
        words::time_of_day(r.request.asked_at, pc)
    );
    let (stamp, color): (String, Rgba) = match r.answer {
        RequestAnswer::Waiting => ("Waiting".to_string(), INK),
        RequestAnswer::Granted { minutes } => (format!("Given {}", words::minutes(minutes)), TEAL),
        RequestAnswer::Refused => ("Not this time".to_string(), CORAL),
    };
    RequestLine { text, stamp, color }
}

fn capitalised(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests;
