//! `limo dev-console`'s lines, with no Windows code: scaffolding until the
//! guardian's console exists, so the owner can knock on the console's door
//! by hand in a run at the PC. Each line the owner types becomes one
//! request of the door's own (`crate::console::Request`); a password or a
//! PIN is never typed on the line, but asked for at a prompt that does not
//! show it.

#![forbid(unsafe_code)]

use limo_core::{DailyBudget, RuleSet, WeekBudgets};
use limo_hub::{AccountLink, AgeBand};

use crate::console::{Reply, Request};

/// What a line asks for.
#[derive(Debug, Clone, PartialEq)]
pub enum DevLine {
    /// A request as it is.
    Send(Request),
    /// Sign in: the password is asked for, not shown.
    SignIn,
    /// Make the family: the password is asked for twice, not shown.
    CreateFamily,
    /// Set the PIN: asked for twice, not shown.
    SetPin,
    /// Removes a device: the password is asked for, not shown.
    RemoveDevice {
        /// The device's id.
        device: String,
    },
    /// Allows LiMo's removal for ten minutes: the password is asked for, not
    /// shown.
    AllowRemoval,
    /// A child's minutes a day, every day: the rules are read, changed and
    /// sent back.
    Minutes {
        /// The child's id.
        child: String,
        /// The minutes, 0 for none.
        minutes: u32,
    },
    /// A request by its name, with its arguments as JSON. Requests that
    /// carry a password or a PIN are not taken this way.
    Raw {
        /// The request's JSON: `{"name": ..., "args": ...}`.
        json: serde_json::Value,
    },
    /// The words of `help`.
    Help,
    /// Close the connection and end.
    Quit,
    /// Nothing typed.
    Nothing,
}

/// What `help` prints.
pub fn dev_console_help() -> String {
    [
        "  state                     whether there is a family, and whether this window is signed in",
        "  create-family             make the family (the password is asked for twice, not shown)",
        "  sign-in                   sign in (the password is asked for, not shown)",
        "  sign-out                  end this window’s session",
        "  set-pin                   set the guardian PIN (asked for twice, not shown)",
        "  add-child NAME            add a child, 10 to 12, in this PC’s time zone",
        "  minutes CHILD_ID N        give the child N minutes every day",
        "  accounts                  every Windows account of this PC, with whose it is",
        "  link ACCOUNT_SID CHILD_ID link a Windows account to a child",
        "  free ACCOUNT_SID          mark a Windows account as not limited (a guardian’s)",
        "  finish                    finish the first start: accounts not set up are locked",
        "  today                     every child’s day",
        "  offer CHILD_ID            a pairing offer for a phone of the child’s",
        "  remove-device DEVICE_ID   remove a device (the password is asked for, not shown)",
        "  allow-removal             allow LiMo to be removed in the next ten minutes, with no",
        "                            password (the password is asked for, not shown)",
        "  about                     LiMo for Windows' version",
        "  request NAME [JSON]       any other request, such as: request lift {\"child\":\"...\"}",
        "  quit                      close and end",
    ]
    .join("\n")
}

/// Reads one line the owner typed.
pub fn parse_dev_line(line: &str) -> Result<DevLine, String> {
    let line = line.trim();
    let (word, rest) = line.split_once(char::is_whitespace).map_or((line, ""), |(w, r)| (w, r.trim()));
    let words: Vec<&str> = rest.split_whitespace().collect();
    Ok(match (word, words.as_slice()) {
        ("", _) => DevLine::Nothing,
        ("help" | "?", _) => DevLine::Help,
        ("quit" | "exit", _) => DevLine::Quit,
        ("state", []) => DevLine::Send(Request::FamilyState),
        ("about", []) => DevLine::Send(Request::About),
        ("today", []) => DevLine::Send(Request::FamilyToday),
        ("accounts", []) => DevLine::Send(Request::Accounts),
        ("finish", []) => DevLine::Send(Request::FinishFirstStart),
        ("sign-out", []) => DevLine::Send(Request::SignOut),
        ("sign-in", []) => DevLine::SignIn,
        ("create-family", []) => DevLine::CreateFamily,
        ("set-pin", []) => DevLine::SetPin,
        ("add-child", _) if !rest.is_empty() => {
            DevLine::Send(Request::AddChild { name: rest.to_string(), age_band: AgeBand::TenToTwelve })
        }
        ("minutes", [child, minutes]) => match minutes.parse::<u32>() {
            Ok(minutes) if minutes <= 1_440 => DevLine::Minutes { child: (*child).to_string(), minutes },
            _ => return Err("minutes takes a child’s id and a whole number from 0 to 1440.".to_string()),
        },
        ("link", [account, child]) => DevLine::Send(Request::LinkAccount {
            account: (*account).to_string(),
            link: AccountLink::Child { child_id: (*child).to_string() },
        }),
        ("free", [account]) => DevLine::Send(Request::LinkAccount { account: (*account).to_string(), link: AccountLink::NotLimited }),
        ("offer", [child]) => DevLine::Send(Request::MakeOffer { child: (*child).to_string() }),
        ("remove-device", [device]) => DevLine::RemoveDevice { device: (*device).to_string() },
        ("allow-removal", []) => DevLine::AllowRemoval,
        ("request", _) if !rest.is_empty() => {
            let (name, args) = rest.split_once(char::is_whitespace).map_or((rest, ""), |(n, a)| (n, a.trim()));
            let mut json = serde_json::json!({ "name": name });
            if !args.is_empty() {
                let parsed: serde_json::Value =
                    serde_json::from_str(args).map_err(|_| "The arguments after the name must be JSON, such as {\"child\":\"...\"}.".to_string())?;
                json["args"] = parsed;
            }
            // A request that carries a secret is typed at a prompt, never on
            // the line, where the terminal keeps it.
            if let Ok(request) = serde_json::from_value::<Request>(json.clone())
                && request.carries_a_secret()
            {
                return Err(format!("{name} carries the password or the PIN: it is not typed on the line."));
            }
            DevLine::Raw { json }
        }
        _ => return Err(format!("dev-console does not know \"{line}\". Type help.")),
    })
}

/// The rules with `minutes` every day (0 for no time at all).
pub fn with_minutes(mut rules: RuleSet, minutes: u32) -> RuleSet {
    rules.budgets = WeekBudgets::every_day(DailyBudget::Minutes(minutes));
    rules
}

/// An answer, in words for the terminal.
pub fn reply_words(reply: &Reply) -> String {
    match reply {
        Reply::Done { value } => {
            let shown = serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string());
            if shown == "{}" { "Done.".to_string() } else { format!("Done:\n{shown}") }
        }
        Reply::Problem { problem } => {
            let mut words = format!("Not done ({:?}): {}", problem.kind, problem.message);
            if let Some(until) = problem.wait_until {
                words.push_str(&format!(" The wait ends at {until} (unix seconds)."));
            }
            for rule in &problem.rule_problems {
                words.push_str(&format!("\n  {}: {}", rule.field, rule.message));
            }
            words
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::console::{Problem, ProblemKind};

    #[test]
    fn each_line_reads_as_the_request_it_names() {
        assert_eq!(parse_dev_line("  "), Ok(DevLine::Nothing));
        assert_eq!(parse_dev_line("state"), Ok(DevLine::Send(Request::FamilyState)));
        assert_eq!(parse_dev_line("sign-in"), Ok(DevLine::SignIn));
        assert_eq!(parse_dev_line("create-family"), Ok(DevLine::CreateFamily));
        assert_eq!(
            parse_dev_line("add-child Asha Rao"),
            Ok(DevLine::Send(Request::AddChild { name: "Asha Rao".into(), age_band: AgeBand::TenToTwelve }))
        );
        assert_eq!(parse_dev_line("minutes c1 20"), Ok(DevLine::Minutes { child: "c1".into(), minutes: 20 }));
        assert!(parse_dev_line("minutes c1 lots").is_err());
        assert_eq!(
            parse_dev_line("link S-1-5-21-1 c1"),
            Ok(DevLine::Send(Request::LinkAccount { account: "S-1-5-21-1".into(), link: AccountLink::Child { child_id: "c1".into() } }))
        );
        assert_eq!(
            parse_dev_line("free S-1-5-21-2"),
            Ok(DevLine::Send(Request::LinkAccount { account: "S-1-5-21-2".into(), link: AccountLink::NotLimited }))
        );
        assert_eq!(
            parse_dev_line(r#"request lift {"child":"c1"}"#),
            Ok(DevLine::Raw { json: serde_json::json!({"name": "lift", "args": {"child": "c1"}}) })
        );
        assert_eq!(parse_dev_line("request waiting_requests"), Ok(DevLine::Raw { json: serde_json::json!({"name": "waiting_requests"}) }));
        assert_eq!(parse_dev_line("offer c1"), Ok(DevLine::Send(Request::MakeOffer { child: "c1".into() })));
        assert_eq!(parse_dev_line("remove-device d1"), Ok(DevLine::RemoveDevice { device: "d1".into() }));
        assert_eq!(parse_dev_line("allow-removal"), Ok(DevLine::AllowRemoval));
        assert!(dev_console_help().contains("allow-removal"));
        assert!(parse_dev_line("take over").is_err());
        assert!(dev_console_help().contains("sign-in"));
    }

    #[test]
    fn a_secret_is_never_taken_on_the_line() {
        let refused = parse_dev_line(r#"request sign_in {"password":"correct horse battery"}"#).unwrap_err();
        assert!(refused.contains("not typed on the line") && !refused.contains("horse"), "{refused}");
        assert!(parse_dev_line(r#"request delete_family {"password":"x"}"#).is_err());
    }

    #[test]
    fn answers_read_in_words() {
        assert_eq!(reply_words(&Reply::Done { value: serde_json::json!({}) }), "Done.");
        let wrong = Problem { wait_until: Some(5), ..Problem::new(ProblemKind::MustWait, "Too many wrong tries.") };
        let words = reply_words(&Reply::Problem { problem: wrong });
        assert!(words.contains("MustWait") && words.contains("Too many wrong tries.") && words.contains("5 (unix seconds)"), "{words}");
        let rules = with_minutes(RuleSet::new(1, 0), 25);
        assert_eq!(rules.budgets, WeekBudgets::every_day(DailyBudget::Minutes(25)));
    }
}
