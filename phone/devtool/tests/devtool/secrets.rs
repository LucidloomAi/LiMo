//! A PIN, a code or the code secret never shows where it should not: not
//! in what is printed (but the code `code` prints on purpose), not in an
//! error, not in the tool's log, and never in an argument of an adb call.

use limo_phone_dev::{DONE, REFUSED};

use crate::fake::{Bench, PIN};

#[test]
fn a_pin_a_code_or_the_secret_never_shows_but_where_it_must() {
    let mut bench = Bench::new();

    // Refused PINs first: neither shows anywhere.
    bench.ui.typed.extend(["48291577".to_string(), "48291578".to_string()]);
    assert_eq!(bench.run("setup --child Asha --minutes 20"), REFUSED);
    bench.ui.typed.extend(["12345".to_string(), "12345".to_string()]);
    assert_eq!(bench.run("setup --child Asha --minutes 20"), REFUSED);

    // A whole session. (The short PIN was refused at its first prompt, so
    // its second typing was never read.)
    bench.ui.typed.clear();
    assert_eq!(bench.setup("setup --child Asha --minutes 20 --block com.example.game"), DONE);
    let pairing = bench.phone.calls.iter().find_map(|c| c.stdin.clone()).expect("the pairing file went on standard input");
    let text = String::from_utf8(pairing).unwrap();
    let at = text.find("\"code_secret_hex\":\"").unwrap() + 19;
    let secret = text[at..at + 64].to_string();

    let mut codes = Vec::new();
    for _ in 0..3 {
        let before = bench.ui.lines.len();
        assert_eq!(bench.run("code --minutes 15"), DONE);
        let code = bench.ui.lines[before..]
            .iter()
            .map(|l| l.trim().to_string())
            .find(|l| l.len() == 9 && l.replace(' ', "").chars().all(|c| c.is_ascii_digit()))
            .unwrap();
        codes.push(code);
    }
    bench.phone.use_app("org.example.reader", 120);
    for command in ["status", "check", "log --minutes 1", "forget", "code", "status", "uninstall", "forget --this-pc-only"] {
        bench.run(command);
    }

    // Printed: each code once, on its own line, and nothing else secret.
    let printed = bench.printed();
    for code in &codes {
        let lines: Vec<&String> = bench.ui.lines.iter().filter(|l| l.contains(code.as_str()) || l.contains(&code.replace(' ', ""))).collect();
        assert_eq!(lines.len(), 1, "{code}: {lines:?}");
        assert_eq!(lines[0].trim(), code);
    }
    for secret_text in [PIN, "48291577", "48291578", "12345", secret.as_str()] {
        assert!(!printed.contains(secret_text), "printed {secret_text}");
    }
    // The tool's log.
    let log = bench.log();
    assert!(!log.is_empty());
    for secret_text in [PIN, "48291577", "12345", secret.as_str()] {
        assert!(!log.contains(secret_text), "logged {secret_text}");
    }
    for code in &codes {
        assert!(!log.contains(code.as_str()) && !log.contains(&code.replace(' ', "")), "logged {code}");
    }
    // Every argument of every adb call.
    for call in &bench.phone.calls {
        for arg in &call.args {
            for secret_text in [PIN, secret.as_str()] {
                assert!(!arg.contains(secret_text), "{arg}");
            }
            for code in &codes {
                assert!(!arg.contains(&code.replace(' ', "")), "{arg}");
            }
        }
    }
    // Only one call ever had input: the pairing file.
    assert_eq!(bench.phone.calls.iter().filter(|c| c.stdin.is_some()).count(), 1);
}

#[test]
fn the_typed_pin_and_the_code_hide_in_debug_output() {
    let mut bench = Bench::new();
    assert_eq!(bench.setup("setup --child Asha --minutes 20"), DONE);
    let mut hub = limo_phone_dev::family::open(&bench.places()).unwrap().unwrap();
    let code = limo_phone_dev::family::make_code(&mut hub, 15, bench.now).unwrap();
    let shown = format!("{code:?}");
    assert!(shown.contains("hidden") && !shown.contains(&code.shown[..4]), "{shown}");
    hub.close().unwrap();
}
