//! What a guardian does for a child: extra time, answers to requests, a lock
//! or an unlock, and one time codes.

use limo_core::{ExtraTime, GuardianSetting, MasterBook, OneTimeCode, RequestReply, RequestView, next_code};
use rusqlite::params;

use crate::error::HubError;
use crate::hub::Hub;
use crate::random;
use crate::store::{self, add_event};
use crate::text::{self, TextField};
use crate::types::{EventKind, ExtraGiven, WaitingRequest};

impl Hub {
    /// Gives a child `minutes` of extra time for today, 1 to 1440. It goes
    /// into the day's pool, and devices in reach get it with their next
    /// leases; ask the devices in [`Hub::wanted_reports`] to report.
    ///
    /// The hub makes the id the time is recorded under. A screen that wants
    /// a tap sent twice (say, retried after a lost answer) to give only once
    /// passes an id it made for that tap as `tap_id`; the same `tap_id` then
    /// gives once. An empty `tap_id` gives every time.
    pub fn give_extra(&mut self, child_id: &str, minutes: u32, tap_id: &str, now: i64) -> Result<ExtraGiven, HubError> {
        let id = if tap_id.trim().is_empty() {
            random::id("hub/")?
        } else {
            format!("tap/{}", text::clean(tap_id, TextField::TapId)?)
        };
        let tx = self.change()?;
        store::child(&tx, child_id)?;
        let mut book = store::load_book(&tx, child_id)?;
        let given_now = book.give_extra(&id, minutes, now).map_err(HubError::from_master)?;
        if given_now {
            store::save_book(&tx, child_id, &book, now)?;
        }
        tx.commit()?;
        Ok(ExtraGiven { id, given_now })
    }

    /// Where a child's extra time today came from: given at the master, a
    /// request granted, a code or the PIN, piece by piece, oldest first.
    pub fn extra_time_today(&self, child_id: &str, now: i64) -> Result<Vec<ExtraTime>, HubError> {
        store::child(&self.conn, child_id)?;
        let book = store::load_book(&self.conn, child_id)?;
        Ok(book.extra_time(book.today(now)))
    }

    /// Every request for more time that waits for an answer, across all
    /// children, oldest first within each child, children in the order they
    /// were added.
    pub fn waiting_requests(&self, now: i64) -> Result<Vec<WaitingRequest>, HubError> {
        let mut out = Vec::new();
        for child in store::children(&self.conn)? {
            let book = store::load_book(&self.conn, &child.child_id)?;
            let waiting = book.waiting_requests(now);
            if waiting.is_empty() {
                continue;
            }
            let devices = store::devices_of(&self.conn, &child.child_id, true)?;
            for request in waiting {
                let device_name = devices
                    .iter()
                    .find(|d| d.id == request.request.device_id)
                    .map_or_else(|| request.request.device_id.clone(), |d| d.name.clone());
                out.push(WaitingRequest {
                    child_id: child.child_id.clone(),
                    child_name: child.name.clone(),
                    device_name,
                    request,
                });
            }
        }
        Ok(out)
    }

    /// Answers a child's request: grants it with a number of minutes (which
    /// may differ from those asked for), or refuses it. A request is
    /// answered once, and only while its day lasts. The answer reaches all
    /// of the child's devices with their next grants; ask the devices in
    /// [`Hub::wanted_reports`] to report.
    pub fn answer_request(
        &mut self,
        child_id: &str,
        request_id: &str,
        reply: RequestReply,
        now: i64,
    ) -> Result<RequestView, HubError> {
        self.change_book(child_id, now, None, |book| {
            book.answer_request(request_id, reply, now).map_err(HubError::from_master)
        })
    }

    /// Locks all of a child's devices now, apart from always allowed apps,
    /// until `until`, or until lifted when None. It reaches each device with
    /// its next grant; ask the devices in [`Hub::wanted_reports`] to report.
    pub fn lock_now(&mut self, child_id: &str, until: Option<i64>, now: i64) -> Result<GuardianSetting, HubError> {
        self.change_book(child_id, now, Some(EventKind::Locked { until }), |book| {
            book.lock_now(until, now).map_err(HubError::from_master)
        })
    }

    /// Unlocks all of a child's devices until `until`, at most a day ahead:
    /// blocked hours and a lack of time are lifted, blocked apps stay
    /// blocked, and time used still counts.
    pub fn unlock_until(&mut self, child_id: &str, until: i64, now: i64) -> Result<GuardianSetting, HubError> {
        self.change_book(child_id, now, Some(EventKind::Unlocked { until }), |book| {
            book.unlock_until(until, now).map_err(HubError::from_master)
        })
    }

    /// Lifts a lock or unlock set at the master, and any unlock set with the
    /// PIN on a device before now. The rules alone decide again.
    pub fn lift(&mut self, child_id: &str, now: i64) -> Result<GuardianSetting, HubError> {
        self.change_book(child_id, now, Some(EventKind::Lifted), |book| Ok(book.lift(now)))
    }

    /// Makes the next one time code for a device, for 15, 30 or 60 minutes,
    /// valid on that device for today's home day only. The hub knows the
    /// device's code secret and records every code it makes, so two
    /// guardian screens never hand out the same one. A device takes at most
    /// ten codes a day; running out is an error to show. Read the code out
    /// as its `shown` form; its `Debug` hides the digits.
    pub fn make_code(&mut self, device_id: &str, minutes: u32, now: i64) -> Result<OneTimeCode, HubError> {
        let tx = self.change()?;
        let row = store::active_device(&tx, device_id)?;
        let book = store::load_book(&tx, &row.child_id)?;
        let secret = book
            .code_secret(device_id)
            .map_err(HubError::from_master)?
            .ok_or_else(|| HubError::NoCodeSecret { device_id: device_id.to_string() })?;
        let day = book.today(now);
        let counters: Vec<u8> = {
            let mut stmt = tx.prepare("SELECT counter FROM code_made WHERE device_id = ?1 AND day = ?2")?;
            let raw = stmt.query_map(params![device_id, day], |r| r.get::<_, i64>(0))?;
            raw.collect::<rusqlite::Result<Vec<i64>>>()?.into_iter().filter_map(|c| u8::try_from(c).ok()).collect()
        };
        let code = next_code(secret, device_id, day, minutes, counters).map_err(|problem| HubError::Code { problem })?;
        tx.execute(
            "INSERT INTO code_made (device_id, day, counter, minutes, made_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![device_id, day, code.counter, code.minutes, now],
        )?;
        tx.commit()?;
        Ok(code)
    }

    /// How many one time codes can still be made for a device today: a
    /// device takes [`limo_core::CODES_PER_DAY`] a day, counted for the home
    /// day of its child's book, whatever their minutes. Refused for a
    /// removed device.
    pub fn codes_left(&self, device_id: &str, now: i64) -> Result<u8, HubError> {
        let row = store::active_device(&self.conn, device_id)?;
        let book = store::load_book(&self.conn, &row.child_id)?;
        let day = book.today(now);
        let made: i64 =
            self.conn.query_row("SELECT count(*) FROM code_made WHERE device_id = ?1 AND day = ?2", params![device_id, day], |r| r.get(0))?;
        let made = u8::try_from(made).unwrap_or(u8::MAX);
        Ok(limo_core::CODES_PER_DAY.saturating_sub(made))
    }

    /// Loads a child's book, changes it, saves it, records an event if one
    /// is given, and commits, all in one transaction.
    fn change_book<T>(
        &mut self,
        child_id: &str,
        now: i64,
        event: Option<EventKind>,
        change: impl FnOnce(&mut MasterBook) -> Result<T, HubError>,
    ) -> Result<T, HubError> {
        let tx = self.change()?;
        store::child(&tx, child_id)?;
        let mut book = store::load_book(&tx, child_id)?;
        let out = change(&mut book)?;
        store::save_book(&tx, child_id, &book, now)?;
        if let Some(event) = event {
            add_event(&tx, now, Some(child_id), &event)?;
        }
        tx.commit()?;
        Ok(out)
    }
}
