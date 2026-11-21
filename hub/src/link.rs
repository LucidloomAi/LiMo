//! What the link between the phones and the master PC keeps here: the
//! master's own key, the pairing offers, and each phone's link key.
//!
//! The hub has no network and no cryptography of its own beyond the
//! password's hash: the link (`../link`) makes the offers, runs the
//! handshakes and makes the master's public key from its private key. The
//! hub keeps what the link must not lose, in the family's file, and pairs a
//! phone, keeps its key and marks its offer used in one transaction.

use limo_core::{CODE_SECRET_BYTES, DeviceKind};
use rusqlite::{Connection, OptionalExtension, Row, params};
use zeroize::Zeroizing;

use crate::devices::pair_in;
use crate::error::HubError;
use crate::hub::Hub;
use crate::store;
use crate::text::{self, TextField};
use crate::types::{LinkPhone, MasterKey, OfferRecord, Pairing, PhoneForm, PhoneRequest};
use crate::{KEY_BYTES, MAX_OFFER_LIFE_SECS, MAX_OFFER_TEXT_CHARS, OFFER_ID_HEX_DIGITS, random};

/// The name a phone is given when the name it sent has nothing that can be
/// shown.
const PHONE_NAME_IF_NONE: &str = "Phone";

/// The newest Android API level the hub keeps as given; anything above it is
/// taken as not said.
const MAX_ANDROID_VERSION: u32 = 1_000;

/// The master's private key, made for a new family.
pub(crate) fn new_master_key() -> Result<Zeroizing<[u8; KEY_BYTES]>, HubError> {
    let mut key = Zeroizing::new([0u8; KEY_BYTES]);
    random::fill(&mut key[..])?;
    Ok(key)
}

/// An offer's id in the form it is kept: 16 lowercase hex digits.
fn offer_id_key(offer_id: &str) -> Option<String> {
    let id = offer_id.trim();
    (id.len() == OFFER_ID_HEX_DIGITS && id.bytes().all(|b| b.is_ascii_hexdigit())).then(|| id.to_ascii_lowercase())
}

/// True for text that can be an offer's text form: printable ASCII with no
/// spaces, of a sane length. The link reads it; the hub only keeps it.
fn looks_like_offer_text(text: &str) -> bool {
    (1..=MAX_OFFER_TEXT_CHARS).contains(&text.len()) && text.bytes().all(|b| b.is_ascii_graphic())
}

const OFFER_COLUMNS: &str = "id, child_id, offer_text, made_at, expires_at, used_at, device_id";

fn offer_from_row(row: &Row<'_>) -> rusqlite::Result<OfferRecord> {
    Ok(OfferRecord {
        offer_id: row.get(0)?,
        child_id: row.get(1)?,
        text: Zeroizing::new(row.get(2)?),
        made_at: row.get(3)?,
        expires_at: row.get(4)?,
        used_at: row.get(5)?,
        device_id: row.get(6)?,
    })
}

/// The offer with this id, if it is kept.
fn offer_in(conn: &Connection, offer_id: &str) -> Result<Option<OfferRecord>, HubError> {
    let Some(id) = offer_id_key(offer_id) else { return Ok(None) };
    Ok(conn
        .query_row(&format!("SELECT {OFFER_COLUMNS} FROM offer WHERE id = ?1"), [&id], offer_from_row)
        .optional()?)
}

/// True when a phone with this link key is paired with the family, now or
/// before it was removed.
fn key_known(conn: &Connection, key: &[u8; KEY_BYTES]) -> Result<bool, HubError> {
    Ok(conn.query_row("SELECT 1 FROM phone WHERE link_key = ?1", [&key[..]], |_| Ok(())).optional()?.is_some())
}

impl Hub {
    /// The master's own private key for the link, made when the family was
    /// made (and, for a family set up before the link, when its file was
    /// brought to this version). The link makes the public key from it. It
    /// stays the same for the family's life: deleting the family deletes it,
    /// and a new family gets a new one, so every phone must pair again.
    pub fn master_key(&self) -> Result<MasterKey, HubError> {
        let key: Option<Vec<u8>> = self
            .conn
            .query_row("SELECT master_key FROM family WHERE id = 1", [], |row| row.get(0))
            .optional()?
            .ok_or(HubError::NoFamily)?;
        let key = Zeroizing::new(key.unwrap_or_default());
        let private: [u8; KEY_BYTES] =
            key.as_slice().try_into().map_err(|_| HubError::damaged("the family's master key is missing"))?;
        Ok(MasterKey { private: Zeroizing::new(private) })
    }

    /// Keeps a pairing offer the link made for a child: its id (16 hex
    /// digits, the link's `OfferId::to_hex`), its text form (the link's
    /// `Offer::to_text`, which holds its secret) and when it expires. A
    /// phone that holds it may pair with the child once, until it expires
    /// ([`Hub::pair_device_with_offer`]).
    ///
    /// Refused with [`HubError::BadOffer`] for an id or a text that cannot be
    /// an offer's, an id kept already, or an offer that has expired already
    /// or would last more than a day ([`crate::MAX_OFFER_LIFE_SECS`]).
    pub fn keep_offer(
        &mut self,
        child_id: &str,
        offer_id: &str,
        text: &str,
        expires_at: i64,
        now: i64,
    ) -> Result<OfferRecord, HubError> {
        let id = offer_id_key(offer_id).ok_or(HubError::BadOffer)?;
        let lasts = expires_at.checked_sub(now);
        if !looks_like_offer_text(text) || lasts.is_none_or(|l| !(0..=MAX_OFFER_LIFE_SECS).contains(&l)) {
            return Err(HubError::BadOffer);
        }
        let tx = self.change()?;
        store::require_family(&tx)?;
        store::child(&tx, child_id)?;
        if offer_in(&tx, &id)?.is_some() {
            return Err(HubError::BadOffer);
        }
        tx.execute(
            "INSERT INTO offer (id, child_id, offer_text, made_at, expires_at, used_at, device_id)
             VALUES (?1, ?2, ?3, ?4, ?5, NULL, NULL)",
            params![id, child_id, text, now, expires_at],
        )?;
        let record = offer_in(&tx, &id)?.ok_or_else(|| HubError::internal("an offer was not kept"))?;
        tx.commit()?;
        Ok(record)
    }

    /// The pairing offer with this id, used or not, expired or not, while
    /// the hub keeps it (a day past its expiry, see [`crate::OFFER_KEEP_SECS`]).
    /// The link needs it to finish a pairing handshake, and then says to the
    /// phone whether it was used or expired.
    pub fn offer(&self, offer_id: &str) -> Result<Option<OfferRecord>, HubError> {
        offer_in(&self.conn, offer_id)
    }

    /// Pairs the phone that holds a pairing offer with the offer's child,
    /// keeps the phone's link key (its static public key from the pairing
    /// handshake), and marks the offer used, all in one transaction: either
    /// all of it is done, or none of it. Hand the phone the [`Pairing`] with
    /// the link's `welcome`.
    ///
    /// The name the phone gave is made fit to show: hidden characters taken
    /// out and cut to the hub's 40 characters ("Phone" when nothing is
    /// left); a guardian can rename it. An Android version above 1,000 is
    /// taken as not said.
    ///
    /// Refused, with nothing changed: an offer the hub does not keep
    /// ([`HubError::UnknownOffer`]), one used already ([`HubError::OfferUsed`];
    /// two phones racing with one offer, the second is refused) or expired
    /// ([`HubError::OfferExpired`]); a guardian's phone
    /// ([`HubError::GuardianPhone`]), which leaves the offer unused; and a
    /// link key the family knows already ([`HubError::LinkKeyKnown`]).
    pub fn pair_device_with_offer(
        &mut self,
        offer_id: &str,
        link_key: &[u8; KEY_BYTES],
        phone: &PhoneRequest,
        now: i64,
    ) -> Result<Pairing, HubError> {
        let fitted = text::fit(&phone.name, TextField::DeviceName.max_chars() as usize);
        let name = if fitted.is_empty() { PHONE_NAME_IF_NONE.to_string() } else { fitted };
        let platform = text::clean(&phone.platform, TextField::Platform)?;
        let android_version = phone.android_version.filter(|v| (1..=MAX_ANDROID_VERSION).contains(v));
        let secret = random::bytes(CODE_SECRET_BYTES as usize)?;
        let tx = self.change()?;
        let offer = offer_in(&tx, offer_id)?.ok_or(HubError::UnknownOffer)?;
        if offer.is_used() {
            return Err(HubError::OfferUsed);
        }
        if offer.is_expired(now) {
            return Err(HubError::OfferExpired);
        }
        if phone.form == PhoneForm::Guardian {
            return Err(HubError::GuardianPhone);
        }
        if key_known(&tx, link_key)? {
            return Err(HubError::LinkKeyKnown);
        }
        let child_id = offer.child_id.as_str();
        let pairing = pair_in(&tx, child_id, name, DeviceKind::Portable, &platform, &secret, now)?;
        store::add_phone(&tx, &pairing.device_id, child_id, Some(link_key), phone.form, android_version)?;
        tx.execute(
            "UPDATE offer SET used_at = ?1, device_id = ?2 WHERE id = ?3",
            params![now, pairing.device_id, offer.offer_id],
        )?;
        tx.commit()?;
        Ok(pairing)
    }

    /// Every phone the link may meet, with its link key, in the order they
    /// were paired: each phone paired with an offer, removed ones too (so that
    /// a removed phone is told so when it connects). A phone paired before
    /// the link has no key and is not listed.
    pub fn link_phones(&self) -> Result<Vec<LinkPhone>, HubError> {
        let mut stmt = self.conn.prepare(
            "SELECT p.device_id, p.child_id, p.link_key, d.removed_at IS NOT NULL
             FROM phone p JOIN device d ON d.id = p.device_id
             WHERE p.link_key IS NOT NULL ORDER BY d.seq",
        )?;
        let rows = stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, Vec<u8>>(2)?, row.get::<_, bool>(3)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows.into_iter()
            .map(|(device_id, child_id, key, removed)| {
                let link_key: [u8; KEY_BYTES] =
                    key.as_slice().try_into().map_err(|_| HubError::damaged("a phone's link key is not a key"))?;
                Ok(LinkPhone { device_id, child_id, link_key, removed })
            })
            .collect()
    }
}
