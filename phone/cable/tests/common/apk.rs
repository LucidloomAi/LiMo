//! APKs built in memory: a zip with Android's binary manifest in it, the way
//! the phone development tool's tests build them
//! (`phone/devtool/tests/devtool/apk.rs`).

use limo_cable::apk::{ATTR_DEBUGGABLE, MANIFEST_ENTRY};

/// A compiled manifest: `<manifest package=.. versionName=..>` and
/// `<application android:debuggable=..>`, strings in UTF-16.
pub fn manifest(package: &str, debuggable: Option<bool>) -> Vec<u8> {
    let strings = ["debuggable", "manifest", "package", package, "versionName", "0.3.0", "application", "http://schemas.android.com/apk/res/android"];
    let mut pool = Vec::new();
    let mut offsets = Vec::new();
    for s in strings {
        offsets.push(pool.len() as u32);
        let units: Vec<u16> = s.encode_utf16().collect();
        pool.extend_from_slice(&(units.len() as u16).to_le_bytes());
        for u in units {
            pool.extend_from_slice(&u.to_le_bytes());
        }
        pool.extend_from_slice(&[0, 0]);
    }
    while !pool.len().is_multiple_of(4) {
        pool.push(0);
    }
    let chunk = |kind: u16, header: Vec<u8>, body: Vec<u8>| {
        let mut c = Vec::new();
        c.extend_from_slice(&kind.to_le_bytes());
        c.extend_from_slice(&((header.len() + 8) as u16).to_le_bytes());
        c.extend_from_slice(&((header.len() + 8 + body.len()) as u32).to_le_bytes());
        c.extend(header);
        c.extend(body);
        c
    };
    let le = |v: u32| v.to_le_bytes().to_vec();
    let mut pool_header = Vec::new();
    for v in [strings.len() as u32, 0, 0, (28 + strings.len() * 4) as u32, 0] {
        pool_header.extend(le(v));
    }
    let mut pool_body: Vec<u8> = offsets.iter().flat_map(|o| o.to_le_bytes()).collect();
    pool_body.extend(pool);
    let pool_chunk = chunk(0x0001, pool_header, pool_body);
    let map_chunk = chunk(0x0180, Vec::new(), le(ATTR_DEBUGGABLE));
    let element = |name: u32, attrs: &[(u32, u32, u8, u32)]| {
        let mut header = Vec::new();
        header.extend(le(1));
        header.extend(le(u32::MAX));
        let mut body = Vec::new();
        body.extend(le(u32::MAX));
        body.extend(le(name));
        body.extend_from_slice(&20u16.to_le_bytes());
        body.extend_from_slice(&20u16.to_le_bytes());
        body.extend_from_slice(&(attrs.len() as u16).to_le_bytes());
        body.extend_from_slice(&[0; 6]);
        for (attr, raw, kind, data) in attrs {
            body.extend(le(u32::MAX));
            body.extend(le(*attr));
            body.extend(le(*raw));
            body.extend_from_slice(&8u16.to_le_bytes());
            body.push(0);
            body.push(*kind);
            body.extend(le(*data));
        }
        (header, body)
    };
    let (h1, b1) = element(1, &[(2, 3, 0x03, 3), (4, 5, 0x03, 5)]);
    let first = chunk(0x0102, h1, b1);
    let attrs: Vec<(u32, u32, u8, u32)> = debuggable.iter().map(|d| (0, u32::MAX, 0x12, if *d { u32::MAX } else { 0 })).collect();
    let (h2, b2) = element(6, &attrs);
    let second = chunk(0x0102, h2, b2);
    let mut all = Vec::new();
    all.extend(pool_chunk);
    all.extend(map_chunk);
    all.extend(first);
    all.extend(second);
    chunk(0x0003, Vec::new(), all)
}

/// A zip file with these entries, deflated.
pub fn zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut central = Vec::new();
    for (name, data) in entries {
        let packed = miniz_oxide::deflate::compress_to_vec(data, 6);
        let offset = out.len() as u32;
        let mut common = Vec::new();
        common.extend_from_slice(&20u16.to_le_bytes());
        common.extend_from_slice(&0u16.to_le_bytes());
        common.extend_from_slice(&8u16.to_le_bytes());
        common.extend_from_slice(&[0; 4]);
        common.extend_from_slice(&0u32.to_le_bytes());
        common.extend_from_slice(&(packed.len() as u32).to_le_bytes());
        common.extend_from_slice(&(data.len() as u32).to_le_bytes());
        common.extend_from_slice(&(name.len() as u16).to_le_bytes());
        common.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
        out.extend_from_slice(&common);
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(&packed);
        central.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
        central.extend_from_slice(&20u16.to_le_bytes());
        central.extend_from_slice(&common);
        central.extend_from_slice(&[0; 10]);
        central.extend_from_slice(&offset.to_le_bytes());
        central.extend_from_slice(name.as_bytes());
    }
    let cd_offset = out.len() as u32;
    out.extend_from_slice(&central);
    out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
    out.extend_from_slice(&[0; 4]);
    out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    out.extend_from_slice(&(central.len() as u32).to_le_bytes());
    out.extend_from_slice(&cd_offset.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out
}

/// An APK of this package, in memory.
pub fn apk_bytes(package: &str, debuggable: Option<bool>) -> Vec<u8> {
    zip(&[("classes.dex", b"dex\n035"), (MANIFEST_ENTRY, &manifest(package, debuggable))])
}

/// An APK of this package, written to a new file in the system's temporary
/// folder; its path. Every call has a folder of its own, so a test never
/// writes over a file another test is reading: the tests of one binary run
/// at the same time, and a file being written over reads as empty or short.
pub fn apk_file(name: &str, package: &str, debuggable: Option<bool>) -> std::path::PathBuf {
    static CALLS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let call = CALLS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("limo-cable-tests-{}", std::process::id())).join(call.to_string());
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(name);
    std::fs::write(&path, apk_bytes(package, debuggable)).unwrap();
    path
}
