//! Pairing: the offer, the handshake with its secret, `pair` and
//! `welcome`; and every way it is refused.

use limo_link::crypto::{PAIR_FINISH_BYTES, PAIR_HELLO_BYTES, PAIR_REPLY_BYTES};
use limo_link::frame::PAIRING_PREAMBLE_BYTES;
use limo_link::messages::{CODE_SECRET_BYTES, MasterSays};
use limo_link::{
    DirectoryList, Form, KeyPair, LinkError, MasterEvent, MasterLink, Offer, OfferOnFile, PairRequest, PhonePairing,
    Refusal,
};

use crate::common::*;
use crate::raw::{OfferBytes, RawHandshake};

#[test]
fn a_phone_pairs_with_the_offer_and_gets_its_welcome() {
    let mut family = Family::new();
    let offer = family.offer();
    let now = family.now();
    let mut phone = PhonePairing::start(&offer, request("Asha's phone"), now).unwrap();
    let mut link = family.master.accept(now);

    // The first frame: the preamble and the first handshake message.
    let first = phone.take_outgoing();
    assert_eq!(first.len(), 4 + PAIRING_PREAMBLE_BYTES + PAIR_HELLO_BYTES);
    assert_eq!(&first[4..8], b"LiMo");
    assert_eq!(first[8], 1);
    assert_eq!(first[9], b'P');
    assert_eq!(&first[10..18], offer.id().as_bytes());
    assert!(family.master.handle(&mut link, &first, now).unwrap().is_empty());
    let reply = link.take_outgoing();
    assert_eq!(reply.len(), 4 + PAIR_REPLY_BYTES);
    assert_eq!(phone.receive(&reply, now).unwrap(), None);
    // The finish and the request go together.
    let finish_and_pair = frames(&phone.take_outgoing());
    assert_eq!(finish_and_pair.len(), 2);
    assert_eq!(finish_and_pair[0].len(), 4 + PAIR_FINISH_BYTES);
    let events = family.master.handle(&mut link, &finish_and_pair.concat(), now).unwrap();
    let [MasterEvent::PairRequest { offer_id, phone_key, request: asked }] = &events[..] else {
        panic!("{events:?}");
    };
    assert_eq!(*offer_id, offer.id());
    assert_eq!(asked, &request("Asha's phone"));
    assert!(link.is_ended());
    let welcome = link.take_outgoing();
    let welcomed = phone.receive(&welcome, now).unwrap().expect("the welcome");
    assert!(phone.is_done());
    // The phone says goodbye, and the master takes it.
    let bye = phone.take_outgoing();
    assert_eq!(family.master.handle(&mut link, &bye, now).unwrap(), vec![MasterEvent::Bye]);

    // What the phone keeps.
    let paired = &welcomed.paired;
    assert_eq!(paired.master_key, family.master.keys().public());
    assert_eq!(paired.master_id(), family.master.master_id());
    assert_eq!(paired.child_id, family.child);
    assert_eq!(paired.port, 47810);
    assert_eq!(paired.addresses, offer.addresses());
    assert_eq!(paired.phone_keys.public(), *phone_key);
    assert_eq!(welcomed.welcome.device_id, paired.device_id);
    assert_eq!(welcomed.code_secret.expose().len(), CODE_SECRET_BYTES);
    assert_eq!(welcomed.master_now, START);
    // What the master keeps: the phone, its key, the device in the hub.
    let known = &family.master.phones_paired()[0];
    assert_eq!((known.device_id.as_str(), known.key), (paired.device_id.as_str(), *phone_key));
    let device = family.master.hub().device(&paired.device_id).unwrap();
    assert_eq!((device.name.as_str(), device.platform.as_str()), ("Asha's phone", "android"));
    // And the phone's state is made from the welcome.
    let phone = Phone::from_welcome(welcomed.clone(), START);
    assert!(phone.state.has_code_secret());
    // What the phone keeps goes to bytes and back.
    let saved = welcomed.paired.to_bytes().unwrap();
    assert_eq!(limo_link::PairedMaster::from_bytes(&saved).unwrap(), welcomed.paired);
}

#[test]
fn the_offer_works_once() {
    let mut family = Family::new();
    let offer = family.offer();
    family.pair_with(&offer, "Asha's phone").unwrap();
    let again = family.pair_with(&offer, "Another phone").unwrap_err();
    assert_eq!(again, Outcome { phone: Some(LinkError::Refused(Refusal::OfferUsed)), master: None });
    assert_eq!(family.master.phones_paired().len(), 1);
    assert_eq!(family.master.hub().devices(&family.child.clone()).unwrap().len(), 1);
}

#[test]
fn an_offer_lasts_ten_minutes_by_the_masters_clock() {
    let mut family = Family::new();
    let offer = family.offer();
    let late = family.offer();
    family.clock.advance_secs(600);
    family.pair_with(&offer, "On time").unwrap();
    family.clock.advance_secs(1);
    let refused = family.pair_with(&late, "Late").unwrap_err();
    assert_eq!(refused, Outcome { phone: Some(LinkError::Refused(Refusal::OfferExpired)), master: None });
    // An expired offer is not used up: it stays expired.
    assert_eq!(family.master.phones_paired().len(), 1);
}

#[test]
fn a_wrong_secret_never_gets_past_the_handshake() {
    let mut family = Family::new();
    let offer = family.offer();
    let mut bytes = OfferBytes::of(&offer.to_text());
    bytes.0[50] ^= 0x01;
    let forged = Offer::from_text(&bytes.text()).unwrap();
    assert_eq!(forged.id(), offer.id());
    let now = family.now();
    let mut phone = PhonePairing::start(&forged, request("Forged"), now).unwrap();
    let mut link = family.master.accept(now);
    let outcome = pump_pairing(&mut phone, &mut link, &mut family.master, now).unwrap_err();
    // The master cannot read the finish; it says nothing, and the phone
    // waits for a welcome that never comes.
    assert_eq!(outcome, Outcome { phone: None, master: Some(LinkError::Garbled) });
    family.clock.advance_ms(9_999);
    assert!(phone.tick(family.now()).is_ok());
    family.clock.advance_ms(1);
    assert_eq!(phone.tick(family.now()), Err(LinkError::NoAnswer { report_waiting: false }));
    // Nothing was paired, and the offer still works for its real holder.
    assert!(family.master.phones_paired().is_empty());
    family.pair_with(&offer, "The real phone").unwrap();
}

#[test]
fn the_phone_accepts_only_the_master_the_offer_names() {
    let mut family = Family::new();
    let offer = family.offer();
    // An offer that names another key, with the real id and secret: the
    // real master answers, and the phone refuses it.
    let mut bytes = OfferBytes::of(&offer.to_text());
    bytes.0[..32].copy_from_slice(KeyPair::generate().unwrap().public().as_bytes());
    let renamed = Offer::from_text(&bytes.text()).unwrap();
    let outcome = family.pair_with(&renamed, "Phone").unwrap_err();
    assert_eq!(outcome, Outcome { phone: Some(LinkError::WrongMaster), master: None });
    // And an impostor that has the whole offer, secret and all (someone
    // who photographed it), but not the master's private key: the phone
    // refuses it at once, before it sends anything about itself.
    let now = family.now();
    let impostor = KeyPair::generate().unwrap();
    let directory = DirectoryList { offers: vec![OfferOnFile { offer: offer.clone(), used: false }], phones: vec![] };
    let mut phone = PhonePairing::start(&offer, request("Phone"), now).unwrap();
    let mut link = MasterLink::accept(&impostor, now);
    link.receive(&phone.take_outgoing(), now, &directory).unwrap();
    assert_eq!(phone.receive(&link.take_outgoing(), now), Err(LinkError::WrongMaster));
    assert!(phone.take_outgoing().is_empty());
    // The offer was never used.
    family.pair_with(&offer, "The real phone").unwrap();
}

#[test]
fn an_offer_this_master_never_made_gets_no_answer() {
    let mut family = Family::new();
    let mut other = Family::new();
    let foreign = other.offer();
    let now = family.now();
    let mut phone = PhonePairing::start(&foreign, request("Phone"), now).unwrap();
    let mut link = family.master.accept(now);
    let outcome = pump_pairing(&mut phone, &mut link, &mut family.master, now).unwrap_err();
    assert_eq!(outcome, Outcome { phone: None, master: Some(LinkError::UnknownOffer) });
    family.clock.advance_ms(5_000);
    assert_eq!(phone.tick(family.now()), Err(LinkError::HandshakeTooSlow));
    assert_eq!(phone.receive(b"anything", family.now()), Err(LinkError::Closed));
}

#[test]
fn a_guardian_phone_is_not_paired_by_this_version() {
    let mut family = Family::new();
    let offer = family.offer();
    let now = family.now();
    let guardian = PairRequest { name: "Mira's phone".into(), form: Form::Guardian, platform: "android".into(), android_version: Some(34) };
    let mut phone = PhonePairing::start(&offer, guardian, now).unwrap();
    let mut link = family.master.accept(now);
    let outcome = pump_pairing(&mut phone, &mut link, &mut family.master, now).unwrap_err();
    assert_eq!(outcome, Outcome { phone: Some(LinkError::Refused(Refusal::NotSupported)), master: None });
    assert!(family.master.phones_paired().is_empty());
}

#[test]
fn two_phones_with_one_offer_get_one_pairing() {
    let mut family = Family::new();
    let offer = family.offer();
    let now = family.now();
    let mut a = PhonePairing::start(&offer, request("A"), now).unwrap();
    let mut b = PhonePairing::start(&offer, request("B"), now).unwrap();
    let mut link_a = family.master.accept(now);
    let mut link_b = family.master.accept(now);
    // Both through the handshake before either asks.
    family.master.handle(&mut link_a, &a.take_outgoing(), now).unwrap();
    family.master.handle(&mut link_b, &b.take_outgoing(), now).unwrap();
    a.receive(&link_a.take_outgoing(), now).unwrap();
    b.receive(&link_b.take_outgoing(), now).unwrap();
    let ask_a = a.take_outgoing();
    let ask_b = b.take_outgoing();
    family.master.handle(&mut link_b, &ask_b, now).unwrap();
    family.master.handle(&mut link_a, &ask_a, now).unwrap();
    assert!(b.receive(&link_b.take_outgoing(), now).unwrap().is_some());
    assert_eq!(a.receive(&link_a.take_outgoing(), now), Err(LinkError::Refused(Refusal::OfferUsed)));
    assert_eq!(family.master.phones_paired().len(), 1);
}

#[test]
fn a_phone_of_a_newer_link_is_told_which_version_the_master_speaks() {
    let mut family = Family::new();
    let offer = family.offer();
    let bytes = OfferBytes::of(&offer.to_text());
    let now = family.now();
    let phone_key = KeyPair::generate().unwrap();
    let (mut raw, first) = RawHandshake::pairing(phone_key.expose_private(), &bytes.id(), &bytes.secret(), 2, 7);
    let mut link = family.master.accept(now);
    family.master.handle(&mut link, &first, now).unwrap();
    assert_eq!(link.phone_speaks(), 2);
    let finish = raw.reply(&link.take_outgoing()).unwrap().unwrap();
    family.master.handle(&mut link, &finish, now).unwrap();
    assert!(link.is_ended());
    let mut session = raw.open();
    let said = session.open(&link.take_outgoing()).unwrap();
    assert_eq!(said, br#"{"say":"refused","why":"version","speaks":1}"#);
    let parsed: MasterSays = serde_json::from_slice(&said).unwrap();
    assert_eq!(parsed, MasterSays::Refused { why: Refusal::Version, speaks: 1 });
    // Nothing was paired, and the offer is not used.
    family.pair_with(&offer, "An up to date phone").unwrap();
}

#[test]
fn a_hand_made_phone_pairs_by_the_readmes_description() {
    // The README's wire, written by hand on snow, pairs with the master:
    // so the description is the code's.
    let mut family = Family::new();
    let offer = family.offer();
    let bytes = OfferBytes::of(&offer.to_text());
    assert_eq!(bytes.master_key(), *family.master.keys().public().as_bytes());
    let now = family.now();
    let phone_key = KeyPair::generate().unwrap();
    let (mut raw, first) = RawHandshake::pairing(phone_key.expose_private(), &bytes.id(), &bytes.secret(), 1, 9);
    let mut link = family.master.accept(now);
    family.master.handle(&mut link, &first, now).unwrap();
    let finish = raw.reply(&link.take_outgoing()).unwrap().unwrap();
    assert_eq!(raw.hs.get_remote_static().unwrap(), &bytes.master_key());
    let mut session = raw.open();
    let mut sent = finish;
    sent.extend(session.seal(
        br#"{"say":"pair","name":"By hand","form":"child","platform":"android","android_version":null}"#,
    ));
    let events = family.master.handle(&mut link, &sent, now).unwrap();
    assert!(matches!(&events[..], [MasterEvent::PairRequest { .. }]));
    let welcome: serde_json::Value = serde_json::from_slice(&session.open(&link.take_outgoing()).unwrap()).unwrap();
    assert_eq!(welcome["say"], "welcome");
    assert_eq!(welcome["code_secret"].as_str().unwrap().len(), 64);
    assert_eq!(welcome["master_now"], START);
    assert_eq!(welcome["welcome"]["device_id"], family.master.phones_paired()[0].device_id.as_str());
}
