// Add a phone, A1 to A9 (design/screens/console.html), against the made up
// console's desk for a phone (src/mock/phoneMock.ts) behind the made up
// door: the cable in each of its cases, the whole way from plugging in to
// Devices, and what goes wrong on the way. The look is asked once a second,
// so these tests run on jest's clock.

import { act, fireEvent, screen, within } from '@testing-library/react-native';
import { StyleSheet } from 'react-native';

import { LOOK_EVERY_MS } from '../src/guardian/AddPhone';
import type { GuardianRoute } from '../src/guardian/GuardianApp';
import * as W from '../src/guardian/words';
import { guardianStates } from '../src/mock/guardianMock';
import { ASK_WAITS_SECS, connected, FINGERPRINT, freshCable, GALAXY, type MockCable, OLD_PHONE, PHONE_WORDS } from '../src/mock/phoneMock';
import { GUARDIAN_FRAMES } from '../src/preview/guardianFrames';
import { asked, press, renderGuardian } from './guardianRender';
import type { TreeNode } from './render';

const A = W.addPhone;
const route = (step = 0, child?: string): GuardianRoute => ({ at: 'app', tab: 'devices', sub: { at: 'add-phone', step, child } });

/** One second of the console's clock: one look, and what it starts. */
async function second(times = 1) {
  for (let i = 0; i < times; i += 1) {
    await act(async () => {
      jest.advanceTimersByTime(LOOK_EVERY_MS);
    });
    await act(async () => {});
  }
}

async function open(cable: MockCable, at = 0, child: string | undefined = 'child-asha') {
  const got = renderGuardian(guardianStates.adding(cable), { route: route(at, child) });
  await act(async () => {});
  await act(async () => {});
  return got;
}

async function go() {
  await act(async () => {
    fireEvent.press(screen.getByTestId('go'));
  });
  await act(async () => {});
}

const strip = () => [screen.getByTestId('cable-line').props.children, screen.queryByTestId('cable-more')?.props.children ?? ''];
const stepNow = () => Number(/screen-add-phone-(\d)/.exec((screen.root.findAll((n: TreeNode) => typeof n.props.testID === 'string' && n.props.testID.startsWith('screen-add-phone-')) as TreeNode[])[0].props.testID as string)?.[1]);

beforeEach(() => jest.useFakeTimers());
afterEach(() => jest.useRealTimers());

describe('the cable’s state at the foot of every step, in the console’s words', () => {
  it.each([
    ['none', ...['No phone on a USB port.', 'If it is plugged in, the cable may only charge it. Try the one that came with the phone.']],
    ['off', 'A phone is plugged in.', 'USB debugging is off on it.'],
    ['driver', 'No working driver for this phone.', 'Install its maker’s USB driver for Windows, such as Samsung’s Android USB Driver.'],
    ['connecting', 'A phone is plugged in.', 'Connecting to it.'],
    ['unknown', 'A phone is plugged in.', 'It does not know this PC yet.'],
    ['allow', 'The phone is asking.', 'Waiting for Allow on the phone.'],
    ['looking', 'Looking at this PC’s USB ports.', ''],
  ] as const)('%s', async (cableCase, line, more) => {
    await open(freshCable({ frozen: true, case: cableCase }), 1);
    expect(strip()).toEqual([line, more]);
  });

  it('connected, in teal; a problem, in coral', async () => {
    await open(connected(GALAXY, { frozen: true }), 3);
    expect(strip()).toEqual(['Connected: Galaxy S20 Ultra.', 'Android 13.']);
    expect(screen.getByTestId('cable-line')).toHaveStyle({ color: '#0e7c66' });
  });

  it('another program holding the phone: its words, in coral, and the step waits', async () => {
    await open(freshCable({ frozen: true, case: 'trouble', trouble: PHONE_WORDS.heldElsewhere }), 1);
    expect(strip()).toEqual([PHONE_WORDS.heldElsewhere.line, PHONE_WORDS.heldElsewhere.more]);
    expect(screen.getByTestId('cable-line')).toHaveStyle({ color: '#cf4326' });
    await second(3);
    expect(stepNow()).toBe(2);
    expect(screen.getByTestId('go')).toBeDisabled();
  });
});

describe('the whole way', () => {
  it('from a phone plugged in to Devices with the new phone, each step moving on by itself as the look says', async () => {
    const { door } = await open(freshCable({ plan: ['none', 'off', 'connecting', 'unknown'] }));
    // A1: whose phone, and what is needed.
    expect(screen.getByRole('radio', { name: 'Asha' })).toBeChecked();
    expect(screen.getByText('Asha’s phone')).toBeOnTheScreen();
    expect(screen.getByText(A.a1.cableText)).toBeOnTheScreen();
    expect(screen.getByText('Step 1 of 9')).toBeOnTheScreen();
    expect(strip()[0]).toBe('No phone on a USB port.');
    await go();
    // A2: waits for USB debugging.
    expect(stepNow()).toBe(2);
    expect(screen.getByTestId('go')).toBeDisabled();
    await second();
    expect(strip()[1]).toBe('USB debugging is off on it.');
    expect(stepNow()).toBe(2);
    await second();
    // Connecting: still A2.
    expect(stepNow()).toBe(2);
    await second();
    // A3: the phone does not know this PC, and nothing was asked of it; the key, and the button that asks.
    expect(stepNow()).toBe(3);
    expect(screen.getByTestId('unlock')).toHaveTextContent(A.a3.unlock);
    expect(screen.getByText(A.a3.once)).toBeOnTheScreen();
    expect(within(screen.getByTestId('fingerprint')).getByText('3C:91:5A:07:E2:4B:D8:16:\nAF:62:0C:B9:75:18:4E:D3')).toBeOnTheScreen();
    expect(screen.getByTestId('fingerprint').props.accessibilityLabel).toBe(FINGERPRINT);
    expect(screen.getByRole('button', { name: A.a3.ask })).toBeEnabled();
    await second(3);
    // Never asked without the button.
    expect(asked(door, 'phone.ask')).toHaveLength(0);
    await go();
    expect(asked(door, 'phone.ask')).toHaveLength(1);
    await second();
    // Asked: waiting for Allow, the button off and saying so.
    expect(screen.getByText(A.a3.line)).toBeOnTheScreen();
    expect(screen.getByRole('button', { name: A.a3.waiting })).toBeDisabled();
    await second();
    // A4: connected; the checks, then LiMo goes on the phone, asked once.
    expect(stepNow()).toBe(4);
    expect(strip()).toEqual(['Connected: Galaxy S20 Ultra.', 'Android 13.']);
    expect(asked(door, 'phone.install')).toHaveLength(1);
    expect(within(screen.getByTestId('checks')).getByText('Android 13, recent enough for LiMo')).toBeOnTheScreen();
    await second();
    expect(screen.getByTestId('sent').props.accessibilityValue).toMatchObject({ now: 50 });
    await second();
    // Sent: Android installing, with no figure.
    expect(screen.getByTestId('installing')).toHaveTextContent(PHONE_WORDS.installing);
    await second();
    // A5: the pairing, asked for the child chosen at A1.
    expect(stepNow()).toBe(5);
    expect(asked(door, 'phone.pair')[0].args).toEqual({ child: 'child-asha' });
    expect(screen.getByTestId('go')).toBeDisabled();
    await second(3);
    expect(screen.getByText(A.a5.heading('Asha'))).toBeOnTheScreen();
    expect(screen.getByText(A.a5.paired)).toBeOnTheScreen();
    expect(screen.getByTestId('pairing-line')).toHaveTextContent(PHONE_WORDS.overCable);
    expect(screen.getByTestId('phone-name').props.value).toBe('Asha’s Galaxy S20 Ultra');
    await act(async () => {
      fireEvent.changeText(screen.getByTestId('phone-name'), 'Asha’s Galaxy');
    });
    await go();
    const id = asked(door, 'rename_device')[0].args.device as string;
    expect(asked(door, 'rename_device')[0].args).toEqual({ device: id, name: 'Asha’s Galaxy' });
    // A6: the phone's own steps, from the service, ticking as the phone says.
    expect(stepNow()).toBe(6);
    await second(2);
    expect(asked(door, 'device').at(-1)?.args).toEqual({ device: id });
    expect(within(screen.getByTestId('six')).getAllByLabelText(new RegExp(`: ${W.devices.on}$`)).length).toBeGreaterThan(0);
    await second(6);
    // A7: reached over the home Wi-Fi.
    expect(stepNow()).toBe(7);
    expect(screen.getByText(A.a7.reached)).toBeOnTheScreen();
    await go();
    // A8: USB debugging off, done by the console.
    expect(stepNow()).toBe(8);
    expect(screen.getByText(A.a8.byHand)).toBeOnTheScreen();
    await go();
    expect(asked(door, 'phone.debugging_off')).toHaveLength(1);
    await second();
    // A9: every tick, and the strip says what is plugged in.
    expect(stepNow()).toBe(9);
    await second();
    for (const words of [A.a9.ticks.installed, A.a9.ticks.paired, A.a9.ticks.setUp, A.a9.ticks.home, A.a9.ticks.off]) {
      expect(within(screen.getByTestId('added')).getByLabelText(words)).toBeOnTheScreen();
    }
    expect(strip()[1]).toBe('USB debugging is off on it.');
    // Done: Devices, the devices asked afresh, the new phone there, the phone let go.
    const before = asked(door, 'devices').length;
    await go();
    expect(screen.getByTestId('screen-devices')).toBeOnTheScreen();
    expect(screen.getByTestId(`phone-${id}`)).toBeOnTheScreen();
    expect(asked(door, 'devices').length).toBeGreaterThan(before);
    expect(asked(door, 'phone.done')).toHaveLength(1);
  });

  it('asks the look once a second while open, and not after it closes', async () => {
    const { door } = await open(freshCable({ frozen: true }));
    const first = asked(door, 'phone.look').length;
    await second(4);
    expect(asked(door, 'phone.look').length).toBe(first + 4);
    await act(async () => {
      fireEvent.press(screen.getByRole('button', { name: W.steps.back }));
    });
    expect(asked(door, 'phone.done')).toHaveLength(1);
    const after = asked(door, 'phone.look').length;
    await second(3);
    expect(asked(door, 'phone.look').length).toBe(after);
  });

  it('a phone connected already: Start goes straight to putting LiMo on it', async () => {
    await open(connected(GALAXY));
    await second();
    await go();
    expect(stepNow()).toBe(4);
  });

  it('Back to a step that waits: it stays, and its button goes on', async () => {
    await open(connected(GALAXY, { frozen: true }), 3);
    await act(async () => {
      fireEvent.press(screen.getByRole('button', { name: W.steps.back }));
    });
    await second();
    expect(stepNow()).toBe(3);
    expect(screen.getByTestId('go')).toBeEnabled();
    await go();
    expect(stepNow()).toBe(4);
  });
});

describe('step 3: the phone is asked only when the button says so', () => {
  it('a phone that already knows this PC never sees the button', async () => {
    const { door } = await open(freshCable({ plan: ['off', 'ok'] }), 1);
    await second(2);
    expect(stepNow()).toBe(4);
    expect(asked(door, 'phone.ask')).toHaveLength(0);
  });

  it('a phone locked when asked shows nothing: after 20 seconds, what to do; gone and back, the button returns, and asked again it answers', async () => {
    const { door } = await open(freshCable({ case: 'unknown', locked: true }), 2);
    await go();
    await second(19);
    expect(screen.getByRole('button', { name: A.a3.waiting })).toBeDisabled();
    expect(screen.queryByTestId('nothing-on-the-phone')).toBeNull();
    await second(2);
    const help = screen.getByTestId('nothing-on-the-phone');
    expect(within(help).getByText(A.a3.helpLabel)).toBeOnTheScreen();
    expect(within(help).getByText(A.a3.help)).toBeOnTheScreen();
    expect(stepNow()).toBe(3);
    // The cure: USB debugging off and on again on the phone, which the PC sees as the phone gone and back.
    door.state.cable.locked = false;
    door.state.cable.plan = ['off', 'unknown'];
    await second(2);
    expect(screen.queryByTestId('nothing-on-the-phone')).toBeNull();
    expect(screen.getByRole('button', { name: A.a3.ask })).toBeEnabled();
    await go();
    expect(asked(door, 'phone.ask')).toHaveLength(2);
    await second(2);
    expect(stepNow()).toBe(4);
  });

  it('Allow given late, after the help showed: on it goes', async () => {
    await open(freshCable({ case: 'unknown', allowAfter: 25 }), 2);
    await go();
    await second(21);
    expect(screen.getByTestId('nothing-on-the-phone')).toBeOnTheScreen();
    await second(4);
    expect(stepNow()).toBe(4);
  });

  it('an ask nobody answered in ten minutes: the window’s words, and the button asks again', async () => {
    const { door } = await open(freshCable({ case: 'unknown', locked: true }), 2);
    await go();
    door.state.cable.askedSecs = ASK_WAITS_SECS - 1;
    await second();
    expect(strip()).toEqual([PHONE_WORDS.notAllowedInTime.line, PHONE_WORDS.notAllowedInTime.more]);
    expect(screen.queryByTestId('nothing-on-the-phone')).toBeNull();
    door.state.cable.locked = false;
    await press(A.a3.ask);
    expect(asked(door, 'phone.ask')).toHaveLength(2);
    await second(2);
    expect(stepNow()).toBe(4);
  });

  it('the button asks once; asked again while waiting, nothing more is sent', async () => {
    const { door } = await open(freshCable({ case: 'unknown', locked: true }), 2);
    await go();
    await second();
    await go();
    expect(asked(door, 'phone.ask')).toHaveLength(1);
  });
});

describe('what goes wrong on the way', () => {
  it('a phone too old: said in the strip and the checks, and nothing is put on it', async () => {
    const { door } = await open(freshCable({ plugs: OLD_PHONE, plan: ['ok'] }), 3);
    await second(2);
    expect(strip()[0]).toBe('This phone has Android 7.1. LiMo needs Android 8.0 or later.');
    expect(within(screen.getByTestId('checks')).getByLabelText(`Android 7.1, older than the Android 8.0 LiMo needs: ${W.devices.notYet}`)).toBeOnTheScreen();
    expect(asked(door, 'phone.install')).toHaveLength(0);
    expect(screen.getByTestId('go')).toBeDisabled();
  });

  it('an install that fails, said in the console’s words, and tried again', async () => {
    const { door } = await open(connected(GALAXY, { installFails: 1 }), 3);
    await second(4);
    const trouble = screen.getByTestId('install-trouble');
    expect(within(trouble).getByText(PHONE_WORDS.notInTime[0])).toBeOnTheScreen();
    expect(within(trouble).getByText(PHONE_WORDS.notInTime[1])).toBeOnTheScreen();
    expect(stepNow()).toBe(4);
    await act(async () => {
      fireEvent.press(within(trouble).getByRole('button', { name: A.tryAgain }));
    });
    expect(asked(door, 'phone.install')).toHaveLength(2);
    await second(3);
    expect(stepNow()).toBe(5);
  });

  it('the cable pulled in the middle of the install', async () => {
    const { door } = await open(connected(GALAXY), 3);
    await second();
    expect(asked(door, 'phone.install')).toHaveLength(1);
    door.state.cable.case = 'none';
    await second();
    expect(strip()[0]).toBe('No phone on a USB port.');
    expect(within(screen.getByTestId('install-trouble')).getByText(PHONE_WORDS.lost[0])).toBeOnTheScreen();
  });

  it('a pairing whose offer runs out, asked again', async () => {
    const withLimo = { ...GALAXY, limo: { looked: true, on_it: true, version: GALAXY.this_pc_carries } };
    const { door } = await open(connected(withLimo, { offerRunsOut: 1, install: { stage: 'done', sent: null, line: PHONE_WORDS.installed, advice: null } }), 4);
    await second(4);
    const trouble = screen.getByTestId('pair-trouble');
    expect(within(trouble).getByText(PHONE_WORDS.ranOut[0])).toBeOnTheScreen();
    await act(async () => {
      fireEvent.press(within(trouble).getByRole('button', { name: A.tryAgain }));
    });
    expect(asked(door, 'phone.pair')).toHaveLength(2);
    await second(3);
    expect(screen.getByText(A.a5.paired)).toBeOnTheScreen();
  });

  it('USB debugging off failed: the console’s words and the way by hand; left on, A9 says so', async () => {
    const state = connected(GALAXY, { debuggingFails: true });
    const { door } = await open(state, 7);
    await go();
    expect(asked(door, 'phone.debugging_off')).toHaveLength(1);
    await second();
    const trouble = screen.getByTestId('debugging-trouble');
    expect(within(trouble).getByText(PHONE_WORDS.stillOn[0])).toBeOnTheScreen();
    expect(within(trouble).getByText(PHONE_WORDS.stillOn[1])).toBeOnTheScreen();
    await act(async () => {
      fireEvent.press(screen.getByTestId('leave-it-on'));
    });
    expect(stepNow()).toBe(9);
    expect(within(screen.getByTestId('added')).getByLabelText(`${A.a9.ticks.on}: ${W.devices.notYet}`)).toBeOnTheScreen();
  });

  it('a request the console refuses: its words under the step', async () => {
    await open(freshCable({ frozen: true, case: 'ok', phone: GALAXY, busy: 'pairing' }), 3);
    await second();
    expect(screen.getByTestId('add-phone-message')).toHaveTextContent(PHONE_WORDS.busy);
  });
});

describe('for a finger and a screen reader', () => {
  const frames = GUARDIAN_FRAMES.filter((f) => /^A\d[uh]?$/.test(f.id));
  const cases = frames.flatMap((f) => [[f.id, 'a phone’s width, 48', false, 48] as const, [f.id, 'the console’s width, 40', true, 40] as const]);
  it.each(cases)('%s: everything to tap has a role, a label and its height at %s', async (id, _what, wide, least) => {
    const frame = frames.find((f) => f.id === id)!;
    renderGuardian(frame.state(), { route: frame.route, wide });
    await act(async () => {});
    await act(async () => {});
    const taps = screen.root.findAll((n: TreeNode) => typeof n.type === 'string' && typeof n.props.onResponderRelease === 'function') as TreeNode[];
    expect(taps.length).toBeGreaterThan(1);
    for (const t of taps) {
      const style = StyleSheet.flatten(t.props.style) as { minHeight?: number; height?: number };
      expect(t.props.accessibilityRole).toEqual(expect.stringMatching(/^(button|checkbox|radio|tab)$/));
      expect(String(t.props.accessibilityLabel ?? '').length).toBeGreaterThan(0);
      expect(Math.max(style.minHeight ?? 0, style.height ?? 0)).toBeGreaterThanOrEqual(least);
    }
  });
});