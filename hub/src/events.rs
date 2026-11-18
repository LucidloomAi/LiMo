//! Reading the record of events. Events are written by the calls that cause
//! them, in the same transaction, and trimmed by [`Hub::tidy`].

use rusqlite::params;

use crate::MAX_EVENTS_PER_CALL;
use crate::error::HubError;
use crate::hub::Hub;
use crate::types::{Event, EventKind};

impl Hub {
    /// The newest events first, at most `limit` of them (and at most
    /// [`crate::MAX_EVENTS_PER_CALL`]). For the next page, pass the id of the
    /// last event of this one as `before_id`.
    pub fn events(&self, limit: u32, before_id: Option<i64>) -> Result<Vec<Event>, HubError> {
        let limit = limit.min(MAX_EVENTS_PER_CALL);
        let mut stmt = self.conn.prepare(
            "SELECT id, at, child_id, detail FROM event
             WHERE ?1 IS NULL OR id < ?1
             ORDER BY id DESC LIMIT ?2",
        )?;
        let rows = stmt
            .query_map(params![before_id, limit], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows.into_iter()
            .map(|(id, at, child_id, detail)| {
                let what: EventKind = serde_json::from_str(&detail)?;
                Ok(Event { id, at, child_id, what })
            })
            .collect()
    }
}
