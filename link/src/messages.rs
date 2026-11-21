//! The messages inside the link, as JSON, each named by its `say` field.
//!
//! | Phone to master | |
//! | --- | --- |
//! | `pair` | On a pairing link, once: the phone's name, form, platform and Android version. |
//! | `report` | The core's `Report`. |
//! | `apps` | The phone's apps with their labels, and a hash of the list. |
//! | `ping` | Nothing; asks for a `pong`. The first message of every session, and every minute after when nothing else went. |
//! | `bye` | The phone is leaving; nothing more comes. |
//!
//! | Master to phone | |
//! | --- | --- |
//! | `welcome` | The answer to `pair`: the first grant, the phone's code secret, the master's clock, and the child's name. |
//! | `grant` | The answer to a `report`: the core's `Grant`, and the master's clock. |
//! | `report_now` | Please report at once (the hub's wanted reports). |
//! | `refused` | No, and why; the link ends. |
//! | `pong` | The answer to `ping`, with the master's clock. |
//!
//! A message with a field it should not have, or without one it must
//! have, is refused, as is a message of the wrong kind for its side.

use std::fmt;
use std::io;

use limo_core::{Grant, Report};
use serde::de::{DeserializeOwned, Deserializer};
use serde::ser::Serializer;
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::crypto::short_hash;
use crate::encoding::{hex, unhex_vec};
use crate::error::LinkError;

/// The length of a device's code secret: the core's `CODE_SECRET_BYTES`.
pub const CODE_SECRET_BYTES: usize = limo_core::CODE_SECRET_BYTES as usize;

/// The longest phone name, in characters. The hub keeps a shorter one.
pub const MAX_NAME_CHARS: usize = 120;
/// The longest platform label, in characters.
pub const MAX_PLATFORM_CHARS: usize = 40;
/// The most apps an `apps` message lists.
pub const MAX_APPS: usize = 2_000;
/// The longest package name, in bytes.
pub const MAX_PACKAGE_BYTES: usize = 255;
/// The longest app label, in characters.
pub const MAX_LABEL_CHARS: usize = 120;

/// Which form of the phone app is pairing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Form {
    /// The child's form, which LiMo limits.
    Child,
    /// The guardian's form. Not paired by this version (see the README,
    /// "Left open").
    Guardian,
}

/// Why the master refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Refusal {
    /// This phone was removed from the family. It should set itself up
    /// again, or be released.
    Removed,
    /// The master no longer knows this phone.
    Unknown,
    /// A report about another device or child than this session's.
    OtherDevice,
    /// The phone speaks a link version the master does not. The refusal
    /// says which version the master speaks.
    Version,
    /// The offer was used already: a new one must be made on the PC.
    OfferUsed,
    /// The offer expired: a new one must be made on the PC.
    OfferExpired,
    /// The master cannot do this yet, such as pairing a guardian's phone.
    NotSupported,
}

impl Refusal {
    /// The reason as part of a sentence.
    pub fn sentence(&self) -> &'static str {
        match self {
            Refusal::Removed => "this phone was removed from the family",
            Refusal::Unknown => "it does not know this phone",
            Refusal::OtherDevice => "the report was about another device",
            Refusal::Version => "it speaks another version of the link; update LiMo",
            Refusal::OfferUsed => "the pairing offer was used already; make a new one on the PC",
            Refusal::OfferExpired => "the pairing offer has expired; make a new one on the PC",
            Refusal::NotSupported => "it cannot do that yet",
        }
    }
}

/// One app on the phone: its package and the label the phone shows.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppEntry {
    /// The package name, such as `com.example.game`.
    pub package: String,
    /// The label, such as "Game".
    pub label: String,
}

/// The hash of an app list: BLAKE2s over the label `LiMo apps v1`, a zero
/// byte, and each app's package and label, each as its length (4 bytes, big
/// endian) and its UTF-8; the first 16 bytes, as 32 hex characters. The
/// order of the list counts.
pub fn apps_hash(apps: &[AppEntry]) -> String {
    let mut parts: Vec<Vec<u8>> = Vec::with_capacity(apps.len() * 2);
    for app in apps {
        for text in [&app.package, &app.label] {
            let mut part = u32::try_from(text.len()).unwrap_or(u32::MAX).to_be_bytes().to_vec();
            part.extend_from_slice(text.as_bytes());
            parts.push(part);
        }
    }
    let refs: Vec<&[u8]> = parts.iter().map(Vec::as_slice).collect();
    hex(&short_hash("LiMo apps v1", &refs)[..16]).to_string()
}

/// A device's code secret, as the master hands it over in a `welcome`: the
/// 32 bytes one time codes are made from. Its `Debug` output hides it, it
/// is wiped when dropped, and in JSON it is 64 hex characters.
#[derive(Clone)]
pub struct CodeSecret(Zeroizing<Vec<u8>>);

impl CodeSecret {
    /// Wraps the secret.
    pub fn new(bytes: &[u8]) -> CodeSecret {
        CodeSecret(Zeroizing::new(bytes.to_vec()))
    }

    /// The bytes, for `DeviceState::set_code_secret`. Never show them or
    /// log them.
    pub fn expose(&self) -> &[u8] {
        &self.0
    }
}

impl PartialEq for CodeSecret {
    /// Compared without stopping at the first difference.
    fn eq(&self, other: &CodeSecret) -> bool {
        self.0.len() == other.0.len() && self.0.iter().zip(other.0.iter()).fold(0u8, |acc, (a, b)| acc | (a ^ b)) == 0
    }
}

impl Eq for CodeSecret {}

impl fmt::Debug for CodeSecret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "CodeSecret(hidden, {} bytes)", self.0.len())
    }
}

impl Serialize for CodeSecret {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&hex(&self.0))
    }
}

impl<'de> Deserialize<'de> for CodeSecret {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<CodeSecret, D::Error> {
        deserializer.deserialize_str(CodeSecretVisitor)
    }
}

struct CodeSecretVisitor;

impl serde::de::Visitor<'_> for CodeSecretVisitor {
    type Value = CodeSecret;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("hex characters")
    }

    fn visit_str<E: serde::de::Error>(self, text: &str) -> Result<CodeSecret, E> {
        unhex_vec(text).map(CodeSecret).ok_or_else(|| E::custom("not hex characters"))
    }
}

/// What a phone says about itself when it pairs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PairRequest {
    /// The phone's name, as the family will see it ("Asha's phone").
    pub name: String,
    /// Which form of the app is pairing.
    pub form: Form,
    /// The platform label, "android".
    pub platform: String,
    /// Android's API level (`Build.VERSION.SDK_INT`), such as 33 for
    /// Android 13; none on anything else.
    pub android_version: Option<u32>,
}

impl PairRequest {
    fn check(&self) -> Result<(), LinkError> {
        let name = self.name.trim().chars().count();
        if name == 0 || self.name.chars().count() > MAX_NAME_CHARS {
            return Err(LinkError::OutOfBounds { what: "a phone name that is empty or too long" });
        }
        let platform = self.platform.chars().count();
        if platform == 0 || platform > MAX_PLATFORM_CHARS {
            return Err(LinkError::OutOfBounds { what: "a platform label that is empty or too long" });
        }
        Ok(())
    }
}

/// What a phone says to the master.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "say", rename_all = "snake_case", deny_unknown_fields)]
pub enum PhoneSays {
    /// On a pairing link, once.
    Pair {
        /// The phone's name, as the family will see it.
        name: String,
        /// Which form of the app.
        form: Form,
        /// The platform label.
        platform: String,
        /// Android's API level.
        android_version: Option<u32>,
    },
    /// A report, for a grant.
    Report {
        /// The core's report, boxed: it is much the largest message, and the
        /// others stay small.
        report: Box<Report>,
    },
    /// The phone's apps.
    Apps {
        /// The apps, in the phone's order.
        apps: Vec<AppEntry>,
        /// [`apps_hash`] of the list.
        hash: String,
    },
    /// Asks for a pong. (An empty struct, not a bare name, so that a field
    /// it should not have is refused.)
    Ping {},
    /// The phone is leaving.
    Bye {},
}

impl PhoneSays {
    /// Checks the bounds a message from a phone must keep.
    pub(crate) fn check(&self) -> Result<(), LinkError> {
        match self {
            PhoneSays::Pair { name, form, platform, android_version } => PairRequest {
                name: name.clone(),
                form: *form,
                platform: platform.clone(),
                android_version: *android_version,
            }
            .check(),
            PhoneSays::Apps { apps, hash } => check_apps(apps, Some(hash)),
            PhoneSays::Report { .. } | PhoneSays::Ping {} | PhoneSays::Bye {} => Ok(()),
        }
    }
}

/// Checks an app list, and its hash when one is given.
pub(crate) fn check_apps(apps: &[AppEntry], hash: Option<&str>) -> Result<(), LinkError> {
    if apps.len() > MAX_APPS {
        return Err(LinkError::OutOfBounds { what: "more apps than a list may have" });
    }
    for app in apps {
        if app.package.is_empty() || app.package.len() > MAX_PACKAGE_BYTES {
            return Err(LinkError::OutOfBounds { what: "an app package name that is empty or too long" });
        }
        if app.label.chars().count() > MAX_LABEL_CHARS {
            return Err(LinkError::OutOfBounds { what: "an app label that is too long" });
        }
    }
    if let Some(hash) = hash
        && hash != apps_hash(apps)
    {
        return Err(LinkError::OutOfBounds { what: "an app list whose hash is not its own" });
    }
    Ok(())
}

/// What the master says to a phone.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "say", rename_all = "snake_case", deny_unknown_fields)]
pub enum MasterSays {
    /// The answer to `pair`.
    Welcome {
        /// The first grant: the phone makes its state from it with
        /// `DeviceState::from_pairing`.
        welcome: Grant,
        /// The phone's code secret.
        code_secret: CodeSecret,
        /// The master's clock, unix seconds.
        master_now: i64,
        /// The child's name, as the guardian gave it on the master, for the
        /// child's own screens; at most [`MAX_NAME_CHARS`] characters. Left
        /// out by a master that does not say it.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        child_name: Option<String>,
    },
    /// The answer to a report.
    Grant {
        /// The core's grant.
        grant: Grant,
        /// The master's clock, unix seconds.
        master_now: i64,
    },
    /// Please report at once.
    ReportNow {},
    /// No; the link ends.
    Refused {
        /// Why.
        why: Refusal,
        /// The link version the master speaks.
        speaks: u8,
    },
    /// The answer to a ping.
    Pong {
        /// The master's clock, unix seconds.
        master_now: i64,
    },
}

/// Writes JSON into a buffer that is wiped when dropped and, when it must
/// grow, wipes the old one before letting it go: so no copy of a secret is
/// left behind in freed memory.
pub(crate) struct WipingWriter {
    buffer: Zeroizing<Vec<u8>>,
}

impl WipingWriter {
    pub fn new() -> WipingWriter {
        WipingWriter { buffer: Zeroizing::new(Vec::with_capacity(4096)) }
    }

    pub fn into_inner(self) -> Zeroizing<Vec<u8>> {
        self.buffer
    }
}

impl io::Write for WipingWriter {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        let needed = self.buffer.len().saturating_add(data.len());
        if needed > crate::MAX_MESSAGE_BYTES {
            return Err(io::Error::other("longer than a message may be"));
        }
        if needed > self.buffer.capacity() {
            let mut bigger = Zeroizing::new(Vec::with_capacity(needed.max(self.buffer.capacity() * 2)));
            bigger.extend_from_slice(&self.buffer);
            // The old buffer is wiped as it is dropped here.
            self.buffer = bigger;
        }
        self.buffer.extend_from_slice(data);
        Ok(data.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// A message as JSON, in memory that is wiped.
pub(crate) fn to_json<T: Serialize>(message: &T) -> Result<Zeroizing<Vec<u8>>, LinkError> {
    let mut writer = WipingWriter::new();
    match serde_json::to_writer(&mut writer, message) {
        Ok(()) => Ok(writer.into_inner()),
        Err(e) if e.is_io() => Err(LinkError::MessageTooLong { bytes: crate::MAX_MESSAGE_BYTES + 1 }),
        Err(_) => Err(LinkError::Internal { what: "write a message as JSON" }),
    }
}

/// A message from JSON. The error says where the reading stopped, never
/// what was there.
pub(crate) fn from_json<T: DeserializeOwned>(text: &[u8]) -> Result<T, LinkError> {
    serde_json::from_slice(text).map_err(|e| LinkError::Malformed { line: e.line(), column: e.column() })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_app_list_hash_is_fixed_and_follows_every_change() {
        let apps = vec![
            AppEntry { package: "com.example.game".into(), label: "Game".into() },
            AppEntry { package: "com.example.maps".into(), label: "Maps".into() },
        ];
        let hash = apps_hash(&apps);
        assert_eq!(hash.len(), 32);
        assert_eq!(apps_hash(&apps), hash);
        // Worked out separately with Python's hashlib.
        assert_eq!(apps_hash(&[]), "d0a1652e66aeda9f7a69ce933a28e059");
        assert_eq!(apps_hash(&apps[..1]), "727ff9ebabe1f146643f8061ce9365b6");
        let mut changed = apps.clone();
        changed[1].label = "Map".into();
        assert_ne!(apps_hash(&changed), hash);
        changed.swap(0, 1);
        assert_ne!(apps_hash(&changed), hash);
        // A package and a label cannot trade characters unnoticed.
        let a = [AppEntry { package: "ab".into(), label: "c".into() }];
        let b = [AppEntry { package: "a".into(), label: "bc".into() }];
        assert_ne!(apps_hash(&a), apps_hash(&b));
    }

    #[test]
    fn every_message_goes_to_json_and_back() {
        let apps = vec![AppEntry { package: "com.example.game".into(), label: "Game".into() }];
        for message in [
            PhoneSays::Pair { name: "Asha's phone".into(), form: Form::Child, platform: "android".into(), android_version: Some(33) },
            PhoneSays::Apps { hash: apps_hash(&apps), apps },
            PhoneSays::Ping {},
            PhoneSays::Bye {},
        ] {
            let json = to_json(&message).unwrap();
            assert_eq!(from_json::<PhoneSays>(&json).unwrap(), message);
            message.check().unwrap();
        }
        for message in [
            MasterSays::ReportNow {},
            MasterSays::Refused { why: Refusal::OfferUsed, speaks: 1 },
            MasterSays::Pong { master_now: 1_800_000_000 },
        ] {
            let json = to_json(&message).unwrap();
            assert_eq!(from_json::<MasterSays>(&json).unwrap(), message);
        }
        assert_eq!(&*to_json(&PhoneSays::Ping {}).unwrap(), br#"{"say":"ping"}"#);
        assert_eq!(
            &*to_json(&MasterSays::Refused { why: Refusal::Version, speaks: 1 }).unwrap(),
            br#"{"say":"refused","why":"version","speaks":1}"#
        );
    }

    #[test]
    fn unknown_and_misshapen_messages_are_refused_by_place_only() {
        for json in [
            "",
            "null",
            r#"{"say":"ping","extra":1}"#,
            r#"{"say":"bye","why":"x"}"#,
            r#"{"say":"take_over"}"#,
            r#"{"say":"pair","name":"x","form":"child","android_version":33}"#,
            r#"{"say":"pair","name":"x","form":"parent","platform":"android","android_version":null}"#,
            r#"{"say":"apps","apps":[{"package":"a","label":"b","icon":"x"}],"hash":""}"#,
        ] {
            assert!(matches!(from_json::<PhoneSays>(json.as_bytes()), Err(LinkError::Malformed { .. })), "{json}");
        }
        // An Android version may be left out: it is none.
        let pair = br#"{"say":"pair","name":"x","form":"child","platform":"android"}"#;
        assert!(matches!(from_json::<PhoneSays>(pair), Ok(PhoneSays::Pair { android_version: None, .. })));
        // A master message is not a phone message, and the other way.
        let pong = to_json(&MasterSays::Pong { master_now: 1 }).unwrap();
        assert!(from_json::<PhoneSays>(&pong).is_err());
        let ping = to_json(&PhoneSays::Ping {}).unwrap();
        assert!(from_json::<MasterSays>(&ping).is_err());
    }

    #[test]
    fn bounds_on_what_a_phone_says() {
        let pair = |name: &str, platform: &str| PhoneSays::Pair {
            name: name.into(),
            form: Form::Child,
            platform: platform.into(),
            android_version: None,
        };
        assert!(pair("Asha's phone", "android").check().is_ok());
        assert!(pair("", "android").check().is_err());
        assert!(pair("   ", "android").check().is_err());
        assert!(pair(&"n".repeat(MAX_NAME_CHARS + 1), "android").check().is_err());
        assert!(pair("Phone", "").check().is_err());
        assert!(pair("Phone", &"p".repeat(MAX_PLATFORM_CHARS + 1)).check().is_err());
        let app = |package: &str, label: &str| AppEntry { package: package.into(), label: label.into() };
        assert!(check_apps(&[app("a", "")], None).is_ok());
        assert!(check_apps(&[app("", "x")], None).is_err());
        assert!(check_apps(&[app(&"p".repeat(MAX_PACKAGE_BYTES + 1), "x")], None).is_err());
        assert!(check_apps(&[app("a", &"l".repeat(MAX_LABEL_CHARS + 1))], None).is_err());
        assert!(check_apps(&vec![app("a", "b"); MAX_APPS + 1], None).is_err());
        assert!(check_apps(&[app("a", "b")], Some("00")).is_err());
        let list = [app("a", "b")];
        assert!(check_apps(&list, Some(&apps_hash(&list))).is_ok());
    }

    #[test]
    fn a_code_secret_never_shows_and_reads_back() {
        let secret = CodeSecret::new(&[0xab; 32]);
        let shown = format!("{secret:?}");
        assert!(!shown.contains("ab") && !shown.contains("171"), "{shown}");
        let json = serde_json::to_string(&secret).unwrap();
        assert_eq!(json, format!("\"{}\"", "ab".repeat(32)));
        assert_eq!(serde_json::from_str::<CodeSecret>(&json).unwrap(), secret);
        let error = serde_json::from_str::<CodeSecret>("\"abzz\"").unwrap_err().to_string();
        assert!(!error.contains("abzz"), "{error}");
        assert_ne!(CodeSecret::new(&[1; 32]), CodeSecret::new(&[2; 32]));
        assert_ne!(CodeSecret::new(&[1; 32]), CodeSecret::new(&[1; 31]));
    }

    #[test]
    fn the_wiping_writer_grows_and_stops_at_the_limit() {
        use std::io::Write;
        let mut writer = WipingWriter::new();
        let chunk = vec![b'x'; 10_000];
        for _ in 0..10 {
            writer.write_all(&chunk).unwrap();
        }
        assert_eq!(writer.into_inner().len(), 100_000);
        let mut writer = WipingWriter::new();
        assert!(writer.write_all(&vec![b'x'; crate::MAX_MESSAGE_BYTES + 1]).is_err());
    }
}
