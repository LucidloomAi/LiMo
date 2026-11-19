//! Secrets never show: not in `Debug` output, not in an error, not on the
//! wire in the clear. The secrets are the master's and the phone's private
//! keys, the offer's secret, and the code secret a `welcome` carries.

use limo_link::{
    DirectoryList, KnownPhone, LinkError, MasterEvent, OfferOnFile, OfferProblem, PhonePairing, PhoneSession, Refusal,
};

use crate::common::*;
use crate::raw::{OfferBytes, base32};
use crate::wire::{Plan, pairing_run, session_run};

/// Every way a secret might be written out: hex either case, base32, and
/// Rust's own way of printing bytes, from its first eight bytes on.
fn forms(secret: &[u8]) -> Vec<String> {
    let head = &secret[..8];
    let hex: String = head.iter().map(|b| format!("{b:02x}")).collect();
    let all_hex: String = secret.iter().map(|b| format!("{b:02x}")).collect();
    let as_list = format!("{head:?}");
    vec![
        hex.clone(),
        hex.to_uppercase(),
        all_hex[16..32].to_string(),
        base32(secret)[..12].to_string(),
        as_list[1..as_list.len() - 1].to_string(),
    ]
}

fn assert_hidden(what: &str, shown: &str, secrets: &[(&str, Vec<u8>)]) {
    for (name, secret) in secrets {
        for form in forms(secret) {
            assert!(!shown.contains(&form), "{what} shows the {name} as {form}: {shown}");
        }
    }
}

fn raw_windows(bytes: &[u8], secret: &[u8]) -> bool {
    bytes.windows(8).any(|w| w == &secret[..8])
}

#[test]
fn secrets_never_show_in_debug_output_or_in_errors() {
    let mut family = Family::new();
    let offer = family.offer();
    let text = offer.to_text();
    let offer_secret = OfferBytes::of(&text).secret().to_vec();
    let now = family.now();
    let mut pairing = PhonePairing::start(&offer, request("Asha's phone"), now).unwrap();
    let mut link = family.master.accept(now);
    let shown_early = format!("{pairing:?} {link:?}");
    let welcomed = pump_pairing(&mut pairing, &mut link, &mut family.master, now).unwrap();
    let phone = Phone::from_welcome(welcomed.clone(), now.unix);
    let (session, master_link) = family.open(&phone);
    let secrets = [
        ("offer secret", offer_secret.clone()),
        ("phone's private key", welcomed.paired.phone_keys.expose_private().to_vec()),
        ("master's private key", family.master.keys().expose_private().to_vec()),
        ("code secret", welcomed.code_secret.expose().to_vec()),
    ];
    let directory = DirectoryList {
        offers: vec![OfferOnFile { offer: offer.clone(), used: true }],
        phones: vec![KnownPhone {
            device_id: phone.id().into(),
            child_id: family.child.clone(),
            key: welcomed.paired.phone_keys.public(),
            removed: false,
        }],
    };
    let event = MasterEvent::PairRequest {
        offer_id: offer.id(),
        phone_key: welcomed.paired.phone_keys.public(),
        request: request("Asha's phone"),
    };
    for (what, shown) in [
        ("an offer", format!("{offer:?}")),
        ("a key pair", format!("{:?}", welcomed.paired.phone_keys)),
        ("the master's key pair", format!("{:?}", family.master.keys())),
        ("a welcome", format!("{welcomed:?}")),
        ("what a phone keeps", format!("{:?}", welcomed.paired)),
        ("a code secret", format!("{:?}", welcomed.code_secret)),
        ("a pairing, early and late", format!("{shown_early} {pairing:?} {link:?}")),
        ("a session", format!("{session:?} {master_link:?}")),
        ("the dev master", format!("{:?}", family.master)),
        ("a directory", format!("{directory:?}")),
        ("an event", format!("{event:?}")),
    ] {
        assert_hidden(what, &shown, &secrets);
    }

    // Every error the hostile runs and the offers give, as text and as
    // Debug.
    let mut errors: Vec<LinkError> = vec![
        LinkError::Refused(Refusal::Removed),
        LinkError::BadOffer(OfferProblem::WrongLength),
        LinkError::Malformed { line: 1, column: 2 },
    ];
    for plan in [
        Plan { dir: crate::wire::Dir::ToMaster, index: 1, mutation: crate::wire::Mutation::Junk },
        Plan { dir: crate::wire::Dir::ToPhone, index: 1, mutation: crate::wire::Mutation::Garble { at: 50, bit: 1 } },
    ] {
        let run = pairing_run(Some(plan), 3);
        errors.extend(run.phone_error);
        errors.extend(run.master_error);
        let run = session_run(Some(plan), 3);
        errors.extend(run.phone_error);
        errors.extend(run.master_error);
    }
    let mut broken = text.to_string();
    broken.truncate(40);
    errors.push(limo_link::Offer::from_text(&broken).unwrap_err().into());
    for error in &errors {
        assert_hidden("an error", &format!("{error} {error:?}"), &secrets);
    }
}

#[test]
fn nothing_secret_crosses_the_wire_in_the_clear() {
    let mut family = Family::new();
    let offer = family.offer();
    let offer_secret = OfferBytes::of(&offer.to_text()).secret();
    let now = family.now();
    let mut pairing = PhonePairing::start(&offer, request("Asha's phone"), now).unwrap();
    let mut link = family.master.accept(now);
    let mut wire = Vec::new();
    let mut welcomed = None;
    for _ in 0..6 {
        let up = pairing.take_outgoing();
        wire.extend_from_slice(&up);
        if !up.is_empty() {
            family.master.handle(&mut link, &up, now).unwrap();
        }
        let down = link.take_outgoing();
        wire.extend_from_slice(&down);
        if !down.is_empty()
            && let Some(w) = pairing.receive(&down, now).unwrap()
        {
            welcomed = Some(w);
        }
    }
    let welcomed = welcomed.unwrap();
    let mut phone = Phone::from_welcome(welcomed.clone(), now.unix);
    let mut session = PhoneSession::start(&phone.paired, now).unwrap();
    let mut link = family.master.accept(now);
    for round in 0..6 {
        if round == 2 {
            let report = phone.state.report(now.unix, true);
            session.send_report(&report, now).unwrap();
        }
        let up = session.take_outgoing();
        wire.extend_from_slice(&up);
        if !up.is_empty() {
            family.master.handle(&mut link, &up, now).unwrap();
        }
        let down = link.take_outgoing();
        wire.extend_from_slice(&down);
        if !down.is_empty() {
            session.receive(&down, now).unwrap();
        }
    }
    assert!(wire.len() > 500);
    for (name, secret) in [
        ("offer secret", offer_secret.to_vec()),
        ("phone's private key", welcomed.paired.phone_keys.expose_private().to_vec()),
        ("master's private key", family.master.keys().expose_private().to_vec()),
        ("code secret", welcomed.code_secret.expose().to_vec()),
    ] {
        assert!(!raw_windows(&wire, &secret), "the {name} crossed the wire");
        for form in forms(&secret) {
            assert!(!raw_windows(&wire, form.as_bytes()), "the {name} crossed the wire as {form}");
        }
    }
    // Neither static public key is in the clear either: XX sends both
    // encrypted, and KK neither.
    assert!(!raw_windows(&wire, welcomed.paired.phone_keys.public().as_bytes()));
    assert!(!raw_windows(&wire, family.master.keys().public().as_bytes()));
}

#[test]
fn what_a_phone_keeps_holds_its_private_key_and_comes_back_whole() {
    let mut family = Family::new();
    let phone = family.pair("Asha's phone");
    let saved = phone.paired.to_bytes().unwrap();
    let text = std::str::from_utf8(&saved).unwrap();
    // By design: this is what the phone keeps in private storage.
    let private_hex: String = phone.paired.phone_keys.expose_private().iter().map(|b| format!("{b:02x}")).collect();
    assert!(text.contains(&private_hex));
    let back = limo_link::PairedMaster::from_bytes(&saved).unwrap();
    assert_eq!(back, phone.paired);
    assert_eq!(back.phone_keys.expose_private(), phone.paired.phone_keys.expose_private());
    // A saved copy whose public key does not match its private key is
    // refused, without quoting it.
    let other = limo_link::KeyPair::generate().unwrap().public().to_hex();
    let tampered = text.replace(&phone.paired.phone_keys.public().to_hex(), &other);
    let error = limo_link::PairedMaster::from_bytes(tampered.as_bytes()).unwrap_err();
    assert!(!format!("{error} {error:?}").contains(&private_hex[..16]));
    // And a session opens from what came back.
    let restored = Phone { paired: back, state: phone.state };
    let _ = family.open(&restored);
}
