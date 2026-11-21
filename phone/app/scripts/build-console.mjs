// Builds the guardian console's page: the guardian screens for the web, into
// dist/console (git ignored), which the console's Tauri window on the PC
// embeds in its exe (../../console).
//
//   node scripts/build-console.mjs        (npm run build:console)
//
// Steps:
//   1. scripts/make-design.mjs (the theme, icons and logo);
//   2. one production bundle from console.tsx for the web, with Expo's
//      `export:embed` (the entry is console.tsx, not the app's index.ts, so
//      neither the preview nor the mock is anywhere in it), Expo's usage
//      data switched off;
//   3. the page itself, written here: one script, one inline style sheet
//      with the theme's three fonts, nothing from any other address, so the
//      console's content security policy holds (script-src 'self'). It also
//      holds react-native-web's own style element, empty: Tauri gives every
//      <style> in the page a nonce and puts the nonces in style-src, which
//      makes a browser ignore 'unsafe-inline', so the element react-native-web
//      would make at run time, with no nonce, would be refused. Found by its
//      id, this one is used instead;
//   4. the fonts beside it, each with its licence, and the logo where the
//      bundle names it;
//   5. checks: the bundle holds the sign in's heading, and none of the
//      mock's marks or the preview's words; no address but the page's own.
//
// Nothing is kept from a run but dist/console.

import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const appDir = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const repoDir = path.resolve(appDir, '..', '..');
const outDir = path.join(appDir, 'dist', 'console');
const fontsDir = path.join(repoDir, 'screens', 'fonts');
const FONTS = JSON.parse(fs.readFileSync(path.join(appDir, 'src', 'theme', 'fonts.json'), 'utf8'));
const TOKENS = JSON.parse(fs.readFileSync(path.join(repoDir, 'design', 'tokens.json'), 'utf8'));

function run(label, command, args, env) {
  console.log(`[build-console] ${label}`);
  // npx is a script, which Windows runs only through a shell. The paths
  // passed are quoted.
  const shell = process.platform === 'win32' && command === 'npx';
  const result = spawnSync(command, args, { cwd: appDir, env, stdio: 'inherit', shell });
  if (result.status !== 0) {
    console.error(`[build-console] ${label} failed (exit ${result.status ?? result.signal}).`);
    process.exit(1);
  }
}

const q = (p) => (process.platform === 'win32' ? `"${p}"` : p);
const started = Date.now();

// The console shows the guardian's screens: the guardian form.
const env = { ...process.env, LIMO_FORM: 'guardian', EXPO_NO_TELEMETRY: '1', CI: '1' };
run('make-design', process.execPath, [path.join('scripts', 'make-design.mjs')], env);

const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'limo-console-'));
try {
  const bundle = path.join(tmp, 'console.js');
  const assets = path.join(tmp, 'assets');
  run('the console’s web bundle', 'npx', [
    'expo', 'export:embed', '--platform', 'web', '--dev', 'false', '--minify', 'true',
    '--entry-file', 'console.tsx', '--bundle-output', q(bundle), '--assets-dest', q(assets),
  ], env);

  fs.rmSync(outDir, { recursive: true, force: true, maxRetries: 3 });
  fs.mkdirSync(path.join(outDir, 'fonts'), { recursive: true });
  fs.copyFileSync(bundle, path.join(outDir, 'console.js'));
  // Expo writes each picture under assets/<its place in the app>; the
  // bundle names it from the page's root as /assets/<that place>.
  if (fs.existsSync(path.join(assets, 'assets'))) fs.cpSync(path.join(assets, 'assets'), path.join(outDir, 'assets'), { recursive: true });

  // The fonts, as the page's @font-face rules name them, each with its licence.
  const NAMES = { display: 'fraunces', body: 'instrument-sans', mono: 'spline-sans-mono' };
  for (const [key, name] of Object.entries(NAMES)) {
    const from = path.join(fontsDir, FONTS.files[key]);
    fs.copyFileSync(from, path.join(outDir, 'fonts', `${name}.ttf`));
    const licence = path.join(path.dirname(from), 'OFL.txt');
    if (fs.existsSync(licence)) fs.copyFileSync(licence, path.join(outDir, 'fonts', `${name}-OFL.txt`));
  }

  const face = (family, file, weights) =>
    `@font-face { font-family: '${family}'; src: url('/fonts/${file}.ttf') format('truetype'); font-weight: ${weights}; font-display: block; }`;
  const faces = [
    face(TOKENS.font.body, NAMES.body, '400 700'),
    face(TOKENS.font.mono, NAMES.mono, '300 700'),
    ...FONTS.displaySizes.map((size) => face(`${TOKENS.font.display} ${size}`, NAMES.display, '100 900')),
  ];
  const html = `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>LiMo</title>
<style>
${faces.join('\n')}
html, body, #root { height: 100%; margin: 0; }
body { background: ${TOKENS.color.paper}; overflow: hidden; -webkit-user-select: none; user-select: none; }
input { -webkit-user-select: text; user-select: text; }
#root { display: flex; }
</style>
<style id="react-native-stylesheet"></style>
</head>
<body>
<div id="root"></div>
<script src="/console.js"></script>
</body>
</html>
`;
  fs.writeFileSync(path.join(outDir, 'index.html'), html);

  // 5. Checks.
  const js = fs.readFileSync(path.join(outDir, 'console.js'), 'utf8');
  const mockSource = fs.readFileSync(path.join(appDir, 'src', 'mock', 'childMock.ts'), 'utf8');
  const pick = (name) => new RegExp(`export const ${name} = '([^']+)'`).exec(mockSource)?.[1];
  const never = [
    ['the mock host’s mark', pick('MOCK_MARK')],
    ['the mock child’s name', pick('CHILD')],
    ['the preview’s words', 'driven by a mock host'],
  ];
  const problems = [];
  // The production bundle writes characters outside ASCII as escapes.
  if (!js.includes('settings are for a guardian')) problems.push('the bundle does not hold the sign in’s heading, so it is not the console’s');
  for (const [what, word] of never) {
    if (!word) problems.push(`src/mock/childMock.ts no longer names ${what}`);
    else if (/^[A-Za-z]+$/.test(word) ? new RegExp(`\\b${word}\\b`).test(js) : js.includes(word)) problems.push(`the bundle holds ${what} ("${word}")`);
  }
  if (/(src|href)=["']https?:/.test(html)) problems.push('the page names another address');
  if (problems.length) {
    console.error(problems.map((p) => `PROBLEM ${p}`).join('\n'));
    process.exit(1);
  }

  const size = (f) => fs.statSync(path.join(outDir, f)).size;
  const files = [];
  const walk = (d) => fs.readdirSync(d, { withFileTypes: true }).forEach((e) => (e.isDirectory() ? walk(path.join(d, e.name)) : files.push(path.join(d, e.name))));
  walk(outDir);
  const total = files.reduce((n, f) => n + fs.statSync(f).size, 0);
  console.log(`[build-console] done in ${Math.round((Date.now() - started) / 1000)} s: dist/console, ${files.length} files, ${total} bytes; console.js ${size('console.js')} bytes`);
  console.log('[build-console] it holds the sign in, and nothing of the mock or the preview');
} finally {
  fs.rmSync(tmp, { recursive: true, force: true, maxRetries: 3 });
}
