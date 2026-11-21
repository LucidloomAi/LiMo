//! Windows' firewall: LiMo's rules, made and removed through the firewall's
//! own interface (`INetFwPolicy2`), never through a command line. Only
//! `limo install` and `limo uninstall` call it, elevated; no test does.

use windows::Win32::Foundation::VARIANT_TRUE;
use windows::Win32::NetworkManagement::WindowsFirewall::{
    INetFwPolicy2, INetFwRule, INetFwRules, NET_FW_ACTION_ALLOW, NET_FW_IP_PROTOCOL_TCP, NET_FW_IP_PROTOCOL_UDP, NET_FW_PROFILE2_DOMAIN,
    NET_FW_PROFILE2_PRIVATE, NET_FW_PROFILE2_PUBLIC, NET_FW_RULE_DIR_IN, NetFwPolicy2, NetFwRule,
};
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance};
use windows::core::BSTR;

use super::com::Com;
use super::error_text;
use crate::installer::{FIREWALL_GROUP, FirewallRule, Profiles, Protocol, RULE_PROFILES, RULE_REMOTE_ADDRESSES};

/// `HRESULT_FROM_WIN32(ERROR_FILE_NOT_FOUND)`: no rule of that name.
const NOT_FOUND: i32 = 0x8007_0002_u32 as i32;

/// The profiles as the firewall's bit mask.
pub(crate) fn profile_mask(profiles: Profiles) -> i32 {
    let mut mask = 0;
    if profiles.private {
        mask |= NET_FW_PROFILE2_PRIVATE.0;
    }
    if profiles.public {
        mask |= NET_FW_PROFILE2_PUBLIC.0;
    }
    if profiles.domain {
        mask |= NET_FW_PROFILE2_DOMAIN.0;
    }
    mask
}

fn rules() -> Result<INetFwRules, String> {
    // SAFETY: makes the firewall's policy object, in this process, and asks
    // it for its rules; both are released when dropped.
    unsafe {
        let policy: INetFwPolicy2 = CoCreateInstance(&NetFwPolicy2, None, CLSCTX_INPROC_SERVER).map_err(|e| error_text(&e))?;
        policy.Rules().map_err(|e| error_text(&e))
    }
}

/// Whether a rule of this name is there.
pub(crate) fn has_rule(name: &str) -> Result<bool, String> {
    let _com = Com::start();
    let rules = rules()?;
    // SAFETY: asks for a rule by name; the answer is released when dropped.
    match unsafe { rules.Item(&BSTR::from(name)) } {
        Ok(_) => Ok(true),
        Err(e) if e.code().0 == NOT_FOUND => Ok(false),
        Err(e) => Err(error_text(&e)),
    }
}

/// Adds a rule as LiMo's installer makes it.
pub(crate) fn add_rule(rule: &FirewallRule) -> Result<(), String> {
    let _com = Com::start();
    let rules = rules()?;
    let protocol = match rule.protocol {
        Protocol::Tcp => NET_FW_IP_PROTOCOL_TCP.0,
        Protocol::Udp => NET_FW_IP_PROTOCOL_UDP.0,
    };
    let text = |e: windows::core::Error| error_text(&e);
    // SAFETY: a new rule object, filled and handed to the firewall; the
    // protocol is set before the port, as the interface asks.
    unsafe {
        let made: INetFwRule = CoCreateInstance(&NetFwRule, None, CLSCTX_INPROC_SERVER).map_err(text)?;
        made.SetName(&BSTR::from(rule.name.as_str())).map_err(text)?;
        made.SetDescription(&BSTR::from(rule.description.as_str())).map_err(text)?;
        made.SetApplicationName(&BSTR::from(rule.program.to_string_lossy().as_ref())).map_err(text)?;
        made.SetProtocol(protocol).map_err(text)?;
        made.SetLocalPorts(&BSTR::from(rule.port.to_string().as_str())).map_err(text)?;
        made.SetRemoteAddresses(&BSTR::from(RULE_REMOTE_ADDRESSES)).map_err(text)?;
        made.SetDirection(NET_FW_RULE_DIR_IN).map_err(text)?;
        made.SetProfiles(profile_mask(RULE_PROFILES)).map_err(text)?;
        made.SetAction(NET_FW_ACTION_ALLOW).map_err(text)?;
        made.SetGrouping(&BSTR::from(FIREWALL_GROUP)).map_err(text)?;
        made.SetEnabled(VARIANT_TRUE).map_err(text)?;
        rules.Add(&made).map_err(text)
    }
}

/// Removes one rule of this name.
pub(crate) fn remove_rule(name: &str) -> Result<(), String> {
    let _com = Com::start();
    let rules = rules()?;
    // SAFETY: removes a rule by name.
    unsafe { rules.Remove(&BSTR::from(name)) }.map_err(|e| error_text(&e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limos_rules_are_on_the_private_and_public_profiles_only() {
        assert_eq!(profile_mask(RULE_PROFILES), 2 | 4);
        assert_eq!(profile_mask(Profiles { private: false, public: false, domain: true }), 1);
        assert_eq!(NOT_FOUND, -2_147_024_894);
    }
}
