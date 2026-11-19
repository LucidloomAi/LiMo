// Writes the preview of the child form's screens: a static web page that
// draws each screen in each state from the app's own code, through
// react-native-web, driven by the mock host. A development tool only: it is
// not part of either built app (App.web.tsx is its page; Android's bundle
// takes App.tsx).
//
//   node scripts/preview.mjs        (npm run preview)
//
// Then open dist/preview/index.html from disk, in any browser: no server.
//
// Steps:
//   1. scripts/make-design.mjs (the theme, icons and logo);
//   2. `expo export --platform web` into dist/preview (git ignored), with
//      Expo's usage data switched off;
//   3. makes it open from disk: the export names its script and its pictures
//      from the root of a web server ("/_expo/...", "/assets/..."), which
//      from disk would mean the root of the drive, so they are made
//      relative ("./_expo/...", "./assets/..."), and the run stops if any
//      such name is left;
//   4. copies the three fonts from ../../screens/fonts beside the page, each
//      with its licence, as the page's @font-face rules expect them.

import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const appDir = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const repoDir = path.resolve(appDir, '..', '..');
const outRel = path.join('dist', 'preview');
const outDir = path.join(appDir, outRel);
const fontsDir = path.join(repoDir, 'screens', 'fonts');
const FONTS = JSON.parse(fs.readFileSync(path.join(appDir, 'src', 'theme', 'fonts.json'), 'utf8'));

function run(label, command, args, env) {
  console.log(`[preview] ${label}`);
  // npx is a script, which Windows runs only through a shell. Nothing passed
  // to it holds a space.
  const shell = process.platform === 'win32' && command === 'npx';
  const result = spawnSync(command, args, { cwd: appDir, env, stdio: 'inherit', shell });
  if (result.status !== 0) {
    console.error(`[preview] ${label} failed (exit ${result.status ?? result.signal}).`);
    process.exit(1);
  }
}

// The form only matters to app.config.ts, which every Expo command reads.
const env = { ...process.env, LIMO_FORM: 'child', EXPO_NO_TELEMETRY: '1', CI: '1' };
run('make-design', process.execPath, [path.join('scripts', 'make-design.mjs')], env);
fs.rmSync(outDir, { recursive: true, force: true, maxRetries: 3 });
run('expo export --platform web', 'npx', ['expo', 'export', '--platform', 'web', '--output-dir', outRel.replaceAll('\\', '/')], env);

// 3. Open from disk.
function walk(dir) {
  return fs.readdirSync(dir, { withFileTypes: true }).flatMap((e) => (e.isDirectory() ? walk(path.join(dir, e.name)) : [path.join(dir, e.name)]));
}
const htmlFile = path.join(outDir, 'index.html');
let html = fs.readFileSync(htmlFile, 'utf8');
html = html.replaceAll('src="/_expo/', 'src="./_expo/').replace(/<title>[^<]*<\/title>/, '<title>LiMo child screens preview</title>');
fs.writeFileSync(htmlFile, html);
const scripts = walk(outDir).filter((f) => f.endsWith('.js'));
for (const file of scripts) {
  const js = fs.readFileSync(file, 'utf8');
  fs.writeFileSync(file, js.replaceAll('uri:"/assets/', 'uri:"./assets/'));
}
const left = [htmlFile, ...scripts].filter((f) => /["'(]\/(_expo|assets)\//.test(fs.readFileSync(f, 'utf8')));
if (left.length) {
  console.error(`[preview] still named from a server's root, so it would not open from disk: ${left.map((f) => path.relative(appDir, f)).join(', ')}`);
  process.exit(1);
}

// 4. The fonts, as the page's @font-face rules name them, each with its licence.
const fontsOut = path.join(outDir, 'fonts');
fs.mkdirSync(fontsOut, { recursive: true });
const NAMES = { display: 'fraunces', body: 'instrument-sans', mono: 'spline-sans-mono' };
for (const [key, name] of Object.entries(NAMES)) {
  const from = path.join(fontsDir, FONTS.files[key]);
  fs.copyFileSync(from, path.join(fontsOut, `${name}.ttf`));
  const licence = path.join(path.dirname(from), 'OFL.txt');
  if (fs.existsSync(licence)) fs.copyFileSync(licence, path.join(fontsOut, `${name}-OFL.txt`));
}

const url = pathToFileURL(htmlFile).href;
console.log(`[preview] done: ${path.relative(appDir, htmlFile)}`);
console.log(`[preview] open ${url}`);
console.log('[preview] one phone alone, for a picture: add #one=S4b&size=390 (or 360), and &scale=1.3 for larger text');
