// The guardian's seam (src/guardianSeam.ts): every answer the screens use, in
// the shapes the door sends (windows/src/service/door.rs, and the hub's and
// the core's serde shapes), read into what the screens use; every reader
// refusing a shape it does not know; and asking, which never throws.
//
// The answers below are WRITTEN BY HAND from that code, to try each reader's
// edges. The door's real answers, written by its own test, are read in
// tests/guardianContract.test.ts.

import { SeamError } from '../src/childSeam';
import {
  ask,
  type DoorOutcome,
  type GuardianDoor,
  MISREAD,
  readAccounts,
  readDeviceNeeds,
  readDoorAnswer,
  readFamilyState,
  readFamilyToday,
  readGuardianMode,
  readGuardianSetting,
  readRuleSet,
  readWaitingRequests,
} from '../src/guardianSeam';
import { guardianMock, guardianStates, startingRules } from '../src/mock/guardianMock';

const DEVICE = {
  device_id: 'dev-1',
  name: 'FAMILY-PC',
  kind: 'on_master',
  platform: 'windows',
  last_heard_at: 1_800_000_000,
  in_use: false,
  used_secs: 1_020,
  holds_secs: null,
  waiting_secs: 0,
  giving_back_secs: 0,
  added_secs: 0,
  wrong_guesses: 0,
  pin_unlocked_until: null,
  clock_off_by_secs: null,
};
const CHILD_TODAY = {
  child_id: 'child-1',
  name: 'Asha',
  age_band: 'ten_to_twelve',
  day: 20_778,
  budget_secs: 7_200,
  extra_secs: 0,
  used_secs: 2_880,
  left_secs: 4_320,
  over_secs: 0,
  guardian: 'nothing',
  waiting_requests: 0,
  devices: [DEVICE],
};

describe('the readers', () => {
  it('read family_state', () => {
    expect(readFamilyState({ has_family: true, has_pin: false, children: 1, unlinked_accounts: 'free', signed_in: true })).toEqual({
      hasFamily: true,
      hasPin: false,
      children: 1,
      unlinkedAccounts: 'free',
      signedIn: true,
      removalAllowedUntil: null,
    });
    // Removing LiMo allowed until a time, once a guardian said so.
    expect(readFamilyState({ has_family: true, has_pin: true, children: 1, unlinked_accounts: 'locked', signed_in: true, removal_allowed_until: 1_800_000_600 }).removalAllowedUntil).toBe(1_800_000_600);
    for (const bad of [null, {}, { has_family: 'yes', has_pin: false, children: 1, unlinked_accounts: 'free', signed_in: true }, { has_family: true, has_pin: false, children: -1, unlinked_accounts: 'free', signed_in: true }, { has_family: true, has_pin: false, children: 1, unlinked_accounts: 'open', signed_in: true }]) {
      expect(() => readFamilyState(bad)).toThrow(SeamError);
    }
  });

  it('read family_today: each child’s day, the guardian’s lock or unlock, and each device', () => {
    const [asha] = readFamilyToday([CHILD_TODAY]);
    expect(asha).toEqual({
      childId: 'child-1',
      name: 'Asha',
      ageBand: 'ten_to_twelve',
      day: 20_778,
      budgetSecs: 7_200,
      extraSecs: 0,
      usedSecs: 2_880,
      leftSecs: 4_320,
      guardian: { mode: 'nothing' },
      devices: [{ deviceId: 'dev-1', name: 'FAMILY-PC', kind: 'on_master', lastHeardAt: 1_800_000_000, inUse: false, usedSecs: 1_020, clockOffBySecs: null, needs: null, standing: null }],
    });
    // No limit today: no budget and no time left.
    expect(readFamilyToday([{ ...CHILD_TODAY, budget_secs: null, left_secs: null }])[0]).toMatchObject({ budgetSecs: null, leftSecs: null });
    // A clock far off: how far, ahead positive.
    expect(readFamilyToday([{ ...CHILD_TODAY, devices: [{ ...DEVICE, clock_off_by_secs: -540 }] }])[0].devices[0].clockOffBySecs).toBe(-540);
    for (const bad of [{}, [{ ...CHILD_TODAY, devices: [{ ...DEVICE, kind: 'tablet' }] }], [{ ...CHILD_TODAY, used_secs: 1.5 }], [{ ...CHILD_TODAY, guardian: 'lockdown' }], [{ ...CHILD_TODAY, age_band: 'teen' }], [{ ...CHILD_TODAY, day: 'today' }]]) {
      expect(() => readFamilyToday(bad)).toThrow(SeamError);
    }
  });

  it('read what a device needs a guardian for, in the shape asked of the door; nothing said is no mark and claims nothing', () => {
    const OFF = 'Accessibility is switched off, so LiMo cannot see which app is open.';
    const PHONE = 'Phone access is switched off, so a lock could hide a call.';
    // Nothing said: null, so no mark and nothing claimed of the six. An empty list: all six on.
    for (const nothing of [undefined, null]) expect(readDeviceNeeds(nothing)).toBeNull();
    expect(readDeviceNeeds({ off: [] })).toEqual([]);
    // In setup order, each once; a permission these screens do not know left out with its sentence.
    expect(
      readDeviceNeeds({
        off: [
          { permission: 'accessibility', words: OFF },
          { permission: 'camera', words: 'Not one of LiMo’s.' },
          { permission: 'phone', words: PHONE },
        ],
      }),
    ).toEqual([
      { permission: 'phone', words: PHONE },
      { permission: 'accessibility', words: OFF },
    ]);
    const [asha] = readFamilyToday([{ ...CHILD_TODAY, devices: [{ ...DEVICE, kind: 'portable', guardian_needed: { off: [{ permission: 'accessibility', words: OFF }] } }] }]);
    expect(asha.devices[0].needs).toEqual([{ permission: 'accessibility', words: OFF }]);
    for (const bad of [{ off: 'accessibility' }, { off: [{ permission: 'accessibility' }] }, { off: [{ permission: 'accessibility', words: '' }] }, 'accessibility']) {
      expect(() => readDeviceNeeds(bad)).toThrow(SeamError);
    }
  });

  it('read the guardian’s lock and unlock as serde writes them', () => {
    expect(readGuardianMode('nothing')).toEqual({ mode: 'nothing' });
    expect(readGuardianMode({ lock: { until: null } })).toEqual({ mode: 'lock', until: null });
    expect(readGuardianMode({ lock: { until: 1_800_000_000 } })).toEqual({ mode: 'lock', until: 1_800_000_000 });
    expect(readGuardianMode({ unlock: { until: 1_800_000_000 } })).toEqual({ mode: 'unlock', until: 1_800_000_000 });
    expect(readGuardianSetting({ version: 2, mode: { lock: { until: null } }, set_at: 1 })).toEqual({ mode: 'lock', until: null });
    expect(() => readGuardianMode({ unlock: {} })).toThrow(SeamError);
    expect(() => readGuardianMode('open')).toThrow(SeamError);
  });

  it('read waiting_requests: who, on which device, how much, the reason, when', () => {
    const wire = [
      {
        child_id: 'child-2',
        child_name: 'Mira',
        device_name: 'FAMILY-PC',
        request: {
          request: { id: 'pc/ask/1', child_id: 'child-2', device_id: 'pc', day: 20_778, minutes: 30, reason: 'To finish my science project', asked_at: 1_800_000_000 },
          answer: 'waiting',
          answered_at: null,
          reached_master: true,
        },
      },
    ];
    expect(readWaitingRequests(wire)).toEqual([
      { childId: 'child-2', childName: 'Mira', deviceId: 'pc', deviceName: 'FAMILY-PC', requestId: 'pc/ask/1', minutes: 30, reason: 'To finish my science project', askedAt: 1_800_000_000 },
    ]);
    expect(() => readWaitingRequests([{ ...wire[0], request: { request: { ...wire[0].request.request, minutes: '30' } } }])).toThrow(SeamError);
  });

  it('read accounts, with each link', () => {
    const row = { account_id: 'S-1', name: 'asha', display_name: 'Asha', is_administrator: false, signed_in: false, on_this_pc: true, this_window: false };
    expect(
      readAccounts([
        { ...row, link: { kind: 'child', child_id: 'child-1', child_name: 'Asha' } },
        { ...row, link: { kind: 'not_limited' } },
        { ...row, link: { kind: 'none' } },
      ]).map((a) => a.link),
    ).toEqual([{ kind: 'child', childId: 'child-1', childName: 'Asha' }, { kind: 'not_limited' }, { kind: 'none' }]);
    expect(() => readAccounts([{ ...row, link: { kind: 'maybe' } }])).toThrow(SeamError);
  });

  it('read a child’s rules whole, so they can be sent back whole', () => {
    const rules = { ...startingRules(), budgets: { ...startingRules().budgets, sunday: 'no_limit' }, blocked_hours: [{ days: ['friday'], start_minute: 1_260, end_minute: 420 }] };
    expect(readRuleSet(rules)).toEqual(rules);
    expect(() => readRuleSet({ ...rules, budgets: { ...rules.budgets, monday: { minutes: -1 } } })).toThrow(SeamError);
    expect(() => readRuleSet({ ...rules, blocked_hours: [{ days: ['someday'], start_minute: 0, end_minute: 60 }] })).toThrow(SeamError);
  });

  it('read a reply’s wait, and the rules’ own problems', () => {
    expect(readDoorAnswer({ outcome: 'problem', problem: { kind: 'wrong_password', message: 'That password is not right. Too many wrong tries: wait before trying again.', tries_before_wait: 0, wait_until: 1_800_000_300 } })).toEqual({
      kind: 'wrong',
      triesBeforeWait: 0,
      waitUntil: 1_800_000_300,
      words: 'That password is not right. Too many wrong tries: wait before trying again.',
    });
    expect(
      readDoorAnswer({
        outcome: 'problem',
        problem: { kind: 'rules', message: 'The rules were not taken. Each problem is said beside its field.', rule_problems: [{ field: 'blocked_hours[0].end_minute', message: 'Blocked hours number 1 starts and ends at 21:00.' }] },
      }),
    ).toEqual({
      kind: 'not-done',
      reason: 'rules',
      words: 'The rules were not taken. Each problem is said beside its field.',
      ruleProblems: [{ field: 'blocked_hours[0].end_minute', message: 'Blocked hours number 1 starts and ends at 21:00.' }],
    });
    expect(() => readDoorAnswer({ outcome: 'problem', problem: { kind: 'rules', message: 'x', rule_problems: [{ field: 'a' }] } })).toThrow(SeamError);
  });
});

describe('asking the door', () => {
  const door = (answer: DoorOutcome | (() => never)): GuardianDoor => ({
    ask: async () => (typeof answer === 'function' ? answer() : answer),
  });

  it('reads a done answer, and says so in words when it cannot', async () => {
    expect(await ask(door({ kind: 'done', value: { version: '0.1.0', protocol: 1, pc_name: 'FAMILY-PC' } }), 'about', {})).toEqual({
      kind: 'done',
      value: { version: '0.1.0', pcName: 'FAMILY-PC' },
    });
    expect(await ask(door({ kind: 'done', value: { has_family: 'maybe' } }), 'family_state', {})).toEqual({ kind: 'problem', code: 'misread', words: MISREAD });
  });

  it('passes the door’s own outcomes on, and never throws', async () => {
    const refused: DoorOutcome = { kind: 'refused', words: 'Not here.' };
    expect(await ask(door(refused), 'family_today', {})).toEqual(refused);
    const broken = door(() => {
      throw new Error('the pipe broke');
    });
    expect(await ask(broken, 'family_today', {})).toEqual({ kind: 'problem', code: 'failed', words: 'the pipe broke' });
  });

  it('the made up door answers in the same shapes, read by the same readers', async () => {
    const mock = guardianMock(guardianStates.twoChildren());
    const today = await ask(mock, 'family_today', {});
    expect(today.kind).toBe('done');
    const requests = await ask(mock, 'waiting_requests', {});
    expect(requests.kind === 'done' && requests.value[0].minutes).toBe(30);
    expect(await ask(mock, 'accounts', {})).toMatchObject({ kind: 'done' });
    const signedOut = guardianMock(guardianStates.signedOut());
    expect(await ask(signedOut, 'family_today', {})).toEqual({ kind: 'not-done', reason: 'signed_out', words: 'Signed out. Type the guardian password to go on.', ruleProblems: [] });
  });
});
