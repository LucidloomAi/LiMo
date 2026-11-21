// What the build, the inspection and the signing tests share about signing.
//
// A release build is signed with the key that LIMO_RELEASE_KEY_FILE names
// (see "Signing" in README.md), or, when it is not set, with Android's debug
// key, as every debug build is. Nothing here ever prints a password, a path of
// the key or anything from its properties file: checkReleaseKey() returns the
// one line scripts/ReleaseKeyCheck.java prints, which holds none of them.

import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const scriptsDir = path.dirname(fileURLToPath(import.meta.url));
const win = process.platform === 'win32';

export const KEY_VARIABLE = 'LIMO_RELEASE_KEY_FILE';
export const CHECKER = path.join(scriptsDir, 'ReleaseKeyCheck.java');

// Android's debug certificates all have this common name: the one in
// android/app/debug.keystore that `expo prebuild` writes, and the one Android
// Studio makes. Its key is public, so anyone can sign an update with it.
export function isDebugCertificate(subject) {
  return /(^|,\s*)CN=Android Debug(,|$)/.test(subject ?? '');
}

function javaExecutable(env) {
  const home = env.JAVA_HOME;
  if (home) {
    const java = path.join(home, 'bin', win ? 'java.exe' : 'java');
    if (fs.existsSync(java)) return java;
  }
  return 'java';
}

// Checks the key LIMO_RELEASE_KEY_FILE names in `env`, with Java, as Gradle
// will use it. Returns { ok: true, sha256, subject }, or { ok: false, message }
// with a message that holds no password, no path and no value from the file.
// `raw` is everything the checker printed, for the tests to search.
export function checkReleaseKey(env = process.env) {
  const r = spawnSync(javaExecutable(env), [CHECKER], { env, encoding: 'utf8', timeout: 120_000 });
  const raw = `${r.stdout ?? ''}${r.stderr ?? ''}`;
  if (r.error) {
    return {
      ok: false,
      raw,
      message: `Java could not be started to check the release key (${r.error.code ?? 'no code'}). The build needs JDK 17: set JAVA_HOME.`,
    };
  }
  const line = (r.stdout ?? '').split(/\r?\n/).find((l) => /^(OK|FAIL)\t/.test(l));
  if (!line) {
    return {
      ok: false,
      raw,
      message: `The release key check did not run (java exit ${r.status}). Run java scripts/ReleaseKeyCheck.java to see why.`,
    };
  }
  const [kind, a, b] = line.split('\t');
  return kind === 'OK' ? { ok: true, raw, sha256: a, subject: b } : { ok: false, raw, message: a };
}

// The Android SDK's newest build tools.
export function buildTools() {
  const sdk = process.env.ANDROID_HOME || process.env.ANDROID_SDK_ROOT;
  if (!sdk) throw new Error('ANDROID_HOME is not set');
  const dir = path.join(sdk, 'build-tools');
  const versions = fs.readdirSync(dir).filter((v) => /^\d+\.\d+\.\d+$/.test(v));
  versions.sort((a, b) => a.localeCompare(b, undefined, { numeric: true }));
  const latest = path.join(dir, versions[versions.length - 1]);
  return {
    version: path.basename(latest),
    aapt2: path.join(latest, win ? 'aapt2.exe' : 'aapt2'),
    apksigner: path.join(latest, win ? 'apksigner.bat' : 'apksigner'),
    zipalign: path.join(latest, win ? 'zipalign.exe' : 'zipalign'),
  };
}

// Runs one of the build tools. Windows runs a .bat only through a shell,
// which splits on spaces, so every argument is quoted there.
export function tool(exe, args) {
  const shell = win && exe.endsWith('.bat');
  const quoted = shell ? args.map((a) => `"${a}"`) : args;
  const r = spawnSync(shell ? `"${exe}"` : exe, quoted, { encoding: 'utf8', shell, maxBuffer: 64 * 1024 * 1024 });
  if (r.error) throw r.error;
  return { status: r.status, out: `${r.stdout ?? ''}${r.stderr ?? ''}` };
}

// How an APK is signed, as apksigner reads it: whether the signature
// verifies, the schemes it verifies with, and each signer's certificate.
export function apkSigning(file, apksigner) {
  const r = tool(apksigner, ['verify', '-v', '--print-certs', file]);
  const schemes = [...r.out.matchAll(/^Verified using (v[\d.]+) scheme \([^)]*\): true$/gm)].map((m) => m[1]);
  const signers = [];
  for (const m of r.out.matchAll(/^Signer #(\d+) certificate DN: (.*)$/gm)) {
    const sha = new RegExp(`^Signer #${m[1]} certificate SHA-256 digest: ([0-9a-f]+)$`, 'm').exec(r.out)?.[1];
    signers.push({ subject: m[2].trim(), sha256: sha, debug: isDebugCertificate(m[2]) });
  }
  return { verified: r.status === 0 && /^Verifies$/m.test(r.out), schemes, signers };
}

// A certificate's SHA-256 as people usually read it: AB:CD:...
export function colons(sha256) {
  return (sha256 ?? '').toUpperCase().match(/../g)?.join(':') ?? '';
}
