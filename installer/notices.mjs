// Gathers the third party notices of everything LiMo's installer puts on a
// PC, from the packages themselves, and writes them into a folder:
//
//   node notices.mjs <out folder> <page source map>
//
//   LICENSE.txt               LiMo's own licence (the repository's LICENSE)
//   THIRD-PARTY-NOTICES.txt   every third party component, where it ships,
//                             its licence, and each licence text as the
//                             component carries it
//
// What is listed, and how it is found:
//
// - the Rust crates in limo.exe and in limo-console.exe: each program's
//   tree from `cargo metadata` for x86_64-pc-windows-msvc with its default
//   features, following normal dependencies only (build scripts' and
//   tests' crates never reach a program) and leaving out procedural macros,
//   which run while compiling and leave no code of their own behind;
//   LiMo's own crates (this repository's) are named once, as LiMo's;
// - the npm packages in the console's page, which limo-console.exe holds:
//   the packages of the files named in the page bundle's source map (made
//   from the same bundle as the page, by build.ps1), so a package that is
//   installed but not bundled is not listed;
// - the three fonts of the page, with the licences beside them;
// - NSIS, whose code is the installer and the uninstaller, with its licence
//   as NSIS 3.13 carries it (kept in licences/, since on a clean PC Tauri's
//   bundler fetches NSIS only when it makes the installer, after this), and
//   Tauri's plugin for it;
// - the WebView2 bootstrapper in the installer, which is Microsoft's.
//
// Each licence text is read from the component's own files (LICENSE,
// COPYING, NOTICE and the like); a text several components carry word for
// word is written once, with their names. A component that carries no
// licence file is said to, with its licence's name. Nothing of the build
// PC is written: no path, no user name.

import { spawnSync } from 'node:child_process';
import crypto from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));
const repo = path.resolve(here, '..');
const [outDir, sourceMap] = process.argv.slice(2);
if (!outDir || !sourceMap) {
  console.error('node notices.mjs <out folder> <page source map>');
  process.exit(2);
}
/** The NSIS whose licence is kept here; build.ps1 checks that the bundler used it. */
export const NSIS_VERSION = '3.13';

const TARGET = 'x86_64-pc-windows-msvc';
const LICENCE_FILE = /^(licen[cs]e|copying|copyright|notice|unlicense)([-_. ].*)?$/i;

/** The licence files in a folder, read: [{ name, text }]. */
function licenceFiles(dir) {
  if (!fs.existsSync(dir)) return [];
  return fs
    .readdirSync(dir, { withFileTypes: true })
    .filter((e) => e.isFile() && LICENCE_FILE.test(e.name))
    .map((e) => ({ name: e.name, text: fs.readFileSync(path.join(dir, e.name), 'utf8').replace(/\r\n/g, '\n').trim() }))
    .filter((f) => f.text.length > 0)
    .sort((a, b) => a.name.localeCompare(b.name));
}

/** The crates in one program, by `cargo metadata`. */
function crates(manifest, program) {
  const got = spawnSync('cargo', ['metadata', '--format-version', '1', '--locked', '--manifest-path', manifest, '--filter-platform', TARGET], {
    cwd: repo,
    encoding: 'utf8',
    maxBuffer: 256 * 1024 * 1024,
  });
  if (got.status !== 0) {
    console.error(got.stderr);
    throw new Error(`cargo metadata failed for ${program}`);
  }
  const meta = JSON.parse(got.stdout);
  const byId = new Map(meta.packages.map((p) => [p.id, p]));
  const nodes = new Map(meta.resolve.nodes.map((n) => [n.id, n]));
  const isMacro = (p) => p.targets.some((t) => t.kind.includes('proc-macro'));
  const seen = new Set();
  const queue = [meta.resolve.root];
  while (queue.length) {
    const id = queue.shift();
    if (seen.has(id)) continue;
    seen.add(id);
    for (const dep of nodes.get(id)?.deps ?? []) {
      const normal = dep.dep_kinds.some((k) => k.kind === null);
      if (!normal || isMacro(byId.get(dep.pkg))) continue;
      queue.push(dep.pkg);
    }
  }
  return [...seen].map((id) => byId.get(id)).map((p) => ({
    kind: 'crate',
    name: p.name,
    version: p.version,
    licence: p.license ?? (p.license_file ? 'its own licence file' : 'not stated'),
    own: p.source === null,
    authors: (p.authors ?? []).join(', '),
    source: p.repository ?? `https://crates.io/crates/${p.name}/${p.version}`,
    files: p.license_file
      ? [{ name: path.basename(p.license_file), text: fs.readFileSync(path.resolve(path.dirname(p.manifest_path), p.license_file), 'utf8').trim() }]
      : licenceFiles(path.dirname(p.manifest_path)),
    in: program,
  }));
}

/** The npm packages whose files the page bundle holds. */
function pagePackages() {
  const map = JSON.parse(fs.readFileSync(sourceMap, 'utf8'));
  const appDir = path.join(repo, 'phone', 'app');
  const names = new Set();
  for (const source of map.sources ?? []) {
    const parts = source.replace(/\\/g, '/').split('/node_modules/');
    if (parts.length < 2) continue;
    const rest = parts[parts.length - 1].split('/');
    names.add(rest[0].startsWith('@') ? `${rest[0]}/${rest[1]}` : rest[0]);
  }
  const out = [];
  for (const name of [...names].sort()) {
    // The package Metro bundled: the app's own copy, or one nested deeper,
    // found the way Node finds it from the app.
    let dir = null;
    for (const source of map.sources) {
      const s = source.replace(/\\/g, '/');
      const at = s.lastIndexOf(`/node_modules/${name}/`);
      if (at >= 0) {
        dir = s.slice(0, at + `/node_modules/${name}`.length);
        break;
      }
    }
    // Metro names a file from the app's own folder ("/node_modules/...").
    if (dir) dir = /^[A-Za-z]:\//.test(dir) ? dir : path.join(appDir, dir.replace(/^\/+/, ''));
    const pkgFile = dir ? path.join(dir, 'package.json') : null;
    const pkg = pkgFile && fs.existsSync(pkgFile) ? JSON.parse(fs.readFileSync(pkgFile, 'utf8')) : {};
    const licence = typeof pkg.license === 'string' ? pkg.license : pkg.license?.type ?? 'not stated';
    const authors = typeof pkg.author === 'string' ? pkg.author : pkg.author?.name ?? '';
    const source = typeof pkg.repository === 'string' ? pkg.repository : pkg.repository?.url ?? `https://www.npmjs.com/package/${name}/v/${pkg.version}`;
    out.push({ kind: 'npm', name, version: pkg.version ?? '?', licence, authors, source, own: false, files: dir ? licenceFiles(dir) : [], in: 'the console’s page, inside limo-console.exe' });
  }
  return out;
}

/** The fonts of the page, from the page's own folder. */
function fonts() {
  const dir = path.join(repo, 'phone', 'app', 'dist', 'console', 'fonts');
  const named = { fraunces: 'Fraunces', 'instrument-sans': 'Instrument Sans', 'spline-sans-mono': 'Spline Sans Mono' };
  return Object.entries(named).map(([file, name]) => {
    const licence = path.join(dir, `${file}-OFL.txt`);
    return {
      kind: 'font',
      name,
      version: '',
      licence: 'OFL-1.1',
      own: false,
      files: fs.existsSync(licence) ? [{ name: 'OFL.txt', text: fs.readFileSync(licence, 'utf8').replace(/\r\n/g, '\n').trim() }] : [],
      in: 'the console’s page, inside limo-console.exe',
    };
  });
}

/** What the installer itself is made of. */
function installerParts() {
  const copying = path.join(here, 'licences', `NSIS-${NSIS_VERSION}-COPYING.txt`);
  return [
    {
      kind: 'tool',
      name: 'NSIS (Nullsoft Scriptable Install System)',
      version: NSIS_VERSION,
      licence: 'Zlib, with parts under bzip2-1.0.6 and CPL-1.0 (see its COPYING)',
      own: false,
      files: fs.existsSync(copying) ? [{ name: 'COPYING', text: fs.readFileSync(copying, 'utf8').replace(/\r\n/g, '\n').trim() }] : [],
      in: 'the installer and the uninstaller',
    },
    {
      kind: 'tool',
      name: 'nsis_tauri_utils (a plugin of NSIS, from the Tauri project)',
      version: '',
      licence: 'MIT OR Apache-2.0',
      authors: 'the Tauri Programme within The Commons Conservancy',
      source: 'https://github.com/tauri-apps/nsis-tauri-utils',
      own: false,
      files: [],
      in: 'the installer and the uninstaller',
    },
    {
      kind: 'tool',
      name: 'Microsoft Edge WebView2 Runtime bootstrapper (MicrosoftEdgeWebview2Setup.exe)',
      version: '',
      licence: 'Microsoft’s own licence terms; not open source',
      own: false,
      files: [],
      in: 'the installer, which runs it only on a PC without the WebView2 Runtime; it then downloads the runtime from Microsoft',
    },
  ];
}

const all = [
  ...crates(path.join(repo, 'windows', 'Cargo.toml'), 'limo.exe'),
  ...crates(path.join(repo, 'console', 'Cargo.toml'), 'limo-console.exe'),
  ...pagePackages(),
  ...fonts(),
  ...installerParts(),
];

// One line per component, the places it ships joined.
const key = (c) => `${c.kind}\u0000${c.name}\u0000${c.version}`;
const merged = new Map();
for (const c of all) {
  const was = merged.get(key(c));
  if (was) {
    if (!was.in.includes(c.in)) was.in.push(c.in);
  } else merged.set(key(c), { ...c, in: [c.in] });
}
const own = [...merged.values()].filter((c) => c.own);
const third = [...merged.values()].filter((c) => !c.own).sort((a, b) => a.kind.localeCompare(b.kind) || a.name.localeCompare(b.name));

// Each distinct text once.
const texts = new Map();
for (const c of third) {
  for (const f of c.files) {
    const h = crypto.createHash('sha256').update(f.text).digest('hex');
    if (!texts.has(h)) texts.set(h, { text: f.text, who: [] });
    texts.get(h).who.push(`${c.name}${c.version ? ` ${c.version}` : ''} (${f.name})`);
  }
}

const KIND = { crate: 'Rust crates', npm: 'npm packages', font: 'Fonts', tool: 'The installer' };
const lines = [];
lines.push('Third party notices for LiMo');
lines.push('');
lines.push('LiMo is free software under the Apache License 2.0 (LICENSE.txt, beside this file), made by LucidLoom AI.');
lines.push('It is built from the components below, each under its own licence, which lets LiMo ship under the');
lines.push('Apache License 2.0. Each is listed with where it ships and its licence; then each licence text, as the');
lines.push('component carries it, follows.');
lines.push('');
lines.push(`LiMo’s own parts, under the Apache License 2.0: ${own.map((c) => `${c.name} ${c.version}`).sort().join(', ')}.`);
lines.push('');
for (const kind of ['crate', 'npm', 'font', 'tool']) {
  const list = third.filter((c) => c.kind === kind);
  if (!list.length) continue;
  lines.push(`${KIND[kind]} (${list.length})`);
  lines.push('');
  for (const c of list) {
    const carries = c.files.length ? '' : '; it carries no licence file of its own';
    lines.push(`  ${c.name}${c.version ? ` ${c.version}` : ''}: ${c.licence}; in ${c.in.join(', ')}${carries}`);
  }
  lines.push('');
}
lines.push('The licence texts');
lines.push('');
for (const { text, who } of texts.values()) {
  lines.push('='.repeat(78));
  lines.push(`The text below is carried by: ${who.join(', ')}`);
  lines.push('='.repeat(78));
  lines.push('');
  lines.push(text);
  lines.push('');
}
const without = third.filter((c) => !c.files.length);
if (without.length) {
  lines.push('='.repeat(78));
  lines.push('Components that carry no licence text of their own');
  lines.push('='.repeat(78));
  lines.push('');
  lines.push('These name their licence but carry no file with its text: each with its authors, who hold its');
  lines.push('copyright, and where its source is. The Apache License 2.0 is the text of LICENSE.txt beside this');
  lines.push('file; the MIT and the BSD 3-Clause licences read as below; the Mozilla Public License 2.0 is at');
  lines.push('https://mozilla.org/MPL/2.0/, and the source of a component under it is where it says.');
  lines.push('');
  for (const c of without) {
    const who = c.authors ? `; by ${c.authors}` : '';
    const where = c.source ? `; its source: ${c.source.replace(/^git\+/, '')}` : '';
    lines.push(`  ${c.name}${c.version ? ` ${c.version}` : ''}: ${c.licence}${who}${where}`);
  }
  lines.push('');
  lines.push('BSD 3-Clause License');
  lines.push('');
  lines.push('Redistribution and use in source and binary forms, with or without modification, are permitted');
  lines.push('provided that the following conditions are met:');
  lines.push('');
  lines.push('1. Redistributions of source code must retain the above copyright notice, this list of conditions and');
  lines.push('   the following disclaimer.');
  lines.push('2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions');
  lines.push('   and the following disclaimer in the documentation and/or other materials provided with the');
  lines.push('   distribution.');
  lines.push('3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or');
  lines.push('   promote products derived from this software without specific prior written permission.');
  lines.push('');
  lines.push('THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED');
  lines.push('WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A');
  lines.push('PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR');
  lines.push('ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT');
  lines.push('LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS');
  lines.push('INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR');
  lines.push('TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF');
  lines.push('ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.');
  lines.push('');
  lines.push('MIT License');
  lines.push('');
  lines.push('Permission is hereby granted, free of charge, to any person obtaining a copy of this software and');
  lines.push('associated documentation files (the "Software"), to deal in the Software without restriction,');
  lines.push('including without limitation the rights to use, copy, modify, merge, publish, distribute, sublicense,');
  lines.push('and/or sell copies of the Software, and to permit persons to whom the Software is furnished to do so,');
  lines.push('subject to the following conditions:');
  lines.push('');
  lines.push('The above copyright notice and this permission notice shall be included in all copies or substantial');
  lines.push('portions of the Software.');
  lines.push('');
  lines.push('THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT');
  lines.push('LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN');
  lines.push('NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY,');
  lines.push('WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE');
  lines.push('SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.');
  lines.push('');
}
lines.push('Microsoft Edge WebView2 Runtime. LiMo’s settings window is drawn by Microsoft’s WebView2 Runtime, which');
lines.push('is part of Windows 11 and of most Windows 10 PCs and is not part of LiMo; on a PC without it, the');
lines.push('installer runs Microsoft’s bootstrapper, which downloads it from Microsoft. Both are Microsoft’s');
lines.push('software, under Microsoft’s own licence terms. Those terms ask that families be told that the');
lines.push('software includes Microsoft Defender SmartScreen and sends information to Microsoft.');
lines.push('');

fs.mkdirSync(outDir, { recursive: true });
fs.copyFileSync(path.join(repo, 'LICENSE'), path.join(outDir, 'LICENSE.txt'));
fs.writeFileSync(path.join(outDir, 'THIRD-PARTY-NOTICES.txt'), lines.join('\r\n'));

const counts = Object.fromEntries(['crate', 'npm', 'font', 'tool'].map((k) => [k, third.filter((c) => c.kind === k).length]));
const licences = [...new Set(third.map((c) => c.licence))].sort();
console.log(
  `[notices] ${counts.crate} Rust crates, ${counts.npm} npm packages, ${counts.font} fonts, ${counts.tool} parts of the installer; ` +
    `${texts.size} distinct licence texts; ${without.length} without a text of their own; LiMo's own: ${own.length}`,
);
console.log(`[notices] licences named: ${licences.join(' | ')}`);
if (without.length) console.log(`[notices] carrying no licence text: ${without.map((c) => `${c.name} ${c.version}`).join(', ')}`);
