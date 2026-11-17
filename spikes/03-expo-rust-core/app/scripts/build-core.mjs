// Builds the Rust core for Android, generates the Kotlin bindings with
// UniFFI, and copies both into the local Expo module:
//
//   modules/limo-core/android/src/generated/jniLibs/<abi>/liblimo_core_spike.so
//   modules/limo-core/android/src/generated/kotlin/ai/lucidloom/limo/core/ffi/limo_core_spike.kt
//
// Both are build output and git ignored. Gradle runs this script before every
// build of the module (see modules/limo-core/android/build.gradle); it can
// also be run by hand: `npm run build:core`.
//
// Uses the NDK's clang as the linker directly, so cargo-ndk is not needed.
//
// Settings, all optional:
//   LIMO_ABIS         Android ABIs, comma separated. Default: arm64-v8a.
//                     Gradle passes the app's reactNativeArchitectures.
//   LIMO_NDK_VERSION  NDK folder under <sdk>/ndk. Default: 27.1.12297006,
//                     the one React Native 0.83 asks for.
//   ANDROID_NDK_HOME  Full NDK path, used instead of the two above.
//   ANDROID_HOME      SDK path. Also read: ANDROID_SDK_ROOT, then the usual
//                     place under the user's profile.

import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const CRATE = 'limo_core_spike';
const LIB_FILE = `lib${CRATE}.so`;
// Lowest Android API level the app supports (React Native 0.83: 24). The
// linker target is named after it.
const MIN_SDK = 24;
const DEFAULT_NDK_VERSION = '27.1.12297006';

// Android ABI name: Rust target, clang target prefix.
const ABIS = {
  'arm64-v8a': { rust: 'aarch64-linux-android', clang: 'aarch64-linux-android' },
  'armeabi-v7a': { rust: 'armv7-linux-androideabi', clang: 'armv7a-linux-androideabi' },
  'x86_64': { rust: 'x86_64-linux-android', clang: 'x86_64-linux-android' },
  'x86': { rust: 'i686-linux-android', clang: 'i686-linux-android' },
};

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const appDir = path.resolve(scriptDir, '..');
const coreDir = path.resolve(appDir, '..', 'core');
const moduleAndroidDir = path.join(appDir, 'modules', 'limo-core', 'android');
const generatedDir = path.join(moduleAndroidDir, 'src', 'generated');
const jniLibsDir = path.join(generatedDir, 'jniLibs');
const kotlinDir = path.join(generatedDir, 'kotlin');

function fail(message) {
  console.error(`build-core: ${message}`);
  process.exit(1);
}

function log(message) {
  console.log(`build-core: ${message}`);
}

function run(command, args, options = {}) {
  const result = spawnSync(command, args, { stdio: 'inherit', ...options });
  if (result.error) fail(`could not start ${command}: ${result.error.message}`);
  if (result.status !== 0) fail(`${command} ${args.join(' ')} failed with exit code ${result.status}`);
}

function capture(command, args) {
  const result = spawnSync(command, args, { encoding: 'utf8' });
  if (result.error) fail(`could not start ${command}: ${result.error.message}`);
  if (result.status !== 0) fail(`${command} ${args.join(' ')} failed: ${result.stderr}`);
  return result.stdout;
}

function wantedAbis() {
  const raw = process.env.LIMO_ABIS || 'arm64-v8a';
  const abis = raw.split(',').map((s) => s.trim()).filter(Boolean);
  for (const abi of abis) {
    if (!ABIS[abi]) fail(`unknown ABI "${abi}". Known: ${Object.keys(ABIS).join(', ')}`);
  }
  return abis;
}

function findNdk() {
  const fromEnv = process.env.ANDROID_NDK_HOME;
  if (fromEnv) {
    if (!fs.existsSync(fromEnv)) fail(`ANDROID_NDK_HOME points to ${fromEnv}, which does not exist`);
    return fromEnv;
  }
  const sdkCandidates = [
    process.env.ANDROID_HOME,
    process.env.ANDROID_SDK_ROOT,
    process.platform === 'win32' && process.env.LOCALAPPDATA
      ? path.join(process.env.LOCALAPPDATA, 'Android', 'Sdk')
      : null,
    process.platform === 'darwin' ? path.join(os.homedir(), 'Library', 'Android', 'sdk') : null,
    process.platform === 'linux' ? path.join(os.homedir(), 'Android', 'Sdk') : null,
  ].filter(Boolean);
  const sdk = sdkCandidates.find((p) => fs.existsSync(p));
  if (!sdk) fail('no Android SDK found. Set ANDROID_HOME.');
  const version = process.env.LIMO_NDK_VERSION || DEFAULT_NDK_VERSION;
  const ndk = path.join(sdk, 'ndk', version);
  if (!fs.existsSync(ndk)) {
    fail(`NDK ${version} not found at ${ndk}. Install it with the SDK Manager, or set LIMO_NDK_VERSION to one that is installed.`);
  }
  return ndk;
}

function findClang(ndk) {
  const hostTag = { win32: 'windows-x86_64', darwin: 'darwin-x86_64', linux: 'linux-x86_64' }[process.platform];
  if (!hostTag) fail(`unsupported build platform ${process.platform}`);
  const clang = path.join(ndk, 'toolchains', 'llvm', 'prebuilt', hostTag, 'bin', process.platform === 'win32' ? 'clang.exe' : 'clang');
  if (!fs.existsSync(clang)) fail(`clang not found at ${clang}`);
  return clang;
}

function checkRustTargets(abis) {
  const installed = new Set(capture('rustup', ['target', 'list', '--installed']).split(/\r?\n/).map((s) => s.trim()));
  const missing = abis.map((abi) => ABIS[abi].rust).filter((t) => !installed.has(t));
  if (missing.length > 0) fail(`Rust targets missing. Run: rustup target add ${missing.join(' ')}`);
}

// Copies only when the content differs, so Gradle sees no change on a
// rebuild with nothing new and skips the work after it.
function copyIfChanged(from, to) {
  const next = fs.readFileSync(from);
  if (fs.existsSync(to) && Buffer.compare(fs.readFileSync(to), next) === 0) return false;
  fs.mkdirSync(path.dirname(to), { recursive: true });
  fs.writeFileSync(to, next);
  return true;
}

function listFiles(dir) {
  if (!fs.existsSync(dir)) return [];
  return fs.readdirSync(dir, { recursive: true, withFileTypes: true })
    .filter((e) => e.isFile())
    .map((e) => path.relative(dir, path.join(e.parentPath ?? e.path, e.name)));
}

function buildForAbi(abi, clang) {
  const { rust, clang: clangTarget } = ABIS[abi];
  const envKey = rust.toUpperCase().replace(/-/g, '_');
  const rustflags = [
    `-Clink-arg=--target=${clangTarget}${MIN_SDK}`,
    // 16 KB pages: needed on Android 15 and later phones that use them, and
    // by Google Play for apps that target Android 15. Harmless on 4 KB phones.
    '-Clink-arg=-Wl,-z,max-page-size=16384',
  ];
  const env = {
    ...process.env,
    [`CARGO_TARGET_${envKey}_LINKER`]: clang,
    // Unit separator between flags, as cargo expects. Applies to this crate
    // only: build scripts and proc macros are built for the PC, without them.
    CARGO_ENCODED_RUSTFLAGS: rustflags.join('\x1f'),
  };
  delete env.RUSTFLAGS;
  log(`cargo build for ${abi} (${rust})`);
  run('cargo', ['build', '--lib', '--release', '--target', rust], { cwd: coreDir, env });
  const built = path.join(coreDir, 'target', rust, 'release', LIB_FILE);
  if (!fs.existsSync(built)) fail(`expected ${built} after the build`);
  return built;
}

function main() {
  const started = Date.now();
  const abis = wantedAbis();
  checkRustTargets(abis);
  const ndk = findNdk();
  const clang = findClang(ndk);
  log(`ABIs: ${abis.join(', ')}. NDK: ${ndk}`);

  const built = {};
  for (const abi of abis) built[abi] = buildForAbi(abi, clang);

  // Libraries into place, and remove any ABI that is no longer wanted.
  for (const abi of abis) {
    const to = path.join(jniLibsDir, abi, LIB_FILE);
    const changed = copyIfChanged(built[abi], to);
    log(`${abi}: ${LIB_FILE} ${fs.statSync(to).size} bytes${changed ? ', updated' : ', unchanged'}`);
  }
  if (fs.existsSync(jniLibsDir)) {
    for (const entry of fs.readdirSync(jniLibsDir)) {
      if (!abis.includes(entry)) {
        fs.rmSync(path.join(jniLibsDir, entry), { recursive: true, force: true });
        log(`removed old ABI folder ${entry}`);
      }
    }
  }

  // Bindings, generated from the library just built ("library mode": UniFFI
  // reads the interface from metadata inside the library). Written to a
  // temporary folder first, then copied over only where the text changed.
  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'limo-bindings-'));
  try {
    log('generating Kotlin bindings');
    run('cargo', [
      'run', '--quiet', '--features', 'bindgen', '--bin', 'uniffi-bindgen', '--',
      'generate', built[abis[0]], '--language', 'kotlin', '--no-format', '--out-dir', tmp,
    ], { cwd: coreDir });
    const fresh = listFiles(tmp);
    if (fresh.length === 0) fail('UniFFI generated no Kotlin files');
    for (const rel of fresh) {
      const changed = copyIfChanged(path.join(tmp, rel), path.join(kotlinDir, rel));
      log(`bindings: ${rel.split(path.sep).join('/')}${changed ? ', updated' : ', unchanged'}`);
    }
    for (const rel of listFiles(kotlinDir)) {
      if (!fresh.includes(rel)) {
        fs.rmSync(path.join(kotlinDir, rel));
        log(`removed old bindings file ${rel}`);
      }
    }
  } finally {
    fs.rmSync(tmp, { recursive: true, force: true });
  }

  log(`done in ${((Date.now() - started) / 1000).toFixed(1)} s`);
}

main();
