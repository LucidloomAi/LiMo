//! What an APK says it is, read before anything is installed: its package
//! name, its version name, and whether it is a debug build. The console
//! uses it to install nothing but LiMo's child form as a release build; the
//! phone development tool, to install nothing but its debug build.
//!
//! An APK is a zip file; Android's compiled `AndroidManifest.xml` is in it,
//! in Android's binary XML. Only what is needed is read: the zip's central
//! directory, that one entry, and in it the `manifest` element's `package`
//! and `versionName` and the `application` element's `debuggable`.

#![forbid(unsafe_code)]

use std::fmt;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

/// The most a manifest may take once inflated: 16 MiB. A real one is a few
/// kilobytes.
pub const MAX_MANIFEST_BYTES: usize = 16 * 1024 * 1024;
/// Android's resource id of the `debuggable` attribute.
pub const ATTR_DEBUGGABLE: u32 = 0x0101_000f;
/// The name of the manifest inside an APK.
pub const MANIFEST_ENTRY: &str = "AndroidManifest.xml";

/// What an APK says it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApkFacts {
    /// Its package name.
    pub package: String,
    /// Its version name, if it gives one.
    pub version_name: Option<String>,
    /// True when it is a debug build (`android:debuggable` is true).
    pub debuggable: bool,
}

/// Why an APK could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApkProblem {
    /// The file could not be opened or read.
    Read(String),
    /// It is not a zip file, so not an APK.
    NotAZip,
    /// It is a zip file with no Android manifest in it.
    NoManifest,
    /// The manifest is there but could not be read.
    BadManifest(&'static str),
}

impl fmt::Display for ApkProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ApkProblem::Read(why) => write!(f, "The file could not be read: {why}."),
            ApkProblem::NotAZip => f.write_str("The file is not an APK: it is not even a zip file."),
            ApkProblem::NoManifest => f.write_str("The file is not an APK: it has no Android manifest."),
            ApkProblem::BadManifest(what) => write!(f, "The APK's manifest could not be read: {what}."),
        }
    }
}

/// Reads what the APK at `path` says it is.
pub fn read_apk(path: &Path) -> Result<ApkFacts, ApkProblem> {
    let mut file = std::fs::File::open(path).map_err(|e| ApkProblem::Read(e.to_string()))?;
    let manifest = manifest_bytes(&mut file)?;
    manifest_facts(&manifest)
}

/// What an APK already in memory says it is.
pub fn apk_facts(apk: &[u8]) -> Result<ApkFacts, ApkProblem> {
    let manifest = manifest_bytes(&mut std::io::Cursor::new(apk))?;
    manifest_facts(&manifest)
}

fn u16_at(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(at..at.checked_add(2)?)?.try_into().ok()?))
}

fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(at..at.checked_add(4)?)?.try_into().ok()?))
}

fn read_exact_at<R: Read + Seek>(r: &mut R, at: u64, n: usize) -> Result<Vec<u8>, ApkProblem> {
    r.seek(SeekFrom::Start(at)).map_err(|e| ApkProblem::Read(e.to_string()))?;
    let mut buf = vec![0u8; n];
    r.read_exact(&mut buf).map_err(|_| ApkProblem::NotAZip)?;
    Ok(buf)
}

/// The manifest's bytes, inflated, from a zip file.
pub fn manifest_bytes<R: Read + Seek>(r: &mut R) -> Result<Vec<u8>, ApkProblem> {
    const EOCD: u32 = 0x0605_4b50;
    const CENTRAL: u32 = 0x0201_4b50;
    const LOCAL: u32 = 0x0403_4b50;
    let len = r.seek(SeekFrom::End(0)).map_err(|e| ApkProblem::Read(e.to_string()))?;
    let tail_len = len.min(22 + 65_535);
    let tail = read_exact_at(r, len - tail_len, tail_len as usize)?;
    let eocd = (0..tail.len().saturating_sub(21)).rev().find(|&i| u32_at(&tail, i) == Some(EOCD)).ok_or(ApkProblem::NotAZip)?;
    let entries = u16_at(&tail, eocd + 10).ok_or(ApkProblem::NotAZip)?;
    let cd_size = u32_at(&tail, eocd + 12).ok_or(ApkProblem::NotAZip)?;
    let cd_offset = u32_at(&tail, eocd + 16).ok_or(ApkProblem::NotAZip)?;
    if cd_size == u32::MAX || cd_offset == u32::MAX || u64::from(cd_offset) + u64::from(cd_size) > len {
        return Err(ApkProblem::NotAZip);
    }
    let cd = read_exact_at(r, u64::from(cd_offset), cd_size as usize)?;
    let mut at = 0usize;
    for _ in 0..entries {
        if u32_at(&cd, at) != Some(CENTRAL) {
            return Err(ApkProblem::NotAZip);
        }
        let field = |offset: usize| u16_at(&cd, at + offset).map(usize::from).ok_or(ApkProblem::NotAZip);
        let method = field(10)?;
        let compressed = u32_at(&cd, at + 20).ok_or(ApkProblem::NotAZip)?;
        let size = u32_at(&cd, at + 24).ok_or(ApkProblem::NotAZip)?;
        let (name_len, extra_len, comment_len) = (field(28)?, field(30)?, field(32)?);
        let local = u32_at(&cd, at + 42).ok_or(ApkProblem::NotAZip)?;
        let name = cd.get(at + 46..at + 46 + name_len).ok_or(ApkProblem::NotAZip)?;
        if name == MANIFEST_ENTRY.as_bytes() {
            if compressed as usize > MAX_MANIFEST_BYTES || size as usize > MAX_MANIFEST_BYTES {
                return Err(ApkProblem::BadManifest("it is far too large"));
            }
            let header = read_exact_at(r, u64::from(local), 30)?;
            if u32_at(&header, 0) != Some(LOCAL) {
                return Err(ApkProblem::NotAZip);
            }
            let skip = 30 + usize::from(u16_at(&header, 26).unwrap_or(0)) + usize::from(u16_at(&header, 28).unwrap_or(0));
            let data = read_exact_at(r, u64::from(local) + skip as u64, compressed as usize)?;
            return match method {
                0 => Ok(data),
                8 => miniz_oxide::inflate::decompress_to_vec_with_limit(&data, MAX_MANIFEST_BYTES)
                    .map_err(|_| ApkProblem::BadManifest("it does not inflate")),
                _ => Err(ApkProblem::BadManifest("it is compressed in a way this tool does not read")),
            };
        }
        at += 46 + name_len + extra_len + comment_len;
    }
    Err(ApkProblem::NoManifest)
}

/// The strings of a binary XML string pool.
fn string_pool(chunk: &[u8]) -> Option<Vec<String>> {
    let header = usize::from(u16_at(chunk, 2)?);
    let count = u32_at(chunk, 8)? as usize;
    let utf8 = u32_at(chunk, 16)? & 0x100 != 0;
    let start = u32_at(chunk, 20)? as usize;
    if count > 1_000_000 {
        return None;
    }
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let at = start.checked_add(u32_at(chunk, header + i * 4)? as usize)?;
        out.push(if utf8 { utf8_string(chunk, at)? } else { utf16_string(chunk, at)? });
    }
    Some(out)
}

fn utf8_string(b: &[u8], at: usize) -> Option<String> {
    let mut at = at;
    let mut length = || -> Option<usize> {
        let first = *b.get(at)?;
        at += 1;
        if first & 0x80 == 0 {
            return Some(usize::from(first));
        }
        let second = *b.get(at)?;
        at += 1;
        Some((usize::from(first & 0x7f) << 8) | usize::from(second))
    };
    let _chars = length()?;
    let bytes = length()?;
    Some(String::from_utf8_lossy(b.get(at..at.checked_add(bytes)?)?).into_owned())
}

fn utf16_string(b: &[u8], at: usize) -> Option<String> {
    let first = u16_at(b, at)?;
    let (units, start) = if first & 0x8000 == 0 {
        (usize::from(first), at + 2)
    } else {
        ((usize::from(first & 0x7fff) << 16) | usize::from(u16_at(b, at + 2)?), at + 4)
    };
    let mut code = Vec::with_capacity(units);
    for i in 0..units {
        code.push(u16_at(b, start + i * 2)?);
    }
    Some(String::from_utf16_lossy(&code))
}

/// What a compiled manifest says.
pub fn manifest_facts(axml: &[u8]) -> Result<ApkFacts, ApkProblem> {
    const XML: u16 = 0x0003;
    const POOL: u16 = 0x0001;
    const RESOURCE_MAP: u16 = 0x0180;
    const START_ELEMENT: u16 = 0x0102;
    const STRING: u8 = 0x03;
    const BOOLEAN: u8 = 0x12;
    let bad = ApkProblem::BadManifest("it is not Android's binary XML");
    if u16_at(axml, 0) != Some(XML) {
        return Err(bad);
    }
    let total = (u32_at(axml, 4).ok_or(ApkProblem::BadManifest("it is cut short"))? as usize).min(axml.len());
    let mut at = usize::from(u16_at(axml, 2).ok_or(ApkProblem::BadManifest("it is cut short"))?);
    let mut strings: Vec<String> = Vec::new();
    let mut ids: Vec<u32> = Vec::new();
    let mut package = None;
    let mut version_name = None;
    let mut debuggable = false;
    while at + 8 <= total {
        let kind = u16_at(axml, at).unwrap_or(0);
        let header = usize::from(u16_at(axml, at + 2).unwrap_or(0));
        let size = u32_at(axml, at + 4).unwrap_or(0) as usize;
        if size < 8 || at + size > total {
            return Err(ApkProblem::BadManifest("a part of it is cut short"));
        }
        let chunk = &axml[at..at + size];
        match kind {
            POOL => strings = string_pool(chunk).ok_or(ApkProblem::BadManifest("its strings are damaged"))?,
            RESOURCE_MAP => ids = (header..size).step_by(4).filter_map(|i| u32_at(chunk, i)).collect(),
            START_ELEMENT => {
                let text = |index: u32| strings.get(index as usize).map(String::as_str);
                let ext = header;
                let element = u32_at(chunk, ext + 4).and_then(text).unwrap_or_default();
                let attr_start = usize::from(u16_at(chunk, ext + 8).unwrap_or(0));
                let attr_size = usize::from(u16_at(chunk, ext + 10).unwrap_or(0)).max(20);
                let count = usize::from(u16_at(chunk, ext + 12).unwrap_or(0));
                for i in 0..count {
                    let a = ext + attr_start + i * attr_size;
                    let (Some(name), Some(raw), Some(&value_type), Some(data)) =
                        (u32_at(chunk, a + 4), u32_at(chunk, a + 8), chunk.get(a + 15), u32_at(chunk, a + 16))
                    else {
                        return Err(ApkProblem::BadManifest("an attribute is cut short"));
                    };
                    let attr = text(name).unwrap_or_default();
                    let id = ids.get(name as usize).copied();
                    let value = || match (raw, value_type) {
                        (r, _) if r != u32::MAX => text(r).map(str::to_string),
                        (_, STRING) => text(data).map(str::to_string),
                        _ => None,
                    };
                    match (element, attr) {
                        ("manifest", "package") => package = value(),
                        ("manifest", "versionName") => version_name = value(),
                        ("application", _) if attr == "debuggable" || id == Some(ATTR_DEBUGGABLE) => {
                            debuggable = value_type == BOOLEAN && data != 0;
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
        at += size;
    }
    let package = package.ok_or(ApkProblem::BadManifest("it names no package"))?;
    Ok(ApkFacts { package, version_name, debuggable })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_are_read_little_end_first_and_never_past_the_end() {
        let b = [0x34, 0x12, 0x78, 0x56, 0x34, 0x12];
        assert_eq!(u16_at(&b, 0), Some(0x1234));
        assert_eq!(u32_at(&b, 2), Some(0x1234_5678));
        assert_eq!(u32_at(&b, 3), None);
        assert_eq!(u16_at(&b, usize::MAX), None);
    }

    /// Reads a real APK when one is named in `LIMO_TEST_APK`, with
    /// `cargo test real_apk -- --ignored`. The phone development tool's
    /// tests (`phone/devtool/tests/devtool/apk.rs`) read APKs built in memory.
    #[test]
    #[ignore]
    fn real_apk() {
        let path = std::env::var("LIMO_TEST_APK").expect("set LIMO_TEST_APK to an APK's path");
        let facts = read_apk(Path::new(&path)).unwrap();
        println!("{facts:?}");
    }
}
