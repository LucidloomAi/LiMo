//! What the first run on a real phone found, on 20 November 2026: adb on
//! Windows hands every line feed of a `shell` answer back as a carriage
//! return and a line feed, and `exec-out` hands bytes over as they are but
//! always ends with exit code 0. The fake phone does the same, and every
//! story runs against it; these are the failure itself, and what `setup`
//! must never do after it.

use std::time::Duration;

use limo_phone_dev::adb::{Adb, Call};
use limo_phone_dev::{DONE, FAILED};

use crate::fake::{Bench, READ, SERIAL, STATE, crlf_bytes};

fn args(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}

#[test]
fn a_state_with_line_feeds_comes_back_exact_though_adb_shell_would_change_it() {
    let mut bench = Bench::new();
    assert_eq!(bench.setup("setup --child Asha --minutes 20"), DONE, "{}", bench.printed());
    let state = bench.phone.files.get(STATE).unwrap().clone();
    assert!(state.iter().filter(|b| **b == b'\n').count() >= 3, "the engine's state has line feeds");

    // The fake phone, asked as the first run asked the real one: through
    // shell the state comes back longer, every line feed made CR LF;
    // through exec-out it comes back exact, with exit code 0.
    let mut exact = b"limo-dev-state\n".to_vec();
    exact.extend_from_slice(&state);
    let shell = bench.phone.run(Call::new(args(&["-s", SERIAL, "shell", "-T", READ]), Duration::from_secs(30))).unwrap();
    assert_eq!(shell.stdout, crlf_bytes(&exact));
    assert_ne!(shell.stdout, exact);
    let exec = bench.phone.run(Call::new(args(&["-s", SERIAL, "exec-out", READ]), Duration::from_secs(30))).unwrap();
    assert_eq!((exec.code, &exec.stdout), (Some(0), &exact));
    // With no state, exec-out's exit code is 0 all the same: only the
    // answer's first line tells.
    let kept = bench.phone.files.remove(STATE).unwrap();
    let none = bench.phone.run(Call::new(args(&["-s", SERIAL, "exec-out", READ]), Duration::from_secs(30))).unwrap();
    assert_eq!((none.code, none.stdout.as_slice()), (Some(0), b"limo-dev-none\n".as_slice()));
    bench.phone.files.insert(STATE.to_string(), kept);

    // So the tool reads the state exactly, everywhere it reads it.
    bench.clear();
    assert_eq!(bench.run("check"), DONE);
    assert!(bench.printed().contains("LiMo's state on the phone: it holds this PC's test family, Asha's test phone."), "{}", bench.printed());
    bench.clear();
    assert_eq!(bench.run("status"), DONE);
    assert!(bench.printed().contains("Child: Asha (this PC's test family), on its test phone."));
    bench.clear();
    assert_eq!(bench.run("forget"), DONE);
    assert!(bench.printed().contains("LiMo wiped it: its kept state is gone from the phone."));
    assert!(bench.printed().contains("Deleted the test family on this PC."));
}

#[test]
fn when_limo_says_taken_but_its_state_cannot_be_read_the_family_stays() {
    // The first run, as it was: LiMo took it, said so, and the tool could
    // not read the state back. The family on this PC must stay.
    let mut bench = Bench::new();
    bench.phone.mangles_exec_out = true;
    assert_eq!(bench.setup("setup --child Asha --minutes 20"), FAILED);
    let printed = bench.printed();
    assert!(
        printed.contains(
            "LiMo says it took the test family (its log says: dev pair taken), but this tool could not read its state back \
             to check that it holds the test phone."
        ),
        "{printed}"
    );
    assert!(printed.contains(
        "The test family on this PC is kept, so that this PC and the phone still match. Run check, then status, to see \
         what the phone holds. To start again, run forget: it wipes the test family from the phone and from this PC."
    ));
    assert!(!printed.contains("deleted again"), "{printed}");
    assert!(bench.places().family_file().exists(), "the family on this PC stays");
    assert!(bench.phone.files.contains_key(STATE));
    assert_eq!(bench.phone.child_name.as_deref(), Some("Asha"));
    assert!(bench.log().contains("LiMo said it took it, and its state could not be read back."));

    // While the state cannot be read, check says so plainly.
    bench.clear();
    assert_eq!(bench.run("check"), FAILED);
    assert!(bench.printed().contains("LiMo's state came back with its line ends changed"), "{}", bench.printed());

    // Once it can be read, the two sides match: nothing was lost.
    bench.phone.mangles_exec_out = false;
    bench.clear();
    assert_eq!(bench.run("check"), DONE);
    assert!(bench.printed().contains("it holds this PC's test family, Asha's test phone."));
    bench.clear();
    assert_eq!(bench.run("forget"), DONE);
    assert!(!bench.places().family_file().exists() && !bench.phone.files.contains_key(STATE));
}

#[test]
fn a_state_on_the_phone_after_the_broadcast_keeps_the_family_even_when_the_log_is_silent() {
    // No word from the log, a state that cannot be read: setup paired only
    // a phone that kept none, so the state came from this pairing.
    let mut bench = Bench::new();
    bench.phone.mangles_exec_out = true;
    bench.phone.log_silent = true;
    assert_eq!(bench.setup("setup --child Asha --minutes 20"), FAILED);
    let printed = bench.printed();
    assert!(printed.contains("LiMo keeps a state on the phone now, which can only have come from this pairing."), "{printed}");
    assert!(printed.contains("The test family on this PC is kept"));
    assert!(bench.places().family_file().exists());

    // And only a phone with no state lets setup undo this PC's side.
    let mut bench = Bench::new();
    bench.phone.refuse_with = Some("old_file".to_string());
    assert_eq!(bench.setup("setup --child Asha --minutes 20"), FAILED);
    assert!(bench.printed().contains("the test family on this PC deleted again, so setup can run again."));
    assert!(!bench.places().family_file().exists());
}
