//! `status`: read only. This PC's IPv4 addresses, each network's category,
//! the firewall profiles, the firewall rules that name this program, and the
//! commands that would add or remove a rule. It changes nothing and needs no
//! elevation: it reads through a PowerShell child process (Get-Net* cmdlets)
//! and `netsh ... show`, and never runs the commands it prints.

use crate::args::DEFAULT_PORT;
use crate::netinfo;
use crate::out::{plain, quote};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// The display name of the rule the printed commands add and remove.
pub const RULE_NAME: &str = "LiMo spike 04 hub";

pub fn run() -> u8 {
    let exe = std::env::current_exe()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "limo-hub-spike.exe".to_string());

    plain(format!(
        "limo-hub-spike {} status: read only, changes nothing",
        env!("CARGO_PKG_VERSION")
    ));
    plain(format!("program: {exe}"));
    plain("(asking Windows, this takes a few seconds)");
    plain("");

    let facts = match powershell(&status_script(&exe, DEFAULT_PORT)) {
        Ok(text) => parse_facts(&text),
        Err(e) => {
            let mut f = Facts::default();
            f.errors.push(("PowerShell".to_string(), e));
            f
        }
    };

    print_addresses(&facts);
    print_firewall(&facts);
    print_rules(&facts);
    print_port(&facts, DEFAULT_PORT);
    print_commands(&exe);

    if !facts.errors.is_empty() || !facts.done {
        plain("");
        plain("Could not read everything:");
        for (what, e) in &facts.errors {
            plain(format!("  {what}: {e}"));
        }
        if !facts.done {
            plain("  the PowerShell part did not finish");
        }
        return 1;
    }
    0
}

// printing

fn print_addresses(facts: &Facts) {
    plain("IPv4 addresses, and each network's category as Windows reports it");
    let list = match netinfo::ipv4_addresses() {
        Ok(l) => l,
        Err(e) => {
            plain(format!("  could not list addresses: {e}"));
            Vec::new()
        }
    };
    for a in &list {
        let category = a
            .index
            .and_then(|i| facts.profiles.iter().find(|p| p.index == i))
            .map(|p| p.category.clone())
            .unwrap_or_else(|| "none (no network profile)".to_string());
        plain(format!(
            "  {} index={} {}/{} category={}{}",
            quote(&a.interface),
            a.index.map(|i| i.to_string()).unwrap_or_else(|| "?".to_string()),
            a.ip,
            a.prefix,
            category,
            if a.up { "" } else { " (interface down)" }
        ));
    }
    for p in &facts.profiles {
        if !list.iter().any(|a| a.index == Some(p.index)) {
            plain(format!(
                "  {} index={} (no IPv4 address) category={}",
                quote(&p.alias),
                p.index,
                p.category
            ));
        }
    }
    plain("  A Public network uses the firewall's Public profile, a Private one the Private profile.");
    plain("");
}

fn yes_no(v: &str) -> String {
    match v {
        "True" => "yes".to_string(),
        "False" => "no".to_string(),
        "NotConfigured" => "not set".to_string(),
        "" => "?".to_string(),
        other => other.to_string(),
    }
}

fn print_firewall(facts: &Facts) {
    plain("Windows Firewall profiles, as in force now");
    if facts.fw_profiles.is_empty() {
        plain("  none read");
    }
    for p in &facts.fw_profiles {
        plain(format!(
            "  {}: firewall_on={} default_inbound={} allow_rules_apply={} prompt_on_new_listener={}",
            p.name,
            yes_no(&p.enabled),
            if p.default_inbound.is_empty() { "?" } else { &p.default_inbound },
            yes_no(&p.allow_inbound_rules),
            yes_no(&p.notify_on_listen)
        ));
    }
    if facts.products.is_empty() {
        plain("  third party firewall products registered with Windows Security: none");
    } else {
        plain(format!(
            "  third party firewall products registered with Windows Security: {}",
            facts.products.join(", ")
        ));
    }
    plain("");
}

fn print_rules(facts: &Facts) {
    plain("Firewall rules that name this program");
    let exact: Vec<&Rule> = facts.rules.iter().filter(|r| r.exact).collect();
    let other: Vec<&Rule> = facts.rules.iter().filter(|r| !r.exact).collect();
    if exact.is_empty() {
        plain("  none");
    }
    for r in exact {
        plain(format!("  {}", rule_words(r)));
    }
    if !other.is_empty() {
        plain("  rules for a program of the same file name at another path:");
        for r in other {
            plain(format!("    {} program={}", rule_words(r), r.program));
        }
    }
    plain("  A Block rule wins over an Allow rule for the same traffic.");
    plain("");
}

fn rule_words(r: &Rule) -> String {
    format!(
        "{} action={} direction={} profile={} protocol={} local_port={} enabled={}",
        quote(&r.name),
        r.action,
        r.direction,
        r.profile,
        r.protocol,
        r.local_port,
        yes_no(&r.enabled)
    )
}

fn print_port(facts: &Facts, port: u16) {
    plain(format!("TCP port {port}"));
    match excluded_ranges() {
        Ok(ranges) => match port_in_ranges(port, &ranges) {
            Some((a, b)) => plain(format!(
                "  reserved by Windows: yes, inside the excluded range {a} to {b} (use serve --port 0)"
            )),
            None => plain(format!(
                "  reserved by Windows: no ({} excluded ranges checked)",
                ranges.len()
            )),
        },
        Err(e) => plain(format!("  reserved by Windows: unknown ({e})")),
    }
    if facts.listeners.is_empty() {
        plain("  listening now: nothing");
    }
    for l in &facts.listeners {
        plain(format!(
            "  listening now: {}:{} pid={} program={}",
            l.address, l.port, l.pid, l.process
        ));
    }
    plain("");
}

fn print_commands(exe: &str) {
    plain("Commands for an elevated PowerShell (Run as administrator). This program never runs them.");
    plain("  Add an allow rule for this program, Private and Public networks, local subnet only:");
    plain(format!("    {}", add_rule_command(exe)));
    plain("  Remove that rule:");
    plain(format!("    {}", remove_rule_command()));
    plain("  Remove every rule that names this program, including any Windows made from its prompt:");
    plain(format!("    {}", remove_all_command(exe)));
}

// commands

/// Quotes text for PowerShell in single quotes. A single quote is doubled,
/// and so are the typographic single quotes PowerShell also treats as quotes.
pub fn ps_quote(s: &str) -> String {
    let mut q = String::with_capacity(s.len() + 2);
    q.push('\'');
    for c in s.chars() {
        if matches!(c, '\'' | '\u{2018}' | '\u{2019}' | '\u{201a}' | '\u{201b}') {
            q.push(c);
        }
        q.push(c);
    }
    q.push('\'');
    q
}

pub fn add_rule_command(exe: &str) -> String {
    format!(
        "New-NetFirewallRule -DisplayName {} -Direction Inbound -Action Allow -Program {} -Profile Private,Public -RemoteAddress LocalSubnet",
        ps_quote(RULE_NAME),
        ps_quote(exe)
    )
}

pub fn remove_rule_command() -> String {
    format!("Remove-NetFirewallRule -DisplayName {}", ps_quote(RULE_NAME))
}

pub fn remove_all_command(exe: &str) -> String {
    format!(
        "Get-NetFirewallApplicationFilter | Where-Object Program -ieq {} | Get-NetFirewallRule | Remove-NetFirewallRule",
        ps_quote(exe)
    )
}

// the PowerShell part

/// The read only script. Each fact comes back as one tab separated line.
pub fn status_script(exe: &str, port: u16) -> String {
    let script = r#"
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
try { [Console]::OutputEncoding = [System.Text.Encoding]::UTF8 } catch {}
function Clean([object]$v) { if ($null -eq $v) { return '' }; return (([string]$v) -replace "[`t`r`n]", ' ') }
try {
  Get-NetConnectionProfile | ForEach-Object {
    "PROFILE`t$($_.InterfaceIndex)`t$(Clean $_.InterfaceAlias)`t$(Clean $_.NetworkCategory)"
  }
} catch { "ERROR`tnetwork categories`t$(Clean $_.Exception.Message)" }
try {
  Get-NetFirewallProfile -PolicyStore ActiveStore | ForEach-Object {
    "FWPROFILE`t$(Clean $_.Name)`t$(Clean $_.Enabled)`t$(Clean $_.DefaultInboundAction)`t$(Clean $_.AllowInboundRules)`t$(Clean $_.NotifyOnListen)"
  }
} catch { "ERROR`tfirewall profiles`t$(Clean $_.Exception.Message)" }
try {
  $exe = __EXE__
  $leaf = [System.IO.Path]::GetFileName($exe)
  Get-NetFirewallApplicationFilter -PolicyStore ActiveStore | ForEach-Object {
    $prog = [string]$_.Program
    if ($prog -and $prog -ne 'Any') {
      $full = [Environment]::ExpandEnvironmentVariables($prog)
      $match = ''
      if ($full -ieq $exe) { $match = 'exact' }
      elseif ([System.IO.Path]::GetFileName($full) -ieq $leaf) { $match = 'name' }
      if ($match) {
        $_ | Get-NetFirewallRule | ForEach-Object {
          $pf = $_ | Get-NetFirewallPortFilter
          "RULE`t$match`t$(Clean $_.DisplayName)`t$(Clean $_.Enabled)`t$(Clean $_.Direction)`t$(Clean $_.Action)`t$(Clean $_.Profile)`t$(Clean $pf.Protocol)`t$(Clean $pf.LocalPort)`t$(Clean $full)"
        }
      }
    }
  }
} catch { "ERROR`tfirewall rules`t$(Clean $_.Exception.Message)" }
try {
  Get-CimInstance -Namespace 'root/SecurityCenter2' -ClassName FirewallProduct | ForEach-Object {
    "FWPRODUCT`t$(Clean $_.displayName)"
  }
} catch { "ERROR`tthird party firewalls`t$(Clean $_.Exception.Message)" }
try {
  Get-NetTCPConnection -State Listen -LocalPort __PORT__ -ErrorAction SilentlyContinue | ForEach-Object {
    $pn = ''
    try { $pn = (Get-Process -Id $_.OwningProcess -ErrorAction Stop).ProcessName } catch {}
    "LISTEN`t$(Clean $_.LocalAddress)`t$($_.LocalPort)`t$($_.OwningProcess)`t$(Clean $pn)"
  }
} catch { "ERROR`tlisteners`t$(Clean $_.Exception.Message)" }
"DONE"
"#;
    script
        .replace("__EXE__", &ps_quote(exe))
        .replace("__PORT__", &port.to_string())
}

fn system_tool(rel: &str, fallback: &str) -> PathBuf {
    match std::env::var_os("SystemRoot") {
        Some(root) => Path::new(&root).join(rel),
        None => PathBuf::from(fallback),
    }
}

fn powershell(script: &str) -> Result<String, String> {
    let exe = system_tool(
        r"System32\WindowsPowerShell\v1.0\powershell.exe",
        "powershell.exe",
    );
    let encoded = base64(&utf16le(script));
    let output = Command::new(exe)
        .args(["-NoProfile", "-NonInteractive", "-EncodedCommand", &encoded])
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("could not run PowerShell: {e}"))?;
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn excluded_ranges() -> Result<Vec<(u16, u16)>, String> {
    let exe = system_tool(r"System32\netsh.exe", "netsh.exe");
    let output = Command::new(exe)
        .args(["interface", "ipv4", "show", "excludedportrange", "protocol=tcp"])
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("could not run netsh: {e}"))?;
    if !output.status.success() {
        return Err(format!("netsh ended with {}", output.status));
    }
    Ok(parse_excluded_ranges(&String::from_utf8_lossy(&output.stdout)))
}

// parsing (pure)

#[derive(Debug, Default, PartialEq, Eq)]
pub struct NetProfile {
    pub index: u32,
    pub alias: String,
    pub category: String,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct FwProfile {
    pub name: String,
    pub enabled: String,
    pub default_inbound: String,
    pub allow_inbound_rules: String,
    pub notify_on_listen: String,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Rule {
    /// The rule names exactly this program (not just the same file name).
    pub exact: bool,
    pub name: String,
    pub enabled: String,
    pub direction: String,
    pub action: String,
    pub profile: String,
    pub protocol: String,
    pub local_port: String,
    pub program: String,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Listener {
    pub address: String,
    pub port: String,
    pub pid: String,
    pub process: String,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Facts {
    pub profiles: Vec<NetProfile>,
    pub fw_profiles: Vec<FwProfile>,
    pub rules: Vec<Rule>,
    pub products: Vec<String>,
    pub listeners: Vec<Listener>,
    /// (what, error text)
    pub errors: Vec<(String, String)>,
    /// The script ran to its end.
    pub done: bool,
}

pub fn parse_facts(text: &str) -> Facts {
    let mut f = Facts::default();
    for raw in text.lines() {
        let line = raw.trim_end_matches('\r');
        let p: Vec<&str> = line.split('\t').collect();
        let get = |i: usize| p.get(i).map(|s| s.trim().to_string()).unwrap_or_default();
        match p[0] {
            "PROFILE" if p.len() >= 4 => {
                if let Ok(index) = get(1).parse() {
                    f.profiles.push(NetProfile {
                        index,
                        alias: get(2),
                        category: get(3),
                    });
                }
            }
            "FWPROFILE" if p.len() >= 6 => f.fw_profiles.push(FwProfile {
                name: get(1),
                enabled: get(2),
                default_inbound: get(3),
                allow_inbound_rules: get(4),
                notify_on_listen: get(5),
            }),
            "RULE" if p.len() >= 10 => f.rules.push(Rule {
                exact: get(1) == "exact",
                name: get(2),
                enabled: get(3),
                direction: get(4),
                action: get(5),
                profile: get(6),
                protocol: get(7),
                local_port: get(8),
                program: get(9),
            }),
            "FWPRODUCT" if p.len() >= 2 => {
                let name = get(1);
                if !name.is_empty() {
                    f.products.push(name);
                }
            }
            "LISTEN" if p.len() >= 5 => f.listeners.push(Listener {
                address: get(1),
                port: get(2),
                pid: get(3),
                process: get(4),
            }),
            "ERROR" if p.len() >= 3 => f.errors.push((get(1), get(2))),
            "DONE" => f.done = true,
            _ => {}
        }
    }
    f
}

/// The ranges from `netsh interface ipv4 show excludedportrange`: every line
/// whose first two words are numbers. The text around them is localised, the
/// numbers are not. A trailing `*` (an administered exclusion) is ignored.
pub fn parse_excluded_ranges(text: &str) -> Vec<(u16, u16)> {
    let mut v = Vec::new();
    for line in text.lines() {
        let mut words = line.split_whitespace();
        let (Some(a), Some(b)) = (words.next(), words.next()) else {
            continue;
        };
        if let (Ok(a), Ok(b)) = (a.parse::<u16>(), b.parse::<u16>()) {
            if a <= b {
                v.push((a, b));
            }
        }
    }
    v
}

pub fn port_in_ranges(port: u16, ranges: &[(u16, u16)]) -> Option<(u16, u16)> {
    ranges.iter().copied().find(|&(a, b)| a <= port && port <= b)
}

/// UTF-16, little endian: what PowerShell's -EncodedCommand expects.
pub fn utf16le(s: &str) -> Vec<u8> {
    s.encode_utf16().flat_map(|u| u.to_le_bytes()).collect()
}

/// Standard base64 with padding.
pub fn base64(data: &[u8]) -> String {
    const ABC: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        s.push(ABC[(n >> 18) as usize & 63] as char);
        s.push(ABC[(n >> 12) as usize & 63] as char);
        s.push(if chunk.len() > 1 {
            ABC[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        s.push(if chunk.len() > 2 {
            ABC[n as usize & 63] as char
        } else {
            '='
        });
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_known_values() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
        assert_eq!(base64(&[0xff, 0xfe, 0xfd]), "//79");
        // What PowerShell's own docs show for "dir": ZABpAHIA
        assert_eq!(base64(&utf16le("dir")), "ZABpAHIA");
    }

    #[test]
    fn powershell_quoting() {
        assert_eq!(ps_quote(r"C:\a\b.exe"), r"'C:\a\b.exe'");
        assert_eq!(ps_quote("it's"), "'it''s'");
        assert_eq!(ps_quote("a\u{2019}b"), "'a\u{2019}\u{2019}b'");
        assert_eq!(ps_quote("$env:x `n"), "'$env:x `n'");
    }

    #[test]
    fn the_commands() {
        let exe = r"C:\LiMo\spikes\04-lan-discovery\pc\target\release\limo-hub-spike.exe";
        assert_eq!(
            add_rule_command(exe),
            "New-NetFirewallRule -DisplayName 'LiMo spike 04 hub' -Direction Inbound -Action Allow -Program 'C:\\LiMo\\spikes\\04-lan-discovery\\pc\\target\\release\\limo-hub-spike.exe' -Profile Private,Public -RemoteAddress LocalSubnet"
        );
        assert_eq!(
            remove_rule_command(),
            "Remove-NetFirewallRule -DisplayName 'LiMo spike 04 hub'"
        );
        assert!(remove_all_command(exe).starts_with("Get-NetFirewallApplicationFilter | Where-Object Program -ieq 'C:\\LiMo"));
        assert!(remove_all_command(exe).ends_with("| Get-NetFirewallRule | Remove-NetFirewallRule"));
    }

    #[test]
    fn the_script_only_reads() {
        let s = status_script(r"C:\x\it's.exe", 47810);
        assert!(s.contains(r"$exe = 'C:\x\it''s.exe'"));
        assert!(s.contains("-LocalPort 47810"));
        assert!(!s.contains("__EXE__") && !s.contains("__PORT__"));
        for verb in ["New-Net", "Set-Net", "Remove-Net", "Enable-Net", "Disable-Net", "netsh"] {
            assert!(!s.contains(verb), "the status script must not use {verb}");
        }
    }

    #[test]
    fn parses_the_script_output() {
        let text = "PROFILE\t12\tWi-Fi\tPublic\r\n\
                    PROFILE\t9\tTailscale\tPrivate\n\
                    PROFILE\tx\tbad\tPublic\n\
                    FWPROFILE\tPublic\tTrue\tBlock\tTrue\tTrue\n\
                    RULE\texact\tlimo-hub-spike.exe\tTrue\tInbound\tBlock\tPublic\tTCP\tAny\tC:\\a\\limo-hub-spike.exe\n\
                    RULE\tname\tolder\tTrue\tInbound\tAllow\tPrivate\tUDP\tAny\tD:\\old\\limo-hub-spike.exe\n\
                    FWPRODUCT\tSome Firewall\n\
                    LISTEN\t0.0.0.0\t47810\t4242\tlimo-hub-spike\n\
                    ERROR\tthird party firewalls\tAccess denied\n\
                    noise line\n\
                    DONE\n";
        let f = parse_facts(text);
        assert_eq!(
            f.profiles,
            vec![
                NetProfile { index: 12, alias: "Wi-Fi".into(), category: "Public".into() },
                NetProfile { index: 9, alias: "Tailscale".into(), category: "Private".into() },
            ]
        );
        assert_eq!(f.fw_profiles.len(), 1);
        assert_eq!(f.fw_profiles[0].default_inbound, "Block");
        assert_eq!(f.rules.len(), 2);
        assert!(f.rules[0].exact);
        assert_eq!(f.rules[0].action, "Block");
        assert_eq!(f.rules[0].protocol, "TCP");
        assert!(!f.rules[1].exact);
        assert_eq!(f.rules[1].program, "D:\\old\\limo-hub-spike.exe");
        assert_eq!(f.products, vec!["Some Firewall".to_string()]);
        assert_eq!(f.listeners[0].pid, "4242");
        assert_eq!(f.errors, vec![("third party firewalls".into(), "Access denied".into())]);
        assert!(f.done);
    }

    #[test]
    fn unfinished_output_is_noticed() {
        let f = parse_facts("PROFILE\t1\tWi-Fi\tPublic\n");
        assert!(!f.done);
        assert_eq!(parse_facts(""), Facts::default());
    }

    #[test]
    fn excluded_ranges() {
        let text = "\r\nProtocol tcp Port Exclusion Ranges\r\n\r\nStart Port    End Port\r\n----------    --------\r\n      5357        5357\r\n     47776       47875\r\n     50000       50059     *\r\n\r\n* - Administered port exclusions.\r\n";
        let r = parse_excluded_ranges(text);
        assert_eq!(r, vec![(5357, 5357), (47776, 47875), (50000, 50059)]);
        assert_eq!(port_in_ranges(47810, &r), Some((47776, 47875)));
        assert_eq!(port_in_ranges(47776, &r), Some((47776, 47875)));
        assert_eq!(port_in_ranges(47875, &r), Some((47776, 47875)));
        assert_eq!(port_in_ranges(47876, &r), None);
        assert_eq!(port_in_ranges(47810, &[]), None);
        assert!(parse_excluded_ranges("70000 70001\n9 8\n").is_empty());
    }
}
