// Looks inside the built APKs in out/ and checks each form against what it
// must and must not hold. Run it after scripts/build.mjs.
//
//   node scripts/inspect.mjs
//
// For each of out/limo-child-release.apk and out/limo-guardian-release.apk
// that exists, with the Android SDK's own tools (aapt2, apksigner, zipalign):
//   - the package, label, versions and ABI;
//   - every permission the app ends up with;
//   - every component in the merged manifest, with its guard;
//   - the checks below, each PASS or FAIL;
//   - the signing certificate and the 16 KB alignment of native libraries;
//   - what the APK's bytes are made of.
//
// The checks, for both forms: no INTERNET or any other permission on the
// never list in app.config.ts; no screen reachable from a link (no VIEW
// intent filter); backups off; not debuggable; no clear text traffic; the
// build PC's user name, home folder and repository folder appear nowhere in
// the APK's files.
// For the child form: every stub component is there with its guard, and the
// development receiver is not (it is in debug builds only), nor anything of
// the page probe (its class, its words, its broadcast), while the watch's
// release WindowPage, which only matches page names, is; it holds the
// Phone permission (READ_PHONE_STATE) and may ask to be left out of battery
// optimisation (REQUEST_IGNORE_BATTERY_OPTIMIZATIONS); the Rust bridge
// (liblimo_phone_ffi.so) is there for every ABI the APK names, with JNA's
// library and the bindings in the dex. For the guardian form: no
// accessibility service, no device admin receiver, no usage access, no
// foreground service, no Phone permission, no asking to be left out of
// battery optimisation, and nothing of the child module
// at all: no Rust bridge, no JNA, no bindings.
// For both: nothing of the testkit (the Rust feature that plays the master
// PC in the JVM tests) in any native library or dex file. The same check is
// run once on the library built for the JVM tests, which has the testkit,
// to show that it finds it.
//
// Signing, for each APK: the certificate's subject and SHA-256, whether it is
// Android's debug certificate, and the schemes it verifies with. With
// LIMO_RELEASE_KEY_FILE set, both release APKs must be signed with the key it
// names (checked with scripts/ReleaseKeyCheck.java) and not with the debug
// certificate; without it, with the debug certificate, as a build without it
// signs them. Both forms must carry the same certificate. The debug build
// (out/limo-child-debug.apk), when there is one, must always carry the debug
// certificate. Nothing here prints a password or a path of the key.
//
// Exit code 0 when every check passes, 1 when one fails, 2 when there is
// nothing to inspect or a tool is missing.

import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import zlib from 'node:zlib';
import { fileURLToPath } from 'node:url';

import { testkitProblems } from './build-engine.mjs';
import { KEY_VARIABLE, apkSigning, buildTools, checkReleaseKey, colons, tool } from './signing.mjs';

const appDir = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const outDir = path.join(appDir, 'out');

// ---------------------------------------------------------------------------
// The manifest, from `aapt2 dump xmltree`, as a tree of elements.

function parseXmlTree(text) {
  const root = { tag: 'root', attrs: {}, children: [], depth: -1 };
  const stack = [root];
  for (const line of text.split(/\r?\n/)) {
    const m = /^(\s*)(E|A): (.*)$/.exec(line);
    if (!m) continue;
    const depth = m[1].length;
    if (m[2] === 'E') {
      const tag = m[3].split(' ')[0];
      while (stack[stack.length - 1].depth >= depth) stack.pop();
      const node = { tag, attrs: {}, children: [], depth };
      stack[stack.length - 1].children.push(node);
      stack.push(node);
    } else {
      const a = /^(?:[^:]+:\/\/[^:]+:)?(?:http:\/\/schemas\.android\.com\/apk\/res\/android:)?([\w:]+)(?:\(0x[0-9a-f]+\))?=(.*)$/.exec(m[3]);
      if (!a) continue;
      let value = a[2];
      const quoted = /^"(.*)" \(Raw: .*\)$/.exec(value) ?? /^"(.*)"$/.exec(value);
      if (quoted) value = quoted[1];
      else value = value.replace(/^\(type 0x[0-9a-f]+\)/, '');
      stack[stack.length - 1].attrs[a[1]] = value;
    }
  }
  return root;
}

function* walk(node) {
  yield node;
  for (const c of node.children) yield* walk(c);
}

function manifestFacts(xml) {
  const tree = parseXmlTree(xml);
  const all = [...walk(tree)];
  const manifest = all.find((n) => n.tag === 'manifest');
  const app = all.find((n) => n.tag === 'application');
  const permissions = all.filter((n) => n.tag === 'uses-permission' || n.tag === 'uses-permission-sdk-23').map((n) => n.attrs.name);
  const declared = all.filter((n) => n.tag === 'permission').map((n) => `${n.attrs.name} (protectionLevel ${n.attrs.protectionLevel})`);
  const components = (app?.children ?? [])
    .filter((n) => ['activity', 'activity-alias', 'service', 'receiver', 'provider'].includes(n.tag))
    .map((n) => {
      const actions = [...walk(n)].filter((c) => c.tag === 'action').map((c) => c.attrs.name);
      const categories = [...walk(n)].filter((c) => c.tag === 'category').map((c) => c.attrs.name);
      const schemes = [...walk(n)].filter((c) => c.tag === 'data').map((c) => c.attrs.scheme).filter(Boolean);
      return {
        kind: n.tag,
        name: n.attrs.name,
        exported: n.attrs.exported,
        permission: n.attrs.permission,
        fgsType: n.attrs.foregroundServiceType,
        actions,
        categories,
        schemes,
      };
    });
  const queries = all.filter((n) => n.tag === 'queries').flatMap((q) => [...walk(q)].filter((c) => c.tag === 'action' || c.tag === 'package').map((c) => c.attrs.name));
  return { package: manifest?.attrs.package, app: app?.attrs ?? {}, permissions, declared, components, queries };
}

// ---------------------------------------------------------------------------
// The APK as a zip: each entry's name and sizes, and its bytes when asked.

function zipEntries(file) {
  const buf = fs.readFileSync(file);
  let eocd = -1;
  for (let i = buf.length - 22; i >= Math.max(0, buf.length - 65557); i--) {
    if (buf.readUInt32LE(i) === 0x06054b50) {
      eocd = i;
      break;
    }
  }
  if (eocd < 0) throw new Error(`${file} is not a zip`);
  const count = buf.readUInt16LE(eocd + 10);
  let at = buf.readUInt32LE(eocd + 16);
  const entries = [];
  for (let i = 0; i < count; i++) {
    const method = buf.readUInt16LE(at + 10);
    const compressed = buf.readUInt32LE(at + 20);
    const size = buf.readUInt32LE(at + 24);
    const nameLen = buf.readUInt16LE(at + 28);
    const extraLen = buf.readUInt16LE(at + 30);
    const commentLen = buf.readUInt16LE(at + 32);
    const local = buf.readUInt32LE(at + 42);
    const name = buf.toString('utf8', at + 46, at + 46 + nameLen);
    entries.push({
      name,
      method,
      compressed,
      size,
      bytes() {
        const start = local + 30 + buf.readUInt16LE(local + 26) + buf.readUInt16LE(local + 28);
        const raw = buf.subarray(start, start + compressed);
        return method === 8 ? zlib.inflateRawSync(raw) : raw;
      },
    });
    at += 46 + nameLen + extraLen + commentLen;
  }
  return entries;
}

function partOf(name) {
  if (name.startsWith('lib/')) return 'native libraries (lib/)';
  if (/^classes\d*\.dex$/.test(name)) return 'dex files';
  if (name === 'assets/index.android.bundle') return 'JavaScript bundle (Hermes)';
  // A release build shortens resource paths (res/font/x.ttf becomes res/Ab.ttf).
  if (name.endsWith('.ttf')) return 'fonts';
  if (name.startsWith('res/') || name === 'resources.arsc') return 'other resources';
  if (name.startsWith('META-INF/')) return 'signature and metadata (META-INF)';
  if (name.startsWith('assets/')) return 'other assets';
  return 'other';
}

// ---------------------------------------------------------------------------
// The checks

const NEVER = [
  'android.permission.INTERNET',
  'android.permission.ACCESS_NETWORK_STATE',
  'android.permission.ACCESS_WIFI_STATE',
  'android.permission.CHANGE_WIFI_MULTICAST_STATE',
  'android.permission.NEARBY_WIFI_DEVICES',
  'android.permission.SYSTEM_ALERT_WINDOW',
  'android.permission.CAMERA',
  'android.permission.RECORD_AUDIO',
  'android.permission.ACCESS_FINE_LOCATION',
  'android.permission.ACCESS_COARSE_LOCATION',
  'android.permission.ACCESS_BACKGROUND_LOCATION',
  'android.permission.READ_CONTACTS',
  'android.permission.WRITE_CONTACTS',
  'android.permission.GET_ACCOUNTS',
  'android.permission.READ_EXTERNAL_STORAGE',
  'android.permission.WRITE_EXTERNAL_STORAGE',
  'android.permission.QUERY_ALL_PACKAGES',
  'com.google.android.gms.permission.AD_ID',
];

// Foreground service types as aapt2 prints them.
const FGS_TYPES = { '0x40000000': 'specialUse', '0x1': 'dataSync', '0x400': 'systemExempted' };

const CHILD = 'ai.lucidloom.limo.child';
const STUBS = [
  ['service', `${CHILD}.watch.WatchService`, 'android.permission.BIND_ACCESSIBILITY_SERVICE'],
  ['receiver', `${CHILD}.watch.ClockReceiver`, undefined],
  ['activity', `${CHILD}.show.WaysOutActivity`, undefined],
  ['service', `${CHILD}.keep.KeepService`, undefined],
  ['receiver', `${CHILD}.keep.RestartReceiver`, undefined],
  ['receiver', `${CHILD}.keep.AlarmReceiver`, undefined],
  ['receiver', `${CHILD}.keep.AdminReceiver`, 'android.permission.BIND_DEVICE_ADMIN'],
];
// The Rust bridge (phone/ffi) and what comes with it.
const BRIDGE = 'liblimo_phone_ffi.so';
const BRIDGE_EXPORT = Buffer.from('uniffi_limo_phone_ffi_fn_method_phoneengine_tick');
const BINDINGS_CLASS = Buffer.from('Lai/lucidloom/limo/child/ffi/PhoneEngine;');
const BINDINGS_PACKAGE = Buffer.from('Lai/lucidloom/limo/child/ffi/');
const TESTKIT_CLASSES = [Buffer.from('Lai/lucidloom/limo/child/ffi/testkit/'), Buffer.from('TestFamily')];
// The debug build's probe of page names (dev/PageProbe): its class, its log
// words, and the broadcast that switches it on. None may be in a release build.
const PROBE_ACTION = 'ai.lucidloom.limo.child.dev.PROBE';
const PROBE_TRACES = [
  Buffer.from('Lai/lucidloom/limo/child/dev/PageProbe;'),
  Buffer.from('probe window'),
  Buffer.from(PROBE_ACTION),
];
// The watch's WindowPage, whose release copy matches a page's class name and logs nothing.
const WINDOW_PAGE = Buffer.from('Lai/lucidloom/limo/child/watch/WindowPage;');

// The child form must hold each of these; the guardian form none of them
// but POST_NOTIFICATIONS. READ_PHONE_STATE is the Phone permission, for the
// child form only: to know that a call rings or is on.
// REQUEST_IGNORE_BATTERY_OPTIMIZATIONS lets setup's Battery step show
// Android's own question, for the child form only.
const CHILD_PERMISSIONS = [
  'android.permission.FOREGROUND_SERVICE',
  'android.permission.FOREGROUND_SERVICE_SPECIAL_USE',
  'android.permission.POST_NOTIFICATIONS',
  'android.permission.RECEIVE_BOOT_COMPLETED',
  'android.permission.PACKAGE_USAGE_STATS',
  'android.permission.READ_PHONE_STATE',
  'android.permission.REQUEST_IGNORE_BATTERY_OPTIMIZATIONS',
];

function inspect(form, file, tools) {
  const checks = [];
  const check = (ok, text) => checks.push([ok, text]);
  console.log(`\n=== ${form}: ${path.relative(appDir, file)} (${fs.statSync(file).size} bytes)`);

  const badging = tool(tools.aapt2, ['dump', 'badging', file]).out;
  const pick = (re) => re.exec(badging)?.[1];
  console.log(`package     ${pick(/package: name='([^']+)'/)} version ${pick(/versionName='([^']+)'/)} (${pick(/versionCode='([^']+)'/)})`);
  console.log(`label       ${pick(/application-label:'([^']+)'/)}`);
  console.log(`sdk         min ${pick(/(?:minSdkVersion|sdkVersion):'(\d+)'/)}, target ${pick(/targetSdkVersion:'(\d+)'/)}`);
  console.log(`native code ${pick(/native-code: (.*)/) ?? 'none'}`);

  const facts = manifestFacts(tool(tools.aapt2, ['dump', 'xmltree', '--file', 'AndroidManifest.xml', file]).out);
  console.log('\npermissions it holds:');
  for (const p of facts.permissions) console.log(`  ${p}`);
  if (facts.declared.length) {
    console.log('permissions it declares (its own):');
    for (const p of facts.declared) console.log(`  ${p}`);
  }
  console.log('\ncomponents:');
  for (const c of facts.components) {
    const guard = c.permission ? ` guarded by ${c.permission}` : '';
    const fgs = c.fgsType ? ` type ${FGS_TYPES[c.fgsType] ?? c.fgsType}` : '';
    const acts = c.actions.length ? ` [${c.actions.join(', ')}]` : '';
    console.log(`  ${c.kind} ${c.name} exported=${c.exported ?? 'unset'}${guard}${fgs}${acts}`);
  }
  console.log(`\nqueries: ${facts.queries.length ? facts.queries.join(', ') : 'none'}`);

  const expectedPackage = form === 'child' ? CHILD : 'ai.lucidloom.limo';
  check(facts.package === expectedPackage, `package is ${expectedPackage}`);
  const held = new Set(facts.permissions);
  for (const p of NEVER) check(!held.has(p), `does not hold ${p}`);
  const links = facts.components.filter((c) => c.actions.includes('android.intent.action.VIEW') || c.schemes.length);
  check(links.length === 0, 'no screen can be opened from a link (no VIEW intent filter, no scheme)');
  check(facts.app.allowBackup === '0' || facts.app.allowBackup === 'false', 'allowBackup is false');
  check(Boolean(facts.app.dataExtractionRules), 'has data extraction rules (nothing in transfers to a new phone)');
  check(!facts.app.debuggable || facts.app.debuggable === '0' || facts.app.debuggable === 'false', 'is not debuggable');
  check(!facts.app.usesCleartextTraffic || facts.app.usesCleartextTraffic === '0' || facts.app.usesCleartextTraffic === 'false', 'allows no clear text traffic');

  const find = (kind, name) => facts.components.find((c) => c.kind === kind && c.name === name);
  if (form === 'child') {
    for (const [kind, name, guard] of STUBS) {
      const c = find(kind, name);
      check(Boolean(c), `has ${kind} ${name}`);
      if (c && guard) check(c.permission === guard, `${name} is guarded by ${guard}`);
      if (c && !guard) check(c.exported === '0' || c.exported === 'false', `${name} is not exported`);
    }
    for (const p of CHILD_PERMISSIONS) check(held.has(p), `holds ${p}`);
    check(!facts.components.some((c) => c.name.endsWith('.dev.DevReceiver')), 'has no development receiver (debug builds only)');
  } else {
    check(!facts.components.some((c) => c.permission === 'android.permission.BIND_ACCESSIBILITY_SERVICE'), 'has no accessibility service');
    check(!facts.components.some((c) => c.permission === 'android.permission.BIND_DEVICE_ADMIN'), 'has no device admin receiver');
    check(!held.has('android.permission.PACKAGE_USAGE_STATS'), 'has no usage access (PACKAGE_USAGE_STATS)');
    check(!facts.components.some((c) => c.fgsType), 'has no foreground service');
    check(!facts.components.some((c) => c.name.startsWith(`${CHILD}.`)), 'has no component of the child module');
    for (const p of CHILD_PERMISSIONS.filter((x) => x !== 'android.permission.POST_NOTIFICATIONS')) {
      check(!held.has(p), `does not hold ${p}`);
    }
  }

  // The APK's bytes, and no trace of the build PC's user.
  const entries = zipEntries(file);
  const parts = new Map();
  for (const e of entries) parts.set(partOf(e.name), (parts.get(partOf(e.name)) ?? 0) + e.compressed);
  console.log('\nwhat the APK is made of (bytes as stored):');
  for (const [part, bytes] of [...parts].sort((a, b) => b[1] - a[1])) console.log(`  ${String(bytes).padStart(10)}  ${part}`);
  console.log('native libraries:');
  for (const e of entries.filter((x) => x.name.startsWith('lib/')).sort((a, b) => b.compressed - a.compressed)) {
    console.log(`  ${String(e.compressed).padStart(10)}  ${e.name}`);
  }
  const dex = entries.filter((e) => /^classes\d*\.dex$/.test(e.name));
  const childClass = Buffer.from('Lai/lucidloom/limo/child/watch/WatchService;');
  const inDex = dex.some((e) => e.bytes().includes(childClass));
  if (form === 'child') check(inDex, 'the child module is compiled in (WatchService in the dex)');
  else check(!inDex, 'the child module is not compiled in (no WatchService in the dex)');
  const devClass = Buffer.from('Lai/lucidloom/limo/child/dev/DevReceiver;');
  check(!dex.some((e) => e.bytes().includes(devClass)), 'the development receiver is not compiled in');
  // The probe of Settings' and the installer's page names (dev/PageProbe,
  // debug builds only): neither its class, nor its words, nor the broadcast
  // that switches it on is in a release build.
  for (const needle of PROBE_TRACES) {
    check(!dex.some((e) => e.bytes().includes(needle)), `nothing of the page probe is compiled in (no "${needle.toString('latin1')}" in the dex)`);
  }
  check(
    !facts.components.some((c) => c.actions.includes(PROBE_ACTION)),
    `no component takes the page probe's broadcast (${PROBE_ACTION})`,
  );
  if (form === 'child') {
    // The watch's own copy of WindowPage, which only matches a class name
    // against the guarded list, is the release one: it is there, and the
    // probe (above) is not.
    check(dex.some((e) => e.bytes().includes(WINDOW_PAGE)), 'the watch matches page names (WindowPage in the dex), with no probe');
  }

  // The Rust bridge: in the child form only, and never with the testkit.
  const libs = entries.filter((e) => e.name.startsWith('lib/') && e.name.endsWith('.so'));
  const bridges = libs.filter((e) => path.posix.basename(e.name) === BRIDGE);
  const abis = [...(pick(/native-code: (.*)/) ?? '').matchAll(/'([^']+)'/g)].map((m) => m[1]);
  const inDexAny = (needle) => dex.some((e) => e.bytes().includes(needle));
  if (form === 'child') {
    check(abis.length > 0, `names its ABIs (${abis.join(', ') || 'none'})`);
    for (const abi of abis) {
      const lib = bridges.find((e) => e.name === `lib/${abi}/${BRIDGE}`);
      check(Boolean(lib), `holds the Rust bridge for ${abi} (lib/${abi}/${BRIDGE}${lib ? `, ${lib.size} bytes` : ''})`);
      check(libs.some((e) => e.name === `lib/${abi}/libjnidispatch.so`), `holds JNA's library for ${abi} (libjnidispatch.so)`);
    }
    check(
      bridges.length > 0 && bridges.every((e) => e.bytes().includes(BRIDGE_EXPORT)),
      'the Rust bridge exports the engine (PhoneEngine.tick)',
    );
    check(inDexAny(BINDINGS_CLASS), 'the bindings are compiled in (PhoneEngine in the dex)');
  } else {
    check(bridges.length === 0, `has no Rust bridge (no ${BRIDGE})`);
    check(!libs.some((e) => e.name.endsWith('/libjnidispatch.so')), 'has no JNA library (no libjnidispatch.so)');
    check(!inDexAny(BINDINGS_PACKAGE), 'has no bindings (nothing of ai.lucidloom.limo.child.ffi in the dex)');
  }
  const testkitIn = libs.filter((e) => testkitProblems(e.bytes()).length > 0).map((e) => e.name);
  check(
    testkitIn.length === 0,
    `no native library holds the testkit or SQLite${testkitIn.length ? ` (found in ${testkitIn.join(', ')})` : ''}`,
  );
  check(!TESTKIT_CLASSES.some(inDexAny), 'no testkit class in the dex (no ...ffi.testkit, no TestFamily)');

  const user = os.userInfo().username;
  const home = os.homedir();
  const repo = path.resolve(appDir, '..', '..');
  const needles = [user, home, repo]
    .flatMap((s) => [s, s.replaceAll('\\', '/'), s.replaceAll('\\', '\\\\')])
    .filter((s) => s && s.length >= 3)
    .flatMap((s) => [Buffer.from(s, 'utf8'), Buffer.from(s, 'utf16le')]);
  const traced = entries.filter((e) => {
    const b = e.bytes();
    return needles.some((n) => b.includes(n));
  });
  check(traced.length === 0, `the build PC's user name, home folder and repository folder appear in no file${traced.length ? ` (found in ${traced.map((e) => e.name).join(', ')})` : ''}`);

  const signer = signedBy(file, tools, check);
  if (releaseKey) {
    check(Boolean(signer) && !signer.debug, `${KEY_VARIABLE} is set: not signed with Android's debug certificate`);
    if (releaseKey.ok) {
      check(signer?.sha256 === releaseKey.sha256, `signed with the key ${KEY_VARIABLE} names (certificate SHA-256 ${colons(releaseKey.sha256)})`);
    } else {
      check(false, `the key ${KEY_VARIABLE} names: ${releaseKey.message}`);
    }
  } else {
    check(Boolean(signer?.debug), `${KEY_VARIABLE} is not set: signed with Android's debug certificate, as a build without it signs it`);
  }
  const aligned = tool(tools.zipalign, ['-c', '-P', '16', '4', file]);
  check(aligned.status === 0, 'zipalign -c -P 16: native libraries are 16 KB aligned');

  printChecks(checks);
  return { ok: checks.every(([ok]) => ok), signer };
}

// Prints who signed an APK and checks that the signature verifies with one
// signer. Returns the signer, or null.
function signedBy(file, tools, check) {
  const signing = apkSigning(file, tools.apksigner);
  const signer = signing.signers.length === 1 ? signing.signers[0] : null;
  for (const s of signing.signers) {
    console.log(`\nsigned by   ${s.subject}`);
    console.log(`certificate SHA-256 ${colons(s.sha256)}`);
    console.log(`Android's debug certificate: ${s.debug ? "yes (fine for a cable, never for a family)" : 'no'}`);
  }
  console.log(`schemes     ${signing.schemes.length ? signing.schemes.join(', ') : 'none'}`);
  check(signing.verified, 'the signature verifies');
  check(Boolean(signer), `one signer (${signing.signers.length})`);
  return signer;
}

function printChecks(checks) {
  console.log('\nchecks:');
  for (const [ok, text] of checks) console.log(`  ${ok ? 'PASS' : 'FAIL'}  ${text}`);
}

let tools;
try {
  tools = buildTools();
} catch (e) {
  console.error(`inspect: ${e.message}`);
  process.exit(2);
}
console.log(`inspect: Android build tools ${tools.version}`);
// The release key the APKs should carry, when LIMO_RELEASE_KEY_FILE is set.
const releaseKey = process.env[KEY_VARIABLE] ? checkReleaseKey(process.env) : null;
console.log(
  releaseKey
    ? `inspect: ${KEY_VARIABLE} is set: the release APKs must carry the key it names`
    : `inspect: ${KEY_VARIABLE} is not set: the release APKs must carry Android's debug certificate`,
);
let found = 0;
let allPass = true;
const signers = {};
for (const form of ['child', 'guardian']) {
  const file = path.join(outDir, `limo-${form}-release.apk`);
  if (!fs.existsSync(file)) {
    console.log(`\n=== ${form}: not built (no ${path.relative(appDir, file)})`);
    continue;
  }
  found += 1;
  const result = inspect(form, file, tools);
  signers[form] = result.signer;
  allPass = result.ok && allPass;
}
if (!found) {
  console.error('\ninspect: nothing to inspect. Build first: npm run build:both');
  process.exit(2);
}

// Signing across the files: both forms with one certificate, and the debug
// build always with the debug certificate.
{
  const checks = [];
  const check = (ok, text) => checks.push([ok, text]);
  console.log('\n=== signing across the files');
  if (signers.child !== undefined && signers.guardian !== undefined) {
    check(
      Boolean(signers.child?.sha256) && signers.child?.sha256 === signers.guardian?.sha256,
      'both forms carry the same certificate',
    );
  } else {
    console.log('  (one form only: nothing to compare)');
  }
  const debugApk = path.join(outDir, 'limo-child-debug.apk');
  if (fs.existsSync(debugApk)) {
    console.log(`\n${path.relative(appDir, debugApk)} (${fs.statSync(debugApk).size} bytes):`);
    const signer = signedBy(debugApk, tools, check);
    check(Boolean(signer?.debug), "the debug build is signed with Android's debug certificate, as it always is");
  } else {
    console.log('\nno debug build (out/limo-child-debug.apk) to look at');
  }
  if (checks.length) printChecks(checks);
  allPass = checks.every(([ok]) => ok) && allPass;
}

// The control: the library built for the JVM tests has the testkit, so the
// check above must find it there. Shown when that library has been built
// (npm run test:child).
const testNative = path.join(appDir, 'modules', 'limo-child', 'android', 'src', 'generated', 'test-native');
if (fs.existsSync(testNative)) {
  for (const name of fs.readdirSync(testNative)) {
    const seen = testkitProblems(fs.readFileSync(path.join(testNative, name)));
    console.log(`\ncontrol: the testkit check on the JVM tests' library ${name}: ${seen.length ? seen.join('; ') : 'FOUND NOTHING'}`);
    if (seen.length === 0) allPass = false;
  }
}
console.log(`\ninspect: ${allPass ? 'every check passed' : 'a check FAILED'}`);
process.exit(allPass ? 0 : 1);
