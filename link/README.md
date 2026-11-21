# limo-link

The link of LiMo: the encrypted conversation between a phone and the
family's master PC, on the home network. There is no server anywhere. A
phone talks to one PC, the one it paired with, and to nothing else.

**What it is.** One Rust library that holds everything about the link
except the sockets: the frames, the two handshakes and the session after
them as state machines, the messages, the pairing offer and its text form,
the names the master announces on the network, and the timing numbers. Two
callers use it: the Windows service on the master PC (directly, later) and
the phone's Kotlin (through a wrapper crate, later).

**What it is not.** It has no sockets, no threads, no clocks and no
discovery calls in it. The callers own all of those: they connect, read,
write, close, announce and search, and they hand the library the bytes that
came and the time. It keeps nothing on disk: what a phone must keep after
pairing it hands back as bytes for the phone to keep, and the master's
records (its offers, its phones and their keys) are its caller's. It has no
unsafe code. It carries the core's `Report` and `Grant` and does not look
inside them, except to check that a report or a grant is for the session's
own device.

The design rests on what proof four showed on real hardware: Android's
`NsdManager` and `mdns-sd` on the PC find each other in about a quarter of a
second, a withdrawn announcement is seen within a second, sockets go through
the Wi-Fi network's own socket factory, and the installer needs a narrow
firewall rule. Encryption, pairing and identity are this crate's.

## Contents

- [How each side calls it](#how-each-side-calls-it)
- [Frames](#frames)
- [The handshakes](#the-handshakes)
- [What the handshakes promise](#what-the-handshakes-promise)
- [The pairing offer](#the-pairing-offer)
- [The messages](#the-messages)
- [Versions](#versions)
- [Limits and timings](#limits-and-timings)
- [Discovery](#discovery)
- [Secrets](#secrets)
- [The dev master](#the-dev-master)
- [Left open](#left-open)
- [What only real hardware can show](#what-only-real-hardware-can-show)
- [What it costs](#what-it-costs)
- [Tests](#tests)
- [Layout](#layout)
- [Dependencies and licences](#dependencies-and-licences)

## How each side calls it

Every state machine works the same way. The caller writes what
`take_outgoing()` gives, feeds every byte it reads into `receive(bytes,
now)`, and calls `tick(now)` at the time the last `tick` returned (or
sooner: after every `receive` is simplest). `now` is a [`Now`](src/lib.rs):
a steady count of milliseconds that never goes back (Android's
`SystemClock.elapsedRealtime()`, or the time since the PC started) for every
limit, and the wall clock in unix seconds, which only the master reads.

The rule for errors is by call. An error from `receive` or `tick` ends the
link: every later call returns `LinkError::Closed`, and the caller closes
the socket. An error from a call that sends (a message too long, a report
for another device, sending before the link is open) was refused before
anything was written, and changes nothing.

**The phone, pairing once** (the offer came over the USB cable, or from a
QR code):

```rust
let offer = Offer::from_text(&text)?;
let request = PairRequest { name: "Asha's phone".into(), form: Form::Child,
                            platform: "android".into(), android_version: Some(33) };
let mut pairing = PhonePairing::start(&offer, request, now)?;  // makes the phone's key pair
// connect to offer.addresses() and offer.port(), or what discovery found
write(pairing.take_outgoing());
loop {
    if let Some(welcomed) = pairing.receive(&read(), now)? {
        write(pairing.take_outgoing());           // the goodbye; then close
        keep_privately(&welcomed.paired.to_bytes()?);   // holds the phone's private key
        let mut state = DeviceState::from_pairing(&welcomed.welcome, now.unix);
        state.set_code_secret(welcomed.code_secret.expose().to_vec())?;
        save(&state);
        break;
    }
    write(pairing.take_outgoing());
}
```

**The phone, a session whenever the master is in reach:**

```rust
let paired = PairedMaster::from_bytes(&kept)?;
let mut session = PhoneSession::start(&paired, now)?;
write(session.take_outgoing());
loop {
    match session.receive(&read_or_wait_until(next_tick), now) {
        Ok(events) => for event in events {
            match event {
                PhoneEvent::Opened => session.send_report(&state.report(now.unix, want_more), now)?,
                PhoneEvent::Grant { grant, .. } => { state.take_grant(&grant, now.unix); save(&state); }
                PhoneEvent::ReportNow => session.send_report(&state.report(now.unix, false), now)?,
                PhoneEvent::Pong { master_now } => note_the_masters_clock(master_now),
            }
        },
        Err(_) => { if session.report_waiting() { state.lost_contact(); } close(); break; }
    }
    // a tick ending the session with NoAnswer { report_waiting: true } means
    // the same: state.lost_contact()
    next_tick = session.tick(now)?;
    write(session.take_outgoing());
}
// leaving (Wi-Fi off, the app stopping): session.send_bye(now), write, close
```

**The master, for every connection a phone makes:**

```rust
let mut link = MasterLink::accept(&master_keys, now);
loop {
    for event in link.receive(&read(), now, &directory)? {   // directory: its offers and phones
        match event {
            MasterEvent::PairRequest { offer_id, phone_key, request } => {
                // in one step: refuse a used offer, pair with the offer's child,
                // mark the offer used, keep phone_key with the new device
                let pairing = hub.pair_device(&child, &request.name, DeviceKind::Portable, &request.platform, now.unix)?;
                // with the child's name, for the phone's screens (or welcome(), with none)
                link.welcome_child(&pairing.welcome, &pairing.code_secret, &child_name, now)?;
            }
            MasterEvent::Opened { device_id, .. } => replace_any_older_session_of(device_id),
            MasterEvent::Report { report } => link.grant(&hub.handle_report(&report, now.unix)?, now)?,
            MasterEvent::Apps { apps, hash } => keep_apps(apps, hash),
            MasterEvent::Bye => break,
        }
    }
    // for the devices in hub.wanted_reports(now): link.report_now()
    next_tick = link.tick(now)?;
    write(link.take_outgoing());
    if link.is_ended() { close(); break; }
}
```

The `Directory` trait is the master's records as the link needs them: an
offer by its id (with whether it was used), and every phone it paired (with
its key, and whether it was removed). `DirectoryList` is the plainest one.
When a hub call refuses a report (a removed or unknown device), the master
answers with `link.refuse(Refusal::Removed)` or `Refusal::Unknown`.

## Frames

Everything on the wire is a frame: 4 bytes of length, big endian, then that
many bytes. The length is checked against what the step allows the moment
its 4 bytes are in, before anything after it is kept.

| Frame | Who | Body | Bytes |
| --- | --- | --- | --- |
| first, pairing | phone | `LiMo`, the version (1 byte), `P`, the offer's id (8 bytes), handshake message 1 | 62 |
| first, session | phone | `LiMo`, the version, `S`, handshake message 1 | 54 |
| pairing reply | master | handshake message 2 | 96 |
| pairing finish | phone | handshake message 3 | 64 |
| session reply | master | handshake message 2 | 48 |
| a message | either | one message, in pieces | 17 to 1,048,865 |

The start of the first frame (the preamble) is in the clear, so the master
knows which handshake to run and, for a pairing, which offer is meant. It is
also the handshake's prologue: a preamble changed on the way makes the
handshake fail (Noise specification, section 6).

**A message in pieces.** A Noise message is at most 65535 bytes (section 3),
and a message here may be up to 1 MiB of JSON. So a message is cut into
pieces of up to 65518 bytes of text; each piece is a flag byte (1 on the last
piece, 0 on the others) and its text, encrypted as one Noise message: the
tag adds 16 bytes, so a full piece is exactly 65535 bytes. Every piece but
the last is full, so the frame's length alone says where each piece ends,
and a length no whole pieces can have is refused at once. A message cut
short at a piece boundary, with its length changed to match, ends on a
piece whose flag is 0, and is refused: the specification leaves noticing a
cut stream to the application (section 13, "Session termination").

## The handshakes

Both run on the `snow` crate (version 0.10.0), with X25519, ChaCha20 and
Poly1305, and BLAKE2s.

**Pairing: `Noise_XXpsk3_25519_ChaChaPoly_BLAKE2s`**, with the offer's 32
byte secret as the pre-shared key:

```text
-> e                     phone: its ephemeral key
<- e, ee, s, es          master: its ephemeral key, and its static key, encrypted
-> s, se, psk            phone: its static key, encrypted; then the secret is mixed in
```

The phone checks that the master's static key is the one the offer names,
and stops if it is not. Then, encrypted: the phone's `pair`, the master's
`welcome`, the phone's `bye`. The connection then closes; the phone has
what it needs for every session after it.

**Every session after it: `Noise_KK_25519_ChaChaPoly_BLAKE2s`**, both static
keys known to both sides in advance:

```text
-> s                     (known: the phone's key, which the master kept)
<- s                     (known: the master's key, from the offer)
...
-> e, es, ss             phone
<- e, ee, se             master
```

KK names nobody in the clear, so the master tries the first message against
each phone it knows until one decrypts (a family has a few phones; each try
costs one X25519 and some hashing). Then the phone sends a `ping` at once,
and the master takes the session as live only when that first message
decrypts: until then, a first message played again by someone on the
network gets an answer that leads nowhere.

Every handshake payload is empty, so each handshake message has one size,
and a wrong size is refused from the frame's length. snow's checks were
read in its source (`builder.rs`, `handshakestate.rs`,
`transportstate.rs`, `cipherstate.rs`): a handshake or transport message
that does not decrypt is an error and changes no state; transport nonces
count up from zero in each direction and never repeat.

## What the handshakes promise

Read in the Noise Protocol Framework, revision 34 (2018), sections 7.5
(the patterns), 7.7 (the payload security table), 7.8 (identity hiding), 9
(pre-shared keys), 12.1 (the 25519 functions), 13 and 14 (what is left to
the application), and in snow's source and documentation.

**Pairing promises:**

- Only someone holding the offer can pair. The secret is mixed into the
  keys (section 9), so the phone's `pair` and everything after it decrypt
  only for someone who has it. Someone on the network who never saw the
  offer cannot finish the handshake, and cannot use up the offer.
- The phone pairs only with the PC the offer names. The master's static key
  arrives encrypted in the reply, and the phone compares it with the
  offer's; section 14 ("Authentication") leaves exactly this check to the
  application. An impostor that holds the whole offer, secret included, but
  not the master's private key, is refused by the phone before it says
  anything about itself (test `the_phone_accepts_only_the_master_the_offer_names`).
- The master learns the phone's static key encrypted, and bound to the
  offer by the secret; XX sends the initiator's key "encrypted with forward
  secrecy to an authenticated party" (section 7.8, property 8).
- What follows the handshake (the `welcome` with the phone's code secret)
  has strong forward secrecy (section 7.7, XX transport messages,
  destination property 5): its keys come from both sides' ephemeral keys
  too. The table lists XX; the pre-shared key only adds to it.

**Pairing does not promise:**

- That the offer stays secret: whoever holds it within its ten minutes can
  pair one phone. That is why the child's phone gets it over the cable, and
  why a guardian should show a QR code only while it is being scanned. The
  real phone then hears that the offer was used, and the guardian sees the
  extra device.
- To hide the master's public key: anyone on the network can start a
  pairing handshake and be sent it (section 7.8, property 1: "can be probed
  by an anonymous initiator"). It is not secret; the offer and the master id
  carry it anyway.
- To tell a wrong secret from bytes damaged on the way: both are "something
  did not decrypt".

**A session promises:**

- Only the phone the master paired, holding its private key, can open a
  session as that phone, and only the master, holding its private key, can
  answer it (section 7.7, KK: every transport message has source property 2
  and destination property 5).
- A report is taken only for the session's own device and child, whatever
  it says; a phone that sends another's is refused and the session ends.
  The phone's side will not send one, and the master's will not send a
  grant for another device.
- Strong forward secrecy for everything after the phone's first message,
  both ways. Before it, the master's messages have only weak forward
  secrecy (section 7.5: "the responder is only guaranteed weak forward
  secrecy for the transport messages it sends until it receives a transport
  message from the initiator"). Here the master sends nothing before it but
  its handshake reply and, at most, a refusal, which holds no secret; and
  the phone's first message comes at once.
- Neither static key is ever sent.

**A session does not promise:**

- That the first message is fresh: it can be played again (section 7.7, KK
  first message, destination property 2: "vulnerable to replay"). The
  master answers it, but takes nothing from it, and a replay never opens a
  session (test `a_replayed_session_start_never_opens_a_session`).
- To hide who connects from someone who later steals the master's private
  key: with it and a phone's public key, they can check from a recording
  which phone made a session (section 7.8, KK, property 5). They still read
  nothing.

**Someone who records everything, and later steals a key.** They cannot read
any pairing or session they recorded, whichever private key they steal (the
phone's, the master's, or both) and with the offer's secret too: every
message after a handshake is encrypted with keys that also come from the
ephemeral keys, which are gone. What they see without stealing anything:
when each side talked, from which address, how big each message was (the
messages are not padded; section 13 recommends padding, and a report's size
says roughly how much use it carries), the preamble (the link version,
pairing or session, an offer's id), and that LiMo is talking.

**Someone who steals a private key and uses it from then on** can be its
owner: with a phone's key, they can report as that phone; with the
master's, they can answer every phone as the master (KK's first message is
open to key compromise impersonation, section 7.7, source property 1, but
the session after it is not: a stolen master key does not let anyone pose
as a phone). So both keys live in private storage only (Android's private
app storage, the hub's protected file). Removing a phone makes the master
refuse its key; a stolen master key needs every phone paired again with a
new one.

**Order and replay.** Each direction's messages carry nonces counting from
zero, and TCP keeps them in order. A frame played again, reordered, dropped
from the middle, or taken from another session does not decrypt, and the
link ends (tests in `tests/link/hostile.rs`). The end of a session can be
cut by anyone on the path (they can close the TCP connection); that costs
nothing, since every message is a level with a number and the phone simply
connects again.

**Invalid public keys.** As section 12.1 asks of the 25519 functions, an
invalid public key gives a Diffie Hellman output of all zeros, not an error
("discouraged because it adds complexity and implementation variance, and
does not improve security"). Both static keys are pinned, and the pairing
is keyed by the secret, so an attacker gains nothing from one.

**The resolver.** snow runs the handshakes with this crate's own resolver
(`src/crypto.rs`) over the same pure Rust crates snow's default resolver
uses: `curve25519-dalek`, `chacha20poly1305` and `blake2`, and the
operating system's random numbers through `getrandom`. The one difference is
the reason for it: snow's own resolver never wipes the keys it holds, and
this one wipes every private key (static and ephemeral), every cipher key
and every hash state when it is dropped. The test
`the_resolver_matches_snows_own_byte_for_byte` runs both handshakes and
messages each way through both resolvers with the same keys, in every
pairing of the two, and checks every byte crossed is the same.

## The pairing offer

What the master hands a phone so that it can pair: the master's public key,
a random 8 byte id, a random 32 byte secret, when it expires (10 minutes
after it was made, by the master's clock), the port, and up to 8 of the
master's IPv4 addresses on the home network. It works once.

Its text form is `limo1:` and the base32 (RFC 4648, uppercase, no padding)
of these bytes, about 150 characters with two addresses:

| Bytes | What |
| --- | --- |
| 32 | the master's public key |
| 8 | the offer's id |
| 32 | the secret |
| 8 | when it expires, unix seconds, big endian |
| 2 | the port, big endian |
| 1 | how many addresses follow, 0 to 8 |
| 4 each | the IPv4 addresses |

Reading it ignores spaces and line ends around it, takes the base32 in
either case, and refuses anything else with a reason that never quotes the
text: not an offer, made by a newer LiMo (`limo2:` and on), a character
base32 does not have (and where), cut short or run on, too many addresses,
or port 0. The base32 is all capitals and digits, which a QR code holds in
its compact alphanumeric mode.

The master id is not in the offer: it is made from the master's key (see
[Discovery](#discovery)), so the two can never disagree. The master decides
whether an offer has expired, by its own clock; a phone's clock may be
wrong, so the phone never refuses an offer for its age (`is_expired` is
there for a phone to say an offer looks old).

**On the master,** an offer is refused at the end of the handshake, once the
phone has shown it holds the secret, so the phone can say why: used
(`offer_used`) or expired (`offer_expired`). An offer the master does not
have gets no answer at all. Two phones racing with one offer: the master's
caller marks it used in the same step as pairing, and refuses the second.

## The messages

JSON inside the encryption, each named by its `say` field. A message with a
field it should not have, without one it must have (`android_version` may
be left out), or of the other side's kind, ends the link. The JSON reader's
errors say only where the reading stopped.

| Phone to master | |
| --- | --- |
| `{"say":"pair","name":...,"form":"child","platform":"android","android_version":33}` | On a pairing link, once. The name is 1 to 120 characters, the platform 1 to 40. |
| `{"say":"report","report":{...}}` | The core's `Report`. Answered by `grant`. |
| `{"say":"apps","apps":[{"package":...,"label":...}],"hash":...}` | The phone's apps, at most 2,000; a package is 1 to 255 bytes, a label at most 120 characters. The hash is checked. Not answered. |
| `{"say":"ping"}` | The first message of every session, and after any minute with nothing else sent. Answered by `pong`. |
| `{"say":"bye"}` | The phone is leaving. |

| Master to phone | |
| --- | --- |
| `{"say":"welcome","welcome":{...},"code_secret":"<64 hex>","master_now":...,"child_name":"Asha"}` | The answer to `pair`: the core's first `Grant`, the device's code secret, the master's clock, and the child's name (1 to 120 characters) for the phone's own screens. A master may leave the name out (`MasterLink::welcome`); `MasterLink::welcome_child` gives it. The phone refuses a name longer than 120 characters. |
| `{"say":"grant","grant":{...},"master_now":...}` | The answer to a report. |
| `{"say":"report_now"}` | Report at once: the hub's wanted reports. |
| `{"say":"refused","why":...,"speaks":1}` | No, and the link ends. `why` is `removed`, `unknown`, `other_device`, `version`, `offer_used`, `offer_expired` or `not_supported`; `speaks` is the link version the master speaks. |
| `{"say":"pong","master_now":...}` | The answer to `ping`. |

The app list's hash is BLAKE2s over the label `LiMo apps v1`, a zero byte,
and each app's package and label, each as its length (4 bytes, big endian)
and its UTF-8; its first 16 bytes, as 32 hex characters. The master can keep
it and see at a glance whether a later list changed.

A phone answers only what it asked: a `grant` with no report waiting, or a
`pong` with no ping waiting, ends the session. The answers may come in any
order between the two kinds.

## Versions

The link version (now 1) is in the clear at the start of the first frame,
in the handshake's prologue, and in the `v` TXT record. Every later version
keeps the frames, the handshakes and the `refused` message exactly as they
are, and may add messages (guardian messages, say). A master that meets a
phone speaking a version it does not speak still finishes the handshake
(the crypto is the same), then refuses it with `version` and the version
it speaks, so the phone can tell its family to update LiMo on the PC. A
version of 0 is refused before any handshake. Changing the version in the
preamble on the way makes the handshake fail, since the preamble is the
prologue.

## Limits and timings

| Name | Value | Meaning |
| --- | --- | --- |
| `MAX_MESSAGE_BYTES` | 1 MiB | the longest message, in bytes of JSON |
| `frame::MAX_FRAME_BYTES` | 1,048,865 | the longest frame body: the longest message in 17 pieces |
| `HANDSHAKE_LIMIT_MS` | 5 s | from the connection to a finished handshake; on the master, including the phone's first message |
| `ANSWER_LIMIT_MS` | 10 s | a report, a ping or a pairing request unanswered this long ends the link; for a report, the phone takes it that the master cannot be reached |
| `PING_EVERY_MS` | 60 s | the phone pings when it sent nothing for this long |
| `SILENCE_LIMIT_MS` | 180 s | the master ends a session it heard nothing on for this long |
| `MAX_WAITING` | 8 | the most reports and pings a phone has waiting for answers |
| `OFFER_LIFETIME_SECS` | 600 | an offer's life, by the master's clock |
| `DEFAULT_PORT` | 47810 | the master's TCP port unless changed |

One session while in reach: a phone that connects again (its Wi-Fi came
back) starts a new session, and the master's caller replaces the older one
when the new one's `Opened` comes, not before. After a pairing's `welcome`,
the master's link is done; the phone's `bye` is a courtesy, and the link
ends quietly 10 seconds later without it.

## Discovery

Names and values only; the callers announce (`mdns-sd`) and search
(`NsdManager`).

| Item | Value |
| --- | --- |
| Service type | `_limo._tcp.local.` (`NSD_SERVICE_TYPE`, `_limo._tcp`, for Android) |
| Host name | `limo-<first 8 hex of the master id>.local.` |
| Instance name | `LiMo <first 8 hex of the master id>` |
| Port | 47810 unless changed |
| TXT | `v=1` and `id=<the master id, 16 hex>` |

The master id is the first 8 bytes of BLAKE2s over the label
`LiMo master id v1`, a zero byte and the master's public key. A phone holds
the key, so it knows its master's announcement before it connects
(`read_txt` reads the TXT records, keys in any case, records it does not
know ignored). The PC's own name is never announced.

## Secrets

The secrets are the master's and each phone's private keys, an offer's
secret, and the code secret a `welcome` carries.

- **Never shown.** No secret is in any `Debug` output (`KeyPair`, `Offer`,
  `CodeSecret`, `Welcomed`, `PairedMaster`, every link and event), in any
  error, or in the clear on the wire. The tests look for each secret in hex
  either case, base32 and Rust's own way of printing bytes, in every `Debug`
  output and every error the hostile runs give, and in every byte of a
  whole pairing and session (`tests/link/secrets.rs`). The phone's and the
  master's public keys are not on the wire in the clear either.
- **Wiped when dropped.** The crate's own copies: `KeyPair`, `Offer` and
  `CodeSecret` hold their secrets in memory that is wiped when dropped, and
  so does the offer's text. Message text is written into a buffer that, when
  it must grow, wipes the old one before letting it go; a message read is
  decrypted into a buffer made once at its full size, wiped when dropped.
  The JSON reader reads the code secret and a saved private key straight
  from that buffer into wiped memory. In snow, the resolver here wipes every
  private key, cipher key and hash state.
- **Not wiped.** What snow keeps itself while a handshake runs (the chaining
  key, the handshake hash, its own copy of the offer's secret), the copies
  the crypto crates make on the stack, a passing copy the JSON reader makes
  of a string with an escape in it, and the core's PIN record inside a
  `Grant` (a salted slow hash). And what the callers hold: the bytes of
  `PairedMaster::to_bytes`, which contain the phone's private key on
  purpose, belong in private storage.

## The dev master

With the `dev` feature, `limo_link::dev::DevMaster` plays the master over a
real family file in a folder the caller gives (a temporary one): it keeps
its offers and the phones it paired in memory, pairs a phone with the hub
(`Hub::pair_device`) and names the child in its welcome, answers reports with the hub's grants
(`Hub::handle_report`), refuses a removed or unknown device as the hub
says, keeps app lists, and asks a phone to report when the hub wants it
(`Hub::wanted_reports`). A phone's side can be tested against it with no PC
service; the stories here do exactly that. `cargo run --example
limo-link-dev` plays a pairing and a short session, all in memory, and
prints what crosses and never a secret.

## Left open

- **IPv6.** Offers carry IPv4 addresses only, as the master listens and
  announces on IPv4 only (proof four).
- **Routers that keep devices apart.** Client isolation, usual on guest
  networks, blocks a phone from the PC entirely; nothing on the phone can
  get around it. The phone then lives on its away time, and its setup
  screens should say so.
- **A guardian's phone.** The `form` is in `pair`, and the master here
  refuses a guardian form with `not_supported`. Pairing one, its rights,
  and the messages it needs come with a later version of the link.
- **Where the master keeps link keys.** The master's own key pair, each
  phone's key and the offers belong in the hub's file (a later piece). Until
  then the dev master keeps them in memory.
- **Padding**, so that message sizes say less.
- **Rekeying.** Not done: nonces last 2^64 messages, and a session lasts
  hours.
- **A changed master key** (a new PC): every phone pairs again.

## What only real hardware can show

- That a phone on Wi-Fi, through the network's own socket factory, and the
  Windows service carry these frames end to end, and that the firewall rule
  the installer adds lets them through.
- The handshake times on a real phone: the X25519 and the trial against
  each known phone are measured here on a desktop only.
- The timings in real life: whether 5 seconds is enough for a handshake on
  a busy home network, whether a ping a minute keeps a session alive
  through a phone's power saving, and whether 180 seconds of silence is the
  right time to give up on one.
- What a phone's Wi-Fi does with a connection while its screen is off, and
  how often sessions break and start again in a day.
- Proof four's limits stay: one phone model, one router, one PC.

## What it costs

Measured on the development PC (an AMD Ryzen 9 7950X3D), release build,
with `cargo test --release --test link cost -- --ignored --nocapture`:

| What | Time |
| --- | --- |
| a whole pairing, both sides, with the hub's work | 6.5 ms |
| a session handshake, the phone known alone | 0.4 ms |
| the same, the phone last of 10 known | 1.4 ms |
| the same, the phone last of 50 known | 5.9 ms |
| an app list in a frame of 773 KiB, sealed, opened and read | 8.7 ms |

## Tests

`cargo test` runs three groups:

| Where | What | Count |
| --- | --- | --- |
| `src/*.rs` | Unit tests: hex and base32 with the known answers of RFC 4648; X25519 with those of RFC 7748; BLAKE2s with RFC 7693's; this crate's resolver against snow's own, byte for byte, both protocols, every pairing of the two, and a rekey; the handshake message sizes; the cipher refusing short, altered and keyless text without a panic; the frame limits, piece counts, lengths refused from four bytes, frames from any pieces; preambles; offers to text and back, with 0, 1, 2 and 8 addresses, and every broken form refused without quoting it; ids and secrets never shared; the master id's known answer (worked out with Python's `hashlib`) and TXT records; the app list hash's known answers; every message to JSON and back; unknown and misshapen messages; bounds; key pairs saved and read again, and refused when they do not fit; secrets out of `Debug` | 32 |
| `tests/link/` | The stories, both ends in memory with the dev master over a real family file. Pairing: a whole pairing, frame by frame; an offer used twice, an offer expired (and one at its last second), a wrong secret, an offer naming another key, an impostor that holds the whole offer, an offer from another master, a guardian's phone, two phones racing for one offer, a phone of a newer version, and a phone written by hand on snow from this README's description. Sessions: opening with a ping, reports and grants, prods, apps, goodbyes, a phone with another key, a phone that pinned another master, a removed phone (before and during its session), a forgotten one, a report for another device (refused by both sides), a grant for another device or nobody, every timing (a report unanswered, a quiet minute's ping, three minutes of silence, a handshake too slow on both sides), a replayed start, a newer version, a new session taking over. A whole day: pairing at 07:00, use and a report every two minutes, a goodbye, school out of reach, home again with the school's slices, a lock from the master and its lift arriving by prods, the evening until the day is used up, the master's count equal to the phone's at every exchange; and a PC asleep mid exchange. The welcome naming the child, or not, and a name empty or too long refused before anything is sent. Hostile: every frame of a whole pairing and a whole session, both ways, garbled in nine places, cut, lengthened, shortened, said to be 4 GiB, replayed, swapped, dropped, replaced with junk and followed by a stray byte (252 runs), each ending the link or the clock, never bringing anything an honest run does not, and refusing everything after; lengths refused from four bytes at every step; a message cut at a piece boundary; a flag that is neither; a long message in seven pieces; a message too long to send. Random: random bytes at every step on both sides, random changes to whole conversations, and random JSON inside good encryption both ways (written by hand on snow), never a panic. Secrets: never in `Debug`, errors or the wire. One more, not run by default, measures the costs above | 42, and 1 |
| `tests/house_rules.rs` | No unsafe code, no attribute that silences a lint, no dash punctuation in anything a person reads, no machine paths | 4 |

`LIMO_LINK_RANDOM=<n> cargo test --test link random` runs the random tests
n times longer. `cargo build`, `cargo test`, `cargo clippy --all-targets`
and `cargo doc --no-deps` pass with no warnings, and the library builds for
Android too (`cargo check --lib --target aarch64-linux-android`). Dependencies are built
optimised in debug and test builds (the hub's slow hashes run in the dev
master), and this crate with a little; overflow checks stay on.

## Layout

```
link/
  Cargo.toml, Cargo.lock, README.md, .gitignore
  src/
    lib.rs        crate notes, the public names, the version, the timings, Now
    crypto.rs     the two Noise protocols, the resolver, the handshake sizes
    frame.rs      frames, the preamble, messages in pieces
    keys.rs       KeyPair and PublicKey
    offer.rs      the pairing offer and its text form
    discovery.rs  the master id, the names and the TXT records
    messages.rs   what each side says, the app list hash, the code secret
    phone.rs      PhonePairing, PhoneSession, PairedMaster
    master.rs     MasterLink, Directory
    encoding.rs   hex and base32
    error.rs      LinkError
    dev.rs        the dev master (feature `dev`)
  tests/
    link/         the stories; raw.rs is the phone and master written by
                  hand on snow, wire.rs the wire that changes frames
    house_rules.rs
  examples/
    limo-link-dev.rs
```

## Dependencies and licences

The library, on Windows and on Android:

| Crate | Version | Licence | Used for |
| --- | --- | --- | --- |
| limo-core | 0.1.0 (path) | Apache-2.0 | the `Report` and `Grant` the link carries |
| snow | 0.10.0 | Apache-2.0 or MIT | the Noise handshakes and transport; no features at all |
| curve25519-dalek | 5.0.0 | BSD-3-Clause | X25519 (default features off; `zeroize`) |
| chacha20poly1305 | 0.11.0 | Apache-2.0 or MIT | ChaCha20 with Poly1305 (default features off; `zeroize`) |
| blake2 | 0.11.0 | MIT or Apache-2.0 | BLAKE2s (default features off; `zeroize`) |
| zeroize | 1.9.2 | Apache-2.0 or MIT | wiping secrets and buffers |
| getrandom | 0.4.3 | MIT or Apache-2.0 | keys, offer ids and secrets from the operating system, as in the hub |
| serde, serde_core, serde_derive | 1.0.229 | MIT or Apache-2.0 | the messages and what a phone keeps |
| serde_json | 1.0.154 | MIT or Apache-2.0 | the messages as JSON |
| subtle | 2.6.1 | BSD-3-Clause | used by snow and curve25519-dalek |
| aead, chacha20, cipher, poly1305, universal-hash, inout | 0.6.1, 0.10.2, 0.5.2, 0.9.1, 0.6.1, 0.2.6 | MIT or Apache-2.0 | parts of chacha20poly1305 |
| digest, crypto-common, block-buffer, hybrid-array, typenum, ctutils, cmov, cpufeatures, cfg-if | 0.11.3, 0.2.2, 0.12.1, 0.4.15, 1.20.1, 0.4.8, 0.5.4, 0.3.1, 1.0.5 | MIT or Apache-2.0 | parts of the RustCrypto crates above, shared with the core |
| hmac, sha2, pbkdf2 | 0.13.0, 0.11.1, 0.13.0 | MIT or Apache-2.0 | the core's |
| itoa, memchr, zmij | 1.0.19, 2.8.3, 1.0.23 | MIT or Apache-2.0; Unlicense or MIT; MIT | parts of serde_json |
| libc | 0.2.191 | MIT or Apache-2.0 | Android and Linux only, used by getrandom and cpufeatures |
| curve25519-dalek-derive, syn | 0.1.1, 2.0.119 | MIT or Apache-2.0 | when building on x86_64 only, for curve25519-dalek; nothing of them is in the library |
| proc-macro2, quote, syn, unicode-ident | 1.0.107, 1.0.47, 3.0.10, 1.0.27 | MIT or Apache-2.0 (unicode-ident also Unicode-3.0) | used by serde_derive when building |
| rustc_version, semver | 0.4.1, 1.0.28 | MIT or Apache-2.0 | used by the build scripts of snow and curve25519-dalek |

Tests and the dev master only:

| Crate | Version | Licence | Used for |
| --- | --- | --- | --- |
| limo-hub | 0.1.0 (path) | Apache-2.0 | the dev master's family file (feature `dev`), with its own crates, listed in its README |
| tempfile, fastrand, once_cell, windows-sys, windows-link, rustix, linux-raw-sys, errno | 3.27.0, 2.5.0, 1.21.4, 0.61.2, 0.2.1, 1.1.6, 0.12.1, 0.3.15 | MIT or Apache-2.0 (rustix and linux-raw-sys also Apache-2.0 with LLVM exception) | temporary folders |
| curve25519-dalek, chacha20poly1305, chacha20, poly1305, aead, cipher, universal-hash, inout, blake2, digest, crypto-common, block-buffer, cpufeatures, opaque-debug | 4.1.3, 0.10.1, 0.9.1, 0.8.0, 0.5.2, 0.4.4, 0.5.1, 0.1.4, 0.10.6, 0.10.7, 0.1.7, 0.10.4, 0.2.17, 0.3.1 | MIT or Apache-2.0 (curve25519-dalek BSD-3-Clause) | snow's own default resolver, to check this crate's against it |
| generic-array, version_check | 0.14.7, 0.9.5 | MIT; MIT or Apache-2.0 | used by the crates above |

`Cargo.lock` also names fiat-crypto (MIT or Apache-2.0 or BSD-1-Clause), a
backend of curve25519-dalek that is only built when asked for by a build
setting, and r-efi (MIT or Apache-2.0 or LGPL-2.1-or-later, taken as MIT or
Apache-2.0), used by getrandom on UEFI only; neither is built for Windows
or Android. Nothing is GPL or AGPL. Two licences here are not MIT or
Apache-2.0: BSD-3-Clause, for curve25519-dalek and subtle, which asks that
their copyright notices go with the program. The code here was written for LiMo;
nothing was copied. The crate is under the Apache License 2.0, as is all of
LiMo.
