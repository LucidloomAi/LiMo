// Builds the release APK of one form or both, on this PC, with nothing sent
// anywhere but the package downloads Gradle needs the first time.
//
//   node scripts/build.mjs child
//   node scripts/build.mjs guardian
//   node scripts/build.mjs child guardian
//   node scripts/build.mjs child --debug
//   node scripts/build.mjs child --out <folder>    the APKs into another folder
//
// First, for a release build with LIMO_RELEASE_KEY_FILE set, the key is
// checked (scripts/ReleaseKeyCheck.java), so a wrong file, a missing key or a
// wrong password stops the build before anything runs, in plain words.
//
// Then for each form, in order:
//   1. scripts/make-design.mjs (the theme, icons and logo from the design);
//   2. deletes android/ and makes it again with `expo prebuild` for the form,
//      so nothing of the other form is left in it;
//   3. `gradlew assembleRelease`, with LIMO_FORM set for the whole run;
//   4. copies the APK to out/limo-<form>-release.apk, and reads who signed it
//      with apksigner; with a release key, an APK signed with any other key
//      stops the build and is not kept.
//
// With --debug, step 3 is `gradlew assembleDebug` and the APK is
// out/limo-<form>-debug.apk: debuggable, so the development tool on the PC
// can reach the app's files with run-as, and with the development receiver
// (src/debug of the child module). scripts/debug-bundle.gradle makes Gradle
// put the JavaScript inside it as a release build does, so it runs without
// a development server (it has no INTERNET to reach one). Never give a debug
// build to a family: anyone with a cable can change what it keeps.
//
// Expo's command line tool is told not to send usage data
// (EXPO_NO_TELEMETRY=1). Gradle runs without a daemon and compiles Kotlin in
// its own process, so nothing keeps running after the build.
//
// Signing (see "Signing" in README.md). A release build is signed with the
// key LIMO_RELEASE_KEY_FILE names (scripts/release-signing.gradle, applied in
// android/app/build.gradle). Without it, a release build is signed with
// Android's debug key, which `expo prebuild` writes into
// android/app/debug.keystore, and the build says at its end, where it cannot
// be missed, that the APKs are for a cable only, never for a family. A debug
// build is always signed with the debug key: the variable is not even passed
// to it.
//
// Then run `node scripts/inspect.mjs` to look inside the built files.

import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { KEY_VARIABLE, apkSigning, buildTools, checkReleaseKey, colons } from './signing.mjs';

const appDir = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const androidDir = path.join(appDir, 'android');
const FORMS = ['child', 'guardian'];

const args = process.argv.slice(2);
const debug = args.includes('--debug');
const outAt = args.indexOf('--out');
const outDir = outAt >= 0 && args[outAt + 1] ? path.resolve(args[outAt + 1]) : path.join(appDir, 'out');
const forms = args.filter((a, i) => a !== '--debug' && a !== '--out' && !(outAt >= 0 && i === outAt + 1));
if (forms.length === 0 || forms.some((f) => !FORMS.includes(f)) || (outAt >= 0 && !args[outAt + 1])) {
  console.error('Usage: node scripts/build.mjs child|guardian [child|guardian] [--debug] [--out <folder>]');
  process.exit(2);
}
const variant = debug ? 'debug' : 'release';
const task = debug ? 'assembleDebug' : 'assembleRelease';

// The release key, checked before anything runs.
const keySet = Boolean(process.env[KEY_VARIABLE]);
let releaseKey = null;
if (!debug && keySet) {
  const key = checkReleaseKey(process.env);
  if (!key.ok) {
    console.error(`\n[build] The release key: ${key.message}`);
    console.error('[build] Stopped before building anything.');
    process.exit(1);
  }
  releaseKey = key;
  console.log(`[build] Release key: ${key.subject}, certificate SHA-256 ${colons(key.sha256)}`);
} else if (!debug) {
  console.log(`[build] ${KEY_VARIABLE} is not set: the release APKs will be signed with Android's debug key.`);
} else if (keySet) {
  console.log(`[build] ${KEY_VARIABLE} is set, but a debug build never uses it: it is signed with Android's debug key.`);
}
let tools;
try {
  tools = buildTools();
} catch (e) {
  console.error(`[build] ${e.message}: the build needs the Android SDK (see Building in README.md).`);
  process.exit(2);
}

function run(label, command, args, cwd, env) {
  const started = Date.now();
  console.log(`\n[build] ${label}`);
  // npx and gradlew.bat are scripts, which Windows runs only through a shell.
  // Of what is passed to the shell, only gradlew.bat's path could hold a space,
  // and it is quoted.
  const shell = process.platform === 'win32' && (command === 'npx' || command.endsWith('.bat"'));
  const result = spawnSync(command, args, { cwd, env, stdio: 'inherit', shell });
  const seconds = ((Date.now() - started) / 1000).toFixed(1);
  if (result.status !== 0) {
    console.error(`[build] ${label} failed after ${seconds} s (exit ${result.status ?? result.signal}).`);
    process.exit(1);
  }
  console.log(`[build] ${label}: ${seconds} s`);
  return Number(seconds);
}

const times = [];
for (const form of forms) {
  const env = { ...process.env, LIMO_FORM: form, EXPO_NO_TELEMETRY: '1' };
  // A debug build keeps the debug key: Gradle never sees the release key.
  if (debug) delete env[KEY_VARIABLE];
  const t = { form };
  t.design = run('make-design', process.execPath, [path.join('scripts', 'make-design.mjs')], appDir, env);

  const started = Date.now();
  // On Windows a file of the build before (a dex file, mostly) can stay held
  // for some seconds after Gradle ends. Each try waits half a second longer
  // than the one before, up to about 40 seconds in all.
  fs.rmSync(androidDir, { recursive: true, force: true, maxRetries: 12, retryDelay: 500 });
  t.clean = Number(((Date.now() - started) / 1000).toFixed(1));
  t.prebuild = run(`expo prebuild (${form})`, 'npx', ['expo', 'prebuild', '--platform', 'android', '--no-install'], appDir, env);

  // The full path, quoted: Windows may be set not to look in the current folder.
  const gradlew = process.platform === 'win32' ? `"${path.join(androidDir, 'gradlew.bat')}"` : './gradlew';
  const gradleArgs = [task, '--no-daemon', '-Pkotlin.compiler.execution.strategy=in-process'];
  // The init script's path has no space in it here; Windows runs gradlew.bat
  // through a shell, so it is quoted all the same.
  const initScript = path.join(appDir, 'scripts', 'debug-bundle.gradle');
  if (debug) gradleArgs.push('--init-script', process.platform === 'win32' ? `"${initScript}"` : initScript);
  t.gradle = run(`gradlew ${task} (${form})`, gradlew, gradleArgs, androidDir, env);

  const apk = path.join(androidDir, 'app', 'build', 'outputs', 'apk', variant, `app-${variant}.apk`);
  fs.mkdirSync(outDir, { recursive: true });
  const target = path.join(outDir, `limo-${form}-${variant}.apk`);
  fs.copyFileSync(apk, target);
  t.bytes = fs.statSync(target).size;
  t.name = path.relative(appDir, target);
  console.log(`[build] ${t.name}: ${t.bytes} bytes`);

  // Who signed it, as Android will read it.
  const signing = apkSigning(target, tools.apksigner);
  t.signer = signing.signers.length === 1 ? signing.signers[0] : null;
  const wrong = !signing.verified
    ? 'its signature does not verify'
    : !t.signer
      ? `it has ${signing.signers.length} signers, not one`
      : releaseKey && t.signer.sha256 !== releaseKey.sha256
        ? `it is signed by ${t.signer.subject}, not with the release key`
        : null;
  if (wrong) {
    fs.rmSync(target, { force: true });
    console.error(`[build] ${t.name} is not kept: ${wrong}. Stopped.`);
    process.exit(1);
  }
  console.log(`[build] ${t.name}: signed by ${t.signer.subject} (${signing.schemes.join(', ')})`);
  times.push(t);
}

console.log('\n[build] done');
for (const t of times) {
  console.log(
    `  ${t.form}: ${t.bytes} bytes; make-design ${t.design} s, delete android ${t.clean} s, prebuild ${t.prebuild} s, gradle ${t.gradle} s`,
  );
}

// How the APKs are signed, said last, where it cannot be missed.
const names = times.map((t) => t.name).join(' and ');
if (debug) {
  console.log(
    `\n[build] ${names}: signed with Android's debug key, as every debug build is. ` +
      'For the development tool over a cable only, never for a family.',
  );
} else if (releaseKey) {
  console.log(`\n[build] Signed with the release key ${KEY_VARIABLE} names: ${names}`);
  console.log(`[build]   ${releaseKey.subject}`);
  console.log(`[build]   certificate SHA-256 ${colons(releaseKey.sha256)}`);
} else {
  const lines = [
    "SIGNED WITH ANDROID'S DEBUG KEY: NOT FOR A FAMILY",
    '',
    `${KEY_VARIABLE} is not set, so ${times.length > 1 ? 'these are' : 'this is'} signed`,
    "with Android's debug key:",
    ...times.map((t) => `  ${t.name}`),
    'For trying on a phone over a cable only, never for a family:',
    'the debug key is public, so anyone could sign an update that',
    'Android would accept in place of LiMo.',
    'To sign with the release key, see "Signing" in README.md.',
  ];
  const width = Math.max(...lines.map((l) => l.length)) + 4;
  const bar = '#'.repeat(width + 2);
  console.log(`\n${bar}`);
  for (const l of lines) console.log(`#  ${l.padEnd(width - 2)}#`);
  console.log(bar);
}
