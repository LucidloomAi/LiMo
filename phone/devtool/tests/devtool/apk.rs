//! `install`, and reading what an APK says it is: APKs are built here in
//! memory, a zip with Android's binary manifest in it, the way aapt2 writes
//! one.

use limo_phone_dev::apk::{ATTR_DEBUGGABLE, ApkFacts, ApkProblem, MANIFEST_ENTRY, manifest_bytes, manifest_facts, read_apk};
use limo_phone_dev::{DONE, FAILED, REFUSED};

use crate::fake::{Bench, SERIAL};

/// A compiled manifest: `<manifest package=.. versionName=..>` and
/// `<application android:debuggable=..>`, with the strings in UTF-16 or
/// UTF-8.
pub fn manifest(package: &str, debuggable: Option<bool>, utf8: bool) -> Vec<u8> {
    // String 0 is "debuggable", mapped to its resource id, as aapt2 puts the
    // attributes it maps first.
    let strings = ["debuggable", "manifest", "package", package, "versionName", "0.1.0", "application", "http://schemas.android.com/apk/res/android"];
    let mut pool = Vec::new();
    let mut offsets = Vec::new();
    for s in strings {
        offsets.push(pool.len() as u32);
        if utf8 {
            pool.push(s.chars().count() as u8);
            pool.push(s.len() as u8);
            pool.extend_from_slice(s.as_bytes());
            pool.push(0);
        } else {
            let units: Vec<u16> = s.encode_utf16().collect();
            pool.extend_from_slice(&(units.len() as u16).to_le_bytes());
            for u in units {
                pool.extend_from_slice(&u.to_le_bytes());
            }
            pool.extend_from_slice(&[0, 0]);
        }
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
    for v in [strings.len() as u32, 0, if utf8 { 0x100 } else { 0 }, (28 + strings.len() * 4) as u32, 0] {
        pool_header.extend(le(v));
    }
    let mut pool_body: Vec<u8> = offsets.iter().flat_map(|o| o.to_le_bytes()).collect();
    pool_body.extend(pool);
    let pool_chunk = chunk(0x0001, pool_header, pool_body);
    let map_chunk = chunk(0x0180, Vec::new(), le(ATTR_DEBUGGABLE));
    let element = |name: u32, attrs: &[(u32, u32, u8, u32)]| {
        let mut header = Vec::new();
        header.extend(le(1)); // the line
        header.extend(le(u32::MAX)); // no comment
        let mut body = Vec::new();
        body.extend(le(u32::MAX)); // no namespace
        body.extend(le(name));
        body.extend_from_slice(&20u16.to_le_bytes()); // where the attributes start
        body.extend_from_slice(&20u16.to_le_bytes()); // each one's size
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

/// A zip file with these entries, stored or deflated.
pub fn zip(entries: &[(&str, &[u8])], deflate: bool) -> Vec<u8> {
    let mut out = Vec::new();
    let mut central = Vec::new();
    for (name, data) in entries {
        let packed = if deflate { miniz_oxide::deflate::compress_to_vec(data, 6) } else { data.to_vec() };
        let offset = out.len() as u32;
        let method: u16 = if deflate { 8 } else { 0 };
        let mut common = Vec::new();
        common.extend_from_slice(&20u16.to_le_bytes());
        common.extend_from_slice(&0u16.to_le_bytes());
        common.extend_from_slice(&method.to_le_bytes());
        common.extend_from_slice(&[0; 4]);
        common.extend_from_slice(&0u32.to_le_bytes()); // the check sum, not read
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

/// An APK of this package written into the bench's folder; its path.
pub fn apk_file(bench: &Bench, name: &str, package: &str, debuggable: Option<bool>) -> String {
    let path = bench.dir.path().join(name);
    let bytes = zip(&[("classes.dex", b"dex\n035"), (MANIFEST_ENTRY, &manifest(package, debuggable, false))], true);
    std::fs::write(&path, bytes).unwrap();
    path.display().to_string()
}

fn facts_of(zip_bytes: Vec<u8>) -> Result<ApkFacts, ApkProblem> {
    let bytes = manifest_bytes(&mut std::io::Cursor::new(zip_bytes))?;
    manifest_facts(&bytes)
}

#[test]
fn the_package_version_and_debug_flag_are_read_from_a_compiled_manifest() {
    for utf8 in [false, true] {
        for deflate in [false, true] {
            let debug = zip(&[("classes.dex", b"dex"), (MANIFEST_ENTRY, &manifest("ai.lucidloom.limo.child", Some(true), utf8))], deflate);
            assert_eq!(
                facts_of(debug).unwrap(),
                ApkFacts { package: "ai.lucidloom.limo.child".to_string(), version_name: Some("0.1.0".to_string()), debuggable: true }
            );
            let release = zip(&[(MANIFEST_ENTRY, &manifest("ai.lucidloom.limo.child", None, utf8))], deflate);
            assert!(!facts_of(release).unwrap().debuggable);
            let off = zip(&[(MANIFEST_ENTRY, &manifest("ai.lucidloom.limo.child", Some(false), utf8))], deflate);
            assert!(!facts_of(off).unwrap().debuggable);
        }
    }
}

#[test]
fn what_is_not_an_apk_is_said_plainly() {
    assert_eq!(facts_of(b"not a zip at all, just words".to_vec()), Err(ApkProblem::NotAZip));
    assert_eq!(facts_of(zip(&[("readme.txt", b"hello")], true)), Err(ApkProblem::NoManifest));
    assert!(matches!(facts_of(zip(&[(MANIFEST_ENTRY, b"<manifest/>")], false)), Err(ApkProblem::BadManifest(_))));
    let mut cut = manifest("ai.lucidloom.limo.child", Some(true), false);
    cut.truncate(cut.len() / 2);
    assert!(manifest_facts(&cut).is_err());
    assert!(manifest_facts(&[]).is_err());
    let missing = std::env::temp_dir().join("limo-phone-dev-no-such-file.apk");
    assert!(matches!(read_apk(&missing), Err(ApkProblem::Read(_))));
}

#[test]
fn install_refuses_anything_but_a_debug_child_form_and_sends_nothing() {
    let mut bench = Bench::new();
    let guardian = apk_file(&bench, "guardian.apk", "ai.lucidloom.limo", Some(true));
    let other = apk_file(&bench, "game.apk", "org.example.game", Some(true));
    let release = apk_file(&bench, "release.apk", "ai.lucidloom.limo.child", None);
    let not_an_apk = bench.dir.path().join("notes.apk");
    std::fs::write(&not_an_apk, b"just words").unwrap();
    let missing = bench.dir.path().join("missing.apk");
    for (path, says) in [
        (guardian, "LiMo's guardian form (ai.lucidloom.limo), not the child form"),
        (other, "the app org.example.game, not LiMo's child form"),
        (release, "but a release build"),
        (not_an_apk.display().to_string(), "not even a zip file"),
        (missing.display().to_string(), "There is no file"),
    ] {
        bench.clear();
        assert_eq!(bench.run_args(&["install".to_string(), "--apk".to_string(), path.clone()]), REFUSED, "{path}");
        assert!(bench.printed().contains(says), "{path}: {}", bench.printed());
        assert!(bench.printed().contains("Nothing was installed."), "{path}");
        // Refused before adb was touched at all.
        assert!(bench.phone.calls.is_empty(), "{path}");
    }
}

#[test]
fn install_puts_the_debug_child_form_on_the_phone_and_keeps_its_files() {
    let mut bench = Bench::new();
    bench.phone.app = None;
    let apk = apk_file(&bench, "limo-child-debug.apk", "ai.lucidloom.limo.child", Some(true));
    assert_eq!(bench.run_args(&["install".to_string(), "--apk".to_string(), apk.clone()]), DONE, "{}", bench.printed());
    assert!(bench.printed().contains("The APK is LiMo's child form, version 0.1.0, a debug build."));
    assert!(bench.printed().contains("Installed LiMo's child form on the phone."));
    let installs: Vec<_> = bench.phone.calls.iter().filter(|c| c.args.contains(&"install".to_string())).collect();
    assert_eq!(installs.len(), 1);
    assert_eq!(installs[0].args, vec!["-s", SERIAL, "install", "-r", "-t", apk.as_str()]);
    assert!(installs[0].stdin.is_none());
    assert!(bench.log().contains("install put a debug build of LiMo's child form on the phone."));

    // Again, over the one there: its files stay.
    bench.phone.files.insert("files/limo/state".to_string(), b"kept".to_vec());
    bench.clear();
    assert_eq!(bench.run_args(&["install".to_string(), "--apk".to_string(), apk.clone()]), DONE);
    assert!(bench.printed().contains("its files kept, so a test family on it stays"));
    assert!(bench.phone.files.contains_key("files/limo/state"));
}

#[test]
fn an_install_android_refuses_is_said_with_what_to_do() {
    let mut bench = Bench::new();
    let apk = apk_file(&bench, "limo-child-debug.apk", "ai.lucidloom.limo.child", Some(true));
    bench.phone.install_refusal = Some(
        "adb: failed to install x.apk: Failure [INSTALL_FAILED_UPDATE_INCOMPATIBLE: Package ai.lucidloom.limo.child signatures do not match]\n"
            .to_string(),
    );
    assert_eq!(bench.run_args(&["install".to_string(), "--apk".to_string(), apk.clone()]), FAILED);
    assert!(bench.printed().contains("Android refused the install: adb: failed to install x.apk: Failure [INSTALL_FAILED_UPDATE_INCOMPATIBLE"));
    assert!(bench.printed().contains("signed with another key. Run uninstall first"));

    bench.clear();
    bench.phone.install_refusal = None;
    bench.phone.sdk = 25;
    bench.phone.release = "7.1".to_string();
    assert_eq!(bench.run_args(&["install".to_string(), "--apk".to_string(), apk]), REFUSED);
    assert!(bench.printed().contains("LiMo needs Android 8.0 (API 26) or later"));
    assert!(bench.phone.changed_nothing());
}
