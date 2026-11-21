// Tests for release.ps1, against a fake of the download service's three
// calls on this PC: GET /downloads/limo/current/release.json, PUT
// /api/downloads/limo/<version>/<file> and POST .../publish. Nothing leaves
// this PC.
//
//   node --test release.test.mjs        (in installer\, on Windows)
//
// The installers are made here, once: the throwaway phone app the dry build
// carries (console's throwaway_apk example, unsigned), the same with an APK
// signing block of a key of its own, and with Android's debug certificate's
// name in it; each in a 7-Zip self-extracting program (a Windows program, as
// the real installer is, from which 7-Zip takes phone\limo-child.apk the same
// way). Needs cargo and 7-Zip, as release.ps1 does.

import { test, before, after } from 'node:test';
import assert from 'node:assert/strict';
import { execFile, execFileSync } from 'node:child_process';
import { createHash, randomBytes } from 'node:crypto';
import fs from 'node:fs';
import http from 'node:http';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const installer = path.dirname(fileURLToPath(import.meta.url));
const repo = path.dirname(installer);
const script = path.join(installer, 'release.ps1');
const sevenZip = path.join(process.env.ProgramFiles, '7-Zip', '7z.exe');
const version = /^\[package\][\s\S]*?^version\s*=\s*"([^"]+)"/m.exec(fs.readFileSync(path.join(repo, 'windows', 'Cargo.toml'), 'utf8'))[1];
const name = `LiMo_${version}_x64-setup.exe`;
const TOKEN = randomBytes(24).toString('hex');
const sha256 = (bytes) => createHash('sha256').update(bytes).digest('hex');

let tmp;
const made = {};

// ------------------------------------------------------------------ fixtures

// An APK signing block (APK Signature Scheme v2's id) put in before the
// central directory, as apksigner puts it, with the zip's end record moved on.
function withSigningBlock(apk, subject) {
  let eocd = -1;
  for (let i = apk.length - 22; i >= Math.max(0, apk.length - 65557); i--) {
    if (apk.readUInt32LE(i) === 0x06054b50) { eocd = i; break; }
  }
  assert.ok(eocd >= 0, 'the throwaway APK is a zip');
  const cd = apk.readUInt32LE(eocd + 16);
  const value = Buffer.from(`a certificate for the test, subject CN=${subject}, O=test`, 'latin1');
  const pair = Buffer.alloc(12 + value.length);
  pair.writeBigUInt64LE(BigInt(4 + value.length), 0);
  pair.writeUInt32LE(0x7109871a, 8);
  value.copy(pair, 12);
  const size = BigInt(pair.length + 8 + 16);
  const head = Buffer.alloc(8); head.writeBigUInt64LE(size);
  const tail = Buffer.alloc(8); tail.writeBigUInt64LE(size);
  const block = Buffer.concat([head, pair, tail, Buffer.from('APK Sig Block 42', 'latin1')]);
  const out = Buffer.concat([apk.subarray(0, cd), block, apk.subarray(cd)]);
  out.writeUInt32LE(cd + block.length, eocd + block.length + 16);
  return out;
}

// A Windows program holding phone\limo-child.apk (or another file), named
// as build.ps1 names an installer, with its .sha256 beside it.
function installerWith(label, apk, { fileName = name, inside = 'limo-child.apk', sum = true } = {}) {
  const work = path.join(tmp, `${label}-work`);
  fs.mkdirSync(path.join(work, 'phone'), { recursive: true });
  fs.writeFileSync(path.join(work, 'phone', inside), apk);
  const sfx = path.join(work, 'out.exe');
  execFileSync(sevenZip, ['a', '-sfx7z.sfx', sfx, `phone\\${inside}`], { cwd: work, stdio: 'ignore' });
  const folder = path.join(tmp, label);
  fs.mkdirSync(folder, { recursive: true });
  const file = path.join(folder, fileName);
  fs.copyFileSync(sfx, file);
  if (sum) fs.writeFileSync(`${file}.sha256`, `${sha256(fs.readFileSync(file))}  ${fileName}\n`);
  return file;
}

before(() => {
  tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'limo-release-test-'));
  execFileSync('cargo', ['run', '--quiet', '--manifest-path', path.join(repo, 'console', 'Cargo.toml'), '--no-default-features',
    '--example', 'throwaway_apk', '--', path.join(tmp, 'apk')], { env: { ...process.env, CARGO_TARGET_DIR: path.join(installer, 'target', 'cargo') }, stdio: 'ignore' });
  const unsigned = fs.readFileSync(path.join(tmp, 'apk', 'phone', 'limo-child.apk'));
  made.signed = installerWith('signed', withSigningBlock(unsigned, 'LiMo test release'));
  made.unsigned = installerWith('unsigned', unsigned);
  made.debug = installerWith('debug', withSigningBlock(unsigned, 'Android Debug'));
});

after(() => { if (tmp) fs.rmSync(tmp, { recursive: true, force: true }); });

// ------------------------------------------------------------------ the fake site

async function fakeSite(setup = {}) {
  const site = { release: null, requests: [], uploaded: null, ...setup };
  const server = http.createServer((req, res) => {
    const chunks = [];
    req.on('data', (c) => chunks.push(c));
    req.on('end', () => {
      const body = Buffer.concat(chunks);
      site.requests.push({ method: req.method, url: req.url, auth: req.headers.authorization ?? null, sha256: sha256(body), size: body.length, text: body.length < 4096 ? body.toString('utf8') : '' });
      const answer = (status, json) => { res.writeHead(status, { 'Content-Type': 'application/json' }); res.end(JSON.stringify(json)); };
      let m;
      if (req.method === 'GET' && req.url === '/downloads/limo/current/release.json') {
        return site.release ? answer(200, site.release) : answer(404, { error: 'not_found', message: 'Nothing here.' });
      }
      if (req.method === 'PUT' && (m = /^\/api\/downloads\/limo\/([^/]+)\/([^/]+)$/.exec(req.url))) {
        if (site.put) return answer(site.put.status, site.put.json);
        if (req.headers.authorization !== `Bearer ${TOKEN}`) return answer(401, { error: 'unauthorised', message: 'That is not the upload token.' });
        site.uploaded = { version: m[1], file: m[2], size: body.length, sha256: sha256(body) };
        return answer(200, { file: m[2], size: body.length, sha256: sha256(body) });
      }
      if (req.method === 'POST' && (m = /^\/api\/downloads\/limo\/([^/]+)\/publish$/.exec(req.url))) {
        if (site.publish) return answer(site.publish.status, site.publish.json);
        if (req.headers.authorization !== `Bearer ${TOKEN}`) return answer(401, { error: 'unauthorised', message: 'That is not the upload token.' });
        const up = site.uploaded;
        if (!up || up.version !== m[1]) return answer(409, { error: 'nothing_to_publish', message: `${m[1]} has no complete installer (the program).` });
        const date = '2026-11-21T12:00:00Z';
        site.release = {
          product: 'limo', updated: date, systems: {
            windows: {
              version: m[1], label: 'Windows 11, 64-bit, early test build', date, file: '/downloads/limo/current/LiMo-setup.exe',
              versioned: `/downloads/limo/${m[1]}/${up.file}`, size: up.size, sha256: up.sha256,
            },
          },
        };
        if (site.afterPublish) site.afterPublish(site.release);
        return answer(200, { version: m[1], systems: ['windows'], feed: null });
      }
      return answer(404, { error: 'not_found', message: 'Nothing here.' });
    });
  });
  await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
  site.url = `http://127.0.0.1:${server.address().port}`;
  site.close = () => new Promise((resolve) => server.close(resolve));
  site.sent = () => site.requests.filter((r) => r.method !== 'GET');
  return site;
}

// release.ps1 as the person runs it; its exit code and everything it said.
function release(args, env = {}) {
  const clean = { ...process.env };
  delete clean.DOWNLOADS_UPLOAD_TOKEN;
  delete clean.LIMO_DOWNLOADS_SITE;
  return new Promise((resolve) => {
    execFile('powershell.exe', ['-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', script, ...args], { env: { ...clean, ...env }, timeout: 180000 },
      (error, stdout, stderr) => {
        const said = `${stdout}${stderr}`;
        assert.ok(!said.includes(TOKEN), 'the token is never printed');
        resolve({ code: error ? error.code : 0, said });
      });
  });
}
const withToken = { DOWNLOADS_UPLOAD_TOKEN: TOKEN };

// ------------------------------------------------------------------ publish

test('without --yes it says what it would send and where, and sends nothing', async () => {
  const site = await fakeSite();
  try {
    const { code, said } = await release(['publish', '--file', made.signed, '--site', site.url]);
    const bytes = fs.readFileSync(made.signed);
    assert.equal(code, 0, said);
    for (const part of [name, `${bytes.length.toLocaleString('en-US')} bytes`, sha256(bytes), `LiMo ${version} for Windows`, site.url,
      `PUT  /api/downloads/limo/${version}/${name}`, `POST /api/downloads/limo/${version}/publish`, 'a release build, signed with a key of its own',
      'this will be the first', 'Nothing was sent']) {
      assert.ok(said.includes(part), `says ${part}:\n${said}`);
    }
    assert.deepEqual(site.requests.map((r) => `${r.method} ${r.url}`), ['GET /downloads/limo/current/release.json'], 'only what the site offers is read');
  } finally { await site.close(); }
});

test('with --yes it uploads the very bytes with the token, publishes, and reads back the size and SHA-256', async () => {
  const site = await fakeSite();
  try {
    const { code, said } = await release(['publish', '--yes', '--file', made.signed, '--site', site.url], withToken);
    const bytes = fs.readFileSync(made.signed);
    assert.equal(code, 0, said);
    assert.deepEqual(site.requests.map((r) => `${r.method} ${r.url}`), [
      'GET /downloads/limo/current/release.json', `PUT /api/downloads/limo/${version}/${name}`, `POST /api/downloads/limo/${version}/publish`,
      'GET /downloads/limo/current/release.json']);
    const [, put, publish, readBack] = site.requests;
    assert.equal(put.auth, `Bearer ${TOKEN}`);
    assert.equal(publish.auth, `Bearer ${TOKEN}`);
    assert.equal(readBack.auth, null, 'the token goes only with the upload and the publish');
    assert.deepEqual([put.size, put.sha256], [bytes.length, sha256(bytes)], 'the bytes of the file, whole');
    assert.deepEqual(JSON.parse(publish.text), { notes: '' });
    assert.match(said, /done: .* offers LiMo [\d.]+ for Windows, the size and SHA-256 of the file sent; the LucidLoom page takes every field/);
  } finally { await site.close(); }
});

test('the token may come from a one line file, and a short one is refused', async () => {
  const site = await fakeSite();
  try {
    const file = path.join(tmp, 'token');
    fs.writeFileSync(file, `${TOKEN}\r\n`);
    let { code, said } = await release(['publish', '--yes', '--file', made.signed, '--site', site.url, '--token-file', file]);
    assert.equal(code, 0, said);
    assert.equal(site.sent()[0].auth, `Bearer ${TOKEN}`);
    fs.writeFileSync(file, 'short\n');
    site.release = null; site.requests.length = 0;
    ({ code, said } = await release(['publish', '--yes', '--file', made.signed, '--site', site.url, '--token-file', file]));
    assert.equal(code, 1);
    assert.match(said, /shorter than the 32 characters/);
    assert.equal(site.sent().length, 0);
  } finally { await site.close(); }
});

test('with --yes and no token nothing is sent', async () => {
  const site = await fakeSite();
  try {
    const { code, said } = await release(['publish', '--yes', '--file', made.signed, '--site', site.url]);
    assert.equal(code, 1);
    assert.match(said, /no upload token: put it in DOWNLOADS_UPLOAD_TOKEN, or give its one line file with --token-file/);
    assert.equal(site.sent().length, 0);
  } finally { await site.close(); }
});

test('never a dry build, nor a file build.ps1 did not write for this version', async () => {
  const site = await fakeSite();
  try {
    const dry = path.join(tmp, 'dry');
    fs.mkdirSync(dry, { recursive: true });
    const cases = [
      [`LiMo_${version}_x64-setup-DRY-BUILD-throwaway-phone-app.exe`, /is a dry build, with a throwaway phone app: it is never published/],
      ['LiMo_99.0.0_x64-setup.exe', /is LiMo 99\.0\.0, and this checkout is LiMo/],
      ['LiMo-setup.exe', /is not an installer build\.ps1 writes for release/],
    ];
    for (const [fileName, words] of cases) {
      const file = path.join(dry, fileName);
      fs.copyFileSync(made.signed, file);
      fs.writeFileSync(`${file}.sha256`, `${sha256(fs.readFileSync(file))}  ${fileName}\n`);
      const { code, said } = await release(['publish', '--yes', '--file', file, '--site', site.url], withToken);
      assert.equal(code, 1, fileName);
      assert.match(said, words, fileName);
    }
    assert.equal(site.requests.length, 0, 'not even a look at the site');
  } finally { await site.close(); }
});

test('never a phone app that is not release signed: unsigned (a dry build renamed), or with the debug certificate', async () => {
  const site = await fakeSite();
  try {
    let { code, said } = await release(['publish', '--yes', '--file', made.unsigned, '--site', site.url], withToken);
    assert.equal(code, 1);
    assert.match(said, /is not release signed: it is not signed \(no APK signing block\)/);
    ({ code, said } = await release(['publish', '--yes', '--file', made.debug, '--site', site.url], withToken));
    assert.equal(code, 1);
    assert.match(said, /is not release signed: it is signed with Android's debug certificate/);
    const other = installerWith('no-app', Buffer.from('not LiMo'), { inside: 'something-else.txt' });
    ({ code, said } = await release(['publish', '--yes', '--file', other, '--site', site.url], withToken));
    assert.equal(code, 1);
    assert.match(said, /holds no phone app \(phone\\limo-child\.apk\)/);
    assert.equal(site.requests.length, 0);
    assert.ok(!fs.existsSync(path.join(installer, 'target', 'release-look')), 'what was taken out to look at is gone');
  } finally { await site.close(); }
});

test('the SHA-256 beside it must be the one build.ps1 wrote, and it must be a Windows program', async () => {
  const site = await fakeSite();
  try {
    const folder = path.join(tmp, 'sums');
    fs.mkdirSync(folder, { recursive: true });
    const file = path.join(folder, name);
    fs.copyFileSync(made.signed, file);
    let { code, said } = await release(['publish', '--file', file, '--site', site.url]);
    assert.equal(code, 1);
    assert.match(said, /has no .*\.sha256 beside it/);
    fs.writeFileSync(`${file}.sha256`, `${'0'.repeat(64)}  ${name}\n`);
    ({ code, said } = await release(['publish', '--file', file, '--site', site.url]));
    assert.equal(code, 1);
    assert.match(said, /is not the file build\.ps1 wrote/);
    fs.writeFileSync(file, Buffer.from('PK\u0003\u0004 not a program'));
    fs.writeFileSync(`${file}.sha256`, `${sha256(fs.readFileSync(file))}  ${name}\n`);
    ({ code, said } = await release(['publish', '--file', file, '--site', site.url]));
    assert.equal(code, 1);
    assert.match(said, /is not a Windows program/);
    assert.equal(site.requests.length, 0);
  } finally { await site.close(); }
});

test('a version already published is said plainly, and nothing is sent', async () => {
  const bytes = fs.readFileSync(made.signed);
  const offered = (v, sha) => ({ product: 'limo', systems: { windows: { version: v, label: 'x', date: 'd', file: '/downloads/limo/current/LiMo-setup.exe', size: 1, sha256: sha } } });
  for (const [release0, words] of [
    [offered(version, sha256(bytes)), new RegExp(`LiMo ${version.replace(/\./g, '\\.')} is published already \\(this same file\\): a version, once published, is never replaced`)],
    [offered(version, 'a'.repeat(64)), /is published already \(another file\)/],
    [offered('999.0.0', 'a'.repeat(64)), /the site offers LiMo 999\.0\.0, newer than/],
  ]) {
    const site = await fakeSite({ release: release0 });
    try {
      const { code, said } = await release(['publish', '--yes', '--file', made.signed, '--site', site.url], withToken);
      assert.equal(code, 1);
      assert.match(said, words);
      assert.match(said, /Nothing was sent\./);
      assert.equal(site.sent().length, 0);
    } finally { await site.close(); }
  }
});

test("the service's refusals are said plainly, and nothing is published after one", async () => {
  for (const [setup, words] of [
    [{ put: { status: 409, json: { error: 'published', message: `${version} is published; its files are never replaced. Upload a new version.` } } },
      /is published already, with other bytes, and the site refused it: a version, once published, is never replaced/],
    [{ put: { status: 401, json: { error: 'unauthorised', message: 'That is not the upload token.' } } }, /the site refused the token/],
    [{ put: { status: 404, json: { error: 'not_found', message: 'Nothing here.' } } }, /the site does not take LiMo yet: its downloads service has no row for LiMo\. Deploy the site/],
    [{ put: { status: 503, json: { error: 'not_switched_on', message: 'Uploads are not switched on here.' } } }, /uploads are not switched on at the site/],
    [{ put: { status: 403, json: null } }, /not as the downloads service does \(Cloudflare may have refused it/],
  ]) {
    const site = await fakeSite(setup);
    try {
      const { code, said } = await release(['publish', '--yes', '--file', made.signed, '--site', site.url], withToken);
      assert.equal(code, 1, said);
      assert.match(said, words);
      assert.deepEqual(site.sent().map((r) => r.method), ['PUT'], 'no publish after a refused upload');
    } finally { await site.close(); }
  }
  const site = await fakeSite({ publish: { status: 409, json: { error: 'older', message: `windows already offers 999.0.0; ${version} is older.` } } });
  try {
    const { code, said } = await release(['publish', '--yes', '--file', made.signed, '--site', site.url], withToken);
    assert.equal(code, 1);
    assert.match(said, /the site refused it: windows already offers 999\.0\.0/);
  } finally { await site.close(); }
});

test('what the site offers afterwards must be the file sent', async () => {
  const site = await fakeSite({ afterPublish: (r) => { r.systems.windows.sha256 = '0'.repeat(64); } });
  try {
    const { code, said } = await release(['publish', '--yes', '--file', made.signed, '--site', site.url], withToken);
    assert.equal(code, 1);
    assert.match(said, /what the site offers is not what was sent: its SHA-256 is 0{64}, not the file's/);
  } finally { await site.close(); }
});

// ------------------------------------------------------------------ check

test('check reads what the site offers and checks it as the LucidLoom page does', async () => {
  const site = await fakeSite();
  try {
    let { code, said } = await release(['check', '--site', site.url]);
    assert.equal(code, 1);
    assert.match(said, /the site offers no LiMo yet: nothing is published \(404\)/);
    site.release = { product: 'limo', systems: { windows: { version: '0.1.0', label: 'Windows 11, 64-bit, early test build', date: '2026-11-21T12:00:00Z', file: '/downloads/limo/current/LiMo-setup.exe', size: 20806157, sha256: 'c'.repeat(64) } } };
    ({ code, said } = await release(['check', '--site', site.url]));
    assert.equal(code, 0, said);
    assert.match(said, /LiMo 0\.1\.0 for Windows, Windows 11, 64-bit, early test build/);
    assert.match(said, /20,806,157 bytes, SHA-256 c{64}/);
    assert.match(said, /every field as the LucidLoom page checks it: it offers this download/);
    site.release.systems.windows.file = '/downloads/limo/../fanout/current/fanout-setup.exe';
    site.release.systems.windows.size = '20806157';
    ({ code, said } = await release(['check', '--site', site.url]));
    assert.equal(code, 1);
    assert.match(said, /the LucidLoom page would not offer it: file is not a plain path under \/downloads\/limo\/ .*; size is not a positive whole number/);
    assert.ok(site.requests.every((r) => r.method === 'GET' && r.auth === null), 'check only reads');
  } finally { await site.close(); }
});

test('the site is https, or this PC', async () => {
  const { code, said } = await release(['check', '--site', 'http://example.test']);
  assert.equal(code, 1);
  assert.match(said, /a site other than this PC is reached over https only/);
  const usage = await release(['publish', '--nope']);
  assert.equal(usage.code, 2);
});
