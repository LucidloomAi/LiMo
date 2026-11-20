// Setting up a child's phone (S0 to S9) and the child's introduction (C1 to
// C3), against the mock host.

import { act, fireEvent, render, screen, waitFor } from '@testing-library/react-native';
import { AppState, type AppStateStatus, DeviceEventEmitter } from 'react-native';

import { ChildApp, TRIES_BEFORE_GOING_ON } from '../src/child/ChildApp';
import { ChildHostProvider } from '../src/child/HostContext';
import * as W from '../src/child/words';
import { STEP_IDS, type StepId } from '../src/childSeam';
import { ALL_ON, CHILD, ENGINE_OFF, mockHost, NOT_SAMSUNG, states, stepsOnUpTo } from '../src/mock/childMock';
import { inPhone, renderChild } from './render';

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
    expect(screen.getByLabelText('Accessibility')).toBeOnTheScreen();
    expect(screen.queryByLabelText('Accessibility, On')).toBeNull();
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
    fireEvent.press(screen.getByRole('button', { name: 'Show Mira how it works' }));

    expect(await screen.findByText('What LiMo counts')).toBeOnTheScreen();
    expect(screen.getByText('For Mira')).toBeOnTheScreen();
    expect(screen.getByLabelText('1 of 3')).toBeOnTheScreen();
    fireEvent.press(screen.getByRole('button', { name: 'Next' }));
    expect(await screen.findByText('What LiMo never looks at')).toBeOnTheScreen();
    fireEvent.press(screen.getByRole('button', { name: 'Next' }));
    expect(await screen.findByText('How to ask for more time')).toBeOnTheScreen();
    fireEvent.press(screen.getByRole('button', { name: 'Got it' }));

    expect(await screen.findByText('1 h 12 min')).toBeOnTheScreen();
    expect(host.calls).toContain('markIntroSeen');
    expect(host.state.introSeen).toBe(true);
  });

  it('ticks only the steps Android says are on now', async () => {
    renderChild({ ...states.allOn, steps: { ...ALL_ON, battery: 'off' } }, { route: { at: 'done' } });
    await screen.findByText(W.done.heading);
    expect(screen.getByLabelText('Battery')).toBeOnTheScreen();
    expect(screen.queryByLabelText('Battery, On')).toBeNull();
    expect(screen.getByLabelText('Usage access, On')).toBeOnTheScreen();
  });
});
