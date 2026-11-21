//! A stand in for LiMo's service at the door, for the tests only: it speaks
//! the door's protocol (`crate::protocol`) over a real named pipe in the
//! test's own account, and can misbehave as the real door never does on
//! purpose, so the console's handling of a lost or strange door is shown.
//! Everything the real door decides is shown against the real door itself
//! (`real_door_tests.rs`).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use serde_json::{Value, json};
use zeroize::Zeroizing;

use crate::protocol::{Decoder, PROTOCOL_VERSION, encode};
use crate::win::Owned;
use crate::win::pipe::serve::{create_server, let_client_go, sddl_of, wait_for_client};
use crate::win::pipe::{CLIENT_ACCESS, open_client, read_some, write_all};

/// The door's access list, as LiMo for Windows makes it.
pub const DOOR_SDDL: &str = "D:P(A;;GA;;;SY)(A;;GA;;;BA)(A;;0x12019b;;;IU)";

/// How the stand in behaves.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Script {
    /// As the door: hello answered, `sign_in` checked against this
    /// password with the tries left, `about` answered, the requests for a
    /// phone answered from its [`Family`] once signed in, anything else
    /// `unknown_request`.
    Door { password: String },
    /// Refuses at hello with this sentence, and closes.
    Refuse { message: String },
    /// Answers hello, then closes at the first request.
    CloseAfterHello,
    /// Answers hello, then sends bytes that are not a frame of the door's.
    Garbage,
    /// Answers each request under another number.
    WrongNumber,
    /// Answers hello, then never answers a request.
    Silent,
}

/// What one connection sent.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Heard {
    /// Bytes in all.
    pub bytes: usize,
    /// The messages, as JSON, with every string inside a request's
    /// arguments kept: the tests look for a password where it must not be.
    pub messages: Vec<Value>,
}

/// What a stand in's family holds for the requests the console asks for a
/// phone, as LiMo for Windows answers them (`../windows/README.md`, the
/// door's requests): `devices` lists `devices`, `make_offer` makes an offer
/// for `port` (or says `offer_problem`), and `device_link` gives `sessions`
/// for a device that is listed.
#[derive(Debug, Default)]
pub struct Family {
    /// The port the offers name.
    pub port: u16,
    /// Every offer made, in order: each one's text, secret and all.
    pub offers: Vec<String>,
    /// The child's devices, as `devices` answers them.
    pub devices: Vec<Value>,
    /// The listed devices' open sessions, as `device_link` answers them.
    pub sessions: Vec<Value>,
    /// The kind and the sentence `make_offer` answers with instead.
    pub offer_problem: Option<(String, String)>,
}

impl Family {
    /// The answer to one of the requests for a phone, or None for any other
    /// name.
    pub fn answer(&mut self, name: &str, args: &Value) -> Option<Value> {
        let problem = |kind: &str, message: &str| json!({ "outcome": "problem", "problem": { "kind": kind, "message": message } });
        Some(match name {
            "devices" => json!({ "outcome": "done", "value": self.devices }),
            "make_offer" => match &self.offer_problem {
                Some((kind, message)) => problem(kind, message),
                None => {
                    let offer = format!("limo1:STANDIN{}SECRET{}", self.offers.len() + 1, args["child"].as_str().unwrap_or("?"));
                    self.offers.push(offer.clone());
                    let expires_at = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs()) + 600;
                    json!({ "outcome": "done", "value": {
                        "offer": offer, "offer_id": "0f".repeat(16), "expires_at": expires_at, "port": self.port,
                        "addresses": [], "master_id": "ab".repeat(16) } })
                }
            },
            "device_link" => {
                let device = args["device"].as_str().unwrap_or_default();
                if self.devices.iter().any(|d| d["device_id"] == device) {
                    json!({ "outcome": "done", "value": { "device_id": device, "sessions": self.sessions } })
                } else {
                    problem("not_found", "That device is not this family's.")
                }
            }
            _ => return None,
        })
    }
}

/// The stand in, serving its pipe on a thread of its own, one client at a
/// time.
pub struct StandIn {
    name: String,
    stop: Arc<AtomicBool>,
    heard: Arc<Mutex<Vec<Heard>>>,
    family: Arc<Mutex<Family>>,
    sddl: Option<String>,
    thread: Option<JoinHandle<()>>,
}

impl StandIn {
    /// Makes the pipe `name` with the door's access list and starts serving.
    pub fn start(name: &str, script: Script) -> StandIn {
        let pipe = create_server(name, DOOR_SDDL).expect("the stand in's pipe");
        let sddl = sddl_of(&pipe);
        let stop = Arc::new(AtomicBool::new(false));
        let heard = Arc::new(Mutex::new(Vec::new()));
        let family = Arc::new(Mutex::new(Family::default()));
        let (stop2, heard2, family2) = (Arc::clone(&stop), Arc::clone(&heard), Arc::clone(&family));
        let thread = std::thread::spawn(move || serve(pipe, script, &stop2, &heard2, &family2));
        StandIn { name: name.to_string(), stop, heard, family, sddl, thread: Some(thread) }
    }

    /// Its family, which a test may change at any time.
    pub fn family(&self) -> std::sync::MutexGuard<'_, Family> {
        self.family.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// The pipe's name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The pipe's owner and access list, as Windows reads them back.
    pub fn sddl(&self) -> Option<&str> {
        self.sddl.as_deref()
    }

    /// What each connection so far sent: a connection that sent nothing is
    /// not among them.
    pub fn heard(&self) -> Vec<Heard> {
        self.heard.lock().map(|h| h.clone()).unwrap_or_default()
    }
}

impl Drop for StandIn {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        // Wakes the thread if it waits for a client.
        let _ = open_client(&self.name, CLIENT_ACCESS, Duration::from_millis(500));
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn serve(pipe: Owned, script: Script, stop: &AtomicBool, heard: &Mutex<Vec<Heard>>, family: &Mutex<Family>) {
    let mut wrong = 0u32;
    while wait_for_client(&pipe).is_ok() && !stop.load(Ordering::SeqCst) {
        serve_one(&pipe, &script, heard, family, &mut wrong);
        let_client_go(&pipe);
    }
}

fn send(pipe: &Owned, message: &Value) -> bool {
    encode(message).ok().is_some_and(|frame| write_all(pipe, &frame).is_ok())
}

fn serve_one(pipe: &Owned, script: &Script, heard: &Mutex<Vec<Heard>>, family: &Mutex<Family>, wrong: &mut u32) {
    let mut decoder = Decoder::new();
    let mut chunk = Zeroizing::new(vec![0u8; 16 * 1024]);
    let mut at: Option<usize> = None;
    let mut greeted = false;
    let mut signed_in = false;
    loop {
        let read = match read_some(pipe, &mut chunk) {
            Ok(0) | Err(_) => return,
            Ok(n) => n,
        };
        let index = *at.get_or_insert_with(|| {
            let mut list = heard.lock().unwrap_or_else(|p| p.into_inner());
            list.push(Heard::default());
            list.len() - 1
        });
        if decoder.push(&chunk[..read]).is_err() {
            return;
        }
        let mut list = heard.lock().unwrap_or_else(|p| p.into_inner());
        list[index].bytes += read;
        drop(list);
        loop {
            let message = match decoder.next_message::<Value>() {
                Ok(Some(m)) => m,
                Ok(None) => break,
                Err(_) => return,
            };
            heard.lock().unwrap_or_else(|p| p.into_inner())[index].messages.push(message.clone());
            if !greeted {
                if message != json!({ "say": "hello", "protocol": PROTOCOL_VERSION }) {
                    return;
                }
                greeted = true;
                if let Script::Refuse { message } = script {
                    send(pipe, &json!({ "say": "refused", "message": message }));
                    return;
                }
                if !send(pipe, &json!({ "say": "hello", "protocol": PROTOCOL_VERSION, "version": "0.1.0" })) {
                    return;
                }
                continue;
            }
            let id = message["id"].as_u64().unwrap_or(0);
            let name = message["request"]["name"].as_str().unwrap_or("");
            let reply = match script {
                Script::CloseAfterHello | Script::Refuse { .. } => return,
                Script::Silent => continue,
                Script::Garbage => {
                    let _ = write_all(pipe, b"\x01\x00\x05\x00\x00\x00hello");
                    continue;
                }
                Script::WrongNumber => {
                    send(pipe, &json!({ "say": "answer", "id": id + 1, "reply": { "outcome": "done", "value": {} } }));
                    continue;
                }
                Script::Door { password } => match name {
                    "about" => json!({ "outcome": "done", "value": { "version": "0.1.0", "protocol": 1, "pc_name": "Family PC" } }),
                    "sign_in" if message["request"]["args"]["password"].as_str() == Some(password) => {
                        *wrong = 0;
                        signed_in = true;
                        json!({ "outcome": "done", "value": {} })
                    }
                    "devices" | "make_offer" | "device_link" if !signed_in => {
                        json!({ "outcome": "problem", "problem": { "kind": "signed_out", "message": "Sign in first." } })
                    }
                    "devices" | "make_offer" | "device_link" => {
                        let mut family = family.lock().unwrap_or_else(|p| p.into_inner());
                        family.answer(name, &message["request"]["args"]).unwrap_or(Value::Null)
                    }
                    "sign_in" => {
                        *wrong += 1;
                        let left = 4u32.saturating_sub(*wrong);
                        json!({ "outcome": "problem", "problem": {
                            "kind": "wrong_password",
                            "message": format!("That password is not right. {left} more tries before a wait."),
                            "tries_before_wait": left } })
                    }
                    _ => json!({ "outcome": "problem", "problem": { "kind": "unknown_request", "message": "LiMo does not know that request." } }),
                },
            };
            if !send(pipe, &json!({ "say": "answer", "id": id, "reply": reply })) {
                return;
            }
        }
    }
}
