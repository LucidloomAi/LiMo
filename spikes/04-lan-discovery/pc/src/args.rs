//! Command line arguments. Pure, so it is unit tested.

pub const DEFAULT_PORT: u16 = 47810;
pub const DEFAULT_SERVE_SECONDS: u64 = 600;
pub const MAX_SERVE_SECONDS: u64 = 3600;
pub const DEFAULT_BROWSE_SECONDS: u64 = 10;
pub const MAX_BROWSE_SECONDS: u64 = 120;
/// A DNS-SD instance name is one DNS label: at most 63 bytes.
pub const MAX_NAME_BYTES: usize = 63;
/// The longest DNS name.
pub const MAX_HOST_CHARS: usize = 253;

pub const USAGE: &str = "\
limo-hub-spike: LiMo spike 04, find the master PC on the home network

Usage:
  limo-hub-spike serve [--port N] [--name TEXT] [--seconds N] [--loopback]
      Listen for HELLO on TCP (default port 47810, 0 lets Windows choose) and
      announce the service with mDNS as _limo-spike._tcp.local. Stops by
      itself after --seconds (default 600, 1 to 3600) and on Ctrl+C.
      --loopback listens on 127.0.0.1 only and announces nothing.
  limo-hub-spike browse [--seconds N]
      Look for _limo-spike._tcp.local. for N seconds (default 10, 1 to 120)
      and send each instance found a HELLO.
  limo-hub-spike hello --host H --port P
      Send one HELLO straight to H:P, no discovery.
  limo-hub-spike status
      Read only: addresses, network categories, firewall profiles and rules
      that name this program, and the commands to add or remove a rule.
  limo-hub-spike help

Exit codes: 0 success, 1 an operation failed, 2 bad arguments.
";

#[derive(Debug, PartialEq, Eq)]
pub struct ServeArgs {
    pub port: u16,
    /// None means the default, "LiMo spike on <computer name>".
    pub name: Option<String>,
    pub seconds: u64,
    pub loopback: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Command {
    Serve(ServeArgs),
    Browse { seconds: u64 },
    Hello { host: String, port: u16 },
    Status,
    Help,
}

pub fn parse(args: &[String]) -> Result<Command, String> {
    let Some(sub) = args.first() else {
        return Ok(Command::Help);
    };
    let rest = &args[1..];
    match sub.as_str() {
        "serve" => parse_serve(rest),
        "browse" => parse_browse(rest),
        "hello" => parse_hello(rest),
        "status" => {
            no_more(rest)?;
            Ok(Command::Status)
        }
        "help" | "--help" | "-h" | "/?" => Ok(Command::Help),
        other => Err(format!("unknown command \"{other}\"")),
    }
}

fn parse_serve(rest: &[String]) -> Result<Command, String> {
    let mut port: Option<u16> = None;
    let mut name: Option<String> = None;
    let mut seconds: Option<u64> = None;
    let mut loopback = false;
    let mut i = 0;
    while i < rest.len() {
        let flag = rest[i].as_str();
        match flag {
            "--port" => {
                once(port.is_some(), flag)?;
                port = Some(parse_port(value(rest, i)?, true)?);
                i += 2;
            }
            "--name" => {
                once(name.is_some(), flag)?;
                name = Some(check_name(value(rest, i)?)?);
                i += 2;
            }
            "--seconds" => {
                once(seconds.is_some(), flag)?;
                seconds = Some(parse_seconds(value(rest, i)?, MAX_SERVE_SECONDS)?);
                i += 2;
            }
            "--loopback" => {
                once(loopback, flag)?;
                loopback = true;
                i += 1;
            }
            other => return Err(format!("serve: unknown option \"{other}\"")),
        }
    }
    Ok(Command::Serve(ServeArgs {
        port: port.unwrap_or(DEFAULT_PORT),
        name,
        seconds: seconds.unwrap_or(DEFAULT_SERVE_SECONDS),
        loopback,
    }))
}

fn parse_browse(rest: &[String]) -> Result<Command, String> {
    let mut seconds: Option<u64> = None;
    let mut i = 0;
    while i < rest.len() {
        let flag = rest[i].as_str();
        match flag {
            "--seconds" => {
                once(seconds.is_some(), flag)?;
                seconds = Some(parse_seconds(value(rest, i)?, MAX_BROWSE_SECONDS)?);
                i += 2;
            }
            other => return Err(format!("browse: unknown option \"{other}\"")),
        }
    }
    Ok(Command::Browse {
        seconds: seconds.unwrap_or(DEFAULT_BROWSE_SECONDS),
    })
}

fn parse_hello(rest: &[String]) -> Result<Command, String> {
    let mut host: Option<String> = None;
    let mut port: Option<u16> = None;
    let mut i = 0;
    while i < rest.len() {
        let flag = rest[i].as_str();
        match flag {
            "--host" => {
                once(host.is_some(), flag)?;
                host = Some(check_host(value(rest, i)?)?);
                i += 2;
            }
            "--port" => {
                once(port.is_some(), flag)?;
                port = Some(parse_port(value(rest, i)?, false)?);
                i += 2;
            }
            other => return Err(format!("hello: unknown option \"{other}\"")),
        }
    }
    let host = host.ok_or("hello: --host is required")?;
    let port = port.ok_or("hello: --port is required")?;
    Ok(Command::Hello { host, port })
}

fn no_more(rest: &[String]) -> Result<(), String> {
    match rest.first() {
        None => Ok(()),
        Some(extra) => Err(format!("unexpected argument \"{extra}\"")),
    }
}

fn once(already: bool, flag: &str) -> Result<(), String> {
    if already {
        Err(format!("{flag} given more than once"))
    } else {
        Ok(())
    }
}

fn value(rest: &[String], i: usize) -> Result<&str, String> {
    match rest.get(i + 1) {
        Some(v) if !v.starts_with("--") => Ok(v.as_str()),
        _ => Err(format!("{} needs a value", rest[i])),
    }
}

/// `allow_zero`: serve may use 0 to let Windows choose; hello may not.
pub fn parse_port(text: &str, allow_zero: bool) -> Result<u16, String> {
    let n: u32 = text
        .trim()
        .parse()
        .map_err(|_| format!("port \"{text}\" is not a whole number"))?;
    let low = if allow_zero { 0 } else { 1 };
    if n < low || n > 65535 {
        return Err(format!("port must be between {low} and 65535, not {n}"));
    }
    Ok(n as u16)
}

pub fn parse_seconds(text: &str, max: u64) -> Result<u64, String> {
    let n: u64 = text
        .trim()
        .parse()
        .map_err(|_| format!("--seconds \"{text}\" is not a whole number"))?;
    if n < 1 || n > max {
        return Err(format!("--seconds must be between 1 and {max}, not {n}"));
    }
    Ok(n)
}

/// An instance name: one DNS label, so 1 to 63 bytes, and no control
/// characters. Dots are allowed; mDNS escapes them.
pub fn check_name(text: &str) -> Result<String, String> {
    if text.trim().is_empty() {
        return Err("--name must not be empty".to_string());
    }
    if text.len() > MAX_NAME_BYTES {
        return Err(format!(
            "--name is {} bytes long; the most is {MAX_NAME_BYTES}",
            text.len()
        ));
    }
    if text.chars().any(char::is_control) {
        return Err("--name must not contain control characters".to_string());
    }
    Ok(text.to_string())
}

pub fn check_host(text: &str) -> Result<String, String> {
    let t = text.trim();
    if t.is_empty() {
        return Err("--host must not be empty".to_string());
    }
    if t.chars().count() > MAX_HOST_CHARS {
        return Err(format!("--host is longer than {MAX_HOST_CHARS} characters"));
    }
    if t.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err("--host must not contain spaces or control characters".to_string());
    }
    Ok(t.to_string())
}

/// The default instance name, cut to 63 bytes on a character boundary.
pub fn default_name(computer: Option<&str>) -> String {
    let who = match computer.map(str::trim) {
        Some(c) if !c.is_empty() && !c.chars().any(char::is_control) => c,
        _ => "this PC",
    };
    let mut name = format!("LiMo spike on {who}");
    while name.len() > MAX_NAME_BYTES {
        name.pop();
    }
    name
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn no_arguments_and_help() {
        assert_eq!(parse(&v(&[])), Ok(Command::Help));
        assert_eq!(parse(&v(&["help"])), Ok(Command::Help));
        assert_eq!(parse(&v(&["--help"])), Ok(Command::Help));
        assert!(parse(&v(&["launch"])).is_err());
    }

    #[test]
    fn serve_defaults() {
        assert_eq!(
            parse(&v(&["serve"])),
            Ok(Command::Serve(ServeArgs {
                port: 47810,
                name: None,
                seconds: 600,
                loopback: false
            }))
        );
    }

    #[test]
    fn serve_all_options() {
        assert_eq!(
            parse(&v(&[
                "serve", "--port", "0", "--name", "Den PC", "--seconds", "30", "--loopback"
            ])),
            Ok(Command::Serve(ServeArgs {
                port: 0,
                name: Some("Den PC".to_string()),
                seconds: 30,
                loopback: true
            }))
        );
    }

    #[test]
    fn serve_rejects_bad_values() {
        assert!(parse(&v(&["serve", "--seconds", "0"])).is_err());
        assert!(parse(&v(&["serve", "--seconds", "3601"])).is_err());
        assert!(parse(&v(&["serve", "--seconds", "-5"])).is_err());
        assert!(parse(&v(&["serve", "--seconds", "ten"])).is_err());
        assert!(parse(&v(&["serve", "--seconds"])).is_err());
        assert!(parse(&v(&["serve", "--seconds", "--loopback"])).is_err());
        assert!(parse(&v(&["serve", "--port", "65536"])).is_err());
        assert!(parse(&v(&["serve", "--port", "x"])).is_err());
        assert!(parse(&v(&["serve", "--name", ""])).is_err());
        assert!(parse(&v(&["serve", "--name", "   "])).is_err());
        assert!(parse(&v(&["serve", "--name", &"a".repeat(64)])).is_err());
        assert!(parse(&v(&["serve", "--name", "bad\tname"])).is_err());
        assert!(parse(&v(&["serve", "--port", "1", "--port", "2"])).is_err());
        assert!(parse(&v(&["serve", "--loopback", "--loopback"])).is_err());
        assert!(parse(&v(&["serve", "--fast"])).is_err());
        assert!(parse(&v(&["serve", "--port=47810"])).is_err());
    }

    #[test]
    fn serve_seconds_limits_are_inclusive() {
        assert!(parse(&v(&["serve", "--seconds", "1"])).is_ok());
        assert!(parse(&v(&["serve", "--seconds", "3600"])).is_ok());
        assert!(parse(&v(&["serve", "--name", &"a".repeat(63)])).is_ok());
    }

    #[test]
    fn browse_options() {
        assert_eq!(parse(&v(&["browse"])), Ok(Command::Browse { seconds: 10 }));
        assert_eq!(
            parse(&v(&["browse", "--seconds", "120"])),
            Ok(Command::Browse { seconds: 120 })
        );
        assert!(parse(&v(&["browse", "--seconds", "121"])).is_err());
        assert!(parse(&v(&["browse", "--port", "1"])).is_err());
    }

    #[test]
    fn hello_options() {
        assert_eq!(
            parse(&v(&["hello", "--host", "192.168.68.200", "--port", "47810"])),
            Ok(Command::Hello {
                host: "192.168.68.200".to_string(),
                port: 47810
            })
        );
        assert_eq!(
            parse(&v(&["hello", "--port", "1", "--host", "127.0.0.1"])),
            Ok(Command::Hello {
                host: "127.0.0.1".to_string(),
                port: 1
            })
        );
        assert!(parse(&v(&["hello", "--host", "1.2.3.4"])).is_err());
        assert!(parse(&v(&["hello", "--port", "47810"])).is_err());
        assert!(parse(&v(&["hello", "--host", "1.2.3.4", "--port", "0"])).is_err());
        assert!(parse(&v(&["hello", "--host", "a b", "--port", "1"])).is_err());
        assert!(parse(&v(&["hello", "--host", "", "--port", "1"])).is_err());
    }

    #[test]
    fn status_takes_nothing() {
        assert_eq!(parse(&v(&["status"])), Ok(Command::Status));
        assert!(parse(&v(&["status", "--fix"])).is_err());
    }

    #[test]
    fn default_name_forms() {
        assert_eq!(default_name(Some("DEN-PC")), "LiMo spike on DEN-PC");
        assert_eq!(default_name(None), "LiMo spike on this PC");
        assert_eq!(default_name(Some("  ")), "LiMo spike on this PC");
        let long = default_name(Some(&"\u{e9}".repeat(40)));
        assert!(long.len() <= MAX_NAME_BYTES);
        assert!(long.starts_with("LiMo spike on "));
    }
}
