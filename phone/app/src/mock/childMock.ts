// A made up phone that plays every state of the child's seam (src/childSeam.ts),
// for the preview on the PC and the tests. DEVELOPMENT ONLY.
//
// Nothing in the app imports this folder: only App.web.tsx (the preview) and
// tests/ do. tests/release.test.ts walks the app's imports to prove it, and
// `npm run check:release-js` bundles the app as a release build does and
// looks for MOCK_MARK and the made up names in the result.
//
// The words here stand in for the engine's. They are the drawings' own
// where the drawings have them; the states the drawings do not show (no
// limit, open until, a long name) use words of the same kind. The sentences
// for a step that is off are the engine's own (ENGINE_OFF).
//
// As the engine will, the mock says what is off from the steps it holds:
// Today's guardianNeeded is never held, it is worked out from `steps` each
// time Today is asked.

import {
  type ChildChange,
  type ChildHost,
  type GuardianNeeded,
  type Pairing,
  type RulesView,
  STEP_IDS,
  type StepId,
  type StepStates,
  type TodayView,
} from '../childSeam';

/** Found in any bundle this file is in. scripts/check-release-js.mjs looks for it. */
export const MOCK_MARK = 'limo-mock-host-development-only';

/** Android's own names for two makers (Build.MANUFACTURER): one that is Samsung, one that is not. */
export const SAMSUNG = 'samsung';
export const NOT_SAMSUNG = 'Google';

/** The engine's sentence for each step that is off, word for word. */
export const ENGINE_OFF: Readonly<Record<StepId, string>> = {
  notifications: 'Notifications are switched off, so LiMo cannot show the time left or a warning.',
  phone: 'Phone access is switched off, so a lock could hide a call.',
  usage: 'Usage access is switched off, so LiMo cannot count the time in each app.',
  accessibility: 'Accessibility is switched off, so LiMo cannot see which app is open.',
  battery: 'Android may put LiMo to sleep to save battery.',
  protection: 'Protection is switched off, so LiMo can be removed.',
};

/** What the engine says is off, from the steps: null while every step is on. */
export function offFrom(steps: StepStates): GuardianNeeded | null {
  const off = STEP_IDS.filter((id) => steps[id] !== 'on');
  return off.length ? { off, words: off.map((id) => ENGINE_OFF[id]) } : null;
}

export type MockState = {
  pairing: Pairing;
  agreed: boolean;
  steps: StepStates;
  introSeen: boolean;
  /** Today as the engine says it; its guardianNeeded is worked out from `steps` instead. */
  today: TodayView;
  rules: RulesView;
  version: string;
  /** Who made the phone, as Android says it. */
  maker: string;
};

export type MockOptions = {
  /** Opening a step's Android screen switches the step on, as a guardian would. Default true. */
  openTurnsOn?: boolean;
  /** Opening a step's Android screen fails. */
  openFails?: boolean;
  /** How long Android takes before the step is on, in milliseconds. Default 0. */
  delayMs?: number;
  /** Finishing setup fails, as a request that gets no answer would. */
  finishFails?: boolean;
};

export type MockHost = ChildHost & {
  readonly mark: string;
  /** Every call, in order, by name: "openStep usage", "askForMoreTime". */
  readonly calls: string[];
  /** What it holds now. */
  readonly state: MockState;
  /** Changes what it holds, and says so to whoever subscribed. */
  set(change: Partial<MockState>, what: readonly ChildChange[]): void;
};

export function mockHost(start: MockState, options: MockOptions = {}): MockHost {
  let state: MockState = { ...start };
  const calls: string[] = [];
  const listeners = new Set<(change: ChildChange) => void>();
  const { openTurnsOn = true, openFails = false, delayMs = 0, finishFails = false } = options;

  const tell = (what: readonly ChildChange[]) => {
    for (const w of what) for (const l of [...listeners]) l(w);
  };
  const set = (change: Partial<MockState>, what: readonly ChildChange[]) => {
    state = { ...state, ...change };
    tell(what);
  };
  const answer = <T>(name: string, value: () => T): Promise<T> => {
    calls.push(name);
    return Promise.resolve().then(value);
  };

  return {
    mark: MOCK_MARK,
    calls,
    get state() {
      return state;
    },
    set,
    pairing: () => answer('pairing', () => state.pairing),
    agreed: () => answer('agreed', () => state.agreed),
    agree: () => answer('agree', () => set({ agreed: true }, ['agreement'])),
    steps: () => answer('steps', () => ({ ...state.steps })),
    openStep: (step: StepId) => {
      calls.push(`openStep ${step}`);
      if (openFails) return Promise.reject(new Error('Android did not open it'));
      if (openTurnsOn) {
        const turnOn = () => set({ steps: { ...state.steps, [step]: 'on' as const } }, ['steps', 'today']);
        if (delayMs > 0) setTimeout(turnOn, delayMs);
        else turnOn();
      }
      return Promise.resolve();
    },
    finishSetup: () => {
      calls.push('finishSetup');
      return finishFails ? Promise.reject(new Error('no answer')) : Promise.resolve();
    },
    introSeen: () => answer('introSeen', () => state.introSeen),
    markIntroSeen: () => answer('markIntroSeen', () => set({ introSeen: true }, ['intro'])),
    today: () => answer('today', () => ({ ...state.today, guardianNeeded: offFrom(state.steps) })),
    rules: () => answer('rules', () => state.rules),
    version: () => answer('version', () => state.version),
    maker: () => answer('maker', () => state.maker),
    askForMoreTime: () => answer('askForMoreTime', () => undefined),
    subscribe(listener) {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },
  };
}

// ---------------------------------------------------------------------------
// States

export const CHILD = 'Mira';
/** A long name, made up, to try every place a name goes. */
export const LONG_CHILD = 'Ashamira Lumenvale-Brightwater Quillfeather';

export const ALL_OFF: StepStates = { notifications: 'off', phone: 'off', usage: 'off', accessibility: 'off', battery: 'off', protection: 'off' };
export const ALL_ON: StepStates = { notifications: 'on', phone: 'on', usage: 'on', accessibility: 'on', battery: 'on', protection: 'on' };

/** The first `count` steps on, the rest off. */
export function stepsOnUpTo(count: number): StepStates {
  const out = { ...ALL_OFF } as Record<StepId, 'on' | 'off'>;
  STEP_IDS.forEach((id, i) => {
    out[id] = i < count ? 'on' : 'off';
  });
  return out;
}

/** Today as the drawings show it (phone.js, today()). */
export const TODAY: TodayView = {
  left: { kind: 'left', words: '1 h 12 min' },
  used: '48 min used of 2 h',
  usedPart: 0.4,
  where: 'On this phone 31 min · on the PC 17 min',
  bedtime: 'Screens are off from 9:00 pm to 7:00 am.',
  apps: [
    { name: 'Maps', minutes: '12 min' },
    { name: 'Chrome', minutes: '9 min' },
    { name: 'Gallery', minutes: '6 min' },
    { name: 'Clock', minutes: '4 min' },
  ],
  guardianNeeded: null,
};

/** The rules as the drawings show them (phone-setup.js, RULES). */
export const RULES: RulesView = {
  timeADay: '2 h',
  screensOff: '9:00 pm to 7:00 am',
  alwaysOpen: ['Phone', 'Messages', 'Clock', 'Maps'],
  notAllowed: ['Game Launcher'],
};

const base: MockState = {
  pairing: { paired: true, childName: CHILD },
  agreed: false,
  steps: ALL_OFF,
  introSeen: false,
  today: TODAY,
  rules: RULES,
  version: '0.1.0',
  // The drawings show the Samsung note on the Battery step.
  maker: SAMSUNG,
};

export const states = {
  /** S0: the PC has not paired this phone. */
  notPaired: { ...base, pairing: { paired: false } } as MockState,
  /** S1: paired, nothing agreed yet. */
  fresh: base,
  /** Agreed, every step still off. */
  agreed: { ...base, agreed: true },
  /** Every step on, the introduction not seen: S8. */
  allOn: { ...base, agreed: true, steps: ALL_ON },
  /** Setup finished: Today and How it works. */
  childApp: { ...base, agreed: true, steps: ALL_ON, introSeen: true },
  /** T: accessibility switched off later. */
  guardianNeeded: {
    ...base,
    agreed: true,
    introSeen: true,
    steps: { ...ALL_ON, accessibility: 'off' },
  } as MockState,
  /** T with three steps off: the banner says the first, and how many more. */
  threeOff: {
    ...base,
    agreed: true,
    introSeen: true,
    steps: { ...ALL_ON, phone: 'off', battery: 'off', protection: 'off' },
  } as MockState,
  /** Agreed, every step off, on a phone that is not a Samsung. */
  notSamsung: { ...base, agreed: true, maker: NOT_SAMSUNG },
  /** Little time left. */
  littleLeft: {
    ...base,
    agreed: true,
    steps: ALL_ON,
    introSeen: true,
    today: {
      ...TODAY,
      left: { kind: 'left', words: '14 min' },
      used: '1 h 46 min used of 2 h',
      usedPart: 0.883,
      where: 'On this phone 1 h 9 min · on the PC 37 min',
    },
  } as MockState,
  /** Under a minute left. */
  underAMinute: {
    ...base,
    agreed: true,
    steps: ALL_ON,
    introSeen: true,
    today: {
      ...TODAY,
      left: { kind: 'left', words: '0:42' },
      used: '1 h 59 min used of 2 h',
      usedPart: 0.994,
      where: 'On this phone 1 h 22 min · on the PC 37 min',
    },
  } as MockState,
  /** A day with no limit. */
  noLimit: {
    ...base,
    agreed: true,
    steps: ALL_ON,
    introSeen: true,
    today: { ...TODAY, left: { kind: 'no-limit', words: 'No limit today' }, used: '48 min used today', usedPart: null },
  } as MockState,
  /** A guardian's unlock keeps the phone open. */
  openUntil: {
    ...base,
    agreed: true,
    steps: ALL_ON,
    introSeen: true,
    today: { ...TODAY, left: { kind: 'open', words: 'Open until 9:30 pm' }, used: '2 h 10 min used of 2 h', usedPart: 1 },
  } as MockState,
  /** Every name at its longest. */
  longNames: {
    ...base,
    pairing: { paired: true, childName: LONG_CHILD },
    agreed: true,
    introSeen: true,
    steps: { ...ALL_ON, usage: 'off' },
    today: {
      ...TODAY,
      apps: [
        { name: 'Super Long Puzzle Adventure Deluxe Edition HD Remastered', minutes: '1 hour 12 min' },
        { name: 'Messages and Video Calls for the Whole Family', minutes: '9 min' },
        { name: 'Ü', minutes: '1 min' },
      ],
    },
    rules: {
      timeADay: '2 hours 30 min',
      screensOff: '9:00 pm to 7:00 am',
      alwaysOpen: ['Phone', 'Messages and Video Calls for the Whole Family', 'Clock', 'Maps', 'Calculator', 'Dictionary'],
      notAllowed: ['Super Long Puzzle Adventure Deluxe Edition HD Remastered', 'Game Launcher'],
    },
  } as MockState,
};

export type StateName = keyof typeof states;
