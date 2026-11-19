// Proves that no mock data and no preview code is in either built app's
// JavaScript, by looking in the bundle a release build makes.
//
//   node scripts/check-release-js.mjs        (npm run check:release-js)
//
// For each form, it makes the JavaScript bundle exactly as Gradle's release
// build does (`expo export:embed --platform android --dev false`, with
// LIMO_FORM set), into a temporary folder, and fails if the bundle holds:
//   - the mock host's mark (MOCK_MARK in src/mock/childMock.ts);
//   - the made up child's names the mock and the preview use;
//   - the preview's own words.
// It also checks that the bundle holds the child form's first screen, so an
// empty or broken bundle cannot pass. As a control, it makes the web
// preview's bundle the same way and checks that the same search finds the
// mock there: the check can see what it looks for.
//
// Expo's command line tool is told not to send usage data. Nothing is kept:
// the temporary folders are removed at the end.

import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const appDir = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const mockSource = fs.readFileSync(path.join(appDir, 'src', 'mock', 'childMock.ts'), 'utf8');
const pick = (name) => {
  const m = new RegExp(`export const ${name} = '([^']+)'`).exec(mockSource);
  if (!m) throw new Error(`src/mock/childMock.ts has no ${name}`);
  return m[1];
};

// What must never be in a built app, and why.
const NEVER = [
  ['the mock host’s mark', pick('MOCK_MARK')],
  ['the mock child’s name', pick('CHILD')],
  ['the mock’s long name', pick('LONG_CHILD')],
  // Plain ASCII: a production bundle may write other characters as escapes.
  ['the preview’s words', 'driven by a mock host'],
];
// What must be: the child form's first screen.
const MUST = 'This phone is not set up yet.';

function run(label, args, env) {
  console.log(`[check-release-js] ${label}`);
  // npx is a script, which Windows runs only through a shell. Nothing passed
  // to it holds a space: the temporary folder's path is quoted.
  const result = spawnSync('npx', args, { cwd: appDir, env, stdio: ['ignore', 'pipe', 'pipe'], shell: process.platform === 'win32' });
  if (result.status !== 0) {
    console.error(String(result.stdout) + String(result.stderr));
    console.error(`[check-release-js] ${label} failed (exit ${result.status ?? result.signal}).`);
    process.exit(1);
  }
}

const q = (p) => (process.platform === 'win32' ? `"${p}"` : p);
const found = (text, word) => (/^[A-Za-z]+$/.test(word) ? new RegExp(`\\b${word}\\b`).test(text) : text.includes(word));

const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'limo-release-js-'));
const problems = [];
try {
  for (const form of ['child', 'guardian']) {
    const env = { ...process.env, LIMO_FORM: form, EXPO_NO_TELEMETRY: '1', CI: '1' };
    const out = path.join(tmp, form);
    fs.mkdirSync(out);
    const bundle = path.join(out, 'index.android.bundle');
    run(`the ${form} form's release bundle`, [
      'expo', 'export:embed', '--platform', 'android', '--dev', 'false', '--minify', 'false', '--reset-cache',
      '--entry-file', 'index.ts', '--bundle-output', q(bundle), '--assets-dest', q(path.join(out, 'res')),
    ], env);
    const text = fs.readFileSync(bundle, 'utf8');
    if (!text.includes(MUST)) problems.push(`${form}: the bundle does not hold "${MUST}", so it is not the app's`);
    for (const [what, word] of NEVER) {
      if (found(text, word)) problems.push(`${form}: the bundle holds ${what} ("${word}")`);
    }
    console.log(`[check-release-js] ${form}: ${text.length} characters, ${problems.length ? 'see below' : 'nothing of the mock or the preview'}`);
  }

  // The control: the preview's bundle has the mock, and this search finds it.
  const web = path.join(tmp, 'web');
  run('the preview’s web bundle (the control)', ['expo', 'export', '--platform', 'web', '--output-dir', q(web)], {
    ...process.env, LIMO_FORM: 'child', EXPO_NO_TELEMETRY: '1', CI: '1',
  });
  const webText = fs
    .readdirSync(path.join(web, '_expo', 'static', 'js', 'web'))
    .map((f) => fs.readFileSync(path.join(web, '_expo', 'static', 'js', 'web', f), 'utf8'))
    .join('\n');
  const seen = NEVER.filter(([, word]) => found(webText, word)).length;
  if (seen !== NEVER.length) problems.push(`control: the search found ${seen} of ${NEVER.length} in the preview's bundle, where all are`);
  else console.log(`[check-release-js] control: the same search finds all ${NEVER.length} in the preview's bundle`);
} finally {
  fs.rmSync(tmp, { recursive: true, force: true, maxRetries: 3 });
}

if (problems.length) {
  console.error(problems.map((p) => `PROBLEM ${p}`).join('\n'));
  process.exit(1);
}
console.log('[check-release-js] passed: neither built app holds the mock or the preview');
