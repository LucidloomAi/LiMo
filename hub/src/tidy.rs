//! Keeping the file small: folding old usage into daily totals, trimming the
//! books, the record of codes and the events, and the setting that says how
//! many days stay in detail.

use rusqlite::params;

use crate::error::HubError;
use crate::hub::Hub;
use crate::store;
use crate::types::TidyReport;
use crate::{BOOK_DAYS, EVENT_KEEP_DAYS, MAX_DETAIL_DAYS, MAX_EVENTS, MIN_DETAIL_DAYS};
use limo_core::SECONDS_PER_DAY;

impl Hub {
    /// How many home days of usage are kept slice by slice, today included.
    pub fn detail_days(&self) -> Result<u32, HubError> {
        let days: i64 = self.conn.query_row("SELECT detail_days FROM settings WHERE id = 1", [], |r| r.get(0))?;
        Ok(store::to_u32(days))
    }

    /// Sets how many home days of usage are kept slice by slice, from
    /// [`crate::MIN_DETAIL_DAYS`] to [`crate::MAX_DETAIL_DAYS`]. It takes
    /// effect at the next [`Hub::tidy`]. Days already folded stay folded.
    pub fn set_detail_days(&mut self, days: u32) -> Result<(), HubError> {
        if !(MIN_DETAIL_DAYS..=MAX_DETAIL_DAYS).contains(&days) {
            return Err(HubError::SettingOutOfRange { min: MIN_DETAIL_DAYS, max: MAX_DETAIL_DAYS });
        }
        let tx = self.change()?;
        tx.execute("UPDATE settings SET detail_days = ?1 WHERE id = 1", [days])?;
        tx.commit()?;
        Ok(())
    }

    /// Keeps the file small, in one transaction:
    ///
    /// - for each child, usage older than the detail days is folded into
    ///   daily totals per day, device and app (so the usage views do not
    ///   change), and the slices are removed;
    /// - each child's book in the core forgets the days before yesterday
    ///   (`forget_days_before`), and is written only when that changed it;
    /// - the record of one time codes made before yesterday is dropped;
    /// - events older than [`crate::EVENT_KEEP_DAYS`] days, and all but the
    ///   newest [`crate::MAX_EVENTS`], are dropped.
    ///
    /// It costs one book read and write per child, and work in proportion
    /// to the slices it folds (some thousands for a day of use). Call it
    /// once an hour; it does nothing that is not due, so calling it more
    /// often is harmless, and calling it less often only lets the books and
    /// the detail grow a little meanwhile.
    pub fn tidy(&mut self, now: i64) -> Result<TidyReport, HubError> {
        let tx = self.change()?;
        let detail_days: i64 = tx.query_row("SELECT detail_days FROM settings WHERE id = 1", [], |r| r.get(0))?;
        let detail_days = detail_days.clamp(MIN_DETAIL_DAYS as i64, MAX_DETAIL_DAYS as i64);
        let mut report = TidyReport::default();

        for child in store::children(&tx)? {
            let id = child.child_id.as_str();
            let mut book = store::load_book(&tx, id)?;
            let today = book.today(now);

            // The book: today and yesterday.
            if book.forget_days_before(today.saturating_sub(BOOK_DAYS - 1)) {
                store::save_book(&tx, id, &book, now)?;
            }

            // Usage: fold the days before the detail window.
            let cutoff = today.saturating_sub(detail_days - 1);
            let folded_before: Option<i64> =
                tx.query_row("SELECT folded_before FROM child WHERE id = ?1", [id], |r| r.get(0))?;
            if folded_before.is_none_or(|f| cutoff > f) {
                tx.execute(
                    "INSERT INTO day_total (child_id, day, device_id, app_id, secs, counted_secs)
                     SELECT child_id, day, device_id, app_id, sum(secs), sum(CASE WHEN counts THEN secs ELSE 0 END)
                     FROM slice WHERE child_id = ?1 AND day < ?2
                     GROUP BY child_id, day, device_id, app_id
                     ON CONFLICT (child_id, day, device_id, app_id) DO UPDATE SET
                         secs = secs + excluded.secs,
                         counted_secs = counted_secs + excluded.counted_secs",
                    params![id, cutoff],
                )?;
                let folded = tx.execute("DELETE FROM slice WHERE child_id = ?1 AND day < ?2", params![id, cutoff])?;
                report.slices_folded = report.slices_folded.saturating_add(folded as u64);
                tx.execute("UPDATE child SET folded_before = ?1 WHERE id = ?2", params![cutoff, id])?;
            }

            // Codes: only today's counters matter; yesterday's are kept for
            // a device whose day has not turned yet.
            let dropped = tx.execute(
                "DELETE FROM code_made WHERE day < ?1 AND device_id IN (SELECT id FROM device WHERE child_id = ?2)",
                params![today.saturating_sub(1), id],
            )?;
            report.codes_dropped = report.codes_dropped.saturating_add(dropped as u64);
        }

        let oldest = now.saturating_sub(EVENT_KEEP_DAYS.saturating_mul(SECONDS_PER_DAY));
        let mut dropped = tx.execute("DELETE FROM event WHERE at < ?1", [oldest])?;
        dropped += tx.execute(
            "DELETE FROM event WHERE id NOT IN (SELECT id FROM event ORDER BY id DESC LIMIT ?1)",
            [MAX_EVENTS],
        )?;
        report.events_dropped = dropped as u64;
        tx.commit()?;
        Ok(report)
    }
}
