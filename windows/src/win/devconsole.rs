//! `limo dev-console`: scaffolding until the guardian's console exists. It
//! knocks on the console's door from the account it runs in, as the console
//! will, and sends what the owner types, one request a line (see
//! `crate::devconsole`). Before it sends anything it checks that the door is
//! served from session 0, where services run, so a program posing as the
//! door in a user's session is never told a password. A password or a PIN is
//! asked for at a prompt that does not show it.

use std::io::{BufRead, Write};

use limo_core::RuleSet;

use super::console::read_hidden;
use super::door::DoorClient;
use crate::console::{CONSOLE_PIPE_NAME, CONSOLE_PROTOCOL_VERSION, FromConsole, MAX_CONSOLE_MESSAGE_BYTES, Reply, Request, ToConsole};
use crate::devconsole::{DevLine, dev_console_help, parse_dev_line, reply_words, with_minutes};
use crate::protocol::{Secret, encode_frame};

/// How long an answer may take, in milliseconds.
const ANSWER_WAIT_MS: u32 = 10_000;

/// The answer to the request numbered `id`; None when the door closed or
/// nothing came, which is said.
fn wait_answer(client: &mut DoorClient, id: u32) -> Option<Reply> {
    loop {
        match client.read(ANSWER_WAIT_MS) {
            Ok(Some(ToConsole::Answer { id: got, reply })) if got == id => return Some(reply),
            Ok(Some(ToConsole::Refused { message })) => {
                say!("{message}");
                return None;
            }
            Ok(Some(_)) => {}
            Ok(None) => {
                say!("No answer came within {} seconds.", ANSWER_WAIT_MS / 1_000);
                return None;
            }
            Err(problem) => {
                say!("The door closed: {problem}.");
                return None;
            }
        }
    }
}

fn ask(client: &mut DoorClient, next_id: &mut u32, request: Request) -> Option<Reply> {
    *next_id += 1;
    let id = *next_id;
    if let Err(problem) = client.send(&FromConsole::Request { id, request }) {
        say!("It could not be sent: {problem}.");
        return None;
    }
    wait_answer(client, id)
}

/// A secret typed twice, the same both times.
fn twice(what: &str) -> Option<Secret> {
    let first = read_hidden(&format!("{what} (not shown): "))?;
    let again = read_hidden("The same again: ")?;
    if *first != *again {
        say!("The two differ. Nothing was sent.");
        return None;
    }
    Some(Secret::new(first.to_string()))
}

/// `limo dev-console`.
pub(crate) fn run() -> i32 {
    say!("dev-console is scaffolding until the guardian’s console exists.");
    let mut client = match DoorClient::connect(CONSOLE_PIPE_NAME, 2_000) {
        Ok(client) => client,
        Err(problem) => {
            say!("LiMo’s settings could not be reached: {problem}. Is the service running? Run limo status.");
            return 1;
        }
    };
    let server = client.server();
    match (server.pid, server.session_id) {
        (Some(pid), Some(0)) => say!("The door is served by process {pid} in session 0, where services run."),
        (pid, session) => {
            say!(
                "The door is served by a process that is not a service (pid {pid:?}, session {session:?}). LiMo’s service runs in session 0, so nothing was sent."
            );
            return 2;
        }
    }
    if let Err(problem) = client.send(&FromConsole::Hello { protocol: CONSOLE_PROTOCOL_VERSION }) {
        say!("Hello could not be sent: {problem}.");
        return 1;
    }
    match client.read(ANSWER_WAIT_MS) {
        Ok(Some(ToConsole::Hello { version, .. })) => say!("LiMo for Windows {version} answered. Type help."),
        Ok(Some(ToConsole::Refused { message })) => {
            say!("{message}");
            return 2;
        }
        Ok(_) => {
            say!("No hello came back.");
            return 1;
        }
        Err(problem) => {
            say!("The door closed: {problem}.");
            return 1;
        }
    }
    let mut next_id = 0u32;
    let stdin = std::io::stdin();
    loop {
        print!("> ");
        let _ = std::io::stdout().flush();
        let mut line = String::new();
        match stdin.lock().read_line(&mut line) {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
        let parsed = match parse_dev_line(&line) {
            Ok(parsed) => parsed,
            Err(problem) => {
                say!("{problem}");
                continue;
            }
        };
        let request = match parsed {
            DevLine::Nothing => continue,
            DevLine::Help => {
                say!("{}", dev_console_help());
                continue;
            }
            DevLine::Quit => break,
            DevLine::Send(request) => request,
            DevLine::SignIn => match read_hidden("Guardian password (not shown): ") {
                Some(password) => Request::SignIn { password: Secret::new(password.to_string()) },
                None => continue,
            },
            DevLine::CreateFamily => match twice("New guardian password, 8 or more characters") {
                Some(password) => Request::CreateFamily { password },
                None => continue,
            },
            DevLine::SetPin => match twice("Guardian PIN, 6 to 8 digits") {
                Some(pin) => Request::SetPin { pin },
                None => continue,
            },
            DevLine::RemoveDevice { device } => match read_hidden("Guardian password, to remove it (not shown): ") {
                Some(password) => Request::RemoveDevice { device, password: Secret::new(password.to_string()) },
                None => continue,
            },
            DevLine::Minutes { child, minutes } => match ask(&mut client, &mut next_id, Request::Rules { child: child.clone() }) {
                Some(Reply::Done { value }) => match serde_json::from_value::<RuleSet>(value) {
                    Ok(rules) => Request::ReplaceRules { child, rules: with_minutes(rules, minutes) },
                    Err(_) => {
                        say!("The rules that came back could not be read.");
                        continue;
                    }
                },
                Some(other) => {
                    say!("{}", reply_words(&other));
                    continue;
                }
                None => break,
            },
            DevLine::Raw { json } => {
                // Sent as typed, even a name LiMo does not know, so the
                // door's answer to it can be seen.
                next_id += 1;
                let message = serde_json::json!({ "say": "request", "id": next_id, "request": json });
                let sent = encode_frame(&message, CONSOLE_PROTOCOL_VERSION, MAX_CONSOLE_MESSAGE_BYTES)
                    .map_err(|problem| problem.to_string())
                    .and_then(|frame| client.write_raw(&frame));
                if let Err(problem) = sent {
                    say!("It could not be sent: {problem}.");
                    break;
                }
                match wait_answer(&mut client, next_id) {
                    Some(reply) => say!("{}", reply_words(&reply)),
                    None => break,
                }
                continue;
            }
        };
        match ask(&mut client, &mut next_id, request) {
            Some(reply) => say!("{}", reply_words(&reply)),
            None => break,
        }
    }
    say!("Closed.");
    0
}
