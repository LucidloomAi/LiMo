// The one seam between the screens and whatever runs them, as in proof six
// (spikes/06-console-window/app/src/host.ts).
//
// Screens call host.request(name, args), or the child's part of the seam
// (childHost, below), and nothing else: never a native module, never the
// network. Only this file knows who answers:
//
//   child-module   the child form's Kotlin module (modules/limo-child), linked
//                  in the child form only
//   console        the guardian console on the PC: the page inside the
//                  console's Tauri window (../../console), whose Rust side
//                  passes each guardian request through the door to LiMo's
//                  service. Found by __TAURI_INTERNALS__ on the page's global
//                  object, which Tauri puts there before any of the page's
//                  scripts run and nothing else does
//   none           nothing native to ask yet: the guardian form today, whose
//                  link to the master PC is a later piece
//   mock           a development run with no native module (never in a
//                  release build); it answers only what JavaScript knows by
//                  itself, and holds no made up data
//
// A later place joins here, not in the screens: the guardian's link on the
// phone.
//
// Each request is listed in Requests, with what it is sent and what it
// answers, so a screen cannot ask for something that does not exist.
//
// The made up family the preview and the tests show lives in src/mock/ and
// is never imported from here or from any screen: the check in
// tests/release.test.ts and `npm run check:release-js` prove that it never
// reaches a built app.

import Constants from 'expo-constants';
import { Platform } from 'react-native';

import { limoChild } from '../modules/limo-child';
import {
  type ChildChange,
  type ChildHost,
  type Pairing,
  readBoolean,
  readChange,
  readMaker,
  readPairing,
  readRules,
  readSteps,
  readToday,
  type RulesView,
  SeamError,
  type StepId,
  type StepStates,
  type TodayView,
} from './childSeam';
import { appForm, type Form } from './form';
import { type DoorOutcome, type GuardianHost, readDoorAnswer, readDoorProblem } from './guardianSeam';

export type * from './childSeam';
export { isSamsung, STEP_IDS } from './childSeam';
export type * from './guardianSeam';

export type Place = 'child-module' | 'console' | 'none' | 'mock';

/** What the start screens show: which form, and who answered. */
export type About = {
  form: Form;
  answeredBy: Place;
  /** The child module's version, or null when it is not linked. */
  childModule: string | null;
  /** Android's version, such as "13", or the platform's name elsewhere. */
  system: string;
};

type None = Record<string, never>;

export type Requests = {
  about: { args: None; answer: About };
  // The child form's requests: what childHost below asks of Kotlin. Each name
  // is what the child module's `request` is called with; the answer is
  // checked by the reader in src/childSeam.ts named beside it.
  'child.pairing': { args: None; answer: Pairing }; // readPairing
  'child.agreed': { args: None; answer: boolean }; // readBoolean
  'child.agree': { args: None; answer: null };
  'child.steps': { args: None; answer: StepStates }; // readSteps
  'child.openStep': { args: { step: StepId }; answer: null };
  'child.finishSetup': { args: None; answer: null };
  'child.introSeen': { args: None; answer: boolean }; // readBoolean
  'child.markIntroSeen': { args: None; answer: null };
  'child.today': { args: None; answer: TodayView }; // readToday
  'child.rules': { args: None; answer: RulesView }; // readRules
  'child.maker': { args: None; answer: string }; // readMaker
  'child.askForMoreTime': { args: None; answer: null };
  // The guardian's requests: what guardianHost below asks of the console's
  // Rust side, which asks the door. Each answer is checked by
  // readDoorAnswer (src/guardianSeam.ts).
  'guardian.about': { args: None; answer: DoorOutcome };
  'guardian.signIn': { args: { password: string }; answer: DoorOutcome };
  // The console's self check (console/README.md, "The self check"). The
  // console's Rust side answers it only when started with --self-check.
  'console.selfCheck': { args: SelfCheckArgs; answer: unknown };
};

/** What the console's self check asks of its Rust side. */
export type SelfCheckArgs =
  | { action: 'state' }
  | { action: 'report'; key: string; text: string }
  | { action: 'use'; door: string };

/** The console's Rust side, by the name it calls each request. */
const CONSOLE_WIRE: Partial<Record<RequestName, string>> = {
  'guardian.about': 'about',
  'guardian.signIn': 'sign_in',
  'console.selfCheck': 'self_check',
};

type TauriInternals = { invoke(command: string, args: Record<string, unknown>): Promise<unknown> };

/** Tauri's own bridge, where the page runs inside the console's window. */
function tauri(): TauriInternals | null {
  const g = globalThis as { __TAURI_INTERNALS__?: Partial<TauriInternals> };
  return g.__TAURI_INTERNALS__ && typeof g.__TAURI_INTERNALS__.invoke === 'function' ? (g.__TAURI_INTERNALS__ as TauriInternals) : null;
}

export type RequestName = keyof Requests;

/** A request that could not be answered. `code` says why. */
export class HostError extends Error {
  constructor(
    readonly code: 'UnknownRequest' | 'NotBuiltYet' | 'Failed',
    message: string,
  ) {
    super(message);
    this.name = 'HostError';
  }
}

function place(): Place {
  if (tauri()) return 'console';
  if (limoChild) return 'child-module';
  return __DEV__ ? 'mock' : 'none';
}

function systemName(): string {
  if (Platform.OS !== 'android') return Platform.OS;
  const release = (Platform.constants as { Release?: string }).Release;
  return release ?? `API ${Platform.Version}`;
}

// What JavaScript can answer by itself, where no native module is linked.
// The child's requests have no answer here: the phone is not paired, so the
// child form shows "not set up yet" and asks nothing else.
function answerHere<N extends RequestName>(name: N, where: Place): Requests[N]['answer'] {
  switch (name) {
    case 'about':
      return { form: appForm(), answeredBy: where, childModule: null, system: systemName() } as Requests[N]['answer'];
    case 'child.pairing':
      return { paired: false } as Requests[N]['answer'];
    default:
      throw new HostError(
        'NotBuiltYet',
        name.startsWith('child.') ? `Nothing here answers ${name}: the child module is not linked.` : `Nothing here answers ${name}.`,
      );
  }
}

// The checks each answer from Kotlin goes through.
const READ: { [N in RequestName]: (answer: unknown) => Requests[N]['answer'] } = {
  about: (answer) => {
    const a = answer as { form?: unknown; module?: unknown; android?: unknown };
    return {
      form: a.form === 'child' ? 'child' : appForm(),
      answeredBy: 'child-module',
      childModule: typeof a.module === 'string' ? a.module : null,
      system: typeof a.android === 'string' ? a.android : systemName(),
    };
  },
  'child.pairing': readPairing,
  'child.agreed': (a) => readBoolean(a, 'agreed'),
  'child.agree': () => null,
  'child.steps': readSteps,
  'child.openStep': () => null,
  'child.finishSetup': () => null,
  'child.introSeen': (a) => readBoolean(a, 'introSeen'),
  'child.markIntroSeen': () => null,
  'child.today': readToday,
  'child.rules': readRules,
  'child.maker': readMaker,
  'child.askForMoreTime': () => null,
  'guardian.about': readDoorAnswer,
  'guardian.signIn': readDoorAnswer,
  'console.selfCheck': (a) => a,
};

// The console: a request goes to the console's one Rust command, `request`,
// by its name there. The door's answers are read as every answer is; a
// door that could not be asked comes back as the console's own words.
async function askConsole<N extends RequestName>(name: N, args: Requests[N]['args']): Promise<Requests[N]['answer']> {
  const wire = CONSOLE_WIRE[name];
  const bridge = tauri();
  if (!wire || !bridge) return answerHere(name, 'console');
  let answer: unknown;
  try {
    answer = await bridge.invoke('request', { name: wire, args: args as Record<string, unknown> });
  } catch (e) {
    const problem = readDoorProblem(e);
    if (problem && name.startsWith('guardian.')) return problem as Requests[N]['answer'];
    throw new HostError('Failed', problem?.kind === 'problem' ? problem.words : String(e));
  }
  try {
    return READ[name](answer);
  } catch (e) {
    throw new HostError('Failed', e instanceof SeamError ? `The console answered ${name} wrongly: ${e.message}` : String(e));
  }
}

async function askChildModule<N extends RequestName>(name: N, args: Requests[N]['args']): Promise<Requests[N]['answer']> {
  if (!limoChild) throw new HostError('NotBuiltYet', 'The child module is not linked in this build.');
  let answer: unknown;
  try {
    answer = await limoChild.request(name, args as Record<string, unknown>);
  } catch (e) {
    const code = (e as { code?: unknown }).code === 'UnknownRequest' ? 'UnknownRequest' : 'Failed';
    throw new HostError(code, e instanceof Error ? e.message : String(e));
  }
  try {
    return READ[name](answer);
  } catch (e) {
    throw new HostError('Failed', e instanceof SeamError ? `The child module answered ${name} wrongly: ${e.message}` : String(e));
  }
}

async function request<N extends RequestName>(name: N, args: Requests[N]['args']): Promise<Requests[N]['answer']> {
  const where = place();
  if (where === 'child-module') return askChildModule(name, args);
  if (where === 'console') return askConsole(name, args);
  return answerHere(name, where);
}

export const host = { form: appForm, place, request };

// ---------------------------------------------------------------------------
// The child's part of the seam, as the app answers it.

const none: None = {};

/** Change events from Kotlin: the child module sends "change" with { what }. */
type Emitter = { addListener(event: string, listener: (event: unknown) => void): { remove(): void } };

function emitter(): Emitter | null {
  const m = limoChild as unknown as Partial<Emitter> | null;
  return m && typeof m.addListener === 'function' ? (m as Emitter) : null;
}

export const childHost: ChildHost = {
  // Until the child module answers it, the phone is not paired: the child
  // form then shows "not set up yet" and nothing else.
  async pairing() {
    try {
      return await request('child.pairing', none);
    } catch (e) {
      if (e instanceof HostError && e.code !== 'Failed') return { paired: false };
      throw e;
    }
  },
  agreed: () => request('child.agreed', none),
  agree: async () => {
    await request('child.agree', none);
  },
  steps: () => request('child.steps', none),
  openStep: async (step) => {
    await request('child.openStep', { step });
  },
  finishSetup: async () => {
    await request('child.finishSetup', none);
  },
  introSeen: () => request('child.introSeen', none),
  markIntroSeen: async () => {
    await request('child.markIntroSeen', none);
  },
  today: () => request('child.today', none),
  rules: () => request('child.rules', none),
  // The version the app was built as (app.config.ts), from its embedded config.
  version: async () => Constants.expoConfig?.version ?? '',
  maker: () => request('child.maker', none),
  askForMoreTime: async () => {
    await request('child.askForMoreTime', none);
  },
  subscribe(listener: (change: ChildChange) => void) {
    const e = emitter();
    if (!e) return () => {};
    const sub = e.addListener('change', (event) => {
      const what = readChange(event);
      if (what) listener(what);
    });
    return () => sub.remove();
  },
};

// ---------------------------------------------------------------------------
// The guardian's part of the seam, as the app answers it.

/** Where nothing answers the guardian's requests yet (the guardian form on a phone, for now). */
const NOT_HERE = 'LiMo’s settings open in the console on the family’s PC. This app cannot open them yet.';

async function askGuardian<N extends 'guardian.about' | 'guardian.signIn'>(name: N, args: Requests[N]['args']): Promise<DoorOutcome> {
  try {
    return await request(name, args);
  } catch (e) {
    if (e instanceof HostError && e.code === 'NotBuiltYet') return { kind: 'problem', code: 'not_here', words: NOT_HERE };
    return { kind: 'problem', code: 'failed', words: e instanceof Error ? e.message : String(e) };
  }
}

export const guardianHost: GuardianHost = {
  about: () => askGuardian('guardian.about', none),
  signIn: (password) => askGuardian('guardian.signIn', { password }),
};
