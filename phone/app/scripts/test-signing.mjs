// Tests the release signing with throwaway keys made here, never with a real
// key, and needs none.
//
//   node scripts/test-signing.mjs           the key checks, then real Gradle builds
//   node scripts/test-signing.mjs --quick   the key checks only, without Gradle
//
// keytool (from the JDK) makes, in a new temporary folder, throwaway keystores
// with a random throwaway password: a PKCS12 keystore as README.md tells an
// owner to make one, a JKS keystore whose key has its own password, a key named
// like Android's debug key, and a keystore that holds a certificate only.
//
// The key checks (scripts/ReleaseKeyCheck.java, as the build and Gradle run it):
//   - storeFile as a bare name beside the properties file, as a path from its
//     folder, and as a full path (forward slashes, and doubled backslashes on
//     Windows) each open the key, whose certificate is the one keytool made;
//   - each way a key can be wrong gives its own message, and nothing printed
//     holds a password, the temporary folder's path, a file name or a line of
//     the properties file;
//   - scripts/build.mjs stops on a wrong key before it runs anything.
// With Gradle (not with --quick), each a real build of the child form:
//   - scripts/build.mjs with the variable set, storeFile a bare name: the APK
//     carries the throwaway certificate; neither the build's output nor any
//     file under android/ holds the password;
//   - gradlew assembleRelease by hand, storeFile a full path: the APK carries
//     it too;
//   - gradlew assembleRelease by hand with a wrong password, and with the
//     variable pointing at no file: stopped by limoCheckReleaseKey with the
//     check's message, and Gradle's output holds no password and no path;
//   - the same wrong password with limoCheckReleaseKey skipped: Android
//     Gradle's own message holds no password either (it is shown, with the
//     keys' folder replaced);
//   - scripts/build.mjs --debug with the variable set: the debug APK carries
//     Android's debug certificate.
//
// The APKs go to a temporary folder, never to out/. Both temporary folders,
// with every throwaway key, are deleted at the end. The Gradle part makes
// android/ again (for the child form), as a build does: never run it while
// another Gradle run uses this app. It takes about eight minutes on the
// development PC with Gradle's caches warm; --quick about half a minute.

import { spawnSync } from 'node:child_process';
import crypto from 'node:crypto';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { KEY_VARIABLE, apkSigning, buildTools, checkReleaseKey, isDebugCertificate } from './signing.mjs';

const appDir = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const androidDir = path.join(appDir, 'android');
const win = process.platform === 'win32';
const quick = process.argv.includes('--quick');
const started = Date.now();

const keysDir = fs.mkdtempSync(path.join(os.tmpdir(), 'limo-signing-keys-'));
const apksDir = fs.mkdtempSync(path.join(os.tmpdir(), 'limo-signing-apks-'));
// Throwaway passwords, made here and forgotten at the end.
const PASSWORD = crypto.randomBytes(18).toString('base64url');
const KEY_PASSWORD = crypto.randomBytes(18).toString('base64url');
const WRONG = crypto.randomBytes(18).toString('base64url');

// What must never be printed: the passwords, the keys' folder in every
// spelling, and the names of the files in it.
const SECRETS = [
  PASSWORD,
  KEY_PASSWORD,
  WRONG,
  keysDir,
  keysDir.replaceAll('\\', '/'),
  keysDir.replaceAll('\\', '\\\\'),
  path.basename(keysDir),
  'throwaway',
  'certonly',
  'debugish',
  'notakeystore',
  'nested',
  'case-',
];
function leaks(text) {
  return SECRETS.filter((s) => text.includes(s));
}

let failures = 0;
function result(ok, text, detail) {
  if (!ok) failures += 1;
  console.log(`  ${ok ? 'PASS' : 'FAIL'}  ${text}`);
  if (detail) console.log(`        ${detail}`);
}
function seconds(since) {
  return ((Date.now() - since) / 1000).toFixed(1);
}

function javaTool(name) {
  const home = process.env.JAVA_HOME;
  const exe = win ? `${name}.exe` : name;
  return home && fs.existsSync(path.join(home, 'bin', exe)) ? path.join(home, 'bin', exe) : name;
}

// keytool reads every password from the environment (-storepass:env), so no
// password is on a command line.
function keytool(args) {
  const r = spawnSync(javaTool('keytool'), args, {
    encoding: 'utf8',
    env: { ...process.env, LIMO_TEST_PASSWORD: PASSWORD, LIMO_TEST_KEY_PASSWORD: KEY_PASSWORD },
  });
  if (r.status !== 0) throw new Error(`keytool ${args[0]} failed (exit ${r.status})`);
}

function makeKeys() {
  const at = (name) => path.join(keysDir, name);
  // As README.md tells an owner, with a short validity: it is thrown away.
  keytool(['-genkeypair', '-keystore', at('throwaway.p12'), '-storetype', 'PKCS12', '-alias', 'limo-test',
    '-keyalg', 'RSA', '-keysize', '4096', '-validity', '2', '-dname', 'CN=LiMo signing test, O=LiMo test',
    '-storepass:env', 'LIMO_TEST_PASSWORD']);
  keytool(['-genkeypair', '-keystore', at('throwaway.jks'), '-storetype', 'JKS', '-alias', 'limo-jks',
    '-keyalg', 'RSA', '-keysize', '2048', '-validity', '2', '-dname', 'CN=LiMo signing test JKS, O=LiMo test',
    '-storepass:env', 'LIMO_TEST_PASSWORD', '-keypass:env', 'LIMO_TEST_KEY_PASSWORD']);
  keytool(['-genkeypair', '-keystore', at('debugish.p12'), '-storetype', 'PKCS12', '-alias', 'androiddebugkey',
    '-keyalg', 'RSA', '-keysize', '2048', '-validity', '2', '-dname', 'CN=Android Debug, O=Android, C=US',
    '-storepass:env', 'LIMO_TEST_PASSWORD']);
  keytool(['-exportcert', '-keystore', at('throwaway.p12'), '-alias', 'limo-test', '-file', at('throwaway.der'),
    '-storepass:env', 'LIMO_TEST_PASSWORD']);
  keytool(['-importcert', '-noprompt', '-keystore', at('certonly.p12'), '-storetype', 'PKCS12', '-alias', 'limo-cert',
    '-file', at('throwaway.der'), '-storepass:env', 'LIMO_TEST_PASSWORD']);
  fs.writeFileSync(at('notakeystore.p12'), 'This is not a keystore.\n');
  fs.mkdirSync(at('nested'));
  fs.copyFileSync(at('throwaway.p12'), path.join(keysDir, 'nested', 'throwaway.p12'));
  // The certificate as keytool made it, to compare with what the check and
  // the APK say.
  return crypto.createHash('sha256').update(fs.readFileSync(at('throwaway.der'))).digest('hex');
}

let caseNumber = 0;
// A properties file in the keys' folder; a key given as undefined is left out.
function properties(values) {
  caseNumber += 1;
  const file = path.join(keysDir, `case-${caseNumber}.properties`);
  const text = Object.entries(values)
    .filter(([, v]) => v !== undefined)
    .map(([k, v]) => `${k}=${v}`)
    .join('\n');
  fs.writeFileSync(file, `# A throwaway key for scripts/test-signing.mjs\n${text}\n`);
  return file;
}

const GOOD = { storeFile: 'throwaway.p12', storePassword: PASSWORD, keyAlias: 'limo-test', keyPassword: PASSWORD };
const fullStore = path.join(keysDir, 'throwaway.p12');

function keyChecks(expectedSha) {
  console.log('\n[test] The key checks (scripts/ReleaseKeyCheck.java)');
  const opens = [
    ['storeFile as a bare name beside the properties file', GOOD],
    ['storeFile as a path from the properties file\'s folder', { ...GOOD, storeFile: 'nested/throwaway.p12' }],
    ['storeFile as a full path with forward slashes', { ...GOOD, storeFile: fullStore.replaceAll('\\', '/') }],
    ...(win ? [['storeFile as a full path with doubled backslashes', { ...GOOD, storeFile: fullStore.replaceAll('\\', '\\\\') }]] : []),
  ];
  for (const [what, values] of opens) {
    const r = checkReleaseKey({ ...process.env, [KEY_VARIABLE]: properties(values) });
    result(r.ok && r.sha256 === expectedSha && leaks(r.raw).length === 0, `${what}: opens the key, with keytool's certificate`, r.ok ? null : r.message);
  }
  const jks = checkReleaseKey({
    ...process.env,
    [KEY_VARIABLE]: properties({ storeFile: 'throwaway.jks', storePassword: PASSWORD, keyAlias: 'limo-jks', keyPassword: KEY_PASSWORD }),
  });
  result(jks.ok && jks.subject === 'CN=LiMo signing test JKS, O=LiMo test', 'a JKS keystore whose key has its own password opens', jks.ok ? null : jks.message);

  const fails = [
    ['the variable is not a full path', 'limo-release.properties', 'must be a full path'],
    ['the variable points at no file', path.join(keysDir, 'none.properties'), 'points at no file'],
    ['the variable points at a folder', keysDir, 'points at no file'],
    ['storeFile is missing', { ...GOOD, storeFile: undefined }, 'has no storeFile.'],
    ['storePassword is missing', { ...GOOD, storePassword: undefined }, 'has no storePassword.'],
    ['keyAlias is missing', { ...GOOD, keyAlias: undefined }, 'has no keyAlias.'],
    ['keyPassword is missing', { ...GOOD, keyPassword: undefined }, 'has no keyPassword.'],
    ['storePassword is empty', { ...GOOD, storePassword: '' }, 'has no storePassword.'],
    ['two keys are missing', { ...GOOD, storePassword: undefined, keyPassword: undefined }, 'has no storePassword and no keyPassword.'],
    ['storeFile does not exist', { ...GOOD, storeFile: 'none.p12' }, 'The keystore that storeFile names does not exist. A storeFile that is not a full path'],
    ...(win ? [['storeFile with single backslashes', { ...GOOD, storeFile: fullStore }, 'A backslash in a properties file starts an escape']] : []),
    ['storeFile is a folder', { ...GOOD, storeFile: 'nested' }, 'storeFile names a folder'],
    ['storeFile is not a keystore', { ...GOOD, storeFile: 'notakeystore.p12' }, 'is not a keystore Java can read'],
    ['storePassword is wrong', { ...GOOD, storePassword: WRONG }, 'does not open with storePassword.'],
    ['storePassword has a space at its end', { ...GOOD, storePassword: `${PASSWORD} ` }, 'storePassword ends with a space'],
    ['keyAlias names no entry', { ...GOOD, keyAlias: 'not-there' }, 'holds no entry named by keyAlias'],
    ['keyAlias names a certificate only', { ...GOOD, storeFile: 'certonly.p12', keyAlias: 'limo-cert' }, 'holds a certificate only'],
    ['keyPassword is wrong (PKCS12)', { ...GOOD, keyPassword: WRONG }, 'keyPassword does not open the key'],
    [
      'keyPassword is wrong (JKS)',
      { storeFile: 'throwaway.jks', storePassword: PASSWORD, keyAlias: 'limo-jks', keyPassword: PASSWORD },
      'keyPassword does not open the key',
    ],
    ['the key is an Android debug key', { ...GOOD, storeFile: 'debugish.p12', keyAlias: 'androiddebugkey' }, 'is an Android debug key'],
  ];
  for (const [what, value, expected] of fails) {
    const variable = typeof value === 'string' ? value : properties(value);
    const r = checkReleaseKey({ ...process.env, [KEY_VARIABLE]: variable });
    const leaked = leaks(r.raw);
    result(
      !r.ok && r.message.includes(expected) && leaked.length === 0,
      `${what}: "${r.message ?? ''}"`,
      r.ok ? 'it opened' : leaked.length ? `printed what it must not (${leaked.length} items)` : null,
    );
  }
}

// scripts/build.mjs stops on a wrong key before it runs anything.
function buildStops() {
  console.log('\n[test] scripts/build.mjs with a wrong key');
  const androidBefore = fs.existsSync(androidDir) ? fs.statSync(androidDir).mtimeMs : null;
  for (const [what, variable, expected] of [
    ['the variable points at no file', path.join(keysDir, 'none.properties'), 'points at no file'],
    ['storePassword is wrong', properties({ ...GOOD, storePassword: WRONG }), 'does not open with storePassword.'],
  ]) {
    const t = Date.now();
    const r = spawnSync(process.execPath, [path.join('scripts', 'build.mjs'), 'child', '--out', apksDir], {
      cwd: appDir,
      encoding: 'utf8',
      env: { ...process.env, [KEY_VARIABLE]: variable },
    });
    const out = `${r.stdout}${r.stderr}`;
    const ranNothing = !out.includes('[build] make-design') && !out.includes('prebuild');
    result(
      r.status === 1 && out.includes(expected) && out.includes('Stopped before building anything.') && ranNothing && leaks(out).length === 0,
      `${what}: stopped in ${seconds(t)} s, before make-design, prebuild and Gradle, with no secret printed`,
    );
  }
  const androidAfter = fs.existsSync(androidDir) ? fs.statSync(androidDir).mtimeMs : null;
  result(androidBefore === androidAfter, 'android/ was not touched');
}

function gradlew(args, variable) {
  const exe = win ? `"${path.join(androidDir, 'gradlew.bat')}"` : './gradlew';
  const env = { ...process.env, LIMO_FORM: 'child', EXPO_NO_TELEMETRY: '1' };
  if (variable) env[KEY_VARIABLE] = variable;
  else delete env[KEY_VARIABLE];
  const r = spawnSync(exe, [...args, '--no-daemon', '-Pkotlin.compiler.execution.strategy=in-process'], {
    cwd: androidDir,
    env,
    encoding: 'utf8',
    shell: win,
    maxBuffer: 512 * 1024 * 1024,
  });
  return { status: r.status, out: `${r.stdout ?? ''}${r.stderr ?? ''}` };
}

function signerOf(apk, tools) {
  const s = apkSigning(apk, tools.apksigner);
  return s.verified && s.signers.length === 1 ? { ...s.signers[0], schemes: s.schemes } : null;
}

// Every file under a folder that holds one of the needles, as UTF-8 or UTF-16.
function filesHolding(dir, needles) {
  const bufs = needles.flatMap((n) => [Buffer.from(n, 'utf8'), Buffer.from(n, 'utf16le')]);
  const found = [];
  let files = 0;
  const walk = (d) => {
    for (const e of fs.readdirSync(d, { withFileTypes: true })) {
      const p = path.join(d, e.name);
      if (e.isDirectory()) walk(p);
      else if (e.isFile()) {
        files += 1;
        const b = fs.readFileSync(p);
        if (bufs.some((n) => b.includes(n))) found.push(path.relative(dir, p));
      }
    }
  };
  walk(dir);
  return { found, files };
}

function gradleBuilds(expectedSha) {
  const tools = buildTools();

  console.log('\n[test] scripts/build.mjs child, with the variable set (storeFile a bare name); a full build, some minutes');
  let t = Date.now();
  const relative = properties(GOOD);
  const a = spawnSync(process.execPath, [path.join('scripts', 'build.mjs'), 'child', '--out', apksDir], {
    cwd: appDir,
    encoding: 'utf8',
    env: { ...process.env, [KEY_VARIABLE]: relative },
    maxBuffer: 512 * 1024 * 1024,
  });
  const aOut = `${a.stdout}${a.stderr}`;
  const aApk = path.join(apksDir, 'limo-child-release.apk');
  const aSigner = a.status === 0 && fs.existsSync(aApk) ? signerOf(aApk, tools) : null;
  result(a.status === 0, `the build went through (${seconds(t)} s)`, a.status === 0 ? null : aOut.split(/\r?\n/).slice(-25).join('\n        '));
  result(aSigner?.sha256 === expectedSha, `the APK carries the throwaway certificate (${aSigner?.subject ?? 'none'}; schemes ${aSigner?.schemes.join(', ') ?? 'none'})`);
  result(aOut.includes('Signed with the release key LIMO_RELEASE_KEY_FILE names'), 'the build says at its end that it signed with the release key');
  result(aOut.includes('[LiMo] Release signing with the key LIMO_RELEASE_KEY_FILE names'), 'Gradle ran limoCheckReleaseKey and said whose key it signs with');
  result(leaks(aOut).length === 0, 'the build\'s output holds no password, no path of the key and no file name');

  console.log('\n[test] gradlew assembleRelease by hand, storeFile a full path');
  t = Date.now();
  const absolute = properties({ ...GOOD, storeFile: fullStore.replaceAll('\\', '/') });
  const b = gradlew(['assembleRelease'], absolute);
  const bApk = path.join(androidDir, 'app', 'build', 'outputs', 'apk', 'release', 'app-release.apk');
  const bSigner = b.status === 0 ? signerOf(bApk, tools) : null;
  result(b.status === 0 && bSigner?.sha256 === expectedSha, `the APK carries the throwaway certificate (${seconds(t)} s)`, b.status === 0 ? null : b.out.split(/\r?\n/).slice(-25).join('\n        '));
  result(leaks(b.out).length === 0, 'Gradle\'s output holds no password, no path of the key and no file name');

  t = Date.now();
  const scan = filesHolding(androidDir, [PASSWORD]);
  result(scan.found.length === 0, `no file under android/ holds the password (${scan.files} files read in ${seconds(t)} s)`, scan.found.join(', ') || null);

  for (const [what, variable, expected] of [
    ['a wrong storePassword', properties({ ...GOOD, storePassword: WRONG }), 'does not open with storePassword.'],
    ['the variable pointing at no file', path.join(keysDir, 'none.properties'), 'points at no file'],
  ]) {
    console.log(`\n[test] gradlew assembleRelease by hand, with ${what}`);
    t = Date.now();
    const c = gradlew(['assembleRelease'], variable);
    const stoppedByCheck = /limoCheckReleaseKey/.test(c.out) && c.out.includes(`[LiMo] `) && c.out.includes(expected);
    result(c.status !== 0 && stoppedByCheck, `stopped by limoCheckReleaseKey with the check's message (${seconds(t)} s)`, c.status !== 0 && stoppedByCheck ? null : c.out.split(/\r?\n/).slice(-25).join('\n        '));
    result(leaks(c.out).length === 0, 'Gradle\'s output holds no password, no path of the key and no file name');
  }

  // What Android Gradle itself says of a wrong password, were the check
  // skipped: it must hold no password either. (It names the keystore's path,
  // which is why the check runs first.)
  console.log('\n[test] gradlew assembleRelease by hand, with a wrong storePassword and limoCheckReleaseKey skipped');
  t = Date.now();
  const e = gradlew(['assembleRelease', '-x', ':app:limoCheckReleaseKey'], properties({ ...GOOD, storePassword: WRONG }));
  const said = e.out.split(/\r?\n/).filter((l) => /password|keystore|key store/i.test(l)).map((l) => l.trim()).slice(0, 4);
  result(e.status !== 0, `Android Gradle stops it (${seconds(t)} s)`);
  result(!e.out.includes(PASSWORD) && !e.out.includes(WRONG), 'Android Gradle\'s own message holds no password');
  console.log(`        it says (paths replaced): ${said.map((l) => SECRETS.slice(3, 6).reduce((s, p) => s.replaceAll(p, '<keys folder>'), l)).join(' | ')}`);

  console.log('\n[test] scripts/build.mjs child --debug, with the variable set; a full build, some minutes');
  t = Date.now();
  const d = spawnSync(process.execPath, [path.join('scripts', 'build.mjs'), 'child', '--debug', '--out', apksDir], {
    cwd: appDir,
    encoding: 'utf8',
    env: { ...process.env, [KEY_VARIABLE]: relative },
    maxBuffer: 512 * 1024 * 1024,
  });
  const dOut = `${d.stdout}${d.stderr}`;
  const dApk = path.join(apksDir, 'limo-child-debug.apk');
  const dSigner = d.status === 0 && fs.existsSync(dApk) ? signerOf(dApk, tools) : null;
  result(d.status === 0, `the build went through (${seconds(t)} s)`, d.status === 0 ? null : dOut.split(/\r?\n/).slice(-25).join('\n        '));
  result(Boolean(dSigner) && isDebugCertificate(dSigner.subject), `the debug APK carries Android's debug certificate (${dSigner?.subject ?? 'none'})`);
  result(dOut.includes('a debug build never uses it') && !dOut.includes('limoCheckReleaseKey'), 'the build says it does not use the key, and Gradle never checked it');
  result(leaks(dOut).length === 0, 'the build\'s output holds no password, no path of the key and no file name');
}

try {
  console.log(`[test] throwaway keys in a new temporary folder${quick ? '' : '; then real Gradle builds of the child form'}`);
  const expectedSha = makeKeys();
  keyChecks(expectedSha);
  buildStops();
  if (!quick) gradleBuilds(expectedSha);
} catch (e) {
  failures += 1;
  console.log(`  FAIL  ${e.message}`);
} finally {
  fs.rmSync(keysDir, { recursive: true, force: true });
  fs.rmSync(apksDir, { recursive: true, force: true, maxRetries: 12, retryDelay: 500 });
}
console.log(`\n[test] ${failures === 0 ? 'every test passed' : `${failures} FAILED`} in ${seconds(started)} s; the throwaway keys are deleted`);
process.exit(failures === 0 ? 0 : 1);
