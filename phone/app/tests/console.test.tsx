// The guardian console's page: the sign in (G0) against a host of the
// test's own, the reader of the door's answers, the console's place in the
// host seam, the page's guard against a browser's keys, and what the
// console's entry reaches.

import fs from 'node:fs';
import path from 'node:path';

import { act, fireEvent, render, screen } from '@testing-library/react-native';
import { StyleSheet } from 'react-native';

import { isBrowserKey, type KeyPress } from '../src/console/pageGuard';
import { SignIn } from '../src/guardian/SignIn';
import * as W from '../src/guardian/words';
import { type DoorOutcome, type GuardianHost, readDoorAnswer, readDoorProblem } from '../src/guardianSeam';
import { guardianHost, host } from '../src/host';
import { SeamError } from '../src/childSeam';
import { inPhone, textsInOrder } from './render';

/** A door that answers about with `about`, and each sign in with the next of `answers`. */
function door(about: DoorOutcome, answers: DoorOutcome[] = []): GuardianHost & { typed: string[] } {
  const typed: string[] = [];
  return {
    typed,
    about: async () => about,
    signIn: async (password) => {
      typed.push(password);
      return answers.shift() ?? { kind: 'problem', code: 'broken', words: 'No more answers.' };
    },
  };
}

const FAMILY: DoorOutcome = { kind: 'about', family: true };

function renderSignIn(host: GuardianHost) {
  return render(inPhone(<SignIn host={host} timeOfDay={() => '18:42'} />));
}

async function typeAndOpen(words: string) {
  fireEvent.changeText(await screen.findByTestId('password'), words);
  // The door answers a moment later: the press and that answer are one act.
  await act(async () => {
    fireEvent.press(screen.getByTestId('open'));
  });
}

describe('the sign in (G0)', () => {
  it('asks who it is first, then shows one password field, the button, and the maker’s mark last', async () => {
    renderSignIn(door(FAMILY));
    expect(await screen.findByText(W.signIn.heading)).toBeOnTheScreen();
    expect(screen.getByLabelText(W.signIn.field)).toBeOnTheScreen();
    expect(screen.getByRole('button', { name: W.signIn.button })).toBeOnTheScreen();
    expect(screen.getByText(W.signIn.forgotten)).toBeOnTheScreen();
    const texts = textsInOrder();
    expect(texts.indexOf('LiMo')).toBeLessThan(texts.indexOf(W.signIn.heading));
    expect(texts[texts.length - 1]).toBe('LucidLoom AI');
  });

  it('says how many tries are left after a wrong password, and empties the field', async () => {
    const d = door(FAMILY, [{ kind: 'wrong', triesBeforeWait: 3 }]);
    renderSignIn(d);
    await typeAndOpen('not it');
    expect(await screen.findByText('That password is not right. 3 more tries before a wait.')).toBeOnTheScreen();
    expect(d.typed).toEqual(['not it']);
    expect(screen.getByTestId('password').props.value).toBe('');
  });

  it('words the last free try, and a wait with the time it ends', async () => {
    expect(W.signIn.wrong(1)).toBe('That password is not right. 1 more try before a wait.');
    expect(W.signIn.wrong(0)).toBe('That password is not right. The next wrong try starts a wait.');
    renderSignIn(door(FAMILY, [{ kind: 'wait', until: 1_800_000_000 }]));
    await typeAndOpen('no');
    expect(await screen.findByText('Too many wrong tries. Try again at 18:42.')).toBeOnTheScreen();
  });

  it('opens on the right password', async () => {
    renderSignIn(door(FAMILY, [{ kind: 'signed-in' }]));
    await typeAndOpen('Mira’s long sentence');
    expect(await screen.findByTestId('signed-in')).toBeOnTheScreen();
    expect(screen.getByText(W.signedIn.heading)).toBeOnTheScreen();
    expect(screen.queryByTestId('password')).toBeNull();
  });

  it('asks for the password before sending nothing', async () => {
    const d = door(FAMILY);
    renderSignIn(d);
    fireEvent.press(await screen.findByTestId('open'));
    expect(await screen.findByText(W.signIn.empty)).toBeOnTheScreen();
    expect(d.typed).toEqual([]);
  });

  it('tells a child’s account at once, with no password field', async () => {
    renderSignIn(door({ kind: 'child-account', child: 'Asha' }));
    expect(
      await screen.findByText('LiMo’s settings cannot be opened from Asha’s account. A guardian opens them from their own.'),
    ).toBeOnTheScreen();
    expect(screen.queryByTestId('password')).toBeNull();
    expect(screen.queryByRole('button')).toBeNull();
  });

  it('says plainly when the door cannot be asked, and offers nothing to press', async () => {
    renderSignIn(door({ kind: 'problem', code: 'not_running', words: 'LiMo’s service is not running on this PC, so its settings cannot open.' }));
    expect(await screen.findByText('LiMo’s service is not running on this PC, so its settings cannot open.')).toBeOnTheScreen();
    expect(screen.getByTestId('open').props.accessibilityState).toEqual(expect.objectContaining({ disabled: true }));
    renderSignIn(door({ kind: 'unknown-account' }));
    expect(await screen.findByText(W.notForThisAccount.unknown)).toBeOnTheScreen();
  });

  it('keeps three lines of room under the field, so a message moves nothing', async () => {
    renderSignIn(door(FAMILY));
    const room = await screen.findByTestId('message-room');
    expect(StyleSheet.flatten(room.props.style)).toEqual(expect.objectContaining({ minHeight: 60 }));
  });
});

describe('the door’s answers', () => {
  it('reads each answer the door gives', () => {
    expect(readDoorAnswer({ answer: 'about', service: 'x', version: '0.1.0', family: false })).toEqual({ kind: 'about', family: false });
    expect(readDoorAnswer({ answer: 'signed_in' })).toEqual({ kind: 'signed-in' });
    expect(readDoorAnswer({ answer: 'wrong_password', tries_before_wait: 2 })).toEqual({ kind: 'wrong', triesBeforeWait: 2 });
    expect(readDoorAnswer({ answer: 'must_wait', until: 1_800_000_000 })).toEqual({ kind: 'wait', until: 1_800_000_000 });
    expect(readDoorAnswer({ answer: 'refused', why: { refusal: 'child_account', child: 'Asha' } })).toEqual({ kind: 'child-account', child: 'Asha' });
    expect(readDoorAnswer({ answer: 'refused', why: { refusal: 'unknown_account' } })).toEqual({ kind: 'unknown-account' });
  });

  it('refuses a shape it does not know, in one place', () => {
    for (const bad of [null, [], 'signed_in', { answer: 'open_sesame' }, { answer: 'wrong_password', tries_before_wait: -1 },
      { answer: 'wrong_password', tries_before_wait: 1.5 }, { answer: 'refused', why: { refusal: 'child_account', child: '' } },
      { answer: 'refused', why: { refusal: 'later' } }, { answer: 'about' }]) {
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
        if (args.name === 'about') return { answer: 'about', service: 's', version: 'v', family: true };
        throw { code: 'not_the_service', words: 'Something that is not LiMo’s service answered.' };
      },
    };
    expect(host.place()).toBe('console');
    expect(await guardianHost.about()).toEqual({ kind: 'about', family: true });
    expect(await guardianHost.signIn('pw')).toEqual({ kind: 'problem', code: 'not_the_service', words: 'Something that is not LiMo’s service answered.' });
    expect(calls).toEqual([
      { command: 'request', args: { name: 'about', args: {} } },
      { command: 'request', args: { name: 'sign_in', args: { password: 'pw' } } },
    ]);
  });

  it('turns an answer it cannot read into words, never a sign in', async () => {
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
    expect(rel).toEqual(expect.arrayContaining(['src/console/ConsoleApp.tsx', 'src/guardian/SignIn.tsx', 'src/host.ts']));
    expect(rel.filter((f) => /^(src\/mock\/|src\/preview\/|tests\/|src\/child\/(ChildApp|SetupScreens|AppScreens))/.test(f))).toEqual([]);
  });
});
