//! Words for what a helper got when it started: its UIAccess flag, its
//! integrity level, the z order band of its windows and its process
//! object's security descriptor. Ported from the first spike's report, so a
//! test on the PC shows in the service's log what Windows really applied.

#![forbid(unsafe_code)]

/// The normal desktop band.
pub const BAND_DESKTOP: u32 = 1;
/// The UIAccess band, above every ordinary always on top window.
pub const BAND_UIACCESS: u32 = 2;

/// Low integrity.
pub const IL_LOW: u32 = 0x1000;
/// Medium integrity: a user's ordinary programs.
pub const IL_MEDIUM: u32 = 0x2000;
/// Medium plus integrity (winnt.h's `SECURITY_MANDATORY_MEDIUM_PLUS_RID`),
/// one step above the user's ordinary programs: the guarded helper's level.
pub const IL_MEDIUM_PLUS: u32 = 0x2100;
/// High integrity: an elevated program.
pub const IL_HIGH: u32 = 0x3000;
/// System integrity.
pub const IL_SYSTEM: u32 = 0x4000;

/// The longest security descriptor kept for the log, in characters.
const MAX_SDDL_CHARS: usize = 1_024;

/// An integrity level as hex, named when it is a known one.
pub fn integrity_label(level: u32) -> String {
    let name = match level {
        IL_LOW => Some("low"),
        IL_MEDIUM => Some("medium"),
        IL_MEDIUM_PLUS => Some("medium plus"),
        IL_HIGH => Some("high"),
        IL_SYSTEM => Some("system"),
        _ => None,
    };
    match name {
        Some(name) => format!("0x{level:04x} ({name})"),
        None => format!("0x{level:04x}"),
    }
}

/// A z order band, named when it is one of the two known ones. Band numbers
/// are not documented by Microsoft.
pub fn band_label(band: u32) -> String {
    match band {
        BAND_DESKTOP => format!("{BAND_DESKTOP} (normal desktop band)"),
        BAND_UIACCESS => format!("{BAND_UIACCESS} (UIAccess band)"),
        n => n.to_string(),
    }
}

/// A security descriptor as SDDL, made safe for one log line: only the
/// characters SDDL uses, and at most 1,024 of them.
pub fn safe_sddl(raw: &str) -> String {
    raw.chars()
        .take(MAX_SDDL_CHARS)
        .map(|c| if c.is_ascii_alphanumeric() || ":;()-_,".contains(c) { c } else { '?' })
        .collect()
}

/// The log line for what a helper got.
pub fn about_line(
    session_id: u32,
    pid: u32,
    uiaccess: Option<bool>,
    integrity: Option<u32>,
    band: Option<u32>,
    process_sddl: Option<&str>,
) -> String {
    let unknown = || "unknown".to_string();
    let uiaccess = uiaccess.map_or_else(unknown, |on| if on { "1".into() } else { "0".into() });
    let integrity = integrity.map_or_else(unknown, integrity_label);
    let band = band.map_or_else(unknown, band_label);
    let sddl = process_sddl.map_or_else(unknown, safe_sddl);
    format!(
        "The helper in session {session_id} (pid {pid}) says it got: uiaccess={uiaccess} integrity={integrity} band={band} process_sddl={sddl}"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_a_helper_got_is_said_in_one_safe_line() {
        assert_eq!(integrity_label(0x2100), "0x2100 (medium plus)");
        assert_eq!(integrity_label(0x2010), "0x2010");
        assert_eq!(band_label(2), "2 (UIAccess band)");
        assert_eq!(band_label(7), "7");
        assert_eq!(
            about_line(1, 4242, Some(true), Some(0x2100), Some(2), Some("O:BAD:P(A;;0x1fffff;;;SY)\r\nforged line")),
            "The helper in session 1 (pid 4242) says it got: uiaccess=1 integrity=0x2100 (medium plus) \
             band=2 (UIAccess band) process_sddl=O:BAD:P(A;;0x1fffff;;;SY)??forged?line"
        );
        assert!(about_line(2, 1, None, None, None, None).ends_with("uiaccess=unknown integrity=unknown band=unknown process_sddl=unknown"));
        assert_eq!(safe_sddl(&"A".repeat(5_000)).len(), 1_024);
    }
}
