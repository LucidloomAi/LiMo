// Builds the release APK of one form or both, on this PC, with nothing sent
// anywhere but the package downloads Gradle needs the first time.
//
//   node scripts/build.mjs child
//   node scripts/build.mjs guardian
//   node scripts/build.mjs child guardian
//
// For each form, in order:
//   1. scripts/make-design.mjs (the theme, icons and logo from the design);
//   2. deletes android/ and makes it again with `expo prebuild` for the form,
//      so nothing of the other form is left in it;
//   3. `gradlew assembleRelease`, with LIMO_FORM set for the whole run;
//   4. copies the APK to out/limo-<form>-release.apk.
//
// Expo's command line tool is told not to send usage data
// (EXPO_NO_TELEMETRY=1). Gradle runs without a daemon and compiles Kotlin in
// its own process, so nothing keeps running after the build.
//
// The APK is signed with the debug key that `expo prebuild` writes into
// android/app/debug.keystore, only so that it installs over USB. A release key
// is the owner's decision, later.
//
// Then run `node scripts/inspect.mjs` to look inside the built files.

import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const appDir = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const androidDir = path.join(appDir, 'android');
const outDir = path.join(appDir, 'out');
const FORMS = ['child', 'guardian'];

const forms = process.argv.slice(2);
if (forms.length === 0 || forms.some((f) => !FORMS.includes(f))) {
  console.error('Usage: node scripts/build.mjs child|guardian [child|guardian]');
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
  const t = { form };
  t.design = run('make-design', process.execPath, [path.join('scripts', 'make-design.mjs')], appDir, env);

  const started = Date.now();
  fs.rmSync(androidDir, { recursive: true, force: true, maxRetries: 3 });
  t.clean = Number(((Date.now() - started) / 1000).toFixed(1));
  t.prebuild = run(`expo prebuild (${form})`, 'npx', ['expo', 'prebuild', '--platform', 'android', '--no-install'], appDir, env);

  // The full path, quoted: Windows may be set not to look in the current folder.
  const gradlew = process.platform === 'win32' ? `"${path.join(androidDir, 'gradlew.bat')}"` : './gradlew';
  t.gradle = run(
    `gradlew assembleRelease (${form})`,
    gradlew,
    ['assembleRelease', '--no-daemon', '-Pkotlin.compiler.execution.strategy=in-process'],
    androidDir,
    env,
  );

  const apk = path.join(androidDir, 'app', 'build', 'outputs', 'apk', 'release', 'app-release.apk');
  fs.mkdirSync(outDir, { recursive: true });
  const target = path.join(outDir, `limo-${form}-release.apk`);
  fs.copyFileSync(apk, target);
  t.bytes = fs.statSync(target).size;
  console.log(`[build] ${path.relative(appDir, target)}: ${t.bytes} bytes`);
  times.push(t);
}

console.log('\n[build] done');
for (const t of times) {
  console.log(
    `  ${t.form}: ${t.bytes} bytes; make-design ${t.design} s, delete android ${t.clean} s, prebuild ${t.prebuild} s, gradle ${t.gradle} s`,
  );
}
