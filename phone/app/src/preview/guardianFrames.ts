// Every guardian screen the drawings draw (design/screens/console.html), each
// from the made up family (src/mock/guardianMock.ts), by the drawings' own
// ids: C0 to C0d, F1 to F8 with F5b and F7b, T1 to T3, and from the second
// sheet R1 to R3, D1 and D2, A1 to A9, K1 and K2, M1 to M4. Each is shown at
// the console's width (1100 by 760) and at a phone's (390 by 844).

import type { GuardianRoute } from '../guardian/GuardianApp';
import type { AccountRow, Weekday } from '../guardianSeam';
import { type GuardianMockState, guardianStates, NEW_PHONE_ID, NOW, startingRules } from '../mock/guardianMock';
import { connected, freshCable, GALAXY, type MockCable, PHONE_WORDS } from '../mock/phoneMock';

export type GuardianFrame = {
  readonly id: string;
  readonly title: string;
  readonly state: () => GuardianMockState;
  readonly route: GuardianRoute;
};

const ASHA = { childId: 'child-asha', name: 'Asha', ageBand: 'ten_to_twelve' } as const;
const TWO_HOURS = startingRules();
const WITH_BEDTIME = { ...TWO_HOURS, version: 3, blocked_hours: [{ days: ['monday', 'tuesday', 'wednesday', 'thursday', 'friday', 'saturday', 'sunday'] as Weekday[], start_minute: 1260, end_minute: 420 }] };
/** The drawing's week: an hour and a half on school days, two on Friday, three at the weekend. */
const WEEK: Record<Weekday, number> = { monday: 90, tuesday: 90, wednesday: 90, thursday: 90, friday: 120, saturday: 180, sunday: 180 };
const FAMILY_SO_FAR = { ticked: true, familyMade: true, pinSet: true, name: 'Asha', band: 'ten_to_twelve', child: ASHA } as const;

/** The accounts as the door lists them once said: the guardian's not limited, Asha's hers, Mira's for later. */
function accounts(ashaIsAdmin: boolean): AccountRow[] {
  return [
    { accountId: 'S-1-5-21-1001', name: 'guardian', displayName: 'Guardian', isAdministrator: true, signedIn: true, onThisPc: true, thisWindow: true, link: { kind: 'not_limited' } },
    { accountId: 'S-1-5-21-1002', name: 'asha', displayName: 'Asha', isAdministrator: ashaIsAdmin, signedIn: false, onThisPc: true, thisWindow: false, link: { kind: 'child', childId: 'child-asha', childName: 'Asha' } },
    { accountId: 'S-1-5-21-1003', name: 'mira', displayName: 'Mira', isAdministrator: false, signedIn: false, onThisPc: true, thisWindow: false, link: { kind: 'none' } },
  ];
}

export const GUARDIAN_FRAMES: readonly GuardianFrame[] = [
  { id: 'C0', title: 'C0 · Sign in', state: guardianStates.signedOut, route: { at: 'sign-in', start: { typed: 'ninechars' } } },
  { id: 'C0b', title: 'C0b · A wrong password', state: guardianStates.signedOut, route: { at: 'sign-in', start: { line: 'That password is not right. 3 more tries before a wait.' } } },
  { id: 'C0c', title: 'C0c · The wait after too many wrong tries', state: guardianStates.waiting, route: { at: 'sign-in', start: { waitUntil: NOW + 5 * 60 } } },
  { id: 'C0d', title: 'C0d · From a child’s Windows account', state: guardianStates.childAccount, route: { at: 'sign-in' } },
  { id: 'F1', title: 'F1 · Welcome, and the guardian agrees', state: () => guardianStates.fresh(), route: { at: 'first' } },
  {
    id: 'F2',
    title: 'F2 · The guardian password, twice',
    state: () => guardianStates.fresh(),
    route: { at: 'first', initial: { step: 1, ticked: true, password: 'elevenchars', passwordAgain: 'sixchr' } },
  },
  {
    id: 'F3',
    title: 'F3 · The guardian PIN, twice',
    state: guardianStates.madeFamily,
    route: { at: 'first', initial: { step: 2, ticked: true, familyMade: true, pin: '204816', pinAgain: '204' } },
  },
  {
    id: 'F4',
    title: 'F4 · The child',
    state: guardianStates.madeFamily,
    route: { at: 'first', initial: { step: 3, ticked: true, familyMade: true, pinSet: true, name: 'Asha', band: 'ten_to_twelve' } },
  },
  {
    id: 'F5',
    title: 'F5 · Time a day: one figure for every day',
    state: () => guardianStates.withAsha(),
    route: { at: 'first', initial: { ...FAMILY_SO_FAR, step: 4, rules: TWO_HOURS } },
  },
  {
    id: 'F5b',
    title: 'F5b · Time a day: each day on its own',
    state: () => guardianStates.withAsha(),
    route: { at: 'first', initial: { ...FAMILY_SO_FAR, step: 4, rules: TWO_HOURS, each: true, days: WEEK } },
  },
  {
    id: 'F6',
    title: 'F6 · Bedtime',
    state: () => guardianStates.withAsha(),
    route: { at: 'first', initial: { ...FAMILY_SO_FAR, step: 5, rules: TWO_HOURS } },
  },
  {
    id: 'F7',
    title: 'F7 · The Windows accounts on this PC',
    state: () => guardianStates.withAsha(false, true),
    route: { at: 'first', initial: { ...FAMILY_SO_FAR, step: 6, rules: WITH_BEDTIME }, accounts: accounts(false) },
  },
  {
    id: 'F7b',
    title: 'F7b · A child’s account is an administrator',
    state: () => guardianStates.withAsha(true, true),
    route: { at: 'first', initial: { ...FAMILY_SO_FAR, step: 6, rules: WITH_BEDTIME }, accounts: accounts(true) },
  },
  {
    id: 'F8',
    title: 'F8 · Done',
    state: () => guardianStates.withAsha(false, true),
    route: { at: 'first', initial: { ...FAMILY_SO_FAR, step: 7, rules: WITH_BEDTIME }, accounts: accounts(false) },
  },
  { id: 'T1', title: 'T1 · Right after the first start', state: guardianStates.firstToday, route: { at: 'app', tab: 'today', fromFirstStart: true } },
  { id: 'T2', title: 'T2 · One child', state: guardianStates.oneChild, route: { at: 'app', tab: 'today' } },
  { id: 'T3', title: 'T3 · Two children, a request waiting', state: guardianStates.twoChildren, route: { at: 'app', tab: 'today' } },
  // The second sheet.
  { id: 'R1', title: 'R1 · Asha’s rules', state: () => guardianStates.family(), route: { at: 'app', tab: 'rules' } },
  {
    id: 'R2',
    title: 'R2 · Save found a problem: the rules’ own sentence beside the row',
    state: () => guardianStates.family(),
    route: { at: 'app', tab: 'rules', show: { rulesDraft: (r) => ({ ...r, blocked_apps: [...r.blocked_apps, 'com.google.android.apps.maps'] }), rulesSave: true } },
  },
  { id: 'R3', title: 'R3 · Saved', state: () => guardianStates.family(), route: { at: 'app', tab: 'rules', show: { rulesSaved: true } } },
  { id: 'D1', title: 'D1 · Devices', state: () => guardianStates.family(), route: { at: 'app', tab: 'devices' } },
  { id: 'D2', title: 'D2 · A phone that needs a guardian', state: () => guardianStates.family(true), route: { at: 'app', tab: 'devices' } },
  { id: 'K1', title: 'K1 · A one time code: for whom, on what, how much', state: () => guardianStates.family(), route: { at: 'app', tab: 'more', sub: { at: 'code' } } },
  { id: 'K2', title: 'K2 · The code, to read out', state: () => guardianStates.family(), route: { at: 'app', tab: 'more', sub: { at: 'code' }, show: { codeMade: true } } },
  { id: 'M1', title: 'M1 · More', state: () => guardianStates.family(), route: { at: 'app', tab: 'more' } },
  { id: 'M2', title: 'M2 · The password and the PIN', state: () => guardianStates.family(), route: { at: 'app', tab: 'more', sub: { at: 'secrets' } } },
  { id: 'M3', title: 'M3 · About LiMo', state: () => guardianStates.family(), route: { at: 'app', tab: 'more', sub: { at: 'about' } } },
  { id: 'M4', title: 'M4 · Delete the family: the password again', state: () => guardianStates.family(), route: { at: 'app', tab: 'more', sub: { at: 'delete-family' } } },
  ...ADD_FRAMES(),
];

/** A1 to A9, each from the cable as the drawing has it, held still. */
function ADD_FRAMES(): GuardianFrame[] {
  const still = (change: Partial<MockCable>) => freshCable({ frozen: true, ...change });
  const on = (change: Partial<MockCable>) => connected(GALAXY, { frozen: true, ...change });
  const installed = { stage: 'done', sent: null, line: PHONE_WORDS.installed, advice: null };
  const withLimo = { ...GALAXY, limo: { looked: true, on_it: true, version: GALAXY.this_pc_carries } };
  const paired = (ways: string[]) => ({
    stage: 'paired',
    device: { device_id: NEW_PHONE_ID, name: 'Asha’s Galaxy S20 Ultra' },
    ways,
    line: ways.includes('home_network') ? PHONE_WORDS.overWifi : PHONE_WORDS.overCable,
    advice: null,
  });
  const at = (step: number) => ({ at: 'app' as const, tab: 'devices' as const, sub: { at: 'add-phone' as const, step, child: 'child-asha' } });
  const rows: [string, string, () => GuardianMockState, number][] = [
    ['A1', 'Start: whose phone, and what is needed', () => guardianStates.adding(still({ case: 'none' })), 0],
    ['A2', 'Switch on USB debugging', () => guardianStates.adding(still({ case: 'off' })), 1],
    ['A3', 'Allow this PC on the phone, and compare its key', () => guardianStates.adding(still({ case: 'allow', askedSecs: 5 })), 2],
    ['A3u', 'Step 3 before the phone is asked: Ask the phone', () => guardianStates.adding(still({ case: 'unknown' })), 2],
    ['A3h', 'Step 3 after 20 seconds of asking: nothing on the phone?', () => guardianStates.adding(still({ case: 'allow', askedSecs: 25 })), 2],
    ['A4', 'Checks, then putting LiMo on the phone', () => guardianStates.adding(on({ install: { stage: 'sending', sent: 0.62, line: PHONE_WORDS.sending, advice: null }, busy: 'installing' })), 3],
    ['A5', 'Paired over the cable, and the phone’s name', () => guardianStates.adding(on({ phone: withLimo, install: installed, pairing: paired(['cable']) }), 0), 4],
    ['A6', 'The phone’s own six steps, done on the phone', () => guardianStates.adding(on({ phone: withLimo, install: installed, pairing: paired(['cable']) }), 3), 5],
    ['A7', 'At home: the phone reaches this PC over Wi-Fi', () => guardianStates.adding(on({ phone: withLimo, install: installed, pairing: paired(['cable', 'home_network']) }), 6), 6],
    ['A8', 'Switch USB debugging off', () => guardianStates.adding(on({ phone: withLimo, install: installed, pairing: paired(['cable', 'home_network']) }), 6), 7],
    [
      'A9',
      'Done',
      () =>
        guardianStates.adding(
          on({ case: 'off', phone: withLimo, install: installed, pairing: paired(['cable', 'home_network']), debugging: { stage: 'off', line: PHONE_WORDS.isOff, advice: null } }),
          6,
        ),
      8,
    ],
  ];
  return rows.map(([id, title, state, step]) => ({ id, title: `${id} · ${title}`, state, route: at(step) }));
}

export function findGuardianFrame(id: string): GuardianFrame | undefined {
  return GUARDIAN_FRAMES.find((f) => f.id === id);
}
