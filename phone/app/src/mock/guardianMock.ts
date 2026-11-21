// A made up family behind a made up door, for the guardian screens' preview
// on the PC and their tests. DEVELOPMENT ONLY.
//
// It answers as LiMo's service does at the console's door
// (windows/README.md, "The console's door"): by the door's request names,
// with the hub's own shapes in snake case, each reply put through the same
// reader the console's replies go through (readDoorAnswer). So every reader
// in src/guardianSeam.ts is used by every test. Its sentences for a wrong
// password, a wait, a sign out and a refusal are the door's own.
//
// Nothing in the app imports this folder: only the preview and tests/ do,
// which tests/release.test.ts and the console's build check. It carries the
// child mock's mark, so a search for that mark finds it too.

import { type DoorName, type DoorOutcome, type GuardianDoor, readDoorAnswer, type RuleSet, type Weekday, WEEKDAYS } from '../guardianSeam';
import { MOCK_MARK } from './childMock';

export { MOCK_MARK };

/** Saturday 21 November 2026, 6:12 pm, on a PC at UTC+5:30: the drawings' own afternoon. */
export const NOW = Date.UTC(2026, 10, 21, 12, 42, 0) / 1000;
export const PC_CLOCK = { utcOffsetSecs: 19_800, hours24: false } as const;

/** The door's own sentences. */
export const DOOR_WORDS = {
  signedOut: 'Signed out. Type the guardian password to go on.',
  childAccount: (name: string) => `LiMo’s settings cannot be opened from ${name}’s account. A guardian opens them from their own.`,
  notRunning: 'LiMo’s service is not running on this PC, so its settings cannot open.',
  mustWait: 'Too many wrong tries. Wait before trying again.',
  wrongWithWait: 'That password is not right. Too many wrong tries: wait before trying again.',
  familyExists: 'A family is already set up on this PC.',
  tooShort: 'The password must be at least 8 characters long.',
  pinShape: 'A PIN is 6 to 8 digits, with nothing else in it.',
  rulesNotTaken: 'The rules were not taken. Each problem is said beside its field.',
  ownAccount:
    'LiMo’s settings are open in this account, so it cannot be linked to a child from here: a guardian would be shut out of them. Link it from another account.',
};

/** Wrong passwords allowed before the first wait, as the family's file counts them. */
const FREE_WRONG = 4;

type WireMode = 'nothing' | { lock: { until: number | null } } | { unlock: { until: number } };

export type MockDevice = {
  device_id: string;
  name: string;
  kind: 'on_master' | 'portable';
  platform: string;
  last_heard_at: number | null;
  in_use: boolean;
  used_secs: number;
};

export type MockChild = {
  child_id: string;
  name: string;
  age_band: string;
  rules: RuleSet;
  extra_secs: number;
  guardian: WireMode;
  devices: MockDevice[];
};

export type MockAccount = {
  account_id: string;
  name: string;
  display_name: string;
  is_administrator: boolean;
  signed_in: boolean;
  on_this_pc: boolean;
  this_window: boolean;
  /** A child's id, "not_limited", or null for no link. */
  link: string | null;
};

export type MockRequest = { child_id: string; id: string; device_id: string; minutes: number; reason: string; asked_at: number };

export type GuardianMockState = {
  hasFamily: boolean;
  password: string | null;
  pin: string | null;
  signedIn: boolean;
  /** The name of the child whose Windows account the window is open in: the door refuses everything. */
  childAccount: string | null;
  /** The service is not running: the console says so for every request. */
  notRunning: boolean;
  wrongGuesses: number;
  waitUntil: number | null;
  unlinked: 'free' | 'locked';
  children: MockChild[];
  accounts: MockAccount[];
  requests: MockRequest[];
  taps: string[];
};

export type GuardianMock = GuardianDoor & {
  readonly mark: string;
  /** Every request, in order, by name, with its arguments. */
  readonly calls: { name: DoorName; args: Record<string, unknown> }[];
  readonly state: GuardianMockState;
  /** Changes what it holds. */
  set(change: Partial<GuardianMockState>): void;
};

/** The rules a child starts with: two hours every day, as the family's file gives them. */
export function startingRules(): RuleSet {
  return {
    version: 1,
    utc_offset_secs: PC_CLOCK.utcOffsetSecs,
    day_start_minute: 0,
    budgets: Object.fromEntries(WEEKDAYS.map((d) => [d, { minutes: 120 }])) as Record<Weekday, { minutes: number }>,
    blocked_hours: [],
    always_allowed_apps: [],
    blocked_apps: [],
    away_share_percent: 50,
    lease_minutes: 10,
    warning_minutes: [15, 5, 1],
  };
}

function weekdayOf(now: number, offset: number): Weekday {
  // 1 January 1970 was a Thursday.
  const day = Math.floor((now + offset) / 86_400);
  return WEEKDAYS[(((day + 3) % 7) + 7) % 7];
}

function done(value: unknown): DoorOutcome {
  return readDoorAnswer({ outcome: 'done', value });
}

function problem(kind: string, message: string, more: Record<string, unknown> = {}): DoorOutcome {
  return readDoorAnswer({ outcome: 'problem', problem: { kind, message, ...more } });
}

const OPEN_TO_ANYONE: readonly DoorName[] = ['sign_in', 'sign_out', 'about', 'family_state', 'create_family'];

export function guardianMock(start: GuardianMockState, options: { now?: () => number } = {}): GuardianMock {
  let state: GuardianMockState = structuredClone(start);
  const calls: { name: DoorName; args: Record<string, unknown> }[] = [];
  const now = options.now ?? (() => NOW);
  let ids = 0;

  const child = (id: unknown) => state.children.find((c) => c.child_id === id);
  const noChild = () => problem('not_found', 'That child is not in the family.');
  const budgetSecs = (c: MockChild) => {
    const b = c.rules.budgets[weekdayOf(now(), c.rules.utc_offset_secs)];
    return b === 'no_limit' ? null : b.minutes * 60;
  };
  const setting = (c: MockChild) => done({ version: 1, mode: c.guardian, set_at: now() });

  function answer(name: DoorName, args: Record<string, unknown>): DoorOutcome {
    if (state.notRunning) return { kind: 'problem', code: 'not_running', words: DOOR_WORDS.notRunning };
    if (state.childAccount) return readDoorAnswer({ outcome: 'refused', message: DOOR_WORDS.childAccount(state.childAccount) });
    if (!OPEN_TO_ANYONE.includes(name) && !state.signedIn) return problem('signed_out', DOOR_WORDS.signedOut);
    switch (name) {
      case 'about':
        return done({ version: '0.1.0', protocol: 1, pc_name: 'FAMILY-PC' });
      case 'family_state':
        return done({
          has_family: state.hasFamily,
          has_pin: state.pin !== null,
          children: state.children.length,
          unlinked_accounts: state.unlinked,
          signed_in: state.signedIn,
        });
      case 'sign_in': {
        if (!state.hasFamily) return problem('no_family', 'No family is set up yet. Set a guardian password first.');
        const t = now();
        if (state.waitUntil !== null && t < state.waitUntil) return problem('must_wait', DOOR_WORDS.mustWait, { wait_until: state.waitUntil, tries_before_wait: 0 });
        if (args.password === state.password) {
          state.wrongGuesses = 0;
          state.waitUntil = null;
          state.signedIn = true;
          return done({});
        }
        state.wrongGuesses += 1;
        if (state.wrongGuesses > FREE_WRONG) {
          state.waitUntil = t + Math.min(3_600, 60 * 2 ** (state.wrongGuesses - FREE_WRONG - 1));
          return problem('wrong_password', DOOR_WORDS.wrongWithWait, { wait_until: state.waitUntil, tries_before_wait: 0 });
        }
        const left = FREE_WRONG - state.wrongGuesses;
        const message =
          left === 0 ? 'That password is not right.' : left === 1 ? 'That password is not right. 1 more try before a wait.' : `That password is not right. ${left} more tries before a wait.`;
        return problem('wrong_password', message, { tries_before_wait: left });
      }
      case 'sign_out':
        state.signedIn = false;
        return done({});
      case 'create_family': {
        if (state.hasFamily) return problem('refused', DOOR_WORDS.familyExists);
        const p = String(args.password ?? '');
        if (p.length < 8) return problem('invalid', DOOR_WORDS.tooShort);
        state.hasFamily = true;
        state.password = p;
        state.signedIn = true;
        return done({});
      }
      case 'set_pin': {
        const pin = String(args.pin ?? '');
        if (!/^[0-9]{6,8}$/.test(pin)) return problem('invalid', DOOR_WORDS.pinShape);
        state.pin = pin;
        return done({});
      }
      case 'add_child': {
        const n = String(args.name ?? '').trim();
        if (n === '') return problem('invalid', 'The child’s name cannot be empty.');
        if (state.children.some((c) => c.name === n)) return problem('invalid', `There is already a child called ${n} in the family.`);
        ids += 1;
        const c: MockChild = { child_id: `child-new-${ids}`, name: n, age_band: String(args.age_band), rules: startingRules(), extra_secs: 0, guardian: 'nothing', devices: [] };
        state.children.push(c);
        return done({ child_id: c.child_id, name: c.name, age_band: c.age_band, added_at: now() });
      }
      case 'rename_child':
      case 'set_age_band': {
        const c = child(args.child);
        if (!c) return noChild();
        if (name === 'rename_child') c.name = String(args.name);
        else c.age_band = String(args.age_band);
        return done({ child_id: c.child_id, name: c.name, age_band: c.age_band, added_at: now() });
      }
      case 'rules': {
        const c = child(args.child);
        return c ? done(c.rules) : noChild();
      }
      case 'replace_rules': {
        const c = child(args.child);
        if (!c) return noChild();
        const rules = args.rules as RuleSet;
        const problems = rules.blocked_hours
          .map((w, i) => (w.start_minute === w.end_minute ? { field: `blocked_hours[${i}].end_minute`, message: `Blocked hours number ${i + 1} starts and ends at the same time.` } : null))
          .filter((p) => p !== null);
        if (problems.length) return problem('rules', DOOR_WORDS.rulesNotTaken, { rule_problems: problems });
        c.rules = { ...rules, version: c.rules.version + 1 };
        return done(c.rules);
      }
      case 'accounts':
        return done(
          state.accounts.map((a) => ({
            account_id: a.account_id,
            name: a.name,
            display_name: a.display_name,
            is_administrator: a.is_administrator,
            signed_in: a.signed_in,
            on_this_pc: a.on_this_pc,
            this_window: a.this_window,
            link:
              a.link === null
                ? { kind: 'none' }
                : a.link === 'not_limited'
                  ? { kind: 'not_limited' }
                  : { kind: 'child', child_id: a.link, child_name: child(a.link)?.name ?? null },
          })),
        );
      case 'link_account': {
        const a = state.accounts.find((x) => x.account_id === args.account);
        if (!a) return problem('not_found', 'There is no such Windows account on this PC.');
        const link = args.link as { child: { child_id: string } } | 'not_limited';
        if (link !== 'not_limited' && a.this_window) return problem('refused', DOOR_WORDS.ownAccount);
        a.link = link === 'not_limited' ? 'not_limited' : link.child.child_id;
        return done({ account_id: a.account_id, display_name: a.display_name, link });
      }
      case 'unlink_account': {
        const a = state.accounts.find((x) => x.account_id === args.account);
        if (!a || a.link === null) return problem('not_found', 'That Windows account is not linked.');
        a.link = null;
        return done({});
      }
      case 'finish_first_start':
        state.unlinked = 'locked';
        return done({ idle_minutes: 3, unlinked_accounts: 'locked' });
      case 'family_today':
        return done(
          state.children.map((c) => {
            const budget = budgetSecs(c);
            const used = c.devices.reduce((s, d) => s + d.used_secs, 0);
            const left = budget === null ? null : Math.max(0, budget + c.extra_secs - used);
            return {
              child_id: c.child_id,
              name: c.name,
              age_band: c.age_band,
              day: Math.floor((now() + c.rules.utc_offset_secs) / 86_400),
              budget_secs: budget,
              extra_secs: c.extra_secs,
              used_secs: used,
              left_secs: left,
              over_secs: budget === null ? 0 : Math.max(0, used - budget - c.extra_secs),
              guardian: c.guardian,
              waiting_requests: state.requests.filter((r) => r.child_id === c.child_id).length,
              devices: c.devices.map((d) => ({
                ...d,
                holds_secs: null,
                waiting_secs: 0,
                giving_back_secs: 0,
                added_secs: 0,
                wrong_guesses: 0,
                pin_unlocked_until: null,
                clock_off_by_secs: null,
              })),
            };
          }),
        );
      case 'waiting_requests':
        return done(
          state.requests.map((r) => {
            const c = child(r.child_id);
            const d = c?.devices.find((x) => x.device_id === r.device_id);
            return {
              child_id: r.child_id,
              child_name: c?.name ?? '',
              device_name: d?.name ?? r.device_id,
              request: {
                request: { id: r.id, child_id: r.child_id, device_id: r.device_id, day: 0, minutes: r.minutes, reason: r.reason, asked_at: r.asked_at },
                answer: 'waiting',
                answered_at: null,
                reached_master: true,
              },
            };
          }),
        );
      case 'answer_request': {
        const c = child(args.child);
        if (!c) return noChild();
        const r = state.requests.find((x) => x.id === args.request && x.child_id === c.child_id);
        if (!r) return problem('not_found', 'That request is not waiting any more.');
        const reply = args.reply as { grant: { minutes: number } } | 'refuse';
        if (reply !== 'refuse') c.extra_secs += reply.grant.minutes * 60;
        state.requests = state.requests.filter((x) => x !== r);
        return done({ request: r, answer: reply === 'refuse' ? 'refused' : { granted: { minutes: reply.grant.minutes } }, answered_at: now(), reached_master: true });
      }
      case 'give_extra': {
        const c = child(args.child);
        if (!c) return noChild();
        const tap = String(args.tap_id);
        const givenNow = !state.taps.includes(tap);
        if (givenNow) {
          state.taps.push(tap);
          c.extra_secs += Number(args.minutes) * 60;
        }
        return done({ id: tap, given_now: givenNow });
      }
      case 'lock_now': {
        const c = child(args.child);
        if (!c) return noChild();
        c.guardian = { lock: { until: typeof args.until === 'number' ? args.until : null } };
        return setting(c);
      }
      case 'unlock_until': {
        const c = child(args.child);
        if (!c) return noChild();
        c.guardian = { unlock: { until: Number(args.until) } };
        return setting(c);
      }
      case 'lift': {
        const c = child(args.child);
        if (!c) return noChild();
        c.guardian = 'nothing';
        return setting(c);
      }
    }
  }

  return {
    mark: MOCK_MARK,
    calls,
    get state() {
      return state;
    },
    set(change) {
      state = { ...state, ...change };
    },
    async ask(name, args) {
      calls.push({ name, args });
      return answer(name, args);
    },
  };
}

// ---------------------------------------------------------------------------
// The family, in the drawings' states

const PC_OF = (child: string, used: number): MockDevice => ({
  device_id: `pc-${child}`,
  name: 'FAMILY-PC',
  kind: 'on_master',
  platform: 'windows',
  last_heard_at: NOW - 20,
  in_use: false,
  used_secs: used,
});

const ASHA_PHONE = (inUse: boolean, heardSecsAgo: number): MockDevice => ({
  device_id: 'phone-asha',
  name: 'Asha’s phone',
  kind: 'portable',
  platform: 'android',
  last_heard_at: NOW - heardSecsAgo,
  in_use: inUse,
  used_secs: 31 * 60,
});

const ACCOUNTS = (ashaIsAdmin: boolean, linked: boolean): MockAccount[] => [
  { account_id: 'S-1-5-21-1001', name: 'guardian', display_name: 'Guardian', is_administrator: true, signed_in: true, on_this_pc: true, this_window: true, link: linked ? 'not_limited' : null },
  { account_id: 'S-1-5-21-1002', name: 'asha', display_name: 'Asha', is_administrator: ashaIsAdmin, signed_in: false, on_this_pc: true, this_window: false, link: linked ? 'child-asha' : null },
  { account_id: 'S-1-5-21-1003', name: 'mira', display_name: 'Mira', is_administrator: false, signed_in: false, on_this_pc: true, this_window: false, link: null },
];

const ASHA = (devices: MockDevice[]): MockChild => ({
  child_id: 'child-asha',
  name: 'Asha',
  age_band: 'ten_to_twelve',
  rules: { ...startingRules(), version: 3, blocked_hours: [{ days: [...WEEKDAYS], start_minute: 1260, end_minute: 420 }] },
  extra_secs: 0,
  guardian: 'nothing',
  devices,
});

const BASE: GuardianMockState = {
  hasFamily: true,
  password: 'Asha and Mira at home',
  pin: '204816',
  signedIn: true,
  childAccount: null,
  notRunning: false,
  wrongGuesses: 0,
  waitUntil: null,
  unlinked: 'locked',
  children: [ASHA([PC_OF('asha', 17 * 60), ASHA_PHONE(true, 10)])],
  accounts: ACCOUNTS(false, true),
  requests: [],
  taps: [],
};

/** Each a fresh copy: the mock changes what it holds. */
export const guardianStates = {
  /** No family yet: the first start. */
  fresh: (ashaIsAdmin = false): GuardianMockState => ({
    ...BASE,
    hasFamily: false,
    password: null,
    pin: null,
    signedIn: false,
    unlinked: 'free',
    children: [],
    accounts: ACCOUNTS(ashaIsAdmin, false),
  }),
  /** The first start part way: the family made with its password, this window signed in, nothing else yet. */
  madeFamily: (): GuardianMockState => ({ ...guardianStates.fresh(), hasFamily: true, password: 'Asha and Mira at home', signedIn: true }),
  /**
   * The first start further on: the PIN set and Asha added with the
   * starting rules; whose each account is said (`linked`) or not yet.
   */
  withAsha: (ashaIsAdmin = false, linked = false): GuardianMockState => ({
    ...guardianStates.madeFamily(),
    pin: '204816',
    children: [{ ...ASHA([]), rules: startingRules() }],
    accounts: ACCOUNTS(ashaIsAdmin, linked),
  }),
  /** A family, and this window not signed in. */
  signedOut: (): GuardianMockState => ({ ...BASE, signedIn: false }),
  /** Too many wrong tries: a wait until 6:17 pm. */
  waiting: (): GuardianMockState => ({ ...BASE, signedIn: false, wrongGuesses: 5, waitUntil: NOW + 5 * 60 }),
  /** The window is open in Asha's Windows account. */
  childAccount: (): GuardianMockState => ({ ...BASE, signedIn: false, childAccount: 'Asha' }),
  /** The service is not running. */
  notRunning: (): GuardianMockState => ({ ...BASE, signedIn: false, notRunning: true }),
  /** T1: right after the first start. */
  firstToday: (): GuardianMockState => ({
    ...BASE,
    children: [{ ...ASHA([PC_OF('asha', 0)]), rules: { ...startingRules(), version: 3 } }],
  }),
  /** T2: one child, an ordinary afternoon. */
  oneChild: (): GuardianMockState => ({ ...BASE }),
  /** T3: two children, Mira's time used up, and Mira asking for more. */
  twoChildren: (): GuardianMockState => ({
    ...BASE,
    children: [
      ASHA([PC_OF('asha', 17 * 60), ASHA_PHONE(false, 3 * 60)]),
      {
        child_id: 'child-mira',
        name: 'Mira',
        age_band: 'six_to_nine',
        rules: { ...startingRules(), budgets: Object.fromEntries(WEEKDAYS.map((d) => [d, { minutes: 60 }])) as Record<Weekday, { minutes: number }> },
        extra_secs: 0,
        guardian: 'nothing',
        devices: [PC_OF('mira', 60 * 60)],
      },
    ],
    accounts: ACCOUNTS(false, true).map((a) => (a.name === 'mira' ? { ...a, link: 'child-mira' } : a)),
    requests: [{ child_id: 'child-mira', id: 'pc-mira/ask/1', device_id: 'pc-mira', minutes: 30, reason: 'To finish my science project', asked_at: NOW - 8 * 60 }],
  }),
};
