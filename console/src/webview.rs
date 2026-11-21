//! The switches WebView2's browser process starts with, by name, as the
//! measurement of its contact with Microsoft tried them (README.md, "What
//! WebView2 contacts").
//!
//! Tauri passes WebView2 one argument of its own when an app passes none:
//! [`TAURI_DEFAULT`]. An app that passes any arguments replaces it, so
//! every set here starts with it.

#![forbid(unsafe_code)]

/// What Tauri (wry 0.57) passes when an app passes nothing: no "mini menu"
/// over selected text, no PDF viewer's own window, no SmartScreen.
pub const TAURI_DEFAULT: &str = "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection";

/// A set of switches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Switches {
    /// Tauri's own, alone.
    TauriDefault,
    /// And `--disable-background-networking`: Chromium's switch for the
    /// requests a browser makes by itself (variations, updates of its
    /// parts, and so on).
    NoBackgroundNetworking,
    /// And `--disable-component-update`: no checks for new versions of the
    /// browser's parts.
    NoComponentUpdate,
    /// Both.
    Both,
}

impl Switches {
    /// Every set, in the order they are measured.
    pub const ALL: [Switches; 4] = [Switches::TauriDefault, Switches::NoBackgroundNetworking, Switches::NoComponentUpdate, Switches::Both];

    /// The set's name on the command line.
    pub fn name(self) -> &'static str {
        match self {
            Switches::TauriDefault => "default",
            Switches::NoBackgroundNetworking => "no-background-networking",
            Switches::NoComponentUpdate => "no-component-update",
            Switches::Both => "both",
        }
    }

    /// The set named so.
    pub fn from_name(name: &str) -> Option<Switches> {
        Switches::ALL.into_iter().find(|s| s.name() == name)
    }

    /// The arguments for WebView2's browser process. With `proxy_port`, it
    /// reaches the network only through a proxy on that loopback port (the
    /// measurement's host log); WebView2 never sends the console's own
    /// pages there, which it serves from inside the exe.
    pub fn arguments(self, proxy_port: Option<u16>) -> String {
        let mut args = TAURI_DEFAULT.to_string();
        match self {
            Switches::TauriDefault => {}
            Switches::NoBackgroundNetworking => args.push_str(" --disable-background-networking"),
            Switches::NoComponentUpdate => args.push_str(" --disable-component-update"),
            Switches::Both => args.push_str(" --disable-background-networking --disable-component-update"),
        }
        if let Some(port) = proxy_port {
            args.push_str(&format!(" --proxy-server=http://127.0.0.1:{port}"));
        }
        args
    }
}

/// The set the console starts WebView2 with.
pub const CONSOLE: Switches = Switches::Both;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_set_keeps_tauri_s_own_and_has_a_name() {
        for s in Switches::ALL {
            assert!(s.arguments(None).starts_with(TAURI_DEFAULT));
            assert_eq!(Switches::from_name(s.name()), Some(s));
        }
        assert_eq!(Switches::TauriDefault.arguments(None), TAURI_DEFAULT);
        assert_eq!(
            Switches::Both.arguments(Some(50123)),
            format!("{TAURI_DEFAULT} --disable-background-networking --disable-component-update --proxy-server=http://127.0.0.1:50123")
        );
        assert_eq!(Switches::from_name("everything"), None);
    }
}
