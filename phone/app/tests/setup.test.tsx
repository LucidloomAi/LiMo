// Setting up a child's phone (S0 to S9) and the child's introduction (C1 to
// C3), against the mock host.

import fs from 'node:fs';
import path from 'node:path';

import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react-native';
import { AppState, type AppStateStatus, DeviceEventEmitter, StyleSheet } from 'react-native';
import Svg, { Circle } from 'react-native-svg';

import { ChildApp, TRIES_BEFORE_GOING_ON } from '../src/child/ChildApp';
import { ChildHostProvider } from '../src/child/HostContext';
import * as W from '../src/child/words';
import { STEP_IDS, type StepId } from '../src/childSeam';
import { Icon } from '../src/icons/Icon';
import { ALL_ON, CHILD, ENGINE_OFF, type MockState, mockHost, NOT_SAMSUNG, states, stepsOnUpTo } from '../src/mock/childMock';
import { color, monoFamily } from '../src/theme';
import { type } from '../src/ui/paper/text';
import { inOrder, inPhone, renderChild, textsInOrder, type TreeNode } from './render';

describe('not set up (S0)', () => {
  it('shows only how to start from the PC, with nothing to tap', async () => {
    renderChild(states.notPaired);
    expect(await screen.findByText(W.notSetUp.heading)).toBeOnTheScreen();
    expect(screen.getByText(/Add a phone/)).toBeOnTheScreen();
    expect(screen.queryAllByRole('button')).toHaveLength(0);
    expect(screen.getByLabelText('Made by LucidLoom AI')).toBeOnTheScreen();
  });

  it('is what the app shows where no native side answers yet', async () => {
    // No provider: the child form asks the real host, which finds no child module here.
    render(inPhone(<ChildApp />));
    expect(await screen.findByText(W.notSetUp.heading)).toBeOnTheScreen();
  });

  it('keeps LiMo’s name at the top and stands the card in the middle, above the maker’s mark', async () => {
    renderChild(states.notPaired);
    await screen.findByText(W.notSetUp.heading);
    inOrder(textsInOrder(), ['LiMo', W.notSetUp.heading, 'Made by', 'LucidLoom AI']);

    // The middle takes all the room between the top row and the maker's mark
    // and centres the card in it, under the drawing's 20 of padding.
    const middle = screen.getByTestId('not-set-up-middle');
    expect(StyleSheet.flatten(middle.props.style)).toEqual({ flexGrow: 1, justifyContent: 'center', paddingTop: 20 });
    expect(within(middle).getByText(W.notSetUp.heading)).toBeOnTheScreen();
    expect(within(middle).queryByText('LiMo')).toBeNull();
    expect(within(middle).queryByLabelText('Made by LucidLoom AI')).toBeNull();

    // The card sits in it with no margin of its own, so its middle is the middle's.
    const card = middle.findAll((n: TreeNode) => typeof n.type === 'string' && n !== middle)[0];
    const look = StyleSheet.flatten(card.props.style) ?? {};
    expect(look.marginTop ?? 0).toBe(0);
    expect(look.marginBottom ?? 0).toBe(0);

    // The middle is the page's only part that grows, besides what holds it: the maker's mark stays at the bottom.
    const grows = (n: TreeNode) => typeof n.type === 'string' && (StyleSheet.flatten(n.props.style)?.flexGrow ?? 0) > 0;
    const others = (screen.root.findAll(grows) as TreeNode[]).filter((n) => n !== middle);
    expect(others.every((n) => n.findAll((m) => m === middle).length > 0)).toBe(true);
  });

  it('moves on to the welcome once the PC pairs the phone', async () => {
    const { host } = renderChild(states.notPaired);
    await screen.findByText(W.notSetUp.heading);
    act(() => host.set({ pairing: { paired: true, childName: CHILD } }, ['pairing']));
    expect(await screen.findByText(W.welcome.heading(CHILD))).toBeOnTheScreen();
  });
});

describe('welcome and agreeing (S1, S2)', () => {
  it('starts at the welcome, naming the child, and says there are six steps', async () => {
    renderChild(states.fresh);
    expect(await screen.findByText('This phone is now Mira’s.')).toBeOnTheScreen();
    expect(screen.getByText('Six short steps finish the setup. Each one asks Android for one thing, and says why.')).toBeOnTheScreen();
    fireEvent.press(screen.getByRole('button', { name: 'Start' }));
    expect(await screen.findByText(W.agree.heading)).toBeOnTheScreen();
  });

  it('keeps Continue switched off until the box is ticked', async () => {
    const { host } = renderChild(states.fresh, { route: { at: 'agree' } });
    const cont = await screen.findByRole('button', { name: 'Continue' });
    expect(cont).toBeDisabled();
    fireEvent.press(cont);
    await act(async () => {});
    expect(host.calls).not.toContain('agree');
    expect(screen.getByText(W.agree.heading)).toBeOnTheScreen();

    const box = screen.getByRole('checkbox', { name: 'I am Mira’s guardian, and I agree.' });
    expect(box).not.toBeChecked();
    fireEvent.press(box);
    expect(screen.getByRole('checkbox')).toBeChecked();
    expect(screen.getByRole('button', { name: 'Continue' })).toBeEnabled();

    // Unticked again, switched off again.
    fireEvent.press(screen.getByRole('checkbox'));
    expect(screen.getByRole('button', { name: 'Continue' })).toBeDisabled();
    fireEvent.press(screen.getByRole('checkbox'));

    fireEvent.press(screen.getByRole('button', { name: 'Continue' }));
    expect(await screen.findByText('Notifications')).toBeOnTheScreen();
    expect(host.calls).toContain('agree');
    expect(host.state.agreed).toBe(true);
  });
});

describe('the six steps (S3 to S8)', () => {
  it('asks in this order: notifications, phone, usage access, accessibility, battery, protection', () => {
    expect(STEP_IDS).toEqual(['notifications', 'phone', 'usage', 'accessibility', 'battery', 'protection']);
  });

  it.each(STEP_IDS.map((id, i) => [id, i] as const))('%s: not on yet, then on when Android says so', async (id: StepId, i: number) => {
    const s = W.steps[id];
    const { host } = renderChild({ ...states.agreed, steps: stepsOnUpTo(i) }, { route: { at: 'step', step: id } });

    // Not on yet: the coral stamp, the line's room kept empty, the step's own button.
    expect(await screen.findByText(s.title)).toBeOnTheScreen();
    expect(screen.getByText(`Step ${i + 1} of 6`)).toBeOnTheScreen();
    expect(screen.getByTestId('stamp-not-on-yet')).toBeOnTheScreen();
    expect(screen.queryByTestId('stamp-on')).toBeNull();
    expect(screen.getByTestId('step-line')).toHaveTextContent('');
    const button = screen.getByRole('button', { name: s.button });

    // The button opens Android's own screen for this step, and nothing else.
    fireEvent.press(button);
    expect(host.calls).toContain(`openStep ${id}`);

    // Android says it is on: the teal stamp, its line, and Next.
    expect(await screen.findByTestId('stamp-on')).toBeOnTheScreen();
    expect(screen.queryByTestId('stamp-not-on-yet')).toBeNull();
    expect(screen.getByTestId('step-line')).toHaveTextContent(W.step.onLine);
    expect(screen.getByRole('button', { name: 'Next' })).toBeOnTheScreen();
  });

  it('phone: says why, what Android asks, and that LiMo never makes a call', async () => {
    const { host } = renderChild({ ...states.agreed, steps: stepsOnUpTo(1) }, { route: { at: 'step', step: 'phone' } });
    expect(await screen.findByText('Phone')).toBeOnTheScreen();
    expect(screen.getByText('Step 2 of 6')).toBeOnTheScreen();
    expect(screen.getByText('So a lock never hides a call. LiMo is told that a call is ringing or on, never who is calling.')).toBeOnTheScreen();
    // "Allow" in semibold, as on the other steps: the same words.
    expect(W.steps.phone.what).toBe(W.steps.notifications.what);
    expect(screen.getByLabelText('Android asks once. Choose Allow.')).toBeOnTheScreen();
    expect(screen.getByText('What Android asks')).toBeOnTheScreen();
    expect(
      screen.getByText('Android’s words are “make and manage phone calls”. LiMo never makes a call. It only learns that one is ringing or on.'),
    ).toBeOnTheScreen();
    // A handset, not S1's picture of the device.
    expect(W.steps.phone.icon).toBe('call');
    expect(screen.getByTestId('stamp-not-on-yet')).toBeOnTheScreen();

    fireEvent.press(screen.getByRole('button', { name: 'Allow' }));
    expect(host.calls).toContain('openStep phone');
    expect(await screen.findByTestId('stamp-on')).toBeOnTheScreen();
    fireEvent.press(screen.getByRole('button', { name: 'Next' }));
    expect(await screen.findByText('Usage access')).toBeOnTheScreen();
  });

  it('shows a step on only when the host says Android says so, and asks again when the app is back in front', async () => {
    // React Native's jest setup makes AppState.addEventListener a mock: the
    // screen's listeners are in its calls.
    const addListener = AppState.addEventListener as unknown as jest.Mock;
    const before = addListener.mock.calls.length;

    // Opening the screen changes nothing by itself here: the guardian has not switched it on yet.
    const { host } = renderChild({ ...states.agreed, steps: stepsOnUpTo(2) }, { route: { at: 'step', step: 'usage' }, mock: { openTurnsOn: false } });
    await screen.findByText('Usage access');
    fireEvent.press(screen.getByRole('button', { name: W.steps.usage.button }));
    await act(async () => {});
    expect(screen.getByTestId('stamp-not-on-yet')).toBeOnTheScreen();

    // The guardian switches it on in Android's settings. The host says
    // nothing; the screen finds out when the app comes back to the front.
    host.set({ steps: stepsOnUpTo(3) }, []);
    const asked = host.calls.filter((c) => c === 'steps').length;
    const listeners = addListener.mock.calls.slice(before).filter(([type]) => type === 'change').map(([, l]) => l as (s: AppStateStatus) => void);
    expect(listeners.length).toBeGreaterThan(0);
    act(() => listeners.forEach((l) => l('active')));
    expect(await screen.findByTestId('stamp-on')).toBeOnTheScreen();
    expect(host.calls.filter((c) => c === 'steps').length).toBeGreaterThan(asked);
  });

  it('says so when Android’s own screen could not be opened, and keeps the room it had', async () => {
    renderChild({ ...states.agreed, steps: stepsOnUpTo(3) }, { route: { at: 'step', step: 'accessibility' }, mock: { openFails: true } });
    await screen.findByText('Accessibility');
    fireEvent.press(screen.getByRole('button', { name: W.steps.accessibility.button }));
    await waitFor(() => expect(screen.getByTestId('step-line')).toHaveTextContent(W.step.notOpened));
    expect(screen.getByTestId('stamp-not-on-yet')).toBeOnTheScreen();
  });

  it('protection: says why in words that are true of the base protection', async () => {
    renderChild({ ...states.agreed, steps: stepsOnUpTo(5) }, { route: { at: 'step', step: 'protection' } });
    expect(await screen.findByText('Protection')).toBeOnTheScreen();
    expect(screen.getByText('So a guardian’s PIN is needed to switch this off, and LiMo cannot be removed while it is on.')).toBeOnTheScreen();
    expect(screen.queryByText(/without the guardian PIN/)).toBeNull();
  });

  it('starts at the first step that is not on', async () => {
    renderChild({ ...states.agreed, steps: stepsOnUpTo(4) });
    expect(await screen.findByText('Battery')).toBeOnTheScreen();
    expect(screen.getByText('Step 5 of 6')).toBeOnTheScreen();
  });
});

describe('the Samsung note on the Battery step', () => {
  const battery = { ...states.agreed, steps: stepsOnUpTo(4) };

  it('shows when the host says the phone is a Samsung', async () => {
    const { host } = renderChild(battery, { route: { at: 'step', step: 'battery' } });
    expect(await screen.findByText('On a Samsung phone')).toBeOnTheScreen();
    expect(screen.getByText(W.steps.battery.note!.text)).toBeOnTheScreen();
    expect(host.calls).toContain('maker');
  });

  it('names the path as newer and older Samsung phones have it', async () => {
    renderChild(battery, { route: { at: 'step', step: 'battery' } });
    expect(
      await screen.findByText(
        'Also open Settings, then Battery (on some phones Battery and device care, then Battery), then Background usage limits, then Never sleeping apps, and add LiMo there.',
      ),
    ).toBeOnTheScreen();
  });

  it('does not show on another maker’s phone', async () => {
    renderChild({ ...battery, maker: NOT_SAMSUNG }, { route: { at: 'step', step: 'battery' } });
    expect(await screen.findByText('Battery')).toBeOnTheScreen();
    expect(screen.queryByText('On a Samsung phone')).toBeNull();
    expect(screen.queryByTestId('step-note')).toBeNull();
  });

  it('does not show when the host cannot say who made the phone', async () => {
    const host = mockHost(battery);
    const noMaker = Object.assign(Object.create(host) as typeof host, { maker: () => Promise.reject(new Error('no answer')) });
    render(
      inPhone(
        <ChildHostProvider host={noMaker}>
          <ChildApp initial={{ at: 'step', step: 'battery' }} />
        </ChildHostProvider>,
      ),
    );
    expect(await screen.findByText('Battery')).toBeOnTheScreen();
    expect(screen.queryByText('On a Samsung phone')).toBeNull();
  });

  it('is the only note that waits for the maker: the Phone step’s note shows on every phone', async () => {
    renderChild({ ...states.agreed, steps: stepsOnUpTo(1), maker: NOT_SAMSUNG }, { route: { at: 'step', step: 'phone' } });
    expect(await screen.findByText('What Android asks')).toBeOnTheScreen();
  });
});

describe('a step Android will not turn on', () => {
  const at = (id: StepId) => ({ ...states.agreed, steps: stepsOnUpTo(STEP_IDS.indexOf(id)) });
  const open = async (id: StepId) => {
    fireEvent.press(screen.getByRole('button', { name: W.steps[id].button }));
    await act(async () => {});
  };

  it.each(STEP_IDS.map((id) => [id] as const))('%s: after two tries still off, offers to go on, and setup goes on', async (id: StepId) => {
    const { host } = renderChild(at(id), { route: { at: 'step', step: id }, mock: { openTurnsOn: false } });
    await screen.findByText(W.steps[id].title);
    expect(TRIES_BEFORE_GOING_ON).toBe(2);

    // Once: nothing yet.
    await open(id);
    expect(screen.getByTestId('step-line')).toHaveTextContent('');
    expect(screen.queryByRole('button', { name: W.step.goOn })).toBeNull();

    // Twice: the line, and the outlined way on; the step's own button stays to try again.
    await open(id);
    expect(screen.getByTestId('step-line')).toHaveTextContent('Android has not turned this on. You can go on and come back to it from Today.');
    expect(screen.getByTestId('stamp-not-on-yet')).toBeOnTheScreen();
    expect(screen.getByRole('button', { name: W.steps[id].button })).toBeOnTheScreen();
    fireEvent.press(screen.getByRole('button', { name: 'Go on without it' }));

    const i = STEP_IDS.indexOf(id);
    if (i + 1 < STEP_IDS.length) expect(await screen.findByText(W.steps[STEP_IDS[i + 1]].title)).toBeOnTheScreen();
    else expect(await screen.findByText(W.done.heading)).toBeOnTheScreen();
    expect(host.calls.filter((c) => c === `openStep ${id}`)).toHaveLength(2);
    expect(host.state.steps[id]).toBe('off');
  });

  it('counts a try only when the step comes back off', async () => {
    const { host } = renderChild(at('usage'), { route: { at: 'step', step: 'usage' }, mock: { openTurnsOn: false } });
    await screen.findByText('Usage access');
    await open('usage');
    // Switched on, then off again, before the second try comes back: one try still off, not two.
    host.set({ steps: stepsOnUpTo(3) }, []);
    await open('usage');
    expect(screen.getByTestId('stamp-on')).toBeOnTheScreen();
    act(() => host.set({ steps: stepsOnUpTo(2) }, ['steps']));
    await waitFor(() => expect(screen.getByTestId('stamp-not-on-yet')).toBeOnTheScreen());
    expect(screen.queryByRole('button', { name: W.step.goOn })).toBeNull();
    await open('usage');
    expect(screen.getByRole('button', { name: W.step.goOn })).toBeOnTheScreen();
  });

  it('turns into Next if Android turns the step on after all', async () => {
    const { host } = renderChild(at('battery'), { route: { at: 'step', step: 'battery' }, mock: { openTurnsOn: false } });
    await screen.findByText('Battery');
    await open('battery');
    await open('battery');
    expect(screen.getByRole('button', { name: W.step.goOn })).toBeOnTheScreen();
    act(() => host.set({ steps: stepsOnUpTo(5) }, ['steps']));
    expect(await screen.findByTestId('stamp-on')).toBeOnTheScreen();
    expect(screen.getByTestId('step-line')).toHaveTextContent(W.step.onLine);
    expect(screen.queryByRole('button', { name: W.step.goOn })).toBeNull();
  });

  it('also offers it when Android’s screen will not even open', async () => {
    renderChild(at('protection'), { route: { at: 'step', step: 'protection' }, mock: { openFails: true } });
    await screen.findByText('Protection');
    await open('protection');
    expect(screen.getByTestId('step-line')).toHaveTextContent(W.step.notOpened);
    await open('protection');
    expect(screen.getByTestId('step-line')).toHaveTextContent(W.step.stuck);
    expect(screen.getByRole('button', { name: W.step.goOn })).toBeOnTheScreen();
  });

  it('keeps its tries when a guardian comes back to it, and Today’s banner names it after setup', async () => {
    const { host } = renderChild(at('accessibility'), { route: { at: 'step', step: 'accessibility' }, mock: { openTurnsOn: false } });
    await screen.findByText('Accessibility');
    await open('accessibility');
    await open('accessibility');
    fireEvent.press(screen.getByRole('button', { name: W.step.goOn }));
    expect(await screen.findByText('Battery')).toBeOnTheScreen();

    // Android's back button: the step again, its way on still offered.
    act(() => {
      DeviceEventEmitter.emit('hardwareBackPress');
    });
    expect(await screen.findByText('Accessibility')).toBeOnTheScreen();
    expect(screen.getByTestId('step-line')).toHaveTextContent(W.step.stuck);
    fireEvent.press(screen.getByRole('button', { name: W.step.goOn }));
    expect(await screen.findByText('Battery')).toBeOnTheScreen();

    // Battery and protection come on; then done, the introduction, and Today.
    host.set({ steps: { ...ALL_ON, accessibility: 'off' } }, []);
    fireEvent.press(screen.getByRole('button', { name: W.steps.battery.button }));
    fireEvent.press(await screen.findByRole('button', { name: 'Next' }));
    fireEvent.press(await screen.findByRole('button', { name: 'Next' }));
    expect(await screen.findByText(W.done.heading)).toBeOnTheScreen();
    expect(screen.getByLabelText('Accessibility, Not on')).toBeOnTheScreen();
    expect(screen.queryByLabelText('Accessibility, On')).toBeNull();
    expect(screen.getByTestId('done-off-line')).toHaveTextContent(
      'Accessibility is not on yet. Mira’s Today screen shows a guardian how to turn it on.',
    );
    fireEvent.press(screen.getByRole('button', { name: 'Show Mira how it works' }));
    fireEvent.press(await screen.findByRole('button', { name: 'Next' }));
    fireEvent.press(await screen.findByRole('button', { name: 'Next' }));
    fireEvent.press(await screen.findByRole('button', { name: 'Got it' }));
    expect(await screen.findByText(ENGINE_OFF.accessibility)).toBeOnTheScreen();
  });
});

describe('done, and the child’s introduction (S8, C1 to C3)', () => {
  it('goes from the last step to done, through the three screens, to Today', async () => {
    const { host } = renderChild({ ...states.agreed, steps: ALL_ON }, { route: { at: 'step', step: 'protection' } });
    fireEvent.press(await screen.findByRole('button', { name: 'Next' }));

    expect(await screen.findByText(W.done.heading)).toBeOnTheScreen();
    for (const id of STEP_IDS) expect(screen.getByLabelText(`${W.steps[id].title}, On`)).toBeOnTheScreen();
    expect(screen.getByText('Hand the phone to Mira. The next three screens are Mira’s.')).toBeOnTheScreen();
    // Setup is not finished on the phone until the guardian leaves this screen.
    expect(host.calls).not.toContain('finishSetup');
    fireEvent.press(screen.getByRole('button', { name: 'Show Mira how it works' }));

    expect(await screen.findByText('What LiMo counts')).toBeOnTheScreen();
    expect(host.calls.filter((c) => c === 'finishSetup')).toHaveLength(1);
    expect(screen.getByText('For Mira')).toBeOnTheScreen();
    expect(screen.getByLabelText('1 of 3')).toBeOnTheScreen();
    fireEvent.press(screen.getByRole('button', { name: 'Next' }));
    expect(await screen.findByText('What LiMo never looks at')).toBeOnTheScreen();
    fireEvent.press(screen.getByRole('button', { name: 'Next' }));
    expect(await screen.findByText('How to ask for more time')).toBeOnTheScreen();
    fireEvent.press(screen.getByRole('button', { name: 'Got it' }));

    expect(await screen.findByText('1 h 12 min')).toBeOnTheScreen();
    expect(host.calls).toContain('markIntroSeen');
    expect(host.calls.indexOf('finishSetup')).toBeLessThan(host.calls.indexOf('markIntroSeen'));
    expect(host.state.introSeen).toBe(true);
  });

  it('finishes setup when a step was passed too, and shows the introduction if the phone does not answer', async () => {
    const { host } = renderChild(
      { ...states.agreed, steps: { ...ALL_ON, battery: 'off' } },
      { route: { at: 'done' }, mock: { finishFails: true } },
    );
    await screen.findByText(W.done.heading);
    fireEvent.press(screen.getByRole('button', { name: 'Show Mira how it works' }));
    expect(await screen.findByText('What LiMo counts')).toBeOnTheScreen();
    expect(host.calls).toContain('finishSetup');
  });

  it('ticks only the steps Android says are on now', async () => {
    renderChild({ ...states.allOn, steps: { ...ALL_ON, battery: 'off' } }, { route: { at: 'done' } });
    await screen.findByText(W.done.heading);
    expect(screen.getByLabelText('Battery, Not on')).toBeOnTheScreen();
    expect(screen.queryByLabelText('Battery, On')).toBeNull();
    expect(screen.getByLabelText('Usage access, On')).toBeOnTheScreen();
  });
});

describe('the redrawn words, word for word as the drawing has them', () => {
  const drawing = fs.readFileSync(path.resolve(__dirname, '..', '..', '..', 'design', 'screens', 'phone-setup.js'), 'utf8');

  it.each([
    ['the Samsung note on Battery', W.steps.battery.note!.text],
    ['why Protection', W.steps.protection.why('Mira')],
    ['NOT ON', `>${W.done.notOn}<`],
    ['how many are off', ' steps are not on yet. '],
    ['one that is off', ' is not on yet. '],
    ['where to turn them on', '’s Today screen shows a guardian how to turn '],
    ['the About box', `<p>${W.how.more}<span class="su-addr">${W.how.site}</span></p>`],
  ])('%s', (_what, words) => {
    expect(drawing).toContain(words);
  });

  it('neither holds the old words any more', () => {
    for (const old of ['without the guardian PIN', 'Free and open source, under the Apache 2.0 licence.', 'github.com/LucidloomAi/LiMo']) {
      expect(drawing).not.toContain(old);
    }
  });
});

describe('done with a step still off (S9b)', () => {
  const offOnDone = (off: readonly StepId[], state: MockState = states.allOn) => {
    const steps = { ...ALL_ON };
    for (const id of off) steps[id] = 'off';
    return renderChild({ ...state, steps }, { route: { at: 'done' } });
  };
  /** The ring's one circle, inside a row's mark. */
  const ringIn = (id: StepId) => within(screen.getByTestId(`done-${id}-off`)).UNSAFE_getByType(Circle);

  it('one off: a hollow ink ring in place of its tick, NOT ON at its end, and one sentence before the hand over', async () => {
    offOnDone(['phone']);
    await screen.findByText(W.done.heading);

    // The title and the big tick stay: the phone works.
    expect(screen.getByText('This phone is set up.')).toBeOnTheScreen();
    expect(screen.UNSAFE_getAllByType(Icon).map((i) => i.props.name)).toContain('check');

    // The ring, in the tick's own room, and no tick there.
    expect(screen.queryByTestId('done-phone-on')).toBeNull();
    const ring = ringIn('phone');
    expect(ring.props).toMatchObject({ cx: 9, cy: 9, r: 5.5, stroke: color.ink, strokeWidth: 2 });
    expect(within(screen.getByTestId('done-phone-off')).UNSAFE_getByType(Svg).props.fill).toBe('none');
    for (const id of STEP_IDS.filter((s) => s !== 'phone')) {
      expect(screen.getByTestId(`done-${id}-on`)).toBeOnTheScreen();
      expect(screen.queryByTestId(`done-${id}-off`)).toBeNull();
    }

    // NOT ON at the row's end: mono capitals in coral, said plainly, no box.
    const notOn = within(screen.getByTestId('done-phone')).getByTestId('done-not-on');
    expect(notOn).toHaveTextContent('Not on');
    expect(screen.getAllByTestId('done-not-on')).toHaveLength(1);
    const look = StyleSheet.flatten(notOn.props.style);
    expect(look).toMatchObject({ fontFamily: monoFamily, fontSize: 12, textTransform: 'uppercase', color: color.coral, marginLeft: 'auto' });
    expect(look.borderWidth).toBeUndefined();
    expect(screen.getByLabelText('Phone, Not on')).toBeOnTheScreen();

    // One sentence, after the rows and before the hand over.
    expect(screen.getByTestId('done-off-line')).toHaveTextContent('Phone is not on yet. Mira’s Today screen shows a guardian how to turn it on.');
    inOrder(textsInOrder(), [
      'Protection',
      'Phone is not on yet. Mira’s Today screen shows a guardian how to turn it on.',
      'Hand the phone to Mira. The next three screens are Mira’s.',
    ]);
    expect(StyleSheet.flatten(screen.getByTestId('done-off-line').props.style)).toMatchObject({ fontSize: 16, lineHeight: 22.4, color: color.ink, marginTop: 10 });
  });

  it('two off: a ring and NOT ON on each, and the sentence counts them in words', async () => {
    offOnDone(['phone', 'battery']);
    await screen.findByText(W.done.heading);
    expect(ringIn('phone')).toBeTruthy();
    expect(ringIn('battery')).toBeTruthy();
    expect(screen.getAllByTestId('done-not-on')).toHaveLength(2);
    expect(screen.getByTestId('done-off-line')).toHaveTextContent('Two steps are not on yet. Mira’s Today screen shows a guardian how to turn them on.');
  });

  it('says the number in words up to six', () => {
    const words = ['Two', 'Three', 'Four', 'Five', 'Six'];
    words.forEach((word, i) => {
      expect(W.done.offLine(STEP_IDS.slice(0, i + 2), 'Asha')).toBe(
        `${word} steps are not on yet. Asha’s Today screen shows a guardian how to turn them on.`,
      );
    });
    expect(W.done.offLine(['usage'], 'Asha')).toBe('Usage access is not on yet. Asha’s Today screen shows a guardian how to turn it on.');
  });

  it('with every step off, still says the phone is set up, and six', async () => {
    offOnDone([...STEP_IDS]);
    await screen.findByText(W.done.heading);
    expect(screen.getAllByTestId('done-not-on')).toHaveLength(6);
    expect(screen.getByTestId('done-off-line')).toHaveTextContent('Six steps are not on yet. Mira’s Today screen shows a guardian how to turn them on.');
  });

  it('with no name kept, says “your child” as the hand over does, with a capital where the sentence starts', async () => {
    // The child module answers so when the pairing keeps no name (tests/seam-answers/pairing-no-name.json).
    offOnDone(['battery'], { ...states.allOn, pairing: { paired: true, childName: 'your child' } });
    await screen.findByText(W.done.heading);
    expect(screen.getByTestId('done-off-line')).toHaveTextContent('Battery is not on yet. Your child’s Today screen shows a guardian how to turn it on.');
    expect(screen.getByText('Hand the phone to your child. The next three screens are your child’s.')).toBeOnTheScreen();
    expect(W.done.offLine(['battery', 'protection'], 'your child')).toBe(
      'Two steps are not on yet. Your child’s Today screen shows a guardian how to turn them on.',
    );
  });

  it('keeps every row the same height whether ticked or not', async () => {
    offOnDone(['phone', 'protection']);
    await screen.findByText(W.done.heading);
    const rowStyle = (id: StepId) => StyleSheet.flatten(screen.getByTestId(`done-${id}`).props.style);
    const markStyle = (id: StepId) => StyleSheet.flatten(screen.getByTestId(`done-${id}-${id === 'phone' || id === 'protection' ? 'off' : 'on'}`).props.style);
    for (const id of STEP_IDS) {
      expect(rowStyle(id)).toEqual(rowStyle('notifications'));
      // The tick and the ring stand in the same 18 by 18.
      expect(markStyle(id)).toEqual({ width: 18, height: 18 });
    }
    // NOT ON is never taller than the step's name beside it, so it adds no height.
    const notOn = StyleSheet.flatten(screen.getAllByTestId('done-not-on')[0].props.style);
    expect(notOn.lineHeight).toBeLessThanOrEqual(type.app.lineHeight as number);
  });

  it('with all six on, is as it was: six ticks, no ring, no NOT ON, no sentence', async () => {
    renderChild(states.allOn, { route: { at: 'done' } });
    await screen.findByText(W.done.heading);
    for (const id of STEP_IDS) {
      expect(screen.getByTestId(`done-${id}-on`)).toBeOnTheScreen();
      expect(within(screen.getByTestId(`done-${id}`)).UNSAFE_queryAllByType(Circle)).toHaveLength(0);
    }
    expect(screen.queryByTestId('done-not-on')).toBeNull();
    expect(screen.queryByTestId('done-off-line')).toBeNull();
    // Every word on the screen, in order: the same as before a step could be off.
    expect(textsInOrder()).toEqual([
      'LiMo',
      'This phone is set up.',
      'Notifications',
      'Phone',
      'Usage access',
      'Accessibility',
      'Battery',
      'Protection',
      'Hand the phone to Mira. The next three screens are Mira’s.',
      'Made by',
      'LucidLoom AI',
      'Show Mira how it works',
    ]);
  });

  it('waits for Android’s first answer, and marks nothing when the host cannot say', async () => {
    const waiting = mockHost({ ...states.allOn, steps: { ...ALL_ON, phone: 'off' } });
    const never = Object.assign(Object.create(waiting) as typeof waiting, { steps: () => new Promise<never>(() => undefined) });
    const { unmount } = render(
      inPhone(
        <ChildHostProvider host={never}>
          <ChildApp initial={{ at: 'done' }} />
        </ChildHostProvider>,
      ),
    );
    await act(async () => {});
    expect(screen.queryByText(W.done.heading)).toBeNull();
    unmount();

    const failing = Object.assign(Object.create(waiting) as typeof waiting, { steps: () => Promise.reject(new Error('no answer')) });
    render(
      inPhone(
        <ChildHostProvider host={failing}>
          <ChildApp initial={{ at: 'done' }} />
        </ChildHostProvider>,
      ),
    );
    expect(await screen.findByText(W.done.heading)).toBeOnTheScreen();
    for (const id of STEP_IDS) expect(screen.getByTestId(`done-${id}-none`)).toBeOnTheScreen();
    expect(screen.queryByTestId('done-not-on')).toBeNull();
    expect(screen.queryByTestId('done-off-line')).toBeNull();
  });
});
