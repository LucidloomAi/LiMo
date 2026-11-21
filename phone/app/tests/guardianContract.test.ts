// The contract between the console's door and the guardian's screens: the
// door's real answers (tests/console-answers/, written by LiMo for Windows'
// test windows/tests/service/answers.rs from a real family file played
// through the real door) each pass the readers the screens read them with,
// unchanged.
//
// Every reply goes through readDoorAnswer, as the console hands it on; every
// answer to a request the screens make goes through that request's own
// reader, by asking for it as the screens do. A change on either side that
// breaks the other fails here. To refresh the files, run
// `cargo test --test service answers` in windows/, then `npm test`.

import fs from 'node:fs';
import path from 'node:path';

import { ask, type DoorName, type DoorRequests, type GuardianDoor, readDoorAnswer } from '../src/guardianSeam';

const DIR = path.resolve(__dirname, 'console-answers');

type AnswerFile = {
  request: string;
  case: string;
  asked: { name?: string; args?: Record<string, unknown> };
  answer: { say: string; reply?: unknown; message?: string };
};

const files: Record<string, AnswerFile> = Object.fromEntries(
  fs
    .readdirSync(DIR)
    .filter((f) => f.endsWith('.json'))
    .map((f) => [f.replace(/\.json$/, ''), JSON.parse(fs.readFileSync(path.join(DIR, f), 'utf8')) as AnswerFile]),
);

/** Every request the guardian's screens make, by the door's name. */
const SCREENS_ASK: readonly DoorName[] = [
  'about',
  'family_state',
  'sign_in',
  'sign_out',
  'create_family',
  'set_pin',
  'add_child',
  'rename_child',
  'set_age_band',
  'rules',
  'replace_rules',
  'accounts',
  'link_account',
  'unlink_account',
  'finish_first_start',
  'family_today',
  'waiting_requests',
  'answer_request',
  'give_extra',
  'lock_now',
  'unlock_until',
  'lift',
] satisfies readonly (keyof DoorRequests)[];

/** A door that gives one reply, as the console hands it on. */
const giving = (reply: unknown): GuardianDoor => ({ ask: async () => readDoorAnswer(reply) });

/** Asks for a request as the screens do, and gets back what the door said, read. */
function readAs<N extends DoorName>(name: N, file: AnswerFile) {
  return ask(giving(file.answer.reply), name, (file.asked.args ?? {}) as DoorRequests[N]['args']);
}

const answers = Object.entries(files).filter(([, f]) => f.answer.say === 'answer');

describe('the door’s answers (tests/console-answers)', () => {
  it('are there, the door’s own, for every request the screens make', () => {
    expect(answers.length).toBeGreaterThan(50);
    const asked = new Set(answers.map(([, f]) => f.request));
    expect(SCREENS_ASK.filter((n) => !asked.has(n))).toEqual([]);
  });

  it.each(answers.map(([name, f]) => [name, f] as const))('%s: the reply reads as the console hands it on', (_name, f) => {
    expect(() => readDoorAnswer(f.answer.reply)).not.toThrow();
  });

  const read = answers.filter(([, f]) => (SCREENS_ASK as readonly string[]).includes(f.request));
  it.each(read.map(([name, f]) => [name, f] as const))('%s: the screens read it with its own reader', async (_name, f) => {
    const got = await readAs(f.request as DoorName, f);
    // A reader that disagrees with the door turns the answer into the screens' "cannot read" problem.
    expect(got.kind === 'problem' && got.code === 'misread' ? `${f.request} misread` : 'read').toBe('read');
  });

  it('reads a child’s day, each device, and the request waiting, as the screens show them', async () => {
    const today = await readAs('family_today', files['family-today']);
    expect(today).toEqual({
      kind: 'done',
      value: [
        {
          childId: 'child-0000000000000001',
          name: 'Asha',
          budgetSecs: 3_600,
          extraSecs: 0,
          usedSecs: 302,
          leftSecs: 3_298,
          guardian: { mode: 'nothing' },
          devices: [{ deviceId: 'device-0000000000000001', name: 'Family PC', kind: 'on_master', lastHeardAt: 1_794_998_128, inUse: true, usedSecs: 302 }],
        },
      ],
    });
    const waiting = await readAs('waiting_requests', files['waiting-requests']);
    expect(waiting).toEqual({
      kind: 'done',
      value: [
        {
          childId: 'child-0000000000000001',
          childName: 'Asha',
          deviceId: 'device-0000000000000001',
          deviceName: 'Family PC',
          requestId: 'device-0000000000000001/ask/1',
          minutes: 20,
          reason: 'For the school project',
          askedAt: 1_794_998_128,
        },
      ],
    });
    expect(await readAs('waiting_requests', files['waiting-requests-none'])).toEqual({ kind: 'done', value: [] });
  });

  it('reads the first start’s answers', async () => {
    expect(await readAs('family_state', files['family-state-no-family'])).toMatchObject({ kind: 'done', value: { hasFamily: false, signedIn: false } });
    expect(await readAs('family_state', files['family-state'])).toMatchObject({ kind: 'done', value: { hasFamily: true, hasPin: true, children: 1, unlinkedAccounts: 'locked', signedIn: true } });
    expect(await readAs('add_child', files['add-child'])).toEqual({ kind: 'done', value: { childId: 'child-0000000000000001', name: 'Asha', ageBand: 'ten_to_twelve' } });
    const accounts = await readAs('accounts', files['accounts-first-start']);
    expect(accounts.kind === 'done' && accounts.value.map((a) => [a.displayName, a.isAdministrator, a.thisWindow, a.link.kind])).toEqual([
      ['guardian', true, true, 'none'],
      ['asha', false, false, 'none'],
      ['guest', false, false, 'none'],
    ]);
    const rules = await readAs('rules', files['rules-new-child']);
    expect(rules.kind === 'done' && rules.value.budgets.monday).toEqual({ minutes: 120 });
  });

  it('reads the rules’ own problems, each with its field', async () => {
    const got = await readAs('replace_rules', files['replace-rules-problems']);
    expect(got.kind).toBe('not-done');
    if (got.kind !== 'not-done') return;
    expect(got.reason).toBe('rules');
    expect(got.ruleProblems[0]).toEqual({ field: 'budgets.monday', message: 'Monday’s budget is 2000 minutes, more than a whole day. Use no limit instead.' });
    expect(got.ruleProblems.at(-1)?.field).toBe('blocked_hours[0].start_minute');
  });

  it('reads the guardian’s lock, unlock and lift', async () => {
    expect(await readAs('lock_now', files['lock-now'])).toEqual({ kind: 'done', value: { mode: 'lock', until: null } });
    expect(await readAs('unlock_until', files['unlock-until'])).toEqual({ kind: 'done', value: { mode: 'unlock', until: 1_795_005_000 } });
    expect(await readAs('lift', files.lift)).toEqual({ kind: 'done', value: { mode: 'nothing' } });
    expect(await readAs('give_extra', files['give-extra'])).toEqual({ kind: 'done', value: { givenNow: true } });
  });

  it('reads a wrong password, the wait it sets, the wait, a session ended and a child’s account, in the door’s words', async () => {
    expect(await readAs('sign_in', files['sign-in-wrong-password'])).toMatchObject({ kind: 'wrong', triesBeforeWait: 4, waitUntil: null });
    expect(await readAs('sign_in', files['sign-in-wrong-password-last-free'])).toMatchObject({ kind: 'wrong', triesBeforeWait: 1 });
    expect(await readAs('sign_in', files['sign-in-wrong-password-wait'])).toMatchObject({ kind: 'wrong', triesBeforeWait: 0, waitUntil: 1_794_998_222 });
    expect(await readAs('sign_in', files['sign-in-must-wait'])).toMatchObject({ kind: 'wait', until: 1_794_998_222 });
    expect(await readAs('family_today', files['family-today-signed-out'])).toMatchObject({ kind: 'not-done', reason: 'signed_out' });
    // The console hands on the door's refusal at hello as a reply of its own.
    const refused = files['refused-child-account'].answer;
    expect(readDoorAnswer({ outcome: 'refused', message: refused.message })).toEqual({
      kind: 'refused',
      words: 'LiMo’s settings cannot be opened from Asha’s account. A guardian opens them from their own.',
    });
  });

  it('takes the arguments the screens send, by the door’s own names', () => {
    // What the screens send for each request, beside what the door was sent.
    const SENT: Partial<Record<DoorName, readonly string[]>> = {
      sign_in: ['password'],
      create_family: ['password'],
      set_pin: ['pin'],
      add_child: ['age_band', 'name'],
      rename_child: ['child', 'name'],
      set_age_band: ['age_band', 'child'],
      rules: ['child'],
      replace_rules: ['child', 'rules'],
      link_account: ['account', 'link'],
      unlink_account: ['account'],
      answer_request: ['child', 'reply', 'request'],
      give_extra: ['child', 'minutes', 'tap_id'],
      unlock_until: ['child', 'until'],
      lift: ['child'],
    };
    for (const [name, keys] of Object.entries(SENT)) {
      const theirs = answers.filter(([, f]) => f.request === name).map(([, f]) => Object.keys(f.asked.args ?? {}).sort());
      expect(theirs.length).toBeGreaterThan(0);
      expect(theirs).toContainEqual([...keys].sort());
    }
    // A lock until lifted: the screens leave `until` out, which the door takes as none.
    expect(Object.keys(files['lock-now'].asked.args ?? {}).sort()).toEqual(['child', 'until']);
  });
});
