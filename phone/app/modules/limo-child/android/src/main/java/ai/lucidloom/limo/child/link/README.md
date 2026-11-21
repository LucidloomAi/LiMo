# link

The phone's half of the link to the family's PC: the cable door that takes
a pairing offer over the USB cable, pairing once, and a session whenever the
PC is in reach, through the cable or over the home Wi-Fi. Kotlin, package
`ai.lucidloom.limo.child.link`, in the child module. The frames, the
handshakes, the encryption and the messages are the Rust link's (`link/`
at the top of the repository, `limo-link`), reached through the bridge (`phone/ffi`: `Offer`, `PhonePairing`, `PairedMaster`,
`PhoneSession`); this folder owns the sockets, the threads and the clocks.

**What it is not.** It decides nothing: the engine does. A phone out of
reach keeps to its last grant and its away share, as the core says; the
link only tells the engine whether the master is in reach, hands it the
master's grants and asks, and carries its reports. It holds no rule, no
word and no time of its own, and it never listens on any port.

## Contents

- [The cable door](#the-cable-door)
- [Pairing](#pairing)
- [Finding the master](#finding-the-master)
- [A session](#a-session)
- [Never too often](#never-too-often)
- [Removed on the PC](#removed-on-the-pc)
- [The phone talks to the family's PC, and to nothing else](#the-phone-talks-to-the-familys-pc-and-to-nothing-else)
- [What it logs](#what-it-logs)
- [Tests](#tests)
- [What only a phone can prove](#what-only-a-phone-can-prove)
- [Left open](#left-open)
- [Layout](#layout)

## The cable door

`CableDoor`, a content provider in the child form's main manifest, so in
release builds too:

| | |
| --- | --- |
| Where | `content://ai.lucidloom.limo.child.cable/offer` |
| Guard | exported, `android:permission="android.permission.DUMP"`, which only the adb shell and the system hold: no app on the phone can open it. The door checks the caller holds it again (`checkCallingPermission`, as the cable proof did on the test phone) |
| How the PC writes | `content write --uri content://ai.lucidloom.limo.child.cable/offer`, the offer's `limo1:` text on standard input: never in a command line |
| What it takes | one write, at most 1,024 bytes (`LinkCore.MAX_OFFER_BYTES`), through a pipe into memory, never a file; the bytes are wiped once the bridge read them (`Offer.fromText`) |
| What it refuses before reading | any path but `/offer`; a mode that reads; a caller without `DUMP`; any offer while the phone is paired (no PC with the cable may pair a child's phone again with another family: forgetting goes through the phone); a second offer while the first is still being paired with. Each throws, so `content write` fails and the PC knows |
| What it refuses after reading | nothing written, more than 1,024 bytes, text that is not an offer |
| Anything else | no query, insert, update or delete; it reads no file |

An offer is held for ten minutes at most (its life on the PC), then dropped.
Once a pairing with it ended (paired, or refused by the PC), the door takes
a new one.

## Pairing

From the offer, through the bridge, with the phone's own name ("Galaxy S20
Ultra", from Android's Settings, else its model, cleaned and cut to 40
characters, `PhoneName`) and Android's API level:

1. **The way.** For its first ten seconds the offer is tried over the
   cable only: a connection to the phone's own loopback, 127.0.0.1, at the
   offer's port, which the PC's `adb reverse` carries to the service on the
   PC. The PC writes the offer a moment before it sets the reverse up, so
   the first tries find nothing there and are made again every second.
   After ten seconds the offer's home network addresses are tried too.
2. **The handshake** with the offer's secret (`PhonePairing`), the request,
   the master's welcome, the goodbye. The phone checks the master's key
   from the offer: nothing on the phone can pose as the PC.
3. **What is kept, before anything else**: the kept pairing
   (`PairedMaster`, which holds the phone's private key) in the app's
   private `files/limo/paired`, written all or nothing with `AtomicFile`;
   then the engine, made in Rust from the welcome and the code secret (the
   secret never reaches Kotlin), handed to the host, which keeps its state
   (`files/limo/state`) before it is used; then the child's name the
   master's welcome gave, which the host cleans and keeps in its own file
   (`files/limo/child.json`, host/README.md, "The child's name"). If the
   state cannot be kept, the kept pairing is taken back and the phone is
   as it was.
4. **Then** the keeper starts (the foreground service and the covers), and
   the host tells the screens the phone is paired, so S0 moves on by
   itself; and a session opens at once, through the same cable.

A refusal from the PC (the offer used already, expired, a version it does
not speak) drops the offer; a new one comes from the PC.

## Finding the master

At each try, in this order, one connection at a time:

1. **The cable**: 127.0.0.1 at the kept port. While the cable is in and
   the PC's console holds the reverse, this reaches the service, with no
   home network at all.
2. **The home address that worked last.**
3. **The announcement on the home network**: Android's `NsdManager` looks
   for `_limo._tcp` for half a minute when a try reached nothing (only on a
   Wi-Fi network, and on the same one at most every five minutes); only an
   announcement whose TXT records say `v=1` and the master id of the kept
   pairing is believed, at an IPv4 address of a home network.
4. **The addresses the pairing offer gave.**

**Why the announcement.** The offer's addresses are the PC's addresses on
the day it paired. A home router gives the PC another one after it is
restarted, or when its lease runs out; with those addresses alone the phone
would live on its away time until it was paired again. The service already
announces itself (windows/README.md, "The announcement on the home
network"), and `NsdManager` needs no permission beyond `INTERNET`.

**Through the Wi-Fi network's own sockets.** A connection on the home
network is made with the Wi-Fi network's socket factory
(`Network.getSocketFactory`), so it goes over the Wi-Fi even when Android
has made mobile data the default network (proof four); with no Wi-Fi, the
home network is not tried. When Wi-Fi comes, the link tries at once.

**Nothing on the phone can stand in the way.** A route whose handshake
failed (something else listening on the phone's loopback at the PC's port,
which cannot be the PC: it lacks the master's key) is tried last the next
time.

## A session

`PhoneSession` through the bridge, on the link's own thread ("LiMo link"),
reading at most half a second at a time:

| What comes | What the link does |
| --- | --- |
| `Opened` | in reach: the engine is told at its next tick (at once), with a report asked for, as the PC expects when a session opens; the app list is sent if it changed, and what the phone is |
| a grant | to the host, which hands it to the engine (`takeGrant`), keeps the state before acting on it, and ticks at once |
| `report_now` | a report asked for at once: the host ticks |
| a refusal: `removed` | see [Removed on the PC](#removed-on-the-pc) |
| a refusal: `unknown`, `version`, `other_device` | logged; tried again in ten minutes |
| the connection closed, or no answer in ten seconds | out of reach; when a report was waiting, the engine is told it got none (`lostContact`), so its warnings follow what it holds |

**Reports.** The engine says when (its README, "When it talks to the
master"): the host writes the report only while a session is open, keeps
the state, and hands the bytes to the link, which sends them at once. A
report the link could not take after all is a contact lost.

**The app list.** The apps a child can open, with their labels, from the
watch (`launchableApps`), the home screens and the diallers left out,
sorted by package, at most 2,000, labels cut to 120 characters: sent when
a session opens and every ten minutes while one is open, whenever its hash
(`appsHash`, the link's own) differs from the list sent last in this
process.

**What the phone is.** For the guardian's list of devices on the PC:
`Build.MANUFACTURER`, `Build.MODEL` and `Build.VERSION.SDK_INT` as Android
gives them, and the app's version name (`AndroidAbout`, read once a
process: an update of the app starts a new one). The link cleans the texts
as it cleans a phone's name, puts the maker before the model unless the
model starts with it, and cuts them at 120 and 40 characters (the link's
README, "The messages"). Told once after each session opens, and again
whenever it differs from what the session said last (asked at every round,
which costs a comparison); a call on the PC like the others. An Android
version the bridge refuses (below zero) is logged once as a failure and not
tried again for the same; the session goes on. Never written in the log:
the line says only that it went.

**Closing.** A connection is always closed both ways at once, never its
sending side alone: the cable's tunnel (adb's streams) has no half close,
and bytes still on their way back would be lost (phone/cable/README.md).

## Never too often

- One connection at a time.
- After a try that reached nothing: 5 seconds, doubling to a minute; after a
  refusal the PC will say again: ten minutes. Wi-Fi coming tries at once.
- At most 40 calls on the PC in any minute (sessions opened, reports, app
  lists, what the phone is; the service cuts a phone off past 60): beyond them the host does
  not write a report, and the engine orders it again a moment later.

## Removed on the PC

When the PC says `refused: removed` (a guardian removed the phone, and it
came in reach), the phone releases itself, as the family decided: the kept
pairing is deleted, the host forgets the engine's state and the child's
name, nothing is covered or counted any more, the keeper's service and its
notifications stop, and LiMo's device admin is switched off (an app may
always switch its own off), so LiMo can be removed from the phone. The PC
notes that it was told. A new offer over the cable can pair the phone
again.

## The phone talks to the family's PC, and to nothing else

- **Where.** Only the phone's own loopback (the cable's tunnel to the PC)
  and IPv4 addresses of home networks (10.0.0.0/8, 172.16.0.0/12,
  192.168.0.0/16), checked before every connection (`SocketConn.connect`);
  never a name looked up, never a public address.
- **Which port.** The one the PC gave, in the offer and the kept pairing.
- **What.** Only the link's frames: everything after the first few bytes
  of a connection is encrypted (Noise, the link's README), and the PC is
  known by its key.
- **Never** a server socket, a datagram socket, a web request, a web view
  or an analytics library, anywhere in the child module or the app's
  screens. `LinkHouseRulesTest` reads every Kotlin file of the module, every
  screen of the app and its `package.json`, and fails on any.

## What it logs

Android's log, tag `LiMo`. Never an offer, a key, a code, a name, an
address, a report, or what the phone is.

| Line | When |
| --- | --- |
| `cable offer taken`, `cable offer refused why=<paired, busy, empty, too_long, not_an_offer, caller, path, mode, unreadable>` | The cable door, each write |
| `link offer dropped why=<too_old, paired>` | An offer held ten minutes with no pairing, or the phone paired another way meanwhile |
| `link pair_refused why=<offer_used, offer_expired, version, not_supported, unknown, not_kept, start>` | A pairing the PC refused, or one that could not be kept |
| `link paired` | Paired and kept |
| `link open via=<cable, wifi>` | A session opened, and which way |
| `link apps sent count=<n>` | The app list sent |
| `link about sent` | What the phone is, sent; never what it said |
| `link closed` | A session ended: the PC out of reach |
| `link refused why=<unknown, version, other_device>` | A session the PC refused |
| `link removed releasing` | The PC removed this phone |
| `link failed what=<what> error=<kind>` | Anything else that failed, once a minute at most for each |

And the host's own: `host paired`, `host released`, `host protection_off`,
`host report_ordered no_link` (a report the engine asked for that could not
be written yet), `host grant not_for_this_phone`.

## Tests

`npm run test:child` (in the app) runs them with the module's other JVM
tests. They drive `LinkCore` with the REAL link and engine (the bridge,
through the bindings that ship) and the REAL host (`HostCore`, on the
host's made up phone, `host/HostRig.kt`), against the family's PC played by
the bridge's testkit: the master's end of each connection over a family
file, answering as the Windows service does. The cable and the home Wi-Fi
are played (`LinkRig.kt`): a connection reaches the master while the test
says the cable is in, or the PC is on the Wi-Fi, and either can be pulled.

| Test | What |
| --- | --- |
| `LinkDayTest.aWholeDayOfOneBudgetWithTheMaster` | Paired over the cable at 07:00, the pairing kept before the state, the child's name from the welcome; a session through the cable at once, the app list at the master; ten minutes in a game reported and counted; the cable pulled, the phone away; the PC used for ten minutes at 16:00; at 18:00 the phone on the Wi-Fi at the offer's address, Today saying "on the PC 10 min" and "20 min used of 30 min"; the rest of the day in the game, locked at 30 minutes in all, "Today's time is used up." |
| `LinkDayTest.whatAGuardianChangesOnThePcReachesThePhoneAtOnceWhenTheMasterAsks` | An idle phone in reach sends nothing for minutes; a new PIN on the PC and the master's request to report bring a report and the PIN within two seconds |
| `LinkDayTest.contactLostWithAReportWaitingIsTheEnginesAndComesBack` | The PC stops answering: ten seconds after a report the session ends, the engine is told, the phone is away; the PC answers again and the phone is back by itself |
| `LinkDayTest.aPhoneRemovedOnThePcReleasesItselfAtItsNextContact` | Removed while away; back in reach, told so: the kept pairing, the state and the name gone, the keeper stopped, nothing tried any more; a new offer pairs it again |
| `LinkDayTest.neverMoreThanFortyCallsOnThePcInAMinute` | A PC asking for a report every second for a minute: the phone answers until forty calls, then waits; the session stays |
| `LinkAboutTest` (2) | What the phone is at the played PC once a session opens, asked at every round and said once, said again when the app's version changes, again once in a new session, and never in the log; what Android gives arriving cleaned, joined and cut, and an Android version below zero refused once without ending the session |
| `LinkPairingTest` (7) | The door's rules (its path, writing only, the caller's permission, at most 1,024 bytes, none read beyond); an offer taken before the PC's tunnel is up, tried over the cable only, paired as soon as the tunnel is, the offer wiped, the child's name kept, the door then refusing as paired, the log holding neither offer nor name; nothing, too long, not an offer and a second offer refused, one never answered dropped after ten minutes; an offer used already refused by the PC and dropped, the pairing of a forgotten phone gone; a pairing whose kept pairing or state cannot be written leaving the phone as it was; a stranger on the phone's loopback passed by for the Wi-Fi; addresses and the phone's name |
| `LinkSocketTest` (2) | A pairing and a session through real sockets on this PC's loopback (`SocketConn`), a minute of use counted at the played PC behind a server socket, the PC closing the connection read as out of reach; a close that ends both ways at once; the cable never to another address, the Wi-Fi never to the loopback, a public address or a name, port 0 never |
| `LinkHouseRulesTest` (3) | Only link/ reaches the network; no server or datagram socket, web request, web view or half close anywhere in the module; the link writes down no address but the loopback and no port; no web request in the app's screens and no analytics in its `package.json` |

## What only a phone can prove

For the owner, with the Galaxy, the console and the service, with `adb
logcat -s LiMo`:

1. **The door from the PC's cable**: `cable offer taken` once, then `link
   paired`, and S0 moving on by itself; a second offer refused with `cable
   offer refused why=paired`, and `content write` failing on the PC.
2. **The cable's tunnel**: `link open via=cable` within a few seconds of
   the reverse, and the setup steps reaching the console as notes.
3. **The home Wi-Fi**: with the cable out, `link open via=wifi` (the
   firewall rule in place), with mobile data on as well; and after the PC
   changes its address, the announcement found.
4. **Out of reach and back**: Wi-Fi off, then on: `link closed`, then `link
   open via=wifi`, the phone away meanwhile and its use counted once back.
5. **Removed**: `link removed releasing`, `host released`, `host
   protection_off`, and LiMo then removable from Settings.
6. **Battery**: with the screen off for an hour in reach, Android's battery
   page for LiMo should show little; how often sessions break and open
   again in a day.

## Left open

- **`content write` on Android 8.0 and 8.1** is not known to exist there;
  no other way in (a broadcast, which would put the offer in a command
  line) is built.
- **The phone's own words for "removed, LiMo can be removed"** on its
  screens: the phone releases itself and S0 shows again; a sentence for it
  belongs to the engine and the screens.
- **USB debugging, the protection level and a permission switched on
  again** as notes for the PC: the engine does not make them yet (its
  README, "Left open").
- **Android 17's local network permission** for an app aimed at it
  (`ACCESS_LOCAL_NETWORK`): LiMo aims at Android 16, so nothing changes now;
  when it moves, setup gains a step.

## Layout

```
link/
  README.md
  LinkCore.kt    the link with no Android in it: the door's offers, pairing,
                 finding the master, the session, the limits, the interfaces
                 it needs (LinkConn, LinkDialer, PairingStore, MasterFinder,
                 AppsSource, AboutSource, LinkHostSide) and the host's view of
                 it (LinkPort)
  LinkHost.kt    on Android: the link's thread, SocketConn, the Wi-Fi network
                 and its sockets, NsdManager, the kept pairing in AtomicFile,
                 the watch's apps, the phone's name, what the phone is
  CableDoor.kt   the cable door, a content provider, and its rules
src/test/java/ai/lucidloom/limo/child/link/
  LinkRig.kt     the played cable and Wi-Fi, the host's side, a phone with its link
  LinkDayTest.kt, LinkAboutTest.kt, LinkPairingTest.kt, LinkSocketTest.kt,
  LinkHouseRulesTest.kt
```
