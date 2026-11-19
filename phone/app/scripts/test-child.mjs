// Runs the child module's JVM tests on this PC: the real Rust engine and the
// real link, driven from Kotlin through the generated bindings, with the
// library built for this PC loaded by JNA. No phone, no emulator.
//
//   node scripts/test-child.mjs
//
// In order:
//   1. scripts/make-design.mjs;
//   2. android/ made again with `expo prebuild` for the child form, unless it
//      is already the child form's (the tests are in the child module, which
//      the guardian form leaves out);
//   3. `gradlew :limo-child:testReleaseUnitTest`, with LIMO_FORM=child. Gradle
//      first runs scripts/build-engine.mjs for the phone (the bindings the
//      tests drive are the ones that ship) and for this PC (the library the
//      tests load, with the testkit);
//   4. a line for each test from Gradle's results.
//
// Gradle runs without a daemon and compiles Kotlin in its own process, as in
// scripts/build.mjs.

import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const appDir = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const androidDir = path.join(appDir, 'android');
const resultsDir = path.join(appDir, 'modules', 'limo-child', 'android', 'build', 'test-results', 'testReleaseUnitTest');
const env = { ...process.env, LIMO_FORM: 'child', EXPO_NO_TELEMETRY: '1' };

function run(label, command, args, cwd) {
  const started = Date.now();
  console.log(`\n[test] ${label}`);
  // npx and gradlew.bat are scripts, which Windows runs only through a shell.
  const shell = process.platform === 'win32' && (command === 'npx' || command.endsWith('.bat"'));
  const result = spawnSync(command, args, { cwd, env, stdio: 'inherit', shell });
  const seconds = ((Date.now() - started) / 1000).toFixed(1);
  console.log(`[test] ${label}: ${seconds} s, exit ${result.status ?? result.signal}`);
  return result.status === 0;
}

function madeForChild() {
  const properties = path.join(androidDir, 'gradle.properties');
  if (!fs.existsSync(properties)) return false;
  return /^limo\.form=child$/m.test(fs.readFileSync(properties, 'utf8'));
}

// One line for each test, from the JUnit files Gradle writes.
function report() {
  if (!fs.existsSync(resultsDir)) return;
  let passed = 0;
  let failed = 0;
  for (const file of fs.readdirSync(resultsDir).filter((f) => f.endsWith('.xml')).sort()) {
    const xml = fs.readFileSync(path.join(resultsDir, file), 'utf8');
    for (const m of xml.matchAll(/<testcase name="([^"]+)" classname="([^"]+)" time="([^"]+)"(\s*\/>|>([\s\S]*?)<\/testcase>)/g)) {
      const failure = m[5] && /<failure/.test(m[5]);
      if (failure) failed += 1;
      else passed += 1;
      console.log(`  ${failure ? 'FAIL' : 'PASS'}  ${m[2].split('.').pop()}.${m[1]} (${m[3]} s)`);
    }
  }
  console.log(`[test] ${passed} passed, ${failed} failed`);
}

if (!run('make-design', process.execPath, [path.join('scripts', 'make-design.mjs')], appDir)) process.exit(1);
if (!madeForChild()) {
  fs.rmSync(androidDir, { recursive: true, force: true, maxRetries: 3 });
  if (!run('expo prebuild (child)', 'npx', ['expo', 'prebuild', '--platform', 'android', '--no-install'], appDir)) {
    process.exit(1);
  }
}
const gradlew = process.platform === 'win32' ? `"${path.join(androidDir, 'gradlew.bat')}"` : './gradlew';
const ok = run(
  'gradlew :limo-child:testReleaseUnitTest',
  gradlew,
  [':limo-child:testReleaseUnitTest', '--no-daemon', '-Pkotlin.compiler.execution.strategy=in-process'],
  androidDir,
);
report();
process.exit(ok ? 0 : 1);
