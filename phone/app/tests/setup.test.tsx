// Setting up a child's phone (S0 to S8) and the child's introduction (C1 to
// C3), against the mock host.

import { act, fireEvent, render, screen, waitFor } from '@testing-library/react-native';
import { AppState, type AppStateStatus } from 'react-native';

import { ChildApp } from '../src/child/ChildApp';
import * as W from '../src/child/words';
import { STEP_IDS, type StepId } from '../src/childSeam';
import { ALL_ON, CHILD, states, stepsOnUpTo } from '../src/mock/childMock';
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
  it('starts at the welcome, naming the child', async () => {
    renderChild(states.fresh);
    expect(await screen.findByText('This phone is now Mira’s.')).toBeOnTheScreen();
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

describe('the five steps (S3 to S7)', () => {
  it.each(STEP_IDS.map((id, i) => [id, i] as const))('%s: not on yet, then on when Android says so', async (id: StepId, i: number) => {
    const s = W.steps[id];
    const { host } = renderChild({ ...states.agreed, steps: stepsOnUpTo(i) }, { route: { at: 'step', step: id } });

    // Not on yet: the coral stamp, the line's room kept empty, the step's own button.
    expect(await screen.findByText(s.title)).toBeOnTheScreen();
    expect(screen.getByText(`Step ${i + 1} of 5`)).toBeOnTheScreen();
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

  it('shows a step on only when the host says Android says so, and asks again when the app is back in front', async () => {
    // React Native's jest setup makes AppState.addEventListener a mock: the
    // screen's listeners are in its calls.
    const addListener = AppState.addEventListener as unknown as jest.Mock;
    const before = addListener.mock.calls.length;

    // Opening the screen changes nothing by itself here: the guardian has not switched it on yet.
    const { host } = renderChild({ ...states.agreed, steps: stepsOnUpTo(1) }, { route: { at: 'step', step: 'usage' }, mock: { openTurnsOn: false } });
    await screen.findByText('Usage access');
    fireEvent.press(screen.getByRole('button', { name: W.steps.usage.button }));
    await act(async () => {});
    expect(screen.getByTestId('stamp-not-on-yet')).toBeOnTheScreen();

    // The guardian switches it on in Android's settings. The host says
    // nothing; the screen finds out when the app comes back to the front.
    host.set({ steps: stepsOnUpTo(2) }, []);
    const asked = host.calls.filter((c) => c === 'steps').length;
    const listeners = addListener.mock.calls.slice(before).filter(([type]) => type === 'change').map(([, l]) => l as (s: AppStateStatus) => void);
    expect(listeners.length).toBeGreaterThan(0);
    act(() => listeners.forEach((l) => l('active')));
    expect(await screen.findByTestId('stamp-on')).toBeOnTheScreen();
    expect(host.calls.filter((c) => c === 'steps').length).toBeGreaterThan(asked);
  });

  it('says so when Android’s own screen could not be opened, and keeps the room it had', async () => {
    renderChild({ ...states.agreed, steps: stepsOnUpTo(2) }, { route: { at: 'step', step: 'accessibility' }, mock: { openFails: true } });
    await screen.findByText('Accessibility');
    fireEvent.press(screen.getByRole('button', { name: W.steps.accessibility.button }));
    await waitFor(() => expect(screen.getByTestId('step-line')).toHaveTextContent(W.step.notOpened));
    expect(screen.getByTestId('stamp-not-on-yet')).toBeOnTheScreen();
  });

  it('starts at the first step that is not on', async () => {
    renderChild({ ...states.agreed, steps: stepsOnUpTo(3) });
    expect(await screen.findByText('Battery')).toBeOnTheScreen();
    expect(screen.getByText('Step 4 of 5')).toBeOnTheScreen();
    expect(screen.getByText('On a Samsung phone')).toBeOnTheScreen();
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
