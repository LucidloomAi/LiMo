//! The service, through the door: the console's own questions for a phone
//! (`devices`, `make_offer`, `device_link`) asked on the same connection as
//! the page's, signed in as the guardian is. An answer's value comes back
//! to the console's thread; the rest of the reply is wiped, and an offer's
//! secret is wiped there once handed to the phone. The door's notes and a
//! door that could not be asked go into the window's own log, when it has
//! one.

#![forbid(unsafe_code)]

use std::sync::{Arc, Mutex};

use serde_json::Value;

use super::{Said, Service};
use crate::console_log::ConsoleLog;
use crate::door::{Door, DoorReply};
use crate::protocol::{PageRequest, wipe};

/// LiMo's service, asked through the console's door.
pub struct DoorService {
    door: Arc<Mutex<Door>>,
    log: Option<Arc<ConsoleLog>>,
}

impl DoorService {
    /// Asks through `door`, the console's own.
    pub fn new(door: Arc<Mutex<Door>>) -> DoorService {
        DoorService { door, log: None }
    }

    /// The same, writing the door's notes and problems into `log`.
    pub fn with_log(door: Arc<Mutex<Door>>, log: Arc<ConsoleLog>) -> DoorService {
        DoorService { door, log: Some(log) }
    }
}

impl Service for DoorService {
    fn ask(&self, name: &str, args: Value) -> Result<Value, Said> {
        let request = PageRequest::from_console(name, args).map_err(|why| Said::new("unknown_request", &why))?;
        let (answer, notes) = {
            let mut door = self.door.lock().unwrap_or_else(|p| p.into_inner());
            let answer = door.ask(request);
            (answer, door.take_notes())
        };
        if let Some(log) = &self.log {
            for note in &notes {
                log.write(&format!("door, for a phone: {note}"));
            }
            if let Err(problem) = &answer {
                log.write(&format!("{name}, for a phone: not asked: {problem}"));
            }
        }
        match answer {
            Ok(DoorReply::Reply(mut reply)) => {
                let said = if reply["outcome"] == "done" {
                    Ok(reply.get_mut("value").map(Value::take).unwrap_or(Value::Null))
                } else {
                    Err(Said {
                        code: reply["problem"]["kind"].as_str().unwrap_or("failed").to_string(),
                        words: reply["problem"]["message"].as_str().unwrap_or("LiMo\u{2019}s service did not say why.").to_string(),
                    })
                };
                wipe(&mut reply);
                said
            }
            Ok(DoorReply::Refused(message)) => Err(Said { code: "refused".to_string(), words: message }),
            Err(problem) => Err(Said::new(problem.code(), problem.words())),
        }
    }
}
