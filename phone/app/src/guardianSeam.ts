// The guardian's part of the seam: what the guardian's screens ask, as plain
// data. In the console on the PC, src/host.ts sends each request to the
// console's Rust side, which passes it through the door to LiMo's service
// (../../console/README.md, and the service's side in
// ../../windows/README.md, "The console's door"); on a guardian's phone it
// will go over the phone's own link, later. The tests and the preview pass a
// door of their own.
//
// Every request goes by the door's own name with the door's own arguments
// (windows/src/console.rs, `Request`), and every answer comes in the hub's
// own shapes (hub/src/types.rs, core/src/rules.rs): the readers below check
// each one and turn it into what the screens use, so a wrong shape fails in
// one place with a message, and the screens say so in words.
//
// The door answers in its own words: a wrong password, a wait, a refusal
// all come with the service's sentence for the screen, and the screens show
// it as it is.

import { SeamError } from './childSeam';

/** One of the rules' own problems: the field it is about (`budgets.monday`) and a sentence for the guardian. */
export type RuleProblem = { readonly field: string; readonly message: string };

/** What became of a request, as the guardian's screens need it. */
export type DoorOutcome =
  /** Done: the family's file's own answer, in the hub's shapes. */
  | { readonly kind: 'done'; readonly value: unknown }
  /** The password was not right; this many more wrong tries before a wait, and the wait it set, if it set one. */
  | { readonly kind: 'wrong'; readonly triesBeforeWait: number; readonly waitUntil: number | null; readonly words: string }
  /** Too many wrong tries: nothing is checked until then (seconds since 1970). */
  | { readonly kind: 'wait'; readonly until: number; readonly words: string }
  /** The door does not open for this Windows account (a child's, or one Windows would not say). */
  | { readonly kind: 'refused'; readonly words: string }
  /** The door did not do it, and said why: signed out, busy, no family yet, the rules' problems, and the rest. */
  | { readonly kind: 'not-done'; readonly reason: string; readonly words: string; readonly ruleProblems: readonly RuleProblem[] }
  /** The door could not be asked, or its answer could not be read: the console's own words, or the screens'. */
  | { readonly kind: 'problem'; readonly code: string; readonly words: string };

/** What the sign in asks: who answers, and the password. Every call answers with an outcome and never throws for the door's sake. */
export interface GuardianHost {
  /** What answers, asked first: a child's account is refused at once. */
  about(): Promise<DoorOutcome>;
  /** Signs in with the family's password. */
  signIn(password: string): Promise<DoorOutcome>;
}

/** The door itself: any request, by its own name. Never throws for the door's sake. */
export interface GuardianDoor {
  ask(name: DoorName, args: Record<string, unknown>): Promise<DoorOutcome>;
}

/** The sign in's two questions, asked of a door. */
export function hostOf(door: GuardianDoor): GuardianHost {
  return {
    about: () => door.ask('about', {}),
    signIn: (password) => door.ask('sign_in', { password }),
  };
}

/**
 * The guardian's host, from one way of asking the door a request by its own
 * name (src/host.ts gives the console's): the sign in's two questions, and
 * every other request. Whatever that way throws becomes a problem in words,
 * so the screens are never thrown at.
 */
export function makeGuardianHost(askDoor: (name: DoorName, args: Record<string, unknown>) => Promise<DoorOutcome>): GuardianHost & GuardianDoor {
  const door: GuardianDoor = {
    ask: (name, args) =>
      askDoor(name, args).catch((e: unknown) => ({ kind: 'problem', code: 'failed', words: e instanceof Error ? e.message : String(e) }) as const),
  };
  return { ...hostOf(door), ask: door.ask };
}

// ---------------------------------------------------------------------------
// The hub's shapes, as these screens use them

export type AgeBand = 'under_six' | 'six_to_nine' | 'ten_to_twelve' | 'thirteen_to_seventeen';
export const AGE_BANDS: readonly AgeBand[] = ['under_six', 'six_to_nine', 'ten_to_twelve', 'thirteen_to_seventeen'];

export type Weekday = 'monday' | 'tuesday' | 'wednesday' | 'thursday' | 'friday' | 'saturday' | 'sunday';
export const WEEKDAYS: readonly Weekday[] = ['monday', 'tuesday', 'wednesday', 'thursday', 'friday', 'saturday', 'sunday'];

/** A day's time, as the rules keep it: so many minutes, or no limit. */
export type DailyBudget = { readonly minutes: number } | 'no_limit';

/** Hours locked whatever time is left, such as bedtime; past midnight when the end is not after the start. */
export type BlockedWindow = { readonly days: readonly Weekday[]; readonly start_minute: number; readonly end_minute: number };

/**
 * A child's rules, exactly as the door sends and takes them (core's
 * `RuleSet`). The screens change what they show and send the rest back as it
 * came: the door replaces the whole set.
 */
export type RuleSet = {
  readonly version: number;
  readonly utc_offset_secs: number;
  readonly day_start_minute: number;
  readonly budgets: Readonly<Record<Weekday, DailyBudget>>;
  readonly blocked_hours: readonly BlockedWindow[];
  readonly always_allowed_apps: readonly string[];
  readonly blocked_apps: readonly string[];
  readonly away_share_percent: number;
  readonly lease_minutes: number;
  readonly warning_minutes: readonly number[];
};

export type AboutView = { readonly version: string; readonly pcName: string };

export type FamilyState = {
  readonly hasFamily: boolean;
  readonly hasPin: boolean;
  readonly children: number;
  /** Locked once the first start is finished. */
  readonly unlinkedAccounts: 'free' | 'locked';
  readonly signedIn: boolean;
  /** Until when LiMo may be removed from this PC, after a guardian allowed it; null when it may not. */
  readonly removalAllowedUntil: number | null;
};

export type ChildView = { readonly childId: string; readonly name: string; readonly ageBand: AgeBand };

/** The guardian's lock or unlock in force for a child's devices. */
export type GuardianMode =
  | { readonly mode: 'nothing' }
  | { readonly mode: 'lock'; readonly until: number | null }
  | { readonly mode: 'unlock'; readonly until: number };

/** What a phone needs from Android that is switched off, by the core's names (`PhonePermission`), in setup order. */
export type PhonePermission = 'notifications' | 'phone' | 'usage_access' | 'accessibility' | 'battery' | 'protection';
export const PHONE_PERMISSIONS: readonly PhonePermission[] = ['notifications', 'phone', 'usage_access', 'accessibility', 'battery', 'protection'];

/** One thing a device needs a guardian for: a permission switched off, and the engine's sentence for it. */
export type DeviceNeed = { readonly permission: PhonePermission; readonly words: string };

export type DeviceToday = {
  readonly deviceId: string;
  readonly name: string;
  /** The family PC itself, or a device that can be carried away. */
  readonly kind: 'on_master' | 'portable';
  readonly lastHeardAt: number | null;
  readonly inUse: boolean;
  readonly usedSecs: number;
  /** How far its clock was from this PC's at its newest report (ahead is positive), when far; null when close enough. */
  readonly clockOffBySecs: number | null;
  /**
   * The steps switched off now, in setup order, each with the engine's
   * sentence: null while none is off (and for the PC's own device).
   */
  readonly needs: readonly DeviceNeed[] | null;
  /** How a phone stands, from its notes: null for the PC's own device, or a phone whose notes cannot be read. */
  readonly standing: Standing | null;
};

/** One of a phone's six setup steps: on, off, or null while no note said; and since when, by the phone's clock. */
export type PhoneStep = { readonly permission: PhonePermission; readonly on: boolean | null; readonly since: number | null };

/** How a phone stands (`standing`, with `setup_finished` and `usb_debugging_on`): what Devices shows of it. */
export type Standing = {
  /** The six, in setup order. */
  readonly steps: readonly PhoneStep[];
  /** Its protection, or null while no note said. */
  readonly protection: 'off' | 'base' | 'strict' | null;
  readonly setupFinished: boolean;
  readonly usbDebuggingOn: boolean;
};

export type ChildToday = {
  readonly childId: string;
  readonly name: string;
  readonly ageBand: AgeBand;
  /** Today's home day number, by the child's rules. */
  readonly day: number;
  readonly budgetSecs: number | null;
  readonly extraSecs: number;
  readonly usedSecs: number;
  readonly leftSecs: number | null;
  readonly guardian: GuardianMode;
  readonly devices: readonly DeviceToday[];
};

export type WaitingRequest = {
  readonly childId: string;
  readonly childName: string;
  readonly deviceId: string;
  readonly deviceName: string;
  readonly requestId: string;
  readonly minutes: number;
  /** The child's own reason, possibly empty. */
  readonly reason: string;
  readonly askedAt: number;
};

export type AccountLink =
  | { readonly kind: 'child'; readonly childId: string; readonly childName: string | null }
  | { readonly kind: 'not_limited' }
  | { readonly kind: 'none' };

export type AccountRow = {
  readonly accountId: string;
  readonly name: string;
  readonly displayName: string;
  readonly isAdministrator: boolean;
  readonly signedIn: boolean;
  readonly onThisPc: boolean;
  /** The account LiMo's settings are open in. */
  readonly thisWindow: boolean;
  readonly link: AccountLink;
};

/** A device paired with a child, as the family's file keeps it (`devices`, `device`, `rename_device`). */
export type DeviceInfo = {
  readonly deviceId: string;
  readonly childId: string;
  readonly name: string;
  readonly kind: 'on_master' | 'portable';
  readonly platform: string;
  readonly pairedAt: number;
  readonly lastHeardAt: number | null;
};

/** Time used on one device over some days, app by app (`usage`). An app the device could not name has an empty id. */
export type DeviceUsage = {
  readonly deviceId: string;
  readonly name: string;
  readonly removed: boolean;
  readonly secs: number;
  readonly apps: readonly { readonly appId: string; readonly secs: number }[];
};

/** The PC's settings a guardian can change. */
export type PcSettings = { readonly idleMinutes: number; readonly unlinkedAccounts: 'free' | 'locked' };

/** A one time code, as the door makes it: the code as shown, in two groups of four. */
export type OneTimeCode = { readonly shown: string; readonly minutes: number; readonly deviceId: string };

/** A guardian's answer to a request, as the door takes it. */
export type RequestReply = { readonly grant: { readonly minutes: number } } | 'refuse';

/** A link for a Windows account, as the door takes it. */
export type AccountLinkWire = { readonly child: { readonly child_id: string } } | 'not_limited';

type None = Record<string, never>;
type Nothing = Record<string, never>;

/** Every request these screens make, by the door's name: what it is sent and what it answers, once read. */
export type DoorRequests = {
  about: { args: None; answer: AboutView };
  family_state: { args: None; answer: FamilyState };
  sign_in: { args: { password: string }; answer: Nothing };
  sign_out: { args: None; answer: Nothing };
  create_family: { args: { password: string }; answer: Nothing };
  set_pin: { args: { pin: string }; answer: Nothing };
  add_child: { args: { name: string; age_band: AgeBand }; answer: ChildView };
  rename_child: { args: { child: string; name: string }; answer: ChildView };
  set_age_band: { args: { child: string; age_band: AgeBand }; answer: ChildView };
  rules: { args: { child: string }; answer: RuleSet };
  replace_rules: { args: { child: string; rules: RuleSet }; answer: RuleSet };
  accounts: { args: None; answer: readonly AccountRow[] };
  link_account: { args: { account: string; link: AccountLinkWire }; answer: Nothing };
  unlink_account: { args: { account: string }; answer: Nothing };
  finish_first_start: { args: None; answer: Nothing };
  family_today: { args: None; answer: readonly ChildToday[] };
  waiting_requests: { args: None; answer: readonly WaitingRequest[] };
  answer_request: { args: { child: string; request: string; reply: RequestReply }; answer: Nothing };
  give_extra: { args: { child: string; minutes: number; tap_id: string }; answer: { readonly givenNow: boolean } };
  lock_now: { args: { child: string; until?: number }; answer: GuardianMode };
  unlock_until: { args: { child: string; until: number }; answer: GuardianMode };
  lift: { args: { child: string }; answer: GuardianMode };
  usage: { args: { child: string; from_day: number; to_day: number }; answer: readonly DeviceUsage[] };
  remove_child: { args: { child: string; password: string }; answer: Nothing };
  devices: { args: { child: string }; answer: readonly DeviceInfo[] };
  rename_device: { args: { device: string; name: string }; answer: DeviceInfo };
  remove_device: { args: { device: string; password: string }; answer: Nothing };
  pc_settings: { args: None; answer: PcSettings };
  set_idle_minutes: { args: { minutes: number }; answer: PcSettings };
  set_unlinked_accounts: { args: { unlinked: 'free' | 'locked' }; answer: PcSettings };
  make_code: { args: { device: string; minutes: number }; answer: OneTimeCode };
  change_password: { args: { old: string; new: string }; answer: Nothing };
  allow_removal: { args: { password: string }; answer: { readonly until: number } };
  delete_family: { args: { password: string }; answer: Nothing };
};

export type DoorName = keyof DoorRequests;

/** What a request comes back as: its answer read, or the door's outcome that is not one. */
export type Asked<T> = { readonly kind: 'done'; readonly value: T } | Exclude<DoorOutcome, { kind: 'done' }>;

// ---------------------------------------------------------------------------
// Readers

type Obj = Record<string, unknown>;

function obj(x: unknown, what: string): Obj {
  if (typeof x !== 'object' || x === null || Array.isArray(x)) throw new SeamError(`${what} is not an object`);
  return x as Obj;
}

function list(x: unknown, what: string): unknown[] {
  if (!Array.isArray(x)) throw new SeamError(`${what} is not a list`);
  return x;
}

function str(o: Obj, key: string, what: string): string {
  const v = o[key];
  if (typeof v !== 'string') throw new SeamError(`${what}.${key} is not a string`);
  return v;
}

function words(o: Obj, key: string, what: string): string {
  const v = str(o, key, what);
  if (v === '') throw new SeamError(`${what}.${key} is not a sentence`);
  return v;
}

function bool(o: Obj, key: string, what: string): boolean {
  const v = o[key];
  if (typeof v !== 'boolean') throw new SeamError(`${what}.${key} is not true or false`);
  return v;
}

function whole(v: unknown, what: string): number {
  if (typeof v !== 'number' || !Number.isInteger(v)) throw new SeamError(`${what} is not a whole number`);
  return v;
}

function count(o: Obj, key: string, what: string): number {
  const v = whole(o[key], `${what}.${key}`);
  if (v < 0) throw new SeamError(`${what}.${key} is below zero`);
  return v;
}

function maybeCount(o: Obj, key: string, what: string): number | null {
  return o[key] === null || o[key] === undefined ? null : count(o, key, what);
}

function maybeWhole(o: Obj, key: string, what: string): number | null {
  return o[key] === null || o[key] === undefined ? null : whole(o[key], `${what}.${key}`);
}

function oneOf<T extends string>(o: Obj, key: string, allowed: readonly T[], what: string): T {
  const v = o[key];
  if (typeof v !== 'string' || !(allowed as readonly string[]).includes(v)) throw new SeamError(`${what}.${key} is not one the screens know`);
  return v as T;
}

/** Reads one reply of the door as the console hands it on. */
export function readDoorAnswer(x: unknown): DoorOutcome {
  const a = obj(x, 'the reply');
  switch (a.outcome) {
    case 'done':
      if (!('value' in a)) throw new SeamError('the reply has no value');
      return { kind: 'done', value: a.value };
    case 'refused':
      return { kind: 'refused', words: words(a, 'message', 'the reply') };
    case 'problem': {
      const p = obj(a.problem, 'the reply.problem');
      if (typeof p.kind !== 'string' || p.kind === '') throw new SeamError('the reply.problem.kind is not a word');
      const said = words(p, 'message', 'the reply.problem');
      if (p.kind === 'wrong_password') {
        return {
          kind: 'wrong',
          triesBeforeWait: count(p, 'tries_before_wait', 'the reply.problem'),
          waitUntil: maybeWhole(p, 'wait_until', 'the reply.problem'),
          words: said,
        };
      }
      if (p.kind === 'must_wait') return { kind: 'wait', until: count(p, 'wait_until', 'the reply.problem'), words: said };
      const ruleProblems = p.rule_problems === undefined ? [] : list(p.rule_problems, 'the reply.problem.rule_problems').map((r, i) => {
        const o = obj(r, `the reply.problem.rule_problems[${i}]`);
        return { field: str(o, 'field', 'a rule problem'), message: words(o, 'message', 'a rule problem') };
      });
      return { kind: 'not-done', reason: p.kind, words: said, ruleProblems };
    }
    default:
      throw new SeamError('the reply is not one the screens know');
  }
}

/** Reads the console's word that the door could not be asked: `{ code, words }`. */
export function readDoorProblem(x: unknown): DoorOutcome | null {
  if (typeof x !== 'object' || x === null) return null;
  const p = x as Obj;
  if (typeof p.code !== 'string' || typeof p.words !== 'string' || p.words === '') return null;
  return { kind: 'problem', code: p.code, words: p.words };
}

export function readAbout(x: unknown): AboutView {
  const o = obj(x, 'about');
  return { version: str(o, 'version', 'about'), pcName: str(o, 'pc_name', 'about') };
}

export function readFamilyState(x: unknown): FamilyState {
  const o = obj(x, 'family_state');
  return {
    hasFamily: bool(o, 'has_family', 'family_state'),
    hasPin: bool(o, 'has_pin', 'family_state'),
    children: count(o, 'children', 'family_state'),
    unlinkedAccounts: oneOf(o, 'unlinked_accounts', ['free', 'locked'] as const, 'family_state'),
    signedIn: bool(o, 'signed_in', 'family_state'),
    removalAllowedUntil: maybeWhole(o, 'removal_allowed_until', 'family_state'),
  };
}

export function readChild(x: unknown): ChildView {
  const o = obj(x, 'the child');
  return {
    childId: words(o, 'child_id', 'the child'),
    name: words(o, 'name', 'the child'),
    ageBand: oneOf(o, 'age_band', AGE_BANDS, 'the child'),
  };
}

function readBudget(x: unknown, what: string): DailyBudget {
  if (x === 'no_limit') return 'no_limit';
  const o = obj(x, what);
  const m = count(o, 'minutes', what);
  return { minutes: m };
}

export function readRuleSet(x: unknown): RuleSet {
  const o = obj(x, 'the rules');
  const b = obj(o.budgets, 'the rules.budgets');
  const budgets = Object.fromEntries(WEEKDAYS.map((d) => [d, readBudget(b[d], `the rules.budgets.${d}`)])) as Record<Weekday, DailyBudget>;
  const strings = (key: string) =>
    list(o[key], `the rules.${key}`).map((s, i) => {
      if (typeof s !== 'string') throw new SeamError(`the rules.${key}[${i}] is not a string`);
      return s;
    });
  return {
    version: count(o, 'version', 'the rules'),
    utc_offset_secs: whole(o.utc_offset_secs, 'the rules.utc_offset_secs'),
    day_start_minute: count(o, 'day_start_minute', 'the rules'),
    budgets,
    blocked_hours: list(o.blocked_hours, 'the rules.blocked_hours').map((w, i) => {
      const what = `the rules.blocked_hours[${i}]`;
      const ow = obj(w, what);
      return {
        days: list(ow.days, `${what}.days`).map((d, j) => {
          if (typeof d !== 'string' || !(WEEKDAYS as readonly string[]).includes(d)) throw new SeamError(`${what}.days[${j}] is not a weekday`);
          return d as Weekday;
        }),
        start_minute: count(ow, 'start_minute', what),
        end_minute: count(ow, 'end_minute', what),
      };
    }),
    always_allowed_apps: strings('always_allowed_apps'),
    blocked_apps: strings('blocked_apps'),
    away_share_percent: count(o, 'away_share_percent', 'the rules'),
    lease_minutes: count(o, 'lease_minutes', 'the rules'),
    warning_minutes: list(o.warning_minutes, 'the rules.warning_minutes').map((m, i) => whole(m, `the rules.warning_minutes[${i}]`)),
  };
}

/** The guardian's lock or unlock, as serde writes it: "nothing", {"lock":{"until":...}}, {"unlock":{"until":...}}. */
export function readGuardianMode(x: unknown, what = 'the guardian'): GuardianMode {
  if (x === 'nothing') return { mode: 'nothing' };
  const o = obj(x, what);
  if ('lock' in o) {
    const l = obj(o.lock, `${what}.lock`);
    return { mode: 'lock', until: maybeWhole(l, 'until', `${what}.lock`) };
  }
  if ('unlock' in o) {
    const u = obj(o.unlock, `${what}.unlock`);
    return { mode: 'unlock', until: whole(u.until, `${what}.unlock.until`) };
  }
  throw new SeamError(`${what} is not a lock, an unlock or nothing`);
}

/** The setting the door answers a lock, an unlock or a lift with: its mode. */
export function readGuardianSetting(x: unknown): GuardianMode {
  return readGuardianMode(obj(x, 'the setting').mode, 'the setting.mode');
}

/**
 * What a device needs a guardian for, in the shape the phone's engine gives
 * the child's own Today (`guardian_needed`): `{ "off": [ { "permission",
 * "words" } ] }`. The door sends null while none is off, and for the PC's
 * own device: no mark. A permission these screens do not know is left out
 * with its sentence; the rest are put in setup order, each once. What is on
 * is read from `standing`, not from here.
 */
export function readDeviceNeeds(x: unknown, what = 'guardian_needed'): DeviceNeed[] | null {
  if (x === undefined || x === null) return null;
  const o = obj(x, what);
  const off = list(o.off, `${what}.off`).map((n, i) => {
    const one = obj(n, `${what}.off[${i}]`);
    return { permission: str(one, 'permission', `${what}.off[${i}]`), words: words(one, 'words', `${what}.off[${i}]`) };
  });
  return PHONE_PERMISSIONS.flatMap((p) => {
    const found = off.find((n) => n.permission === p);
    return found ? [{ permission: p, words: found.words }] : [];
  });
}

function readDevice(x: unknown, what: string): DeviceToday {
  const o = obj(x, what);
  return {
    deviceId: words(o, 'device_id', what),
    name: str(o, 'name', what),
    kind: oneOf(o, 'kind', ['on_master', 'portable'] as const, what),
    lastHeardAt: maybeWhole(o, 'last_heard_at', what),
    inUse: bool(o, 'in_use', what),
    usedSecs: count(o, 'used_secs', what),
    clockOffBySecs: maybeWhole(o, 'clock_off_by_secs', what),
    needs: readDeviceNeeds(o.guardian_needed, `${what}.guardian_needed`),
    standing: readStanding(o, what),
  };
}

/**
 * How a phone stands, from a device's `standing`, `setup_finished` and
 * `usb_debugging_on` (LiMo for Windows' README, "What a device needs a
 * guardian for"). All null, or left out, is null: nothing known. Of the
 * standing's many fields only what Devices shows is read; a step these
 * screens do not know is left out, and the six come back in setup order.
 */
export function readStanding(o: Obj, what = 'the device'): Standing | null {
  if (o.standing === undefined || o.standing === null) return null;
  const s = obj(o.standing, `${what}.standing`);
  const steps = list(s.steps, `${what}.standing.steps`).map((x, i) => {
    const w = `${what}.standing.steps[${i}]`;
    const step = obj(x, w);
    if (step.on !== null && typeof step.on !== 'boolean') throw new SeamError(`${w}.on is not true, false or null`);
    return { permission: str(step, 'permission', w), on: step.on as boolean | null, since: maybeWhole(step, 'since', w) };
  });
  const protection = s.protection === null || s.protection === undefined ? null : oneOf(obj(s.protection, `${what}.standing.protection`), 'level', ['off', 'base', 'strict'] as const, `${what}.standing.protection`);
  return {
    steps: PHONE_PERMISSIONS.map((p) => {
      const found = steps.find((x) => x.permission === p);
      return { permission: p, on: found ? found.on : null, since: found ? found.since : null };
    }),
    protection,
    setupFinished: bool(s, 'setup_finished', `${what}.standing`),
    usbDebuggingOn: o.usb_debugging_on === true,
  };
}

function readDeviceInfo(x: unknown, what = 'the device'): DeviceInfo {
  const o = obj(x, what);
  return {
    deviceId: words(o, 'device_id', what),
    childId: words(o, 'child_id', what),
    name: str(o, 'name', what),
    kind: oneOf(o, 'kind', ['on_master', 'portable'] as const, what),
    platform: str(o, 'platform', what),
    pairedAt: whole(o.paired_at, `${what}.paired_at`),
    lastHeardAt: maybeWhole(o, 'last_heard_at', what),
  };
}

export function readDevices(x: unknown): DeviceInfo[] {
  return list(x, 'devices').map((d, i) => readDeviceInfo(d, `devices[${i}]`));
}

/** A child's usage: each device, with each app's time. */
export function readUsage(x: unknown): DeviceUsage[] {
  const o = obj(x, 'usage');
  return list(o.devices, 'usage.devices').map((d, i) => {
    const what = `usage.devices[${i}]`;
    const od = obj(d, what);
    return {
      deviceId: words(od, 'device_id', what),
      name: str(od, 'name', what),
      removed: bool(od, 'removed', what),
      secs: count(od, 'secs', what),
      apps: list(od.apps, `${what}.apps`).map((a, j) => {
        const oa = obj(a, `${what}.apps[${j}]`);
        return { appId: str(oa, 'app_id', `${what}.apps[${j}]`), secs: count(oa, 'secs', `${what}.apps[${j}]`) };
      }),
    };
  });
}

export function readPcSettings(x: unknown): PcSettings {
  const o = obj(x, 'the PC’s settings');
  return {
    idleMinutes: count(o, 'idle_minutes', 'the PC’s settings'),
    unlinkedAccounts: oneOf(o, 'unlinked_accounts', ['free', 'locked'] as const, 'the PC’s settings'),
  };
}

export function readCode(x: unknown): OneTimeCode {
  const o = obj(x, 'the code');
  return { shown: words(o, 'shown', 'the code'), minutes: count(o, 'minutes', 'the code'), deviceId: words(o, 'device_id', 'the code') };
}

function readUntil(x: unknown): { until: number } {
  const o = obj(x, 'the answer');
  return { until: whole(o.until, 'the answer.until') };
}

export function readFamilyToday(x: unknown): ChildToday[] {
  return list(x, 'family_today').map((c, i) => {
    const what = `family_today[${i}]`;
    const o = obj(c, what);
    return {
      childId: words(o, 'child_id', what),
      name: words(o, 'name', what),
      ageBand: oneOf(o, 'age_band', AGE_BANDS, what),
      day: whole(o.day, `${what}.day`),
      budgetSecs: maybeCount(o, 'budget_secs', what),
      extraSecs: count(o, 'extra_secs', what),
      usedSecs: count(o, 'used_secs', what),
      leftSecs: maybeCount(o, 'left_secs', what),
      guardian: readGuardianMode(o.guardian, `${what}.guardian`),
      devices: list(o.devices, `${what}.devices`).map((d, j) => readDevice(d, `${what}.devices[${j}]`)),
    };
  });
}

export function readWaitingRequests(x: unknown): WaitingRequest[] {
  return list(x, 'waiting_requests').map((r, i) => {
    const what = `waiting_requests[${i}]`;
    const o = obj(r, what);
    const view = obj(o.request, `${what}.request`);
    const req = obj(view.request, `${what}.request.request`);
    const inner = `${what}.request.request`;
    return {
      childId: words(o, 'child_id', what),
      childName: words(o, 'child_name', what),
      deviceId: words(req, 'device_id', inner),
      deviceName: str(o, 'device_name', what),
      requestId: words(req, 'id', inner),
      minutes: count(req, 'minutes', inner),
      reason: str(req, 'reason', inner),
      askedAt: whole(req.asked_at, `${inner}.asked_at`),
    };
  });
}

function readLink(x: unknown, what: string): AccountLink {
  const o = obj(x, what);
  const kind = oneOf(o, 'kind', ['child', 'not_limited', 'none'] as const, what);
  if (kind === 'child') {
    const name = o.child_name;
    if (name !== null && name !== undefined && typeof name !== 'string') throw new SeamError(`${what}.child_name is not a string`);
    return { kind, childId: words(o, 'child_id', what), childName: (name as string | null | undefined) ?? null };
  }
  return { kind };
}

export function readAccounts(x: unknown): AccountRow[] {
  return list(x, 'accounts').map((a, i) => {
    const what = `accounts[${i}]`;
    const o = obj(a, what);
    return {
      accountId: words(o, 'account_id', what),
      name: str(o, 'name', what),
      displayName: str(o, 'display_name', what),
      isAdministrator: bool(o, 'is_administrator', what),
      signedIn: bool(o, 'signed_in', what),
      onThisPc: bool(o, 'on_this_pc', what),
      thisWindow: bool(o, 'this_window', what),
      link: readLink(o.link, `${what}.link`),
    };
  });
}

export function readExtraGiven(x: unknown): { givenNow: boolean } {
  const o = obj(x, 'the extra time');
  words(o, 'id', 'the extra time');
  return { givenNow: bool(o, 'given_now', 'the extra time') };
}

/** An answer that carries nothing the screens use: any object. */
export function readNothing(x: unknown): Nothing {
  obj(x, 'the answer');
  return {};
}

const READ: { [N in DoorName]: (x: unknown) => DoorRequests[N]['answer'] } = {
  about: readAbout,
  family_state: readFamilyState,
  sign_in: readNothing,
  sign_out: readNothing,
  create_family: readNothing,
  set_pin: readNothing,
  add_child: readChild,
  rename_child: readChild,
  set_age_band: readChild,
  rules: readRuleSet,
  replace_rules: readRuleSet,
  accounts: readAccounts,
  link_account: readNothing,
  unlink_account: readNothing,
  finish_first_start: readNothing,
  family_today: readFamilyToday,
  waiting_requests: readWaitingRequests,
  answer_request: readNothing,
  give_extra: readExtraGiven,
  lock_now: readGuardianSetting,
  unlock_until: readGuardianSetting,
  lift: readGuardianSetting,
  usage: readUsage,
  remove_child: readNothing,
  devices: readDevices,
  rename_device: (x) => readDeviceInfo(x),
  remove_device: readNothing,
  pc_settings: readPcSettings,
  set_idle_minutes: readPcSettings,
  set_unlinked_accounts: readPcSettings,
  make_code: readCode,
  change_password: readNothing,
  allow_removal: readUntil,
  delete_family: readNothing,
};

/** Said when an answer of the door cannot be read: something the screens were not built for. */
export const MISREAD = 'LiMo’s service answered in a way these settings cannot read. Nothing was shown wrongly. Please report this.';

/** Asks the door by the request's own name, and reads its answer. Never throws. */
export async function ask<N extends DoorName>(door: GuardianDoor, name: N, args: DoorRequests[N]['args']): Promise<Asked<DoorRequests[N]['answer']>> {
  let outcome: DoorOutcome;
  try {
    outcome = await door.ask(name, args as Record<string, unknown>);
  } catch (e) {
    return { kind: 'problem', code: 'failed', words: e instanceof Error ? e.message : String(e) };
  }
  if (outcome.kind !== 'done') return outcome;
  try {
    return { kind: 'done', value: READ[name](outcome.value) };
  } catch (e) {
    if (e instanceof SeamError) return { kind: 'problem', code: 'misread', words: MISREAD };
    throw e;
  }
}
