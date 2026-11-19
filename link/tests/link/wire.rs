//! A whole pairing and a whole session, run frame by frame over a wire
//! that can change one frame on the way: garble it, cut it, lie about its
//! length, play it twice, swap it with the next, drop it, replace it with
//! junk, or put a stray byte after it. Then the clock runs on, so a stall
//! ends as the timings say.

use limo_link::{LinkError, MasterEvent, MasterLink, PhoneEvent, PhonePairing, PhoneSession};

use crate::common::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dir {
    ToMaster,
    ToPhone,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mutation {
    /// One bit flipped, in the length or the body.
    Garble { at: usize, bit: u8 },
    /// Only the first half arrives, and nothing after it.
    Cut,
    /// The length says one byte more.
    Longer,
    /// The length says one byte less.
    Shorter,
    /// The length says 4 GiB.
    Huge,
    /// The frame arrives twice.
    Replay,
    /// The frame arrives after the next one.
    Swap,
    /// The frame never arrives.
    Drop,
    /// Random bytes in its place, with its length.
    Junk,
    /// A stray byte after it.
    Extra,
}

pub const EVERY_MUTATION: [Mutation; 10] = [
    Mutation::Garble { at: 7, bit: 3 },
    Mutation::Cut,
    Mutation::Longer,
    Mutation::Shorter,
    Mutation::Huge,
    Mutation::Replay,
    Mutation::Swap,
    Mutation::Drop,
    Mutation::Junk,
    Mutation::Extra,
];

#[derive(Debug, Clone, Copy)]
pub struct Plan {
    pub dir: Dir,
    pub index: usize,
    pub mutation: Mutation,
}

/// The wire: carries frames, and changes the one the plan names.
pub struct Wire {
    plan: Option<Plan>,
    sent: [usize; 2],
    held: [Option<Vec<u8>>; 2],
    stopped: [bool; 2],
    rng: Rng,
    pub swapped_with_none: bool,
}

impl Wire {
    pub fn new(plan: Option<Plan>, seed: u64) -> Wire {
        Wire { plan, sent: [0; 2], held: [None, None], stopped: [false; 2], rng: Rng::new(seed), swapped_with_none: false }
    }

    /// What arrives at the other side for the bytes one side wrote, as one
    /// string of bytes per delivery.
    pub fn carry(&mut self, dir: Dir, bytes: Vec<u8>) -> Vec<Vec<u8>> {
        let d = dir as usize;
        let mut out = Vec::new();
        for f in frames(&bytes) {
            if self.stopped[d] {
                continue;
            }
            let i = self.sent[d];
            self.sent[d] += 1;
            let mutation = self.plan.filter(|p| p.dir == dir && p.index == i).map(|p| p.mutation);
            let mut held = self.held[d].take();
            match mutation {
                None => out.push(f),
                Some(Mutation::Garble { at, bit }) => {
                    let mut g = f.clone();
                    let k = at % g.len();
                    g[k] ^= 1 << (bit % 8);
                    out.push(g);
                }
                Some(Mutation::Cut) => {
                    out.push(f[..f.len() / 2].to_vec());
                    self.stopped[d] = true;
                }
                Some(Mutation::Longer) | Some(Mutation::Shorter) | Some(Mutation::Huge) => {
                    let len = u32::from_be_bytes(f[..4].try_into().unwrap());
                    let said = match mutation {
                        Some(Mutation::Longer) => len + 1,
                        Some(Mutation::Shorter) => len - 1,
                        _ => u32::MAX,
                    };
                    let mut g = said.to_be_bytes().to_vec();
                    g.extend_from_slice(&f[4..]);
                    out.push(g);
                }
                Some(Mutation::Replay) => {
                    out.push(f.clone());
                    out.push(f);
                }
                Some(Mutation::Swap) => {
                    self.held[d] = Some(f);
                }
                Some(Mutation::Drop) => {}
                Some(Mutation::Junk) => {
                    let mut g = f[..4].to_vec();
                    g.extend(self.rng.bytes(f.len() - 4));
                    out.push(g);
                }
                Some(Mutation::Extra) => {
                    let mut g = f.clone();
                    g.push(self.rng.next() as u8);
                    out.push(g);
                }
            }
            if let Some(h) = held.take() {
                out.push(h);
            }
        }
        out
    }

    /// A frame still held back for a swap, delivered late.
    pub fn late(&mut self, dir: Dir) -> Option<Vec<u8>> {
        let late = self.held[dir as usize].take();
        if late.is_some() {
            self.swapped_with_none = true;
        }
        late
    }
}

/// How a conversation went.
#[derive(Debug, Default)]
pub struct Run {
    pub phone_error: Option<LinkError>,
    pub master_error: Option<LinkError>,
    pub phone_events: Vec<String>,
    pub master_events: Vec<String>,
    pub frames: [usize; 2],
    pub finished: bool,
    pub swapped_with_none: bool,
}

fn phone_kind(e: &PhoneEvent) -> String {
    match e {
        PhoneEvent::Opened => "opened",
        PhoneEvent::Grant { .. } => "grant",
        PhoneEvent::ReportNow => "report_now",
        PhoneEvent::Pong { .. } => "pong",
    }
    .to_string()
}

fn master_kind(e: &MasterEvent) -> String {
    match e {
        MasterEvent::PairRequest { .. } => "pair",
        MasterEvent::Opened { .. } => "opened",
        MasterEvent::Report { .. } => "report",
        MasterEvent::Apps { .. } => "apps",
        MasterEvent::Bye => "bye",
    }
    .to_string()
}

/// After an end, every side refuses more.
fn closed_after(session_like: Result<(), LinkError>) {
    assert_eq!(session_like, Err(LinkError::Closed));
}

/// A whole session: open, a report and its grant, the apps, goodbye.
pub fn session_run(plan: Option<Plan>, seed: u64) -> Run {
    let mut family = Family::new();
    let mut phone = family.pair("Asha's phone");
    let now = family.now();
    let mut session = PhoneSession::start(&phone.paired, now).unwrap();
    let mut link: MasterLink = family.master.accept(now);
    let mut wire = Wire::new(plan, seed);
    let mut run = Run::default();
    let (mut reported, mut said_bye) = (false, false);
    for step in 0..60 {
        let now = family.now();
        let mut moved = false;
        let to_master = session.take_outgoing();
        moved |= !to_master.is_empty();
        for delivery in wire.carry(Dir::ToMaster, to_master) {
            if run.master_error.is_some() {
                break;
            }
            match family.master.handle(&mut link, &delivery, now) {
                Ok(events) => run.master_events.extend(events.iter().map(master_kind)),
                Err(e) => run.master_error = Some(link_error(e)),
            }
        }
        let to_phone = link.take_outgoing();
        moved |= !to_phone.is_empty();
        for delivery in wire.carry(Dir::ToPhone, to_phone) {
            if run.phone_error.is_some() {
                break;
            }
            match session.receive(&delivery, now) {
                Ok(events) => run.phone_events.extend(events.iter().map(phone_kind)),
                Err(e) => run.phone_error = Some(e),
            }
        }
        // The phone's side of the script.
        if run.phone_error.is_none() && session.is_open() {
            if !reported && run.phone_events.iter().any(|e| e == "pong") {
                let report = phone.state.report(now.unix, true);
                session.send_report(&report, now).unwrap();
                reported = true;
            } else if reported && !said_bye && run.phone_events.iter().any(|e| e == "grant") {
                let apps = vec![limo_link::AppEntry { package: "com.example.game".into(), label: "Game".into() }];
                session.send_apps(apps, now).unwrap();
                session.send_bye(now).unwrap();
                said_bye = true;
            }
        }
        if !moved && step > 2 {
            // Frames held back for a swap arrive late, then the clock runs.
            for (dir, late) in [(Dir::ToMaster, wire.late(Dir::ToMaster)), (Dir::ToPhone, wire.late(Dir::ToPhone))] {
                let Some(late) = late else { continue };
                match dir {
                    Dir::ToMaster if run.master_error.is_none() => {
                        match family.master.handle(&mut link, &late, now) {
                            Ok(events) => run.master_events.extend(events.iter().map(master_kind)),
                            Err(e) => run.master_error = Some(link_error(e)),
                        }
                    }
                    Dir::ToPhone if run.phone_error.is_none() => match session.receive(&late, now) {
                        Ok(events) => run.phone_events.extend(events.iter().map(phone_kind)),
                        Err(e) => run.phone_error = Some(e),
                    },
                    _ => {}
                }
            }
            family.clock.advance_ms(10_000);
            let now = family.now();
            if run.phone_error.is_none()
                && let Err(e) = session.tick(now)
            {
                run.phone_error = Some(e);
            }
            if run.master_error.is_none()
                && let Err(e) = link.tick(now)
            {
                run.master_error = Some(e);
            }
            if family.clock.ms > 1_000_000 + 400_000 {
                break;
            }
        }
    }
    run.finished = said_bye && run.master_events.last().is_some_and(|e| e == "bye");
    run.swapped_with_none = wire.swapped_with_none;
    run.frames = wire.sent;
    if run.phone_error.is_some() {
        closed_after(session.receive(b"\x00\x00\x00\x01x", family.now()).map(|_| ()));
    }
    if run.master_error.is_some() {
        closed_after(family.master.handle(&mut link, b"\x00\x00\x00\x01x", family.now()).map(|_| ()).map_err(link_error));
    }
    run
}

/// A whole pairing: the handshake, `pair`, `welcome`, `bye`.
pub fn pairing_run(plan: Option<Plan>, seed: u64) -> Run {
    let mut family = Family::new();
    let offer = family.offer();
    let now = family.now();
    let mut phone = PhonePairing::start(&offer, request("Asha's phone"), now).unwrap();
    let mut link: MasterLink = family.master.accept(now);
    let mut wire = Wire::new(plan, seed);
    let mut run = Run::default();
    let mut welcomed = false;
    for step in 0..40 {
        let now = family.now();
        let mut moved = false;
        let to_master = phone.take_outgoing();
        moved |= !to_master.is_empty();
        for delivery in wire.carry(Dir::ToMaster, to_master) {
            if run.master_error.is_some() {
                break;
            }
            match family.master.handle(&mut link, &delivery, now) {
                Ok(events) => run.master_events.extend(events.iter().map(master_kind)),
                Err(e) => run.master_error = Some(link_error(e)),
            }
        }
        let to_phone = link.take_outgoing();
        moved |= !to_phone.is_empty();
        for delivery in wire.carry(Dir::ToPhone, to_phone) {
            if run.phone_error.is_some() {
                break;
            }
            match phone.receive(&delivery, now) {
                Ok(Some(_)) => {
                    welcomed = true;
                    run.phone_events.push("welcome".into());
                }
                Ok(None) => {}
                Err(e) => run.phone_error = Some(e),
            }
        }
        if !moved && step > 2 {
            for (dir, late) in [(Dir::ToMaster, wire.late(Dir::ToMaster)), (Dir::ToPhone, wire.late(Dir::ToPhone))] {
                let Some(late) = late else { continue };
                match dir {
                    Dir::ToMaster if run.master_error.is_none() => {
                        match family.master.handle(&mut link, &late, now) {
                            Ok(events) => run.master_events.extend(events.iter().map(master_kind)),
                            Err(e) => run.master_error = Some(link_error(e)),
                        }
                    }
                    Dir::ToPhone if run.phone_error.is_none() => match phone.receive(&late, now) {
                        Ok(Some(_)) => {
                            welcomed = true;
                            run.phone_events.push("welcome".into());
                        }
                        Ok(None) => {}
                        Err(e) => run.phone_error = Some(e),
                    },
                    _ => {}
                }
            }
            family.clock.advance_ms(5_000);
            let now = family.now();
            if run.phone_error.is_none()
                && let Err(e) = phone.tick(now)
            {
                run.phone_error = Some(e);
            }
            if run.master_error.is_none()
                && let Err(e) = link.tick(now)
            {
                run.master_error = Some(e);
            }
            if family.clock.ms > 1_000_000 + 60_000 {
                break;
            }
        }
    }
    run.finished = welcomed && run.master_events.last().is_some_and(|e| e == "bye");
    run.swapped_with_none = wire.swapped_with_none;
    run.frames = wire.sent;
    // At most one phone was paired, and only one that finished pairing.
    let paired = family.master.phones_paired().len();
    assert!(paired <= 1);
    if paired == 1 {
        assert!(run.master_events.iter().any(|e| e == "pair"));
    }
    if run.phone_error.is_some() {
        assert_eq!(phone.receive(b"\x00\x00\x00\x01x", family.now()).map(|_| ()), Err(LinkError::Closed));
    }
    run
}

/// True when `seen` is the start of `honest`: nothing came that an honest
/// run does not bring, nothing twice, nothing out of order.
pub fn is_prefix(seen: &[String], honest: &[String]) -> bool {
    seen.len() <= honest.len() && seen.iter().zip(honest).all(|(a, b)| a == b)
}
