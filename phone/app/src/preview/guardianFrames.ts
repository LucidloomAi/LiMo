// Every guardian screen the drawings draw (design/screens/console.html), each
// from the made up family (src/mock/guardianMock.ts), by the drawings' own
// ids: C0 to C0d, F1 to F8 with F5b and F7b, T1 to T3. Each is shown at the
// console's width (1100 by 760) and at a phone's (390 by 844).

import type { GuardianRoute } from '../guardian/GuardianApp';
import type { AccountRow, Weekday } from '../guardianSeam';
import { type GuardianMockState, guardianStates, NOW, startingRules } from '../mock/guardianMock';

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
];

export function findGuardianFrame(id: string): GuardianFrame | undefined {
  return GUARDIAN_FRAMES.find((f) => f.id === id);
}
