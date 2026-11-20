// The child's own app: Today (E), its banner (T), How it works (H), the
// floating tab bar, and long names.

import { act, fireEvent, render, screen, within } from '@testing-library/react-native';
import { StyleSheet } from 'react-native';

import { ChildApp } from '../src/child/ChildApp';
import { ChildHostProvider } from '../src/child/HostContext';
import * as W from '../src/child/words';
import { ALL_ON, ENGINE_OFF, LONG_CHILD, mockHost, states, TODAY } from '../src/mock/childMock';
import { tabStyles } from '../src/ui/paper/Tabs';
import { inPhone, renderChild } from './render';

describe('Today (E)', () => {
  it('shows the engine’s words as they come, and never says counting', async () => {
    renderChild(states.childApp);
    expect(await screen.findByText('1 h 12 min')).toBeOnTheScreen();
    expect(screen.getByText(W.today.left)).toBeOnTheScreen();
    expect(screen.getByText(W.today.quiet)).toBeOnTheScreen();
    expect(screen.getByText('48 min used of 2 h')).toBeOnTheScreen();
    expect(screen.getByText('On this phone 31 min · on the PC 17 min')).toBeOnTheScreen();
    expect(screen.getByText('Screens are off from 9:00 pm to 7:00 am.')).toBeOnTheScreen();
    for (const [name, minutes] of [['Maps', '12 min'], ['Chrome', '9 min'], ['Gallery', '6 min'], ['Clock', '4 min']]) {
      expect(screen.getByLabelText(`${name}, ${minutes}`)).toBeOnTheScreen();
    }
    expect(screen.getByText('Mira')).toBeOnTheScreen();
    expect(screen.queryByText(/counting/i)).toBeNull();
    expect(screen.queryByTestId('banner')).toBeNull();
  });

  it('keeps a time and its am or pm together, and a span and its unit', async () => {
    renderChild(states.childApp);
    const line = await screen.findByText('Screens are off from 9:00 pm to 7:00 am.');
    expect(line).toHaveTextContent('Screens are off from 9:00\u00a0pm to 7:00\u00a0am.', { normalizer: (s) => s });
    const where = screen.getByText('On this phone 31 min \u00b7 on the PC 17 min');
    expect(where).toHaveTextContent('On this phone 31\u00a0min \u00b7 on the PC 17\u00a0min', { normalizer: (s) => s });
  });

  it('shows a time left under a minute as the engine says it', async () => {
    renderChild(states.underAMinute);
    expect(await screen.findByText('0:42')).toBeOnTheScreen();
    expect(screen.getByText(W.today.left)).toBeOnTheScreen();
  });

  it.each([
    ['noLimit', 'No limit today'],
    ['openUntil', 'Open until 9:30 pm'],
  ] as const)('%s: the engine’s words, without “Left today”', async (state, words) => {
    renderChild(states[state]);
    expect(await screen.findByText(words)).toBeOnTheScreen();
    expect(screen.queryByText(W.today.left)).toBeNull();
  });

  it('asks for more time only by opening LiMo’s own ways out screen', async () => {
    const { host } = renderChild(states.childApp);
    fireEvent.press(await screen.findByRole('button', { name: W.today.ask }));
    await screen.findByText('1 h 12 min');
    expect(host.calls.filter((c) => c === 'askForMoreTime')).toHaveLength(1);
  });
});

describe('the banner (T)', () => {
  it('says what is switched off in the engine’s words and sends for a guardian, who can put it right', async () => {
    const { host } = renderChild(states.guardianNeeded);
    const banner = await screen.findByTestId('banner');
    expect(within(banner).getByText(W.today.needsGuardian)).toBeOnTheScreen();
    expect(within(banner).getByText('Accessibility is switched off, so LiMo cannot see which app is open.')).toBeOnTheScreen();
    expect(within(banner).queryByTestId('banner-more')).toBeNull();

    fireEvent.press(within(banner).getByRole('button', { name: W.today.showGuardian }));
    expect(await screen.findByText(W.steps.accessibility.why(''))).toBeOnTheScreen();
    fireEvent.press(screen.getByRole('button', { name: W.steps.accessibility.button }));
    expect(host.calls).toContain('openStep accessibility');
    fireEvent.press(await screen.findByRole('button', { name: 'Next' }));

    // Back on Today, and the banner is gone.
    expect(await screen.findByText('1 h 12 min')).toBeOnTheScreen();
    expect(screen.queryByTestId('banner')).toBeNull();
  });

  it('shows exactly the sentence the host gives, and holds none of its own', async () => {
    const host = mockHost(states.guardianNeeded);
    const said = Object.assign(Object.create(host) as typeof host, {
      today: async () => ({ ...TODAY, guardianNeeded: { off: ['accessibility'], words: ['Words the engine chose.'] } }),
    });
    render(
      inPhone(
        <ChildHostProvider host={said}>
          <ChildApp />
        </ChildHostProvider>,
      ),
    );
    const banner = await screen.findByTestId('banner');
    expect(within(banner).getByTestId('banner-words')).toHaveTextContent('Words the engine chose.');
    expect(within(banner).queryByText(ENGINE_OFF.accessibility)).toBeNull();
    for (const step of Object.values(W.steps)) expect(Object.keys(step)).not.toContain('banner');
  });

  it('with three off, says the first and how many more, and walks a guardian through all three', async () => {
    const { host } = renderChild(states.threeOff);
    const banner = await screen.findByTestId('banner');
    expect(within(banner).getByTestId('banner-words')).toHaveTextContent('Phone access is switched off, so a lock could hide a call.');
    expect(within(banner).getByTestId('banner-more')).toHaveTextContent('And 2 more');

    fireEvent.press(within(banner).getByRole('button', { name: W.today.showGuardian }));
    for (const id of ['phone', 'battery', 'protection'] as const) {
      expect(await screen.findByText(W.steps[id].title)).toBeOnTheScreen();
      fireEvent.press(screen.getByRole('button', { name: W.steps[id].button }));
      fireEvent.press(await screen.findByRole('button', { name: 'Next' }));
    }
    expect(await screen.findByText('1 h 12 min')).toBeOnTheScreen();
    expect(screen.queryByTestId('banner')).toBeNull();
    expect(host.calls.filter((c) => c.startsWith('openStep'))).toEqual(['openStep phone', 'openStep battery', 'openStep protection']);
  });

  it('with two off, says one more', async () => {
    renderChild({ ...states.childApp, steps: { ...ALL_ON, usage: 'off', battery: 'off' } });
    expect(await screen.findByTestId('banner-more')).toHaveTextContent('And 1 more');
  });

  it('comes when something is switched off while Today shows', async () => {
    const { host } = renderChild(states.childApp);
    await screen.findByText('1 h 12 min');
    act(() => host.set({ steps: { ...ALL_ON, usage: 'off', battery: 'off' } }, ['steps']));
    expect(await screen.findByText(ENGINE_OFF.usage)).toBeOnTheScreen();
  });

  it('lets a guardian go on past a step that will not turn on, back to Today, still naming it', async () => {
    renderChild({ ...states.childApp, steps: { ...ALL_ON, phone: 'off', battery: 'off' } }, { mock: { openTurnsOn: false } });
    fireEvent.press(await screen.findByRole('button', { name: W.today.showGuardian }));
    expect(await screen.findByText('Phone')).toBeOnTheScreen();
    for (let i = 0; i < 2; i += 1) {
      fireEvent.press(screen.getByRole('button', { name: W.steps.phone.button }));
      await act(async () => {});
    }
    fireEvent.press(screen.getByRole('button', { name: W.step.goOn }));
    // The next step that is off after this one, never back to the one gone past.
    expect(await screen.findByText('Battery')).toBeOnTheScreen();
    for (let i = 0; i < 2; i += 1) {
      fireEvent.press(screen.getByRole('button', { name: W.steps.battery.button }));
      await act(async () => {});
    }
    fireEvent.press(screen.getByRole('button', { name: W.step.goOn }));
    expect(await screen.findByText(ENGINE_OFF.phone)).toBeOnTheScreen();
    expect(screen.getByTestId('banner-more')).toHaveTextContent('And 1 more');
  });
});

describe('How it works (H) and the tab bar', () => {
  it('shows the child’s rules, what LiMo does, and the version', async () => {
    renderChild(states.childApp);
    fireEvent.press(await screen.findByRole('tab', { name: W.tabs.how }));
    expect(await screen.findByLabelText('Time a day: 2 h')).toBeOnTheScreen();
    expect(screen.getByLabelText('Screens off: 9:00 pm to 7:00 am')).toBeOnTheScreen();
    expect(screen.getByLabelText('Always open: Phone, Messages, Clock, Maps')).toBeOnTheScreen();
    expect(screen.getByLabelText('Not allowed: Game Launcher')).toBeOnTheScreen();
    expect(screen.getByText('What LiMo never looks at')).toBeOnTheScreen();
    expect(await screen.findByText('LiMo 0.1.0')).toBeOnTheScreen();
    expect(screen.getByText(W.how.repo)).toBeOnTheScreen();
    expect(screen.getByRole('tab', { name: W.tabs.how })).toBeSelected();
    expect(screen.getByRole('tab', { name: W.tabs.today })).not.toBeSelected();
  });

  it('keeps every tab the same size, picked or not: only colours change', async () => {
    renderChild(states.childApp);
    await screen.findByText('1 h 12 min');
    const layout = (name: string) => {
      const { backgroundColor: _bg, ...rest } = StyleSheet.flatten(screen.getByRole('tab', { name }).props.style) as Record<string, unknown>;
      return rest;
    };
    const before = { today: layout(W.tabs.today), how: layout(W.tabs.how) };
    expect(before.today).toEqual(before.how);
    fireEvent.press(screen.getByRole('tab', { name: W.tabs.how }));
    await screen.findByText(W.how.rules);
    expect(layout(W.tabs.today)).toEqual(before.today);
    expect(layout(W.tabs.how)).toEqual(before.how);
    // What the picked tab adds is a colour, nothing that takes room.
    expect(Object.keys(tabStyles.tabOn)).toEqual(['backgroundColor']);
  });
});

describe('long names', () => {
  it('fit everywhere a name goes', async () => {
    renderChild({ ...states.longNames, agreed: false, introSeen: false });
    expect(await screen.findByText(`This phone is now ${LONG_CHILD}’s.`)).toBeOnTheScreen();
    fireEvent.press(screen.getByRole('button', { name: 'Start' }));
    expect(await screen.findByRole('checkbox', { name: `I am ${LONG_CHILD}’s guardian, and I agree.` })).toBeOnTheScreen();
  });

  it('keep the name at the top on one line, and long app names whole', async () => {
    renderChild(states.longNames);
    const who = await screen.findByText(LONG_CHILD);
    expect(who.props.numberOfLines).toBe(1);
    expect(screen.getByText('Super Long Puzzle Adventure Deluxe Edition HD Remastered')).toBeOnTheScreen();
    expect(screen.getByLabelText('Super Long Puzzle Adventure Deluxe Edition HD Remastered, 1 hour 12 min')).toBeOnTheScreen();
    expect(screen.getByText(ENGINE_OFF.usage)).toBeOnTheScreen();
  });

  it('give the button the room it needs', async () => {
    renderChild({ ...states.longNames, introSeen: false, steps: states.allOn.steps }, { route: { at: 'done' } });
    expect(await screen.findByRole('button', { name: `Show ${LONG_CHILD} how it works` })).toBeOnTheScreen();
  });
});
