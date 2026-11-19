// Builds the Rust bridge (../ffi, the crate limo-phone-ffi: the phone's
// engine and its side of the link) and generates its Kotlin bindings with
// UniFFI, into the child module's git ignored src/generated folder.
//
//   node scripts/build-engine.mjs android    the library on a phone
//   node scripts/build-engine.mjs host       the library for the JVM tests on this PC
//
// android, for each ABI (LIMO_ABIS, default arm64-v8a):
//   1. cargo build --lib --release --target <rust target>, linked with the
//      NDK's clang (no cargo-ndk), 16 KB pages, and the build PC's folders
//      written as short names (--remap-path-prefix) so the library does not
//      say who built it or where;
//   2. checks the library: no testkit in it (the feature a phone library
//      never has), no SQLite, no path of the build PC;
//   3. copies it to src/generated/jniLibs/<abi>/liblimo_phone_ffi.so;
//   4. generates the Kotlin bindings from it (UniFFI's library mode) into
//      src/generated/kotlin, package ai.lucidloom.limo.child.ffi.
// The child module's Gradle task buildRustEngine runs this before every
// build of the module, so `npm run build:child` gives an APK with the
// library inside. The guardian form leaves the module out, so it never runs
// there.
//
// host, for the JVM unit tests of the child module (Gradle task
// buildRustEngineForTests, run before them):
//   1. cargo build --lib --features testkit, for this PC (a debug build with
//      optimised dependencies: the PIN hash is slow on purpose);
//   2. copies it to src/generated/test-native/ (jna.library.path in the
//      tests);
//   3. generates its bindings into src/generated/test-kotlin, package
//      ai.lucidloom.limo.child.ffi.testkit. The tests drive the engine and
//      the link through the main bindings above, the ones that ship, and
//      use this second copy only for TestFamily, which the main ones never
//      have.
//
// Files are rewritten only when their content changed, so Gradle skips the
// work after them on a rebuild.
//
// Settings, all optional:
//   LIMO_ABIS         Android ABIs, comma separated. Default: arm64-v8a.
//                     Gradle passes the app's reactNativeArchitectures.
//   LIMO_NDK_VERSION  NDK folder under <sdk>/ndk. Default: 27.1.12297006.
//   ANDROID_NDK_HOME  Full NDK path, used instead of the two above.
//   ANDROID_HOME      SDK path. Also read: ANDROID_SDK_ROOT, then the usual
//                     place under the user's profile.
//
// From proof three (spikes/03-expo-rust-core), which built the first Rust
// library for this app's phone the same way.

import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const CRATE = 'limo_phone_ffi';
const PACKAGE = 'ai.lucidloom.limo.child.ffi';
const TEST_PACKAGE = `${PACKAGE}.testkit`;
// The app's oldest Android (app.config.ts, plugins/with-limo-android.js): 8.0.
const MIN_SDK = 26;
const DEFAULT_NDK_VERSION = '27.1.12297006';

// Android ABI name: Rust target, clang target prefix.
const ABIS = {
  'arm64-v8a': { rust: 'aarch64-linux-android', clang: 'aarch64-linux-android' },
  'armeabi-v7a': { rust: 'armv7-linux-androideabi', clang: 'armv7a-linux-androideabi' },
  x86_64: { rust: 'x86_64-linux-android', clang: 'x86_64-linux-android' },
  x86: { rust: 'i686-linux-android', clang: 'i686-linux-android' },
};

const appDir = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const repoDir = path.resolve(appDir, '..', '..');
const ffiDir = path.resolve(appDir, '..', 'ffi');
const generatedDir = path.join(appDir, 'modules', 'limo-child', 'android', 'src', 'generated');
const out = {
  jniLibs: path.join(generatedDir, 'jniLibs'),
  kotlin: path.join(generatedDir, 'kotlin'),
  testNative: path.join(generatedDir, 'test-native'),
  testKotlin: path.join(generatedDir, 'test-kotlin'),
};

function fail(message) {
  console.error(`build-engine: ${message}`);
  process.exit(1);
}

function log(message) {
  console.log(`build-engine: ${message}`);
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

// Copies only when the content differs, so Gradle sees no change on a
// rebuild with nothing new.
function copyIfChanged(from, to) {
  const next = fs.readFileSync(from);
  if (fs.existsSync(to) && Buffer.compare(fs.readFileSync(to), next) === 0) return false;
  fs.mkdirSync(path.dirname(to), { recursive: true });
  fs.writeFileSync(to, next);
  return true;
}

function listFiles(dir) {
  if (!fs.existsSync(dir)) return [];
  return fs
    .readdirSync(dir, { recursive: true, withFileTypes: true })
    .filter((e) => e.isFile())
    .map((e) => path.relative(dir, path.join(e.parentPath ?? e.path, e.name)));
}

// Puts the files of `from` into `to`, rewriting only what changed and
// removing what is no longer there.
function mirror(from, to, label) {
  const fresh = listFiles(from);
  if (fresh.length === 0) fail(`${label}: nothing was generated`);
  for (const rel of fresh) {
    const changed = copyIfChanged(path.join(from, rel), path.join(to, rel));
    log(`${label}: ${rel.split(path.sep).join('/')}${changed ? ', updated' : ', unchanged'}`);
  }
  for (const rel of listFiles(to)) {
    if (!fresh.includes(rel)) {
      fs.rmSync(path.join(to, rel));
      log(`${label}: removed old ${rel}`);
    }
  }
}

// ---------------------------------------------------------------------------
// What a library on a phone must never hold.

// The testkit's names, as UniFFI exports them (its functions, and its
// metadata), in any case.
const TESTKIT_MARKS = ['testfamily', 'testkit'];

// The build PC's user name, home folder and repository folder, as they
// could appear in a library: as written, with forward slashes, and doubled
// backslashes.
function machineMarks() {
  return [os.userInfo().username, os.homedir(), repoDir]
    .flatMap((s) => [s, s.replaceAll('\\', '/'), s.replaceAll('\\', '\\\\')])
    .filter((s) => s && s.length >= 3);
}

// Returns what of the testkit a library holds: its names, and SQLite (the
// family file it plays the master over). scripts/inspect.mjs uses it too.
export function testkitProblems(bytes) {
  const problems = [];
  const lower = Buffer.from(bytes.toString('latin1').toLowerCase(), 'latin1');
  for (const mark of TESTKIT_MARKS) {
    if (lower.includes(Buffer.from(mark))) problems.push(`it holds the testkit ("${mark}")`);
  }
  if (bytes.includes(Buffer.from('SQLite format'))) problems.push('it holds SQLite');
  return problems;
}

// Returns what a phone library must not hold but does.
export function phoneLibraryProblems(bytes) {
  const problems = testkitProblems(bytes);
  for (const mark of machineMarks()) {
    if (bytes.includes(Buffer.from(mark, 'utf8'))) problems.push('it holds a folder or the user name of the build PC');
  }
  return [...new Set(problems)];
}

// ---------------------------------------------------------------------------
// Android

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
    process.platform === 'win32' && process.env.LOCALAPPDATA ? path.join(process.env.LOCALAPPDATA, 'Android', 'Sdk') : null,
    process.platform === 'darwin' ? path.join(os.homedir(), 'Library', 'Android', 'sdk') : null,
    process.platform === 'linux' ? path.join(os.homedir(), 'Android', 'Sdk') : null,
  ].filter(Boolean);
  const sdk = sdkCandidates.find((p) => fs.existsSync(p));
  if (!sdk) fail('no Android SDK found. Set ANDROID_HOME.');
  const version = process.env.LIMO_NDK_VERSION || DEFAULT_NDK_VERSION;
  const ndk = path.join(sdk, 'ndk', version);
  if (!fs.existsSync(ndk)) {
    fail(`NDK ${version} not found under the SDK's ndk folder. Install it with the SDK Manager, or set LIMO_NDK_VERSION to one that is installed.`);
  }
  return ndk;
}

function findClang(ndk) {
  const hostTag = { win32: 'windows-x86_64', darwin: 'darwin-x86_64', linux: 'linux-x86_64' }[process.platform];
  if (!hostTag) fail(`unsupported build platform ${process.platform}`);
  const clang = path.join(ndk, 'toolchains', 'llvm', 'prebuilt', hostTag, 'bin', process.platform === 'win32' ? 'clang.exe' : 'clang');
  if (!fs.existsSync(clang)) fail(`the NDK's clang is missing (${path.relative(ndk, clang)})`);
  return clang;
}

function checkRustTargets(abis) {
  const installed = new Set(capture('rustup', ['target', 'list', '--installed']).split(/\r?\n/).map((s) => s.trim()));
  const missing = abis.map((abi) => ABIS[abi].rust).filter((t) => !installed.has(t));
  if (missing.length > 0) fail(`Rust targets missing. Run: rustup target add ${missing.join(' ')}`);
}

// The build PC's folders, written as short names in the library: panic
// messages carry source paths. The last match wins, so the most specific
// folder comes last.
function remapFlags() {
  const cargoHome = process.env.CARGO_HOME || path.join(os.homedir(), '.cargo');
  const pairs = [
    [os.homedir(), 'home'],
    [cargoHome, 'cargo'],
    [repoDir, 'limo'],
  ];
  return pairs.flatMap(([from, to]) => {
    const forms = [...new Set([from, from.replaceAll('\\', '/')])];
    return forms.map((f) => `--remap-path-prefix=${f}=${to}`);
  });
}

function buildForAbi(abi, clang) {
  const { rust, clang: clangTarget } = ABIS[abi];
  const envKey = rust.toUpperCase().replace(/-/g, '_');
  const rustflags = [
    `-Clink-arg=--target=${clangTarget}${MIN_SDK}`,
    // 16 KB pages: needed on Android 15 and later phones that use them, and
    // by Google Play. Harmless on 4 KB phones.
    '-Clink-arg=-Wl,-z,max-page-size=16384',
    ...remapFlags(),
  ];
  const env = {
    ...process.env,
    [`CARGO_TARGET_${envKey}_LINKER`]: clang,
    // Unit separator between flags, as cargo expects. With --target they
    // apply to the library's crates only, not to build scripts or macros.
    CARGO_ENCODED_RUSTFLAGS: rustflags.join('\x1f'),
  };
  delete env.RUSTFLAGS;
  log(`cargo build for ${abi} (${rust}), release`);
  run('cargo', ['build', '--lib', '--release', '--target', rust], { cwd: ffiDir, env });
  const built = path.join(ffiDir, 'target', rust, 'release', `lib${CRATE}.so`);
  if (!fs.existsSync(built)) fail(`expected lib${CRATE}.so after the build`);
  const problems = phoneLibraryProblems(fs.readFileSync(built));
  if (problems.length) fail(`the ${abi} library cannot go on a phone: ${problems.join('; ')}`);
  return built;
}

function generateBindings(library, into, label, configText) {
  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'limo-bindings-'));
  try {
    const args = ['run', '--quiet', '--features', 'bindgen', '--bin', 'uniffi-bindgen', '--', 'generate', library, '--language', 'kotlin', '--no-format', '--out-dir', path.join(tmp, 'out')];
    if (configText) {
      const config = path.join(tmp, 'uniffi-global.toml');
      fs.writeFileSync(config, configText);
      args.push('--config', config);
    }
    log(`${label}: generating the Kotlin bindings`);
    // The generator has its own target folder: built with other features,
    // it would otherwise relink the library the tests use at every run.
    run('cargo', args, { cwd: ffiDir, env: { ...process.env, CARGO_TARGET_DIR: path.join(ffiDir, 'target', 'bindgen') } });
    mirror(path.join(tmp, 'out'), into, label);
  } finally {
    fs.rmSync(tmp, { recursive: true, force: true });
  }
}

function kotlinText(dir) {
  return listFiles(dir)
    .map((rel) => fs.readFileSync(path.join(dir, rel), 'utf8'))
    .join('\n');
}

function android() {
  const abis = wantedAbis();
  checkRustTargets(abis);
  const clang = findClang(findNdk());
  log(`ABIs: ${abis.join(', ')}`);

  const built = {};
  for (const abi of abis) built[abi] = buildForAbi(abi, clang);
  for (const abi of abis) {
    const to = path.join(out.jniLibs, abi, `lib${CRATE}.so`);
    const changed = copyIfChanged(built[abi], to);
    log(`${abi}: lib${CRATE}.so ${fs.statSync(to).size} bytes${changed ? ', updated' : ', unchanged'}`);
  }
  if (fs.existsSync(out.jniLibs)) {
    for (const entry of fs.readdirSync(out.jniLibs)) {
      if (!abis.includes(entry)) {
        fs.rmSync(path.join(out.jniLibs, entry), { recursive: true, force: true });
        log(`removed old ABI folder ${entry}`);
      }
    }
  }

  generateBindings(built[abis[0]], out.kotlin, 'bindings');
  const text = kotlinText(out.kotlin);
  if (!text.includes(`package ${PACKAGE}`)) fail(`the bindings are not in package ${PACKAGE}`);
  if (/TestFamily/.test(text)) fail('the bindings name TestFamily: the phone library holds the testkit');
}

// ---------------------------------------------------------------------------
// Host, for the JVM tests

function hostLibraryName() {
  if (process.platform === 'win32') return `${CRATE}.dll`;
  if (process.platform === 'darwin') return `lib${CRATE}.dylib`;
  return `lib${CRATE}.so`;
}

function host() {
  log('cargo build for this PC, with the testkit');
  run('cargo', ['build', '--lib', '--features', 'testkit'], { cwd: ffiDir });
  const name = hostLibraryName();
  const built = path.join(ffiDir, 'target', 'debug', name);
  if (!fs.existsSync(built)) fail(`expected ${name} after the build`);
  const library = path.join(out.testNative, name);
  const changed = copyIfChanged(built, library);
  log(`test library: ${name} ${fs.statSync(library).size} bytes${changed ? ', updated' : ', unchanged'}`);
  for (const old of listFiles(out.testNative)) {
    if (old !== name) fs.rmSync(path.join(out.testNative, old));
  }

  const config = `[crates.${CRATE}.bindings.kotlin]\npackage_name = "${TEST_PACKAGE}"\n`;
  generateBindings(library, out.testKotlin, 'test bindings', config);
  const text = kotlinText(out.testKotlin);
  if (!text.includes(`package ${TEST_PACKAGE}`)) fail(`the test bindings are not in package ${TEST_PACKAGE}`);
  if (!/class TestFamily/.test(text)) fail('the test bindings have no TestFamily');
}

// ---------------------------------------------------------------------------

const modes = { android, host };
const isMain = process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url);
if (isMain) {
  const mode = process.argv[2] ?? 'android';
  if (!modes[mode]) {
    console.error('Usage: node scripts/build-engine.mjs android|host');
    process.exit(2);
  }
  const started = Date.now();
  modes[mode]();
  log(`${mode}: done in ${((Date.now() - started) / 1000).toFixed(1)} s`);
}
