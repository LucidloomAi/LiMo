# The window's answers

Every `.json` file here is written by a test of LiMo's console, and never
edited by hand: the test
`phone_answers::the_phone_requests_answers_are_written_for_the_screens_in_every_state`
in `console/src/phone_answers.rs`. It runs with the console's other tests
(`cargo test --no-default-features` in `console/`, on Windows), or alone:

```
cargo test --no-default-features phone_answers
```

These are the answers the console's window gives itself, for adding a
phone (`phone.look`, `phone.install`, `phone.pair`, `phone.debugging_off`,
`phone.done`), as against the door's, which are in `../console-answers/`.
The test plays limo-cable's fake phone through the console's real code and
writes each answer as the page gets it; a few moments too short for the
fake phone to hold (looking, connecting, two phones, sending, installing, an
install whose connection ended, asking the service, handing, switching
off, USB debugging still on) come from the same answer code, given that
moment's state. `console/README.md`, "Adding a phone", says what every part
of an answer holds.

Each file holds:

- `request`: the request's name, such as `phone.look`.
- `case`: in a few words, what the file shows.
- `asked`: the request as the page sent it, its `name` and its `args`.
- `answer`: what the request resolved with, or `error`: what it was
  rejected with, `{ "code", "words" }`.

A run with no change writes no file. What would differ from run to run is
a fixed sample, the door's own where it has one: the key's fingerprint as
the drawing has it, the child `child-0000000000000001`, the phone
`device-0000000000000002`, named "Asha’s phone". No file holds an offer, a
key, a password or anything of the PC the test ran on, and the test checks
every file for them before it writes. A `.json` file here that a run did
not write is removed; this README is left as it is.
