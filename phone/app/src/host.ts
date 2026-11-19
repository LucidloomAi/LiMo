// The one seam between the screens and whatever runs them, as in proof six
// (spikes/06-console-window/app/src/host.ts).
//
// Screens call host.request(name, args), or the child's part of the seam
// (childHost, below), and nothing else: never a native module, never the
// network. Only this file knows who answers:
//
//   child-module   the child form's Kotlin module (modules/limo-child), linked
//                  in the child form only
//   none           nothing native to ask yet: the guardian form today, whose
//                  link to the master PC is a later piece
//   mock           a development run with no native module (never in a
//                  release build); it answers only what JavaScript knows by
//                  itself, and holds no made up data
//
// A later place joins here, not in the screens: the guardian's link on the
// phone, and the console in a Tauri window on the PC (proof six found it by
// __TAURI_INTERNALS__ on the page's global object).
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

export type * from './childSeam';
export { STEP_IDS } from './childSeam';

export type Place = 'child-module' | 'none' | 'mock';

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
  'child.introSeen': { args: None; answer: boolean }; // readBoolean
  'child.markIntroSeen': { args: None; answer: null };
  'child.today': { args: None; answer: TodayView }; // readToday
  'child.rules': { args: None; answer: RulesView }; // readRules
  'child.askForMoreTime': { args: None; answer: null };
};

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
      throw new HostError('NotBuiltYet', `Nothing here answers ${name}: the child module is not linked.`);
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
  'child.introSeen': (a) => readBoolean(a, 'introSeen'),
  'child.markIntroSeen': () => null,
  'child.today': readToday,
  'child.rules': readRules,
  'child.askForMoreTime': () => null,
};

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
  return where === 'child-module' ? askChildModule(name, args) : answerHere(name, where);
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
  introSeen: () => request('child.introSeen', none),
  markIntroSeen: async () => {
    await request('child.markIntroSeen', none);
  },
  today: () => request('child.today', none),
  rules: () => request('child.rules', none),
  // The version the app was built as (app.config.ts), from its embedded config.
  version: async () => Constants.expoConfig?.version ?? '',
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
