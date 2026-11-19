// What a built app may and may not hold, read from the source:
//
// - no screen names a native module, or the network: only src/host.ts
//   reaches the child module;
// - the mock host (src/mock) and the preview (src/preview, App.web.tsx) are
//   never reached from the app's entry point as Android resolves it, so no
//   made up data can be in either built app.
//
// npm run check:release-js shows the same on the bundle a release build makes.

import fs from 'node:fs';
import path from 'node:path';

const appDir = path.resolve(__dirname, '..');
const rel = (f: string) => path.relative(appDir, f).replaceAll('\\', '/');

function files(dir: string): string[] {
  return fs
    .readdirSync(dir, { withFileTypes: true })
    .flatMap((e) => (e.isDirectory() ? files(path.join(dir, e.name)) : [path.join(dir, e.name)]))
    .filter((f) => /\.(ts|tsx)$/.test(f));
}

const IMPORT = /(?:import|export)\s[^'"]*?from\s*['"]([^'"]+)['"]|import\s*['"]([^'"]+)['"]|require\(\s*['"]([^'"]+)['"]\s*\)/g;

function imports(file: string): string[] {
  const source = fs.readFileSync(file, 'utf8');
  return [...source.matchAll(IMPORT)].map((m) => m[1] ?? m[2] ?? m[3]);
}

// How Metro resolves a relative import for Android: never a .web file.
const ANDROID_EXTS = ['.android.tsx', '.android.ts', '.native.tsx', '.native.ts', '.tsx', '.ts', '.js', '.json'];
function resolve(from: string, spec: string): string | null {
  const base = path.resolve(path.dirname(from), spec);
  const tries = [...ANDROID_EXTS.map((e) => base + e), ...ANDROID_EXTS.map((e) => path.join(base, 'index' + e)), base];
  return tries.find((t) => fs.existsSync(t) && fs.statSync(t).isFile()) ?? null;
}

/** Every source file of the app reached from index.ts, as Android's bundle reaches them. */
function reachedOnAndroid(): Set<string> {
  const seen = new Set<string>();
  const todo = [path.join(appDir, 'index.ts')];
  while (todo.length) {
    const file = todo.pop()!;
    if (seen.has(file)) continue;
    seen.add(file);
    if (!/\.(ts|tsx|js)$/.test(file)) continue;
    for (const spec of imports(file)) {
      if (!spec.startsWith('.')) continue;
      const target = resolve(file, spec);
      if (target) todo.push(target);
    }
  }
  return new Set([...seen].map(rel));
}

describe('a built app', () => {
  const reached = reachedOnAndroid();

  it('reaches the child form, its seam and the real host', () => {
    for (const f of ['App.tsx', 'app/child/index.tsx', 'src/child/ChildApp.tsx', 'src/host.ts', 'src/childSeam.ts', 'modules/limo-child/index.ts']) {
      expect(reached).toContain(f);
    }
  });

  it('never reaches the mock host or the preview', () => {
    const leaks = [...reached].filter((f) => f.startsWith('src/mock/') || f.startsWith('src/preview/') || /\.web\.tsx?$/.test(f) || f.startsWith('tests/'));
    expect(leaks).toEqual([]);
  });

  it('has the mock imported only by the preview and the tests', () => {
    const importers = [...files(appDir).filter((f) => !/[\\/](node_modules|android|dist|out)[\\/]/.test(f))]
      .filter((f) => imports(f).some((s) => s.startsWith('.') && resolve(f, s)?.includes(`${path.sep}mock${path.sep}`)))
      .map(rel)
      .filter((f) => !f.startsWith('src/mock/'));
    expect(importers.length).toBeGreaterThan(0);
    for (const f of importers) expect(f).toMatch(/^(src\/preview\/|tests\/|App\.web\.tsx$)/);
  });
});

describe('the screens', () => {
  const screens = [...files(path.join(appDir, 'src', 'child')), ...files(path.join(appDir, 'src', 'ui')), ...files(path.join(appDir, 'app'))];

  it('are all looked at', () => {
    expect(screens.map(rel)).toEqual(expect.arrayContaining(['src/child/ChildApp.tsx', 'src/child/SetupScreens.tsx', 'src/child/AppScreens.tsx', 'app/child/index.tsx']));
  });

  it.each([
    ['a native module', /NativeModules|TurboModuleRegistry|requireNativeModule|requireOptionalNativeModule|requireNativeComponent|expo-modules-core|modules\/limo-child/],
    ['the network', /\bfetch\(|XMLHttpRequest|WebSocket|expo\/fetch/],
    ['a link out', /Linking|openURL/],
  ])('never name %s', (_what, pattern) => {
    const named = screens.filter((f) => pattern.test(fs.readFileSync(f, 'utf8'))).map(rel);
    expect(named).toEqual([]);
  });

  it('reach the child module only through src/host.ts', () => {
    const all = files(path.join(appDir, 'src')).concat(files(path.join(appDir, 'app')), [path.join(appDir, 'App.tsx'), path.join(appDir, 'App.web.tsx')]);
    const users = all.filter((f) => imports(f).some((s) => s.includes('modules/limo-child'))).map(rel);
    expect(users).toEqual(['src/host.ts']);
  });
});
