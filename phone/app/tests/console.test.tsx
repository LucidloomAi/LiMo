// The guardian console's page: the sign in (G0) against a host of the
// test's own, the reader of the door's replies as the console hands them on,
// the console's place in the host seam, the page's guard against a browser's
// keys, and what the console's entry reaches.

import fs from 'node:fs';
import path from 'node:path';

import { act, fireEvent, render, screen } from '@testing-library/react-native';
import { StyleSheet } from 'react-native';

import { SeamError } from '../src/childSeam';
import { isBrowserKey, type KeyPress } from '../src/console/pageGuard';
import { ClockContext } from '../src/guardian/clock';
import { SignIn } from '../src/guardian/SignIn';
import * as W from '../src/guardian/words';
import { type DoorOutcome, type GuardianHost, readDoorAnswer, readDoorProblem } from '../src/guardianSeam';
import { guardianHost, host } from '../src/host';
import { NOW, PC_CLOCK } from '../src/mock/guardianMock';
import { inPhone, textsInOrder } from './render';

/** A door that answers about with `about`, and each sign in with the next of `answers`. */
function door(about: DoorOutcome, answers: DoorOutcome[] = []): GuardianHost & { typed: string[] } {
  const typed: string[] = [];
  return {
    typed,
    about: async () => about,
    signIn: async (password) => {
      typed.push(password);
      return answers.shift() ?? { kind: 'problem', code: 'lost', words: 'No more answers.' };
    },
  };
}

const OPEN: DoorOutcome = { kind: 'done', value: { version: '0.1.0', protocol: 1, pc_name: 'Family PC' } };
const WRONG = 'That password is not right. 4 more tries before a wait.';
const ASHA = 'LiMo’s settings cannot be opened from Asha’s account. A guardian opens them from their own.';

function renderSignIn(host: GuardianHost) {
  return render(
    inPhone(
      <ClockContext.Provider value={{ now: () => NOW, clock: PC_CLOCK }}>
        <SignIn host={host} />
      </ClockContext.Provider>,
    ),
  );
}

async function typeAndOpen(words: string) {
  fireEvent.changeText(await screen.findByTestId('password'), words);
  // The door answers a moment later: the press and that answer are one act.
  await act(async () => {
    fireEvent.press(screen.getByTestId('open'));
  });
}

describe('the sign in (G0)', () => {
  it('asks the door first, then shows one password field, the button, and what to do about a forgotten password last', async () => {
    renderSignIn(door(OPEN));
    expect(await screen.findByText(W.signIn.heading)).toBeOnTheScreen();
    expect(screen.getByLabelText(W.signIn.field)).toBeOnTheScreen();
    expect(screen.getByRole('button', { name: W.signIn.button })).toBeOnTheScreen();
    expect(screen.getByText(W.signIn.forgotten)).toBeOnTheScreen();
    const texts = textsInOrder();
    expect(texts.indexOf('LiMo')).toBeLessThan(texts.indexOf(W.signIn.heading));
    // As drawn (C0), the sign in has no maker's mark: it is said once, at the end of the first start.
    expect(texts[texts.length - 1]).toBe(W.signIn.forgotten);
  });

  it('shows the door’s own words for a wrong password, and empties the field', async () => {
    const d = door(OPEN, [{ kind: 'wrong', triesBeforeWait: 4, waitUntil: null, words: WRONG }]);
    renderSignIn(d);
    await typeAndOpen('not it');
    expect(await screen.findByText(WRONG)).toBeOnTheScreen();
    expect(d.typed).toEqual(['not it']);
    expect(screen.getByTestId('password').props.value).toBe('');
  });

  it('shows a wait at its time on the PC’s clock, a door that is busy in the door’s words, and keeps the field', async () => {
    renderSignIn(door(OPEN, [{ kind: 'wait', until: NOW + 5 * 60, words: 'Too many wrong tries. Wait before trying again.' }]));
    await typeAndOpen('no');
    expect(await screen.findByText('Too many wrong tries. Try again at 6:17 pm.')).toBeOnTheScreen();
    renderSignIn(door(OPEN, [{ kind: 'not-done', reason: 'busy', words: 'One thing at a time: try again in a moment.', ruleProblems: [] }]));
    await typeAndOpen('no');
    expect(await screen.findByText('One thing at a time: try again in a moment.')).toBeOnTheScreen();
    expect(screen.getByTestId('password')).toBeOnTheScreen();
  });

  it('opens on the right password', async () => {
    renderSignIn(door(OPEN, [{ kind: 'done', value: {} }]));
    await typeAndOpen('Mira’s long sentence');
    // With nothing to go on to (the guardian's flow passes onSignedIn), the field goes.
    expect(await screen.findByTestId('signed-in')).toBeOnTheScreen();
    expect(screen.queryByTestId('password')).toBeNull();
  });

  it('asks for the password before sending nothing', async () => {
    const d = door(OPEN);
    renderSignIn(d);
    fireEvent.press(await screen.findByTestId('open'));
    expect(await screen.findByText(W.signIn.empty)).toBeOnTheScreen();
    expect(d.typed).toEqual([]);
  });

  it('tells a child’s account at once, in the door’s words, with no password field', async () => {
    renderSignIn(door({ kind: 'refused', words: ASHA }));
    expect(await screen.findByText(ASHA)).toBeOnTheScreen();
    expect(screen.queryByTestId('password')).toBeNull();
    expect(screen.queryByRole('button')).toBeNull();
  });

  it('a refusal after a sign in also leaves no field', async () => {
    renderSignIn(door(OPEN, [{ kind: 'refused', words: ASHA }]));
    await typeAndOpen('Mira’s long sentence');
    expect(await screen.findByText(ASHA)).toBeOnTheScreen();
    expect(screen.queryByTestId('password')).toBeNull();
  });

  it('says plainly when the service is absent or lost, and offers nothing to press until it answers', async () => {
    const absent = 'LiMo’s service is not running on this PC, so its settings cannot open.';
    renderSignIn(door({ kind: 'problem', code: 'not_running', words: absent }));
    expect(await screen.findByText(absent)).toBeOnTheScreen();
    expect(screen.getByTestId('open').props.accessibilityState).toEqual(expect.objectContaining({ disabled: true }));
    const lost = 'LiMo’s service stopped answering. Try again: the console asks it afresh, and you sign in again.';
    renderSignIn(door(OPEN, [{ kind: 'problem', code: 'lost', words: lost }]));
    await typeAndOpen('Mira’s long sentence');
    expect(await screen.findByText(lost)).toBeOnTheScreen();
    expect(screen.getByTestId('password')).toBeOnTheScreen();
  });

  it('keeps two lines of room under the field, as the drawing does, so a message moves nothing', async () => {
    renderSignIn(door(OPEN));
    const room = await screen.findByTestId('message-room');
    expect(StyleSheet.flatten(room.props.style)).toEqual(expect.objectContaining({ minHeight: 40 }));
  });
});

describe('the door’s replies', () => {
  it('reads each reply the console hands on', () => {
    expect(readDoorAnswer({ outcome: 'done', value: { has_family: true } })).toEqual({ kind: 'done', value: { has_family: true } });
    expect(readDoorAnswer({ outcome: 'done', value: {} })).toEqual({ kind: 'done', value: {} });
    expect(readDoorAnswer({ outcome: 'problem', problem: { kind: 'wrong_password', message: WRONG, tries_before_wait: 4 } })).toEqual({
      kind: 'wrong',
      triesBeforeWait: 4,
      waitUntil: null,
      words: WRONG,
    });
    expect(readDoorAnswer({ outcome: 'problem', problem: { kind: 'must_wait', message: 'Wait.', wait_until: 1_800_000_000 } })).toEqual({
      kind: 'wait',
      until: 1_800_000_000,
      words: 'Wait.',
    });
    expect(readDoorAnswer({ outcome: 'problem', problem: { kind: 'signed_out', message: 'Sign in.' } })).toEqual({
      kind: 'not-done',
      reason: 'signed_out',
      words: 'Sign in.',
      ruleProblems: [],
    });
    expect(readDoorAnswer({ outcome: 'refused', message: ASHA })).toEqual({ kind: 'refused', words: ASHA });
  });

  it('refuses a shape it does not know, in one place', () => {
    for (const bad of [
      null,
      [],
      'done',
      { outcome: 'open_sesame' },
      { outcome: 'done' },
      { outcome: 'refused', message: '' },
      { outcome: 'problem', problem: { kind: 'wrong_password', message: WRONG } },
      { outcome: 'problem', problem: { kind: 'wrong_password', message: WRONG, tries_before_wait: 1.5 } },
      { outcome: 'problem', problem: { kind: 'must_wait', message: 'Wait.' } },
      { outcome: 'problem', problem: { kind: '', message: 'x' } },
      { outcome: 'problem', problem: { kind: 'busy' } },
    ]) {
      expect(() => readDoorAnswer(bad)).toThrow(SeamError);
    }
  });

  it('reads the console’s word that the door could not be asked', () => {
    expect(readDoorProblem({ code: 'not_running', words: 'Not running.' })).toEqual({ kind: 'problem', code: 'not_running', words: 'Not running.' });
    expect(readDoorProblem('nope')).toBeNull();
    expect(readDoorProblem({ code: 'x', words: '' })).toBeNull();
  });
});

describe('the console in the host seam', () => {
  afterEach(() => {
    delete (globalThis as Record<string, unknown>).__TAURI_INTERNALS__;
  });

  it('is not the place anywhere but the console’s window, where nothing answers a guardian yet', async () => {
    expect(host.place()).not.toBe('console');
    const outcome = await guardianHost.about();
    expect(outcome.kind).toBe('problem');
  });

  it('is found by Tauri’s bridge, and sends each request to the one command by the door’s name', async () => {
    const calls: { command: string; args: Record<string, unknown> }[] = [];
    (globalThis as Record<string, unknown>).__TAURI_INTERNALS__ = {
      invoke: async (command: string, args: Record<string, unknown>) => {
        calls.push({ command, args });
        if (args.name === 'about') return { outcome: 'done', value: { version: '0.1.0' } };
        if (args.name === 'more_info') return { opened: false, address: 'https://lucidloom.ai' };
        throw { code: 'not_the_service', words: 'Something that is not LiMo’s service answered.' };
      },
    };
    expect(host.place()).toBe('console');
    expect(await guardianHost.about()).toEqual({ kind: 'done', value: { version: '0.1.0' } });
    expect(await guardianHost.signIn('pw')).toEqual({ kind: 'problem', code: 'not_the_service', words: 'Something that is not LiMo’s service answered.' });
    expect(await host.request('console.moreInfo', {})).toEqual({ opened: false, address: 'https://lucidloom.ai' });
    expect(calls).toEqual([
      { command: 'request', args: { name: 'about', args: {} } },
      { command: 'request', args: { name: 'sign_in', args: { password: 'pw' } } },
      { command: 'request', args: { name: 'more_info', args: {} } },
    ]);
  });

  it('sends every other request of the guardian’s screens by the door’s own name, with its own arguments', async () => {
    const calls: { command: string; args: Record<string, unknown> }[] = [];
    (globalThis as Record<string, unknown>).__TAURI_INTERNALS__ = {
      invoke: async (command: string, args: Record<string, unknown>) => {
        calls.push({ command, args });
        if (args.name === 'family_today') return { outcome: 'done', value: [] };
        return { outcome: 'problem', problem: { kind: 'signed_out', message: 'Signed out. Type the guardian password to go on.' } };
      },
    };
    expect(await guardianHost.ask('family_today', {})).toEqual({ kind: 'done', value: [] });
    expect(await guardianHost.ask('lock_now', { child: 'child-1' })).toEqual({
      kind: 'not-done',
      reason: 'signed_out',
      words: 'Signed out. Type the guardian password to go on.',
      ruleProblems: [],
    });
    expect(calls).toEqual([
      { command: 'request', args: { name: 'family_today', args: {} } },
      { command: 'request', args: { name: 'lock_now', args: { child: 'child-1' } } },
    ]);
  });

  it('turns a reply it cannot read into words, never a sign in', async () => {
    (globalThis as Record<string, unknown>).__TAURI_INTERNALS__ = { invoke: async () => ({ answer: 'signed_in_maybe' }) };
    const outcome = await guardianHost.signIn('pw');
    expect(outcome.kind).toBe('problem');
  });
});

describe('the page’s guard', () => {
  const press = (key: string, mods: Partial<KeyPress> = {}): KeyPress => ({ key, ctrlKey: false, metaKey: false, altKey: false, shiftKey: false, ...mods });

  it('stops the browser’s keys', () => {
    for (const p of [press('F5'), press('F12'), press('F3'), press('F7'), press('p', { ctrlKey: true }), press('f', { ctrlKey: true }),
      press('R', { ctrlKey: true }), press('I', { ctrlKey: true, shiftKey: true }), press('ArrowLeft', { altKey: true }),
      press('+', { ctrlKey: true }), press('0', { ctrlKey: true }), press('BrowserBack')]) {
      expect(isBrowserKey(p)).toBe(true);
    }
  });

  it('leaves alone every key a guardian needs in a field', () => {
    for (const p of [press('a'), press('A', { shiftKey: true }), press('Tab'), press('Enter'), press('Backspace'),
      press('c', { ctrlKey: true }), press('v', { ctrlKey: true }), press('x', { ctrlKey: true }), press('a', { ctrlKey: true }),
      press('z', { ctrlKey: true }), press('ArrowLeft'), press('Home'), press('@', { ctrlKey: true, altKey: true })]) {
      expect(isBrowserKey(p)).toBe(false);
    }
  });
});

describe('the console’s entry', () => {
  // How Metro resolves a relative import for the web.
  const appDir = path.resolve(__dirname, '..');
  const EXTS = ['.web.tsx', '.web.ts', '.tsx', '.ts', '.js', '.json'];
  const IMPORT = /(?:import|export)\s[^'"]*?from\s*['"]([^'"]+)['"]|import\s*['"]([^'"]+)['"]|require\(\s*['"]([^'"]+)['"]\s*\)/g;
  const resolve = (from: string, spec: string) => {
    const base = path.resolve(path.dirname(from), spec);
    return [...EXTS.map((e) => base + e), ...EXTS.map((e) => path.join(base, 'index' + e)), base].find(
      (t) => fs.existsSync(t) && fs.statSync(t).isFile(),
    );
  };

  it('never reaches the mock, the preview, the child form’s screens or the tests', () => {
    const seen = new Set<string>();
    const todo = [path.join(appDir, 'console.tsx')];
    while (todo.length) {
      const file = todo.pop()!;
      if (seen.has(file) || !/\.(ts|tsx|js)$/.test(file)) continue;
      seen.add(file);
      for (const m of fs.readFileSync(file, 'utf8').matchAll(IMPORT)) {
        const spec = m[1] ?? m[2] ?? m[3];
        const target = spec.startsWith('.') ? resolve(file, spec) : undefined;
        if (target) todo.push(target);
      }
    }
    const rel = [...seen].map((f) => path.relative(appDir, f).replaceAll('\\', '/'));
    expect(rel).toEqual(
      expect.arrayContaining(['src/console/ConsoleApp.tsx', 'src/guardian/GuardianApp.tsx', 'src/guardian/SignIn.tsx', 'src/guardian/FirstStart.tsx', 'src/guardian/Today.tsx', 'src/host.ts']),
    );
    expect(rel.filter((f) => /^(src\/mock\/|src\/preview\/|tests\/|src\/child\/(ChildApp|SetupScreens|AppScreens))/.test(f))).toEqual([]);
  });
});
