// The child form's part of the seam: what the child's screens ask of the
// phone, as plain data. src/host.ts answers it in the app (through the child
// module in Kotlin); a mock answers it in the preview and the tests only.
//
// Every call is asynchronous, every answer is plain data (strings, numbers,
// booleans, arrays and objects of them), and `subscribe` says when an
// answer may have changed. The Kotlin side is built against this file: see
// the README, "The child's seam".
//
// The words in TodayView and RulesView are the engine's (phone/engine, through
// Kotlin), so the screens never make a sentence about time or rules, or about
// a step that is switched off, themselves. Labels that never change ("Left
// today", "Your rules") and the setup's own words are the screens'.

/**
 * The six things Android has to grant, in the order setup asks for them.
 * "phone" is Android's Phone permission, used only to know that a call is
 * ringing or on, so that a lock never hides a call.
 */
export const STEP_IDS = ['notifications', 'phone', 'usage', 'accessibility', 'battery', 'protection'] as const;
export type StepId = (typeof STEP_IDS)[number];

/**
 * A step as Android reports it now. Only "on" when Android itself says so:
 * anything else, or no answer, is "off".
 */
export type StepState = 'on' | 'off';
export type StepStates = Readonly<Record<StepId, StepState>>;

/** Whether the family PC has paired this phone, and the child's name. */
export type Pairing = { readonly paired: false } | { readonly paired: true; readonly childName: string };

/** The time left today, in the engine's words. */
export type TimeLeft =
  /** "1 h 12 min", "14 min", "0:42": shown large, with "Left today" under it. */
  | { readonly kind: 'left'; readonly words: string }
  /** "No limit today". */
  | { readonly kind: 'no-limit'; readonly words: string }
  /** While a guardian's unlock keeps the phone open: "Open until 9:30 pm". */
  | { readonly kind: 'open'; readonly words: string };

/** One app on Today's list: its name and its minutes, nothing else. */
export type TodayApp = { readonly name: string; readonly minutes: string };

/**
 * Something LiMo needs is switched off: the steps that are off, in setup
 * order, never empty, and beside them the engine's sentence for each, in the
 * same order: `words[i]` says what it means that `off[i]` is off.
 */
export type GuardianNeeded = { readonly off: readonly StepId[]; readonly words: readonly string[] };

/** The child's Today screen. */
export type TodayView = {
  readonly left: TimeLeft;
  /** The line under the bar: "48 min used of 2 h". */
  readonly used: string;
  /** How much of today's time is used, from 0 to 1, for the bar. Null with no limit today: no bar. */
  readonly usedPart: number | null;
  /** Where the time went: "On this phone 31 min · on the PC 17 min". */
  readonly where: string;
  /** "Screens are off from 9:00 pm to 7:00 am.", or null when no hours are off. */
  readonly bedtime: string | null;
  /** Today on this phone, as the engine orders them: names and minutes only. */
  readonly apps: readonly TodayApp[];
  /** Null while everything LiMo needs is on. */
  readonly guardianNeeded: GuardianNeeded | null;
};

/** The child's rules, for How it works. Each value is the engine's words. */
export type RulesView = {
  /** "2 h". */
  readonly timeADay: string;
  /** "9:00 pm to 7:00 am", or null when no hours are off. */
  readonly screensOff: string | null;
  /** The apps that stay open under a lock, by their names. */
  readonly alwaysOpen: readonly string[];
  /** The apps that are blocked, by their names. */
  readonly notAllowed: readonly string[];
};

/** What may have changed, said by `subscribe`. */
export type ChildChange = 'pairing' | 'agreement' | 'steps' | 'intro' | 'today' | 'rules';

/** What the child's screens ask of the phone. */
export interface ChildHost {
  /** Whether the family PC has paired this phone, and the child's name. */
  pairing(): Promise<Pairing>;
  /** Whether a guardian has agreed to what LiMo does on this phone (S2). */
  agreed(): Promise<boolean>;
  /** A guardian agreed: keep it. */
  agree(): Promise<void>;
  /** Each step as Android reports it at this moment. Never remembered: asked of Android every time. */
  steps(): Promise<StepStates>;
  /**
   * Opens Android's own screen or question for one step. Resolves once it
   * is open, or, for a question Android asks over the app, once it is
   * answered. The screen asks `steps()` again then, and again whenever the
   * app comes back to the front.
   */
  openStep(step: StepId): Promise<void>;
  /** Whether the child has seen the introduction (C1 to C3). */
  introSeen(): Promise<boolean>;
  /** The child has seen it: keep it. */
  markIntroSeen(): Promise<void>;
  /** The child's Today screen. */
  today(): Promise<TodayView>;
  /** The child's rules. */
  rules(): Promise<RulesView>;
  /** The app's version, such as "0.1.0". */
  version(): Promise<string>;
  /**
   * Who made the phone, as Android itself says it (Build.MANUFACTURER), such
   * as "samsung" or "Google". Passed through as it is; `isSamsung` reads it.
   */
  maker(): Promise<string>;
  /** Opens LiMo's own ways out screen (Kotlin), at asking for more time. Nothing else. */
  askForMoreTime(): Promise<void>;
  /** Calls `listener` whenever an answer may have changed. Returns a function that stops it. */
  subscribe(listener: (change: ChildChange) => void): () => void;
}

// ---------------------------------------------------------------------------
// Reading what Kotlin sends. Plain data comes in as unknown and is checked
// here, so a wrong shape fails in one place with a message, and a step is
// never read as on by accident.

export class SeamError extends Error {
  constructor(message: string) {
    super(message);
    this.name = 'SeamError';
  }
}

type Obj = Record<string, unknown>;

function obj(x: unknown, what: string): Obj {
  if (typeof x !== 'object' || x === null || Array.isArray(x)) throw new SeamError(`${what} is not an object`);
  return x as Obj;
}

function str(o: Obj, key: string, what: string): string {
  const v = o[key];
  if (typeof v !== 'string') throw new SeamError(`${what}.${key} is not a string`);
  return v;
}

function strOrNull(o: Obj, key: string, what: string): string | null {
  const v = o[key];
  if (v === null || v === undefined) return null;
  if (typeof v !== 'string') throw new SeamError(`${what}.${key} is not a string or null`);
  return v;
}

function strings(o: Obj, key: string, what: string): string[] {
  const v = o[key];
  if (!Array.isArray(v) || !v.every((s) => typeof s === 'string')) throw new SeamError(`${what}.${key} is not a list of strings`);
  return v as string[];
}

export function isStepId(x: unknown): x is StepId {
  return typeof x === 'string' && (STEP_IDS as readonly string[]).includes(x);
}

export function readPairing(x: unknown): Pairing {
  const o = obj(x, 'pairing');
  if (o.paired !== true) return { paired: false };
  const childName = str(o, 'childName', 'pairing').trim();
  if (!childName) throw new SeamError('pairing.childName is empty');
  return { paired: true, childName };
}

export function readBoolean(x: unknown, what: string): boolean {
  if (typeof x !== 'boolean') throw new SeamError(`${what} is not true or false`);
  return x;
}

/** Only the exact word "on" is on. */
export function readSteps(x: unknown): StepStates {
  const o = obj(x, 'steps');
  const out = {} as Record<StepId, StepState>;
  for (const id of STEP_IDS) out[id] = o[id] === 'on' ? 'on' : 'off';
  return out;
}

function readTimeLeft(x: unknown): TimeLeft {
  const o = obj(x, 'today.left');
  const words = str(o, 'words', 'today.left');
  if (o.kind === 'left' || o.kind === 'no-limit' || o.kind === 'open') return { kind: o.kind, words };
  throw new SeamError('today.left.kind is not left, no-limit or open');
}

export function readToday(x: unknown): TodayView {
  const o = obj(x, 'today');
  const part = o.usedPart;
  if (part !== null && (typeof part !== 'number' || !Number.isFinite(part))) throw new SeamError('today.usedPart is not a number or null');
  const appsIn = o.apps;
  if (!Array.isArray(appsIn)) throw new SeamError('today.apps is not a list');
  const apps = appsIn.map((a, i) => {
    const app = obj(a, `today.apps[${i}]`);
    return { name: str(app, 'name', `today.apps[${i}]`), minutes: str(app, 'minutes', `today.apps[${i}]`) };
  });
  return {
    left: readTimeLeft(o.left),
    used: str(o, 'used', 'today'),
    usedPart: part === null ? null : Math.min(1, Math.max(0, part as number)),
    where: str(o, 'where', 'today'),
    bedtime: strOrNull(o, 'bedtime', 'today'),
    apps,
    guardianNeeded: readGuardianNeeded(o.guardianNeeded),
  };
}

/**
 * The steps that are off and the engine's sentence for each. The two lists
 * must line up, one sentence for each step, none empty; anything else is
 * refused. A step this app does not know is left out with its sentence (a
 * newer engine may know more steps), and the rest are put in setup order,
 * each once.
 */
function readGuardianNeeded(x: unknown): GuardianNeeded | null {
  if (x === null || x === undefined) return null;
  const what = 'today.guardianNeeded';
  const g = obj(x, what);
  if (!Array.isArray(g.off)) throw new SeamError(`${what}.off is not a list`);
  const words = strings(g, 'words', what);
  if (words.length !== g.off.length) {
    throw new SeamError(`${what} has ${g.off.length} steps and ${words.length} sentences: they do not line up`);
  }
  const said = new Map<StepId, string>();
  g.off.forEach((id: unknown, i) => {
    if (!words[i].trim()) throw new SeamError(`${what}.words[${i}] is empty`);
    if (isStepId(id) && !said.has(id)) said.set(id, words[i]);
  });
  const off = STEP_IDS.filter((id) => said.has(id));
  return off.length ? { off, words: off.map((id) => said.get(id) as string) } : null;
}

/** Who made the phone, as Android says it. */
export function readMaker(x: unknown): string {
  if (typeof x !== 'string') throw new SeamError('maker is not a string');
  return x.trim();
}

/** Whether Android's own name for the phone's maker is Samsung's ("samsung"). */
export function isSamsung(maker: string): boolean {
  return maker.trim().toLowerCase() === 'samsung';
}

export function readRules(x: unknown): RulesView {
  const o = obj(x, 'rules');
  return {
    timeADay: str(o, 'timeADay', 'rules'),
    screensOff: strOrNull(o, 'screensOff', 'rules'),
    alwaysOpen: strings(o, 'alwaysOpen', 'rules'),
    notAllowed: strings(o, 'notAllowed', 'rules'),
  };
}

const CHANGES: readonly ChildChange[] = ['pairing', 'agreement', 'steps', 'intro', 'today', 'rules'];

/** What a change event from Kotlin says, or null when it says nothing known. */
export function readChange(x: unknown): ChildChange | null {
  if (typeof x !== 'object' || x === null) return null;
  const what = (x as Obj).what;
  return (CHANGES as readonly unknown[]).includes(what) ? (what as ChildChange) : null;
}
