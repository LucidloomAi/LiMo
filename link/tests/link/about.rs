//! What a phone says it is (`about`), and what each side does with a
//! message of a kind it does not know: a newer phone talking to an older
//! master, and the other way.

use limo_link::messages::{MAX_APP_VERSION_CHARS, MAX_MODEL_CHARS};
use limo_link::{KeyPair, LinkError, MasterEvent, PhoneAbout, PhoneEvent, PhoneSession};

use crate::common::*;
use crate::raw::{OfferBytes, RawHandshake, raw_master};

fn galaxy() -> PhoneAbout {
    PhoneAbout::new("samsung", "SM-S911B", Some(34), "0.4.0")
}

fn json(text: &[u8]) -> serde_json::Value {
    serde_json::from_slice(text).unwrap()
}

#[test]
fn a_phone_says_what_it_is_once_a_session_and_again_when_it_changes() {
    let mut family = Family::new();
    let mut phone = family.pair("Asha's phone");
    let (mut session, mut link) = family.open(&phone);
    let now = family.now();
    assert!(session.send_about(&galaxy(), now).unwrap());
    let pumped = pump(&mut session, &mut link, &mut family.master, now);
    assert_eq!(pumped.master_events, vec![MasterEvent::About { about: galaxy() }]);
    // Nothing answers it.
    assert!(pumped.phone_events.is_empty());
    // Said already in this session: nothing goes.
    assert!(!session.send_about(&galaxy(), now).unwrap());
    assert!(session.take_outgoing().is_empty());
    // An update of the app changes its version: said again.
    let updated = PhoneAbout::new("samsung", "SM-S911B", Some(34), "0.5.0");
    assert!(session.send_about(&updated, now).unwrap());
    let pumped = pump(&mut session, &mut link, &mut family.master, now);
    assert_eq!(pumped.master_events, vec![MasterEvent::About { about: updated.clone() }]);
    // A new session says it again, once.
    session.send_bye(now).unwrap();
    family.master.handle(&mut link, &session.take_outgoing(), now).unwrap();
    let (mut session, mut link) = family.open(&phone);
    assert!(session.send_about(&updated, now).unwrap());
    assert!(!session.send_about(&updated, now).unwrap());
    pump(&mut session, &mut link, &mut family.master, now);
    assert_eq!(family.master.abouts_of(phone.id()), [galaxy(), updated.clone(), updated]);
    // And the session goes on as ever.
    family.exchange(&mut phone, &mut session, &mut link, false);
}

#[test]
fn what_a_phone_is_goes_as_the_readme_says_and_within_its_bounds() {
    let mut family = Family::new();
    let phone = family.pair("Asha's phone");
    let now = family.now();
    // On the wire, read by a master made by hand.
    let master_private = *family.master.keys().expose_private();
    let phone_public = *phone.paired.phone_keys.public().as_bytes();
    let mut session = PhoneSession::start(&phone.paired, now).unwrap();
    let (mut raw, reply) = raw_master(&master_private, &phone_public, &session.take_outgoing(), 4);
    session.receive(&reply, now).unwrap();
    raw.open(&session.take_outgoing()).unwrap();
    // What Android gives, odd characters and all, goes cleaned.
    let odd = PhoneAbout::new("Google\u{0}", " Pixel 7\u{202e} ", Some(34), "0.4.0\n");
    assert!(session.send_about(&odd, now).unwrap());
    assert_eq!(
        raw.open(&session.take_outgoing()).unwrap(),
        br#"{"say":"about","model":"Google Pixel 7","android_version":34,"app_version":"0.4.0"}"#
    );
    // What a phone that is not Android says.
    assert!(session.send_about(&PhoneAbout::new("", "", None, ""), now).unwrap());
    assert_eq!(
        raw.open(&session.take_outgoing()).unwrap(),
        br#"{"say":"about","model":"","android_version":null,"app_version":""}"#
    );
    // One made by hand past the bounds is refused before it is sent, and
    // changes nothing.
    let long = PhoneAbout { model: "m".repeat(MAX_MODEL_CHARS + 1), android_version: None, app_version: String::new() };
    assert!(matches!(session.send_about(&long, now), Err(LinkError::OutOfBounds { .. })));
    let odd = PhoneAbout { model: "Pixel\n7".into(), android_version: None, app_version: String::new() };
    assert!(matches!(session.send_about(&odd, now), Err(LinkError::OutOfBounds { .. })));
    assert!(session.take_outgoing().is_empty());
    assert!(session.is_open());

    // A phone made by hand that sends one past the bounds is refused by the
    // master, and the link ends.
    let private = *phone.paired.phone_keys.expose_private();
    let master_key = *phone.paired.master_key.as_bytes();
    for (model, version) in [
        ("m".repeat(MAX_MODEL_CHARS + 1), "1".to_string()),
        ("Pixel".to_string(), "9".repeat(MAX_APP_VERSION_CHARS + 1)),
        ("Pixel\u{7}".to_string(), "1".to_string()),
        (" Pixel".to_string(), "1".to_string()),
    ] {
        let (mut raw, first) = RawHandshake::session(&private, &master_key, 1, 5);
        let mut link = family.master.accept(now);
        family.master.handle(&mut link, &first, now).unwrap();
        raw.reply(&link.take_outgoing()).unwrap();
        let mut open = raw.open();
        family.master.handle(&mut link, &open.seal(br#"{"say":"ping"}"#), now).unwrap();
        let said = serde_json::json!({"say": "about", "model": model, "android_version": 34, "app_version": version});
        let refused = family.master.handle(&mut link, &open.seal(&serde_json::to_vec(&said).unwrap()), now);
        assert!(matches!(refused.map_err(link_error), Err(LinkError::OutOfBounds { .. })), "{model:?} {version:?}");
        assert!(link.is_ended());
    }
    assert!(family.master.abouts_of(phone.id()).is_empty());
    // What the master's event shows in a log never holds the model.
    let shown = format!("{:?}", MasterEvent::About { about: galaxy() });
    assert!(!shown.contains("SM-S911B") && !shown.contains("samsung"), "{shown}");
}

#[test]
fn a_phone_that_never_says_what_it_is_is_served_as_ever() {
    // An older phone: a session, a report and its grant, and the master
    // knows nothing of what it is.
    let mut family = Family::new();
    let mut phone = family.pair("Asha's phone");
    let (mut session, mut link) = family.open(&phone);
    family.exchange(&mut phone, &mut session, &mut link, false);
    family.exchange(&mut phone, &mut session, &mut link, true);
    assert!(family.master.abouts_of(phone.id()).is_empty());
    assert!(session.is_open() && !link.is_ended());
}

#[test]
fn a_master_passes_over_a_message_of_a_kind_it_does_not_know() {
    // A newer phone, made by hand with the real key: its very first message
    // is of a kind this version does not know.
    let mut family = Family::new();
    let mut phone = family.pair("Asha's phone");
    let now = family.now();
    let private = *phone.paired.phone_keys.expose_private();
    let master_key = *phone.paired.master_key.as_bytes();
    let (mut raw, first) = RawHandshake::session(&private, &master_key, 1, 3);
    let mut link = family.master.accept(now);
    assert!(family.master.handle(&mut link, &first, now).unwrap().is_empty());
    assert_eq!(raw.reply(&link.take_outgoing()).unwrap(), None);
    let mut open = raw.open();
    let newer = open.seal(br#"{"say":"battery","level":40,"charging":true}"#);
    let events = family.master.handle(&mut link, &newer, now).unwrap();
    // It still proves the phone live; nothing else comes of it, and
    // nothing is said back.
    assert_eq!(events, vec![MasterEvent::Opened { device_id: phone.id().to_string(), child_id: family.child.clone() }]);
    assert!(link.take_outgoing().is_empty());
    // The session goes on: a ping gets its pong, a report its grant.
    assert!(family.master.handle(&mut link, &open.seal(br#"{"say":"ping"}"#), now).unwrap().is_empty());
    assert_eq!(json(&open.open(&link.take_outgoing()).unwrap())["say"], "pong");
    let report = phone.state.report(now.unix, false);
    let said = serde_json::to_vec(&serde_json::json!({"say": "report", "report": report})).unwrap();
    let events = family.master.handle(&mut link, &open.seal(&said), now).unwrap();
    assert!(matches!(&events[..], [MasterEvent::Report { .. }]), "{events:?}");
    assert_eq!(json(&open.open(&link.take_outgoing()).unwrap())["say"], "grant");
    // Another, in the middle of the session: passed over too.
    let events = family.master.handle(&mut link, &open.seal(br#"{"say":"photo","bytes":"AAAA"}"#), now).unwrap();
    assert!(events.is_empty() && !link.is_ended());
    // A kind this version knows, misshapen, still ends the link.
    let bad = family.master.handle(&mut link, &open.seal(br#"{"say":"ping","extra":1}"#), now);
    assert!(matches!(bad.map_err(link_error), Err(LinkError::Malformed { .. })));
    assert!(link.is_ended());

    // On a pairing too: a newer phone may say something before its request.
    let offer = family.offer();
    let bytes = OfferBytes::of(&offer.to_text());
    let phone_key = KeyPair::generate().unwrap();
    let (mut raw, first) = RawHandshake::pairing(phone_key.expose_private(), &bytes.id(), &bytes.secret(), 1, 6);
    let mut link = family.master.accept(now);
    family.master.handle(&mut link, &first, now).unwrap();
    let mut sent = raw.reply(&link.take_outgoing()).unwrap().unwrap();
    let mut open = raw.open();
    sent.extend(open.seal(br#"{"say":"hello_v2","colour":"amber"}"#));
    sent.extend(open.seal(br#"{"say":"pair","name":"Mira's phone","form":"child","platform":"android","android_version":34}"#));
    let events = family.master.handle(&mut link, &sent, now).unwrap();
    assert!(matches!(&events[..], [MasterEvent::PairRequest { .. }]), "{events:?}");
    assert_eq!(json(&open.open(&link.take_outgoing()).unwrap())["say"], "welcome");
}

#[test]
fn a_phone_passes_over_a_message_of_a_kind_it_does_not_know() {
    // A newer master, made by hand with the real key.
    let mut family = Family::new();
    let phone = family.pair("Asha's phone");
    let now = family.now();
    let master_private = *family.master.keys().expose_private();
    let phone_public = *phone.paired.phone_keys.public().as_bytes();
    let mut session = PhoneSession::start(&phone.paired, now).unwrap();
    let (mut raw, reply) = raw_master(&master_private, &phone_public, &session.take_outgoing(), 5);
    assert_eq!(session.receive(&reply, now).unwrap(), vec![PhoneEvent::Opened]);
    assert_eq!(raw.open(&session.take_outgoing()).unwrap(), br#"{"say":"ping"}"#);
    // Something newer first, then the pong, in the same bytes.
    let mut bytes = raw.seal(br#"{"say":"lights","colour":"amber"}"#);
    bytes.extend(raw.seal(br#"{"say":"pong","master_now":1795051800}"#));
    assert_eq!(session.receive(&bytes, now).unwrap(), vec![PhoneEvent::Pong { master_now: 1_795_051_800 }]);
    // Nothing is said back, and the session goes on.
    assert!(session.take_outgoing().is_empty());
    assert!(session.is_open());
    assert!(session.tick(now).unwrap().is_some());
    assert!(session.send_about(&galaxy(), now).unwrap());
    assert_eq!(json(&raw.open(&session.take_outgoing()).unwrap())["say"], "about");
    // A kind this version knows, misshapen, still ends the session.
    assert!(matches!(session.receive(&raw.seal(br#"{"say":"pong"}"#), now), Err(LinkError::Malformed { .. })));
    assert!(!session.is_open());
}
