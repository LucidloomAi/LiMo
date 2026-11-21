//! A master PC played in memory over a real family file in a folder the
//! caller gives (a temporary one), so that a phone's side can be tested
//! with no PC service. Only with the `dev` feature; never part of the
//! product.
//!
//! It does what the Windows service will do with a link, and nothing more:
//! it keeps the offers it made and the phones it paired in memory, pairs a
//! phone with the hub (`Hub::pair_device`) and tells it the child's name in
//! the welcome, answers reports with the hub's
//! grants (`Hub::handle_report`), keeps each phone's app list, and asks a
//! phone to report when the hub wants it to (`Hub::wanted_reports`). The
//! hub does not keep link keys yet (a later piece), which is why this keeps
//! them itself.

use std::collections::BTreeMap;
use std::fmt;
use std::net::Ipv4Addr;
use std::path::Path;

use limo_core::DeviceKind;
use limo_hub::{AgeBand, Hub, HubError};

use crate::discovery::{DEFAULT_PORT, MasterId};
use crate::error::LinkError;
use crate::keys::KeyPair;
use crate::master::{Directory, KnownPhone, MasterEvent, MasterLink, OfferOnFile};
use crate::messages::{AppEntry, Form, Refusal};
use crate::offer::{Offer, OfferId};
use crate::Now;

/// The family password the `dev` master sets on a new family file.
pub const DEV_PASSWORD: &str = "a family password for tests";

/// What went wrong in the `dev` master.
#[derive(Debug)]
pub enum DevError {
    /// The link ended.
    Link(LinkError),
    /// The hub refused.
    Hub(HubError),
}

impl fmt::Display for DevError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DevError::Link(e) => write!(f, "the link: {e}"),
            DevError::Hub(e) => write!(f, "the hub: {e}"),
        }
    }
}

impl std::error::Error for DevError {}

impl From<LinkError> for DevError {
    fn from(e: LinkError) -> DevError {
        DevError::Link(e)
    }
}

impl From<HubError> for DevError {
    fn from(e: HubError) -> DevError {
        DevError::Hub(e)
    }
}

struct DevOffer {
    on_file: OfferOnFile,
    child_id: String,
}

/// The master, played over a family file.
pub struct DevMaster {
    hub: Hub,
    keys: KeyPair,
    offers: Vec<DevOffer>,
    phones: Vec<KnownPhone>,
    apps: BTreeMap<String, Vec<AppEntry>>,
    port: u16,
}

impl fmt::Debug for DevMaster {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DevMaster")
            .field("master_key", &self.keys.public())
            .field("offers", &self.offers.len())
            .field("phones", &self.phones)
            .finish()
    }
}

impl Directory for DevMaster {
    fn offer(&self, id: &OfferId) -> Option<OfferOnFile> {
        self.offers.iter().find(|o| o.on_file.offer.id() == *id).map(|o| o.on_file.clone())
    }

    fn phones(&self) -> Vec<KnownPhone> {
        self.phones.clone()
    }
}

impl DevMaster {
    /// Opens (or makes) the family file `family.limo` in `folder`, with a
    /// family set up and a new master key pair.
    pub fn open(folder: &Path, now: i64) -> Result<DevMaster, DevError> {
        let mut hub = Hub::open(&folder.join("family.limo"))?;
        if !hub.has_family()? {
            hub.create_family(DEV_PASSWORD, now)?;
        }
        Ok(DevMaster {
            hub,
            keys: KeyPair::generate()?,
            offers: Vec::new(),
            phones: Vec::new(),
            apps: BTreeMap::new(),
            port: DEFAULT_PORT,
        })
    }

    /// Adds a child with the core's default rules; returns its id.
    pub fn add_child(&mut self, name: &str, utc_offset_secs: i32, now: i64) -> Result<String, DevError> {
        Ok(self.hub.add_child(name, AgeBand::TenToTwelve, utc_offset_secs, now)?.child_id)
    }

    /// The hub, for what a guardian does at the master.
    pub fn hub(&mut self) -> &mut Hub {
        &mut self.hub
    }

    /// The master's key pair.
    pub fn keys(&self) -> &KeyPair {
        &self.keys
    }

    /// The master's id.
    pub fn master_id(&self) -> MasterId {
        MasterId::of(&self.keys.public())
    }

    /// Makes an offer to pair a phone with this child.
    pub fn make_offer(&mut self, child_id: &str, addresses: Vec<Ipv4Addr>, now: i64) -> Result<Offer, DevError> {
        self.hub.child(child_id)?;
        let offer = Offer::new(self.keys.public(), self.port, addresses, now)?;
        self.offers.push(DevOffer {
            on_file: OfferOnFile { offer: offer.clone(), used: false },
            child_id: child_id.to_string(),
        });
        Ok(offer)
    }

    /// A link for a connection just made.
    pub fn accept(&self, now: Now) -> MasterLink {
        MasterLink::accept(&self.keys, now)
    }

    /// Feeds bytes from a phone into its link and acts on what came, as the
    /// Windows service will: pairs, answers reports, keeps app lists.
    /// Returns what came, for the caller's log.
    pub fn handle(&mut self, link: &mut MasterLink, bytes: &[u8], now: Now) -> Result<Vec<MasterEvent>, DevError> {
        let events = link.receive(bytes, now, &*self)?;
        for event in &events {
            match event {
                MasterEvent::PairRequest { offer_id, phone_key, request } => {
                    let Some(at) = self.offers.iter().position(|o| o.on_file.offer.id() == *offer_id) else {
                        link.refuse(Refusal::Unknown)?;
                        continue;
                    };
                    if self.offers[at].on_file.used {
                        link.refuse(Refusal::OfferUsed)?;
                        continue;
                    }
                    if request.form == Form::Guardian {
                        link.refuse(Refusal::NotSupported)?;
                        continue;
                    }
                    let child_id = self.offers[at].child_id.clone();
                    let pairing = match self.hub.pair_device(
                        &child_id,
                        &request.name,
                        DeviceKind::Portable,
                        &request.platform,
                        now.unix,
                    ) {
                        Ok(pairing) => pairing,
                        Err(e) => {
                            link.refuse(Refusal::NotSupported)?;
                            return Err(e.into());
                        }
                    };
                    self.offers[at].on_file.used = true;
                    let child_name = self.hub.child(&child_id)?.name;
                    self.phones.push(KnownPhone {
                        device_id: pairing.device_id.clone(),
                        child_id,
                        key: *phone_key,
                        removed: false,
                    });
                    link.welcome_child(&pairing.welcome, &pairing.code_secret, &child_name, now)?;
                }
                MasterEvent::Report { report } => match self.hub.handle_report(report, now.unix) {
                    Ok(grant) => link.grant(&grant, now)?,
                    Err(HubError::DeviceRemoved { .. }) => link.refuse(Refusal::Removed)?,
                    Err(HubError::UnknownDevice { .. }) => link.refuse(Refusal::Unknown)?,
                    Err(HubError::DeviceOfAnotherChild { .. }) => link.refuse(Refusal::OtherDevice)?,
                    Err(e) => return Err(e.into()),
                },
                MasterEvent::Apps { apps, .. } => {
                    if let Some(phone) = link.phone() {
                        self.apps.insert(phone.device_id.clone(), apps.clone());
                    }
                }
                MasterEvent::Opened { .. } | MasterEvent::Bye => {}
            }
        }
        Ok(events)
    }

    /// Asks the link's phone to report at once if the hub wants to hear
    /// from it. True when it asked.
    pub fn prod(&mut self, link: &mut MasterLink, now: Now) -> Result<bool, DevError> {
        let Some(phone) = link.phone() else {
            return Ok(false);
        };
        if !self.hub.wanted_reports(now.unix)?.contains(&phone.device_id) {
            return Ok(false);
        }
        link.report_now()?;
        Ok(true)
    }

    /// Removes a phone at the hub; its next session is refused as removed.
    pub fn remove_phone(&mut self, device_id: &str, now: i64) -> Result<(), DevError> {
        self.hub.remove_device(device_id, now)?;
        for phone in self.phones.iter_mut().filter(|p| p.device_id == device_id) {
            phone.removed = true;
        }
        Ok(())
    }

    /// Forgets a phone altogether, as a master that lost its records would.
    pub fn forget_phone(&mut self, device_id: &str) {
        self.phones.retain(|p| p.device_id != device_id);
    }

    /// The phones paired so far.
    pub fn phones_paired(&self) -> &[KnownPhone] {
        &self.phones
    }

    /// The app list a phone sent last.
    pub fn apps_of(&self, device_id: &str) -> Option<&[AppEntry]> {
        self.apps.get(device_id).map(Vec::as_slice)
    }
}
