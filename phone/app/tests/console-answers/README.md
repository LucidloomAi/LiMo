# The console door's answers

Every `.json` file here is written by a test of LiMo for Windows, and
never edited by hand: the test
`answers::a_small_family_through_the_real_door_writes_each_answer_the_screens_read`
in `windows/tests/service/answers.rs`. It runs with the crate's other
tests (`cargo test` in `windows/`, on Windows), or alone:

```
cargo test --test service answers
```

The test plays a small family through the real door of LiMo's settings,
over a real named pipe, and writes each message as it left the door, one
file per request and case. The family is made by the first start's own
requests (the password, the PIN, Asha, the accounts, her rules); then Asha
uses the PC for five minutes and asks for more time, a phone is paired, a
wrong password is tried until the wait, the phone's notes come in (setup not
finished, a step switched off, USB debugging on, all well: the
`device-phone-*` files and `family-today-phone-step-off`), a console from
Asha's account is refused, and at the end the family is removed. Each
device in `family_today`, `child_today`, `devices` and `device` has
`standing` and `guardian_needed`, null for the PC's own; LiMo for Windows'
README says what each field holds. Every request in the door's
list has at least one file, and the test fails when the door gains a
request it does not play.

Each file holds:

- `request`: the request's name (`hello` for the door's first message).
- `case`: in a few words, what the file shows.
- `asked`: the request as the console sent it.
- `answer`: the door's message, as it left the door.

A run with no change in the door changes no file: the test's clock is its
own (Wednesday 18 November 2026, from 16:00 in India's time zone), a file
is written only when it changed, and a file a run did not write is
removed. What would differ from run to run is a fixed sample:

- the family file's ids: `child-0000000000000001`, `device-0000000000000001`
  (the PC) and `device-0000000000000002` (the phone);
- the guardian's Windows account, `S-1-5-21-1000` (the test's own);
- the link's port, `47810`, and the offer's `offer_id` and `master_id`;
- the offer itself, which holds a secret, as a sentence after `limo1:`;
- the one time code's digits, as `1234 5678`.

No file holds a password, a PIN, a key or a code: a secret the console sent
is written as `(a secret: not written here)`, and the test checks every
file for each of them before it writes.
