// The contract between the two sides of the child's seam: the answers the
// child module's Kotlin really gives, read through the readers the screens
// use. The JVM tests (`npm run test:child`) turn the real engine's view,
// from a made up family, into each answer and write it into
// tests/seam-answers/ (again at every run, and only when it changed); this
// test reads each through src/childSeam.ts. A change on either side that
// breaks the other fails here.

import fs from 'node:fs';
import path from 'node:path';

import { readBoolean, readMaker, readPairing, readRules, readSteps, readToday, STEP_IDS } from '../src/childSeam';

const folder = path.join(__dirname, 'seam-answers');

type Fixture = { request: string; answer: unknown };

function fixture(name: string): Fixture {
  return JSON.parse(fs.readFileSync(path.join(folder, `${name}.json`), 'utf8')) as Fixture;
}

// Each request the Kotlin side answers with data, and the reader the app's host checks it with (src/host.ts).
const READERS: Record<string, (answer: unknown) => unknown> = {
  'child.pairing': readPairing,
  'child.agreed': (a) => readBoolean(a, 'agreed'),
  'child.introSeen': (a) => readBoolean(a, 'introSeen'),
  'child.steps': readSteps,
  'child.today': readToday,
  'child.rules': readRules,
  'child.maker': readMaker,
};

// Every answer the JVM tests write. A file gone missing fails, as does one this list does not know.
const EXPECTED = [
  'agreed',
  'agreed-not-yet',
  'intro-seen',
  'maker',
  'pairing-no-name',
  'pairing-not-paired',
  'pairing-paired',
  'rules-bedtime',
  'rules-limit',
  'rules-limit-named',
  'rules-no-limit',
  'steps-all',
  'steps-none',
  'steps-some',
  'today-all-off',
  'today-bedtime',
  'today-limit',
  'today-long-names',
  'today-no-limit',
  'today-open',
  'today-switched-off',
];

describe('the answers the child module gives', () => {
  it('are all there, and only those', () => {
    const files = fs
      .readdirSync(folder)
      .filter((f) => f.endsWith('.json'))
      .map((f) => f.replace(/\.json$/, ''))
      .sort();
    expect(files).toEqual([...EXPECTED].sort());
  });

  it.each(EXPECTED)('%s passes its reader unchanged', (name) => {
    const { request, answer } = fixture(name);
    const read = READERS[request];
    expect(read).toBeDefined();
    // Readers trim names and clamp the bar's part; what Kotlin sends is already so, so nothing changes.
    expect(read(answer)).toEqual(answer);
  });
});

describe('what the answers say', () => {
  it('names the child once paired, says "your child" when no name is kept, and not paired with no engine', () => {
    expect(readPairing(fixture('pairing-paired').answer)).toEqual({ paired: true, childName: 'Mira' });
    expect(readPairing(fixture('pairing-no-name').answer)).toEqual({ paired: true, childName: 'your child' });
    expect(readPairing(fixture('pairing-not-paired').answer)).toEqual({ paired: false });
  });

  it('gives each of the six steps by the screens’ own names', () => {
    expect(Object.keys(fixture('steps-some').answer as object)).toEqual([...STEP_IDS]);
    expect(readSteps(fixture('steps-all').answer)).toEqual(Object.fromEntries(STEP_IDS.map((id) => [id, 'on'])));
    expect(readSteps(fixture('steps-none').answer)).toEqual(Object.fromEntries(STEP_IDS.map((id) => [id, 'off'])));
    const some = readSteps(fixture('steps-some').answer);
    expect([some.battery, some.protection, some.usage]).toEqual(['off', 'off', 'on']);
  });

  it('turns the engine’s kinds into the seam’s', () => {
    expect(readToday(fixture('today-limit').answer).left.kind).toBe('left');
    expect(readToday(fixture('today-no-limit').answer).left).toEqual({ kind: 'no-limit', words: 'No limit today' });
    expect(readToday(fixture('today-no-limit').answer).usedPart).toBeNull();
    expect(readToday(fixture('today-open').answer).left.kind).toBe('open');
  });

  it('lists apps by their names only, never a package', () => {
    for (const name of EXPECTED.filter((n) => n.startsWith('today-'))) {
      for (const app of readToday(fixture(name).answer).apps) expect(app.name).not.toMatch(/^[a-z][a-z0-9_]*(\.[a-z0-9_]+)+$/);
    }
    for (const name of EXPECTED.filter((n) => n.startsWith('rules-'))) {
      const rules = readRules(fixture(name).answer);
      for (const app of [...rules.alwaysOpen, ...rules.notAllowed]) expect(app).not.toMatch(/^[a-z][a-z0-9_]*(\.[a-z0-9_]+)+$/);
    }
    expect(readToday(fixture('today-limit').answer).apps.map((a) => a.name)).toEqual(['Game', 'Clock']);
    expect(readRules(fixture('rules-limit').answer).notAllowed).toEqual([]);
    expect(readRules(fixture('rules-limit-named').answer).notAllowed).toEqual(['Casino']);
  });

  it('lines up what is switched off with the engine’s sentences, in setup order', () => {
    const off = readToday(fixture('today-switched-off').answer).guardianNeeded;
    expect(off?.off).toEqual(['phone', 'usage', 'battery']);
    expect(off?.words).toHaveLength(3);
    const all = readToday(fixture('today-all-off').answer).guardianNeeded;
    expect(all?.off).toEqual([...STEP_IDS]);
    expect(readToday(fixture('today-limit').answer).guardianNeeded).toBeNull();
  });

  it('says the hours screens are off, and none when there are none', () => {
    expect(readToday(fixture('today-bedtime').answer).bedtime).toMatch(/^Screens are off from /);
    expect(readToday(fixture('today-limit').answer).bedtime).toBeNull();
    expect(readRules(fixture('rules-bedtime').answer).screensOff).not.toBeNull();
    expect(readRules(fixture('rules-limit').answer).screensOff).toBeNull();
  });
});
