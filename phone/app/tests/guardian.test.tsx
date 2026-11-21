// The guardian's screens (src/guardian/) against the made up family behind a
// made up door (src/mock/guardianMock.ts), which answers by the door's own
// request names in the hub's own shapes: sign in (C0 to C0d), the first start
// (F1 to F8), Today (T1 to T3), and which one shows when. At a phone's width
// (390 by 844) unless a test says the console's (1100 by 760).

import fs from 'node:fs';
import path from 'node:path';

import { act, fireEvent, screen, within } from '@testing-library/react-native';
import { AppState, type AppStateStatus, StyleSheet } from 'react-native';

import { ASK_AGAIN_MS } from '../src/guardian/Today';
import * as W from '../src/guardian/words';
import { guardianMock, guardianStates, NOW } from '../src/mock/guardianMock';
import { asked, pick, press, renderGuardian, type } from './guardianRender';
import { inOrder, textsInOrder, type TreeNode } from './render';

describe('which screen shows', () => {
  it('no family yet: the first start, on its first step, with Back’s room empty', async () => {
    renderGuardian(guardianStates.fresh());
    expect(await screen.findByText(W.welcome.heading)).toBeOnTheScreen();
    expect(screen.getByText('Step 1 of 8')).toBeOnTheScreen();
    expect(screen.queryByRole('button', { name: W.steps.back })).toBeNull();
    expect(screen.getByRole('button', { name: W.steps.go })).toBeDisabled();
  });

  it('a family, this window signed out: sign in, then Today', async () => {
    const { door } = renderGuardian(guardianStates.signedOut());
    expect(await screen.findByText(W.signIn.heading)).toBeOnTheScreen();
    await type('password', 'Asha and Mira at home');
    await press(W.signIn.button);
    expect(await screen.findByText('1 h 12 min')).toBeOnTheScreen();
    expect(door.calls.map((c) => c.name)).toEqual(expect.arrayContaining(['family_state', 'about', 'sign_in', 'family_today', 'waiting_requests']));
    expect(asked(door, 'sign_in')[0].args).toEqual({ password: 'Asha and Mira at home' });
  });

  it('signed in already: Today', async () => {
    renderGuardian(guardianStates.oneChild());
    expect(await screen.findByText('1 h 12 min')).toBeOnTheScreen();
  });

  it('a first start left before its PIN goes on from there after sign in', async () => {
    renderGuardian({ ...guardianStates.madeFamily(), signedIn: false });
    await type('password', 'Asha and Mira at home');
    await press(W.signIn.button);
    expect(await screen.findByText(W.pin.heading)).toBeOnTheScreen();
    expect(screen.getByText('Step 3 of 8')).toBeOnTheScreen();
  });
});

describe('sign in (C0 to C0d)', () => {
  const signIn = (state = guardianStates.signedOut()) => renderGuardian(state, { route: { at: 'sign-in' } });

  it('shows the door’s own words for a wrong password, and empties the field', async () => {
    signIn();
    await type('password', 'not it');
    await press(W.signIn.button);
    expect(await screen.findByText('That password is not right. 3 more tries before a wait.')).toBeOnTheScreen();
    expect(screen.getByTestId('password').props.value).toBe('');
  });

  it('after too many wrong tries, says when to try again on the PC’s clock, and switches the field and the button off', async () => {
    signIn();
    for (let i = 0; i < 5; i += 1) {
      await type('password', 'not it');
      await press(W.signIn.button);
    }
    expect(await screen.findByText('Too many wrong tries. Try again at 6:13 pm.')).toBeOnTheScreen();
    expect(screen.getByRole('button', { name: W.signIn.button })).toBeDisabled();
    expect(screen.getByTestId('password').props.editable).toBe(false);
  });

  it('says a wait already in force at once (C0c)', async () => {
    renderGuardian(guardianStates.waiting(), { route: { at: 'sign-in', start: { waitUntil: NOW + 5 * 60 } } });
    expect(await screen.findByText('Too many wrong tries. Try again at 6:17 pm.')).toBeOnTheScreen();
    expect(screen.getByRole('button', { name: W.signIn.button })).toBeDisabled();
  });

  it('tells a child’s account at once, in the door’s words, with no field and nothing to press (C0d)', async () => {
    signIn(guardianStates.childAccount());
    expect(await screen.findByText('LiMo’s settings cannot be opened from Asha’s account. A guardian opens them from their own.')).toBeOnTheScreen();
    expect(screen.queryByTestId('password')).toBeNull();
    expect(screen.queryAllByRole('button')).toHaveLength(0);
  });

  it('says plainly when the service is not running, with the button off', async () => {
    signIn(guardianStates.notRunning());
    expect(await screen.findByText('LiMo’s service is not running on this PC, so its settings cannot open.')).toBeOnTheScreen();
    expect(screen.getByRole('button', { name: W.signIn.button })).toBeDisabled();
  });

  it('keeps two lines of room under the field, and says what to do about a forgotten password', async () => {
    signIn();
    const room = await screen.findByTestId('message-room');
    expect(StyleSheet.flatten(room.props.style)).toMatchObject({ minHeight: 40 });
    expect(screen.getByText(W.signIn.forgottenLabel)).toBeOnTheScreen();
    expect(screen.getByText(W.signIn.forgotten)).toBeOnTheScreen();
  });
});

describe('the first start (F1 to F8)', () => {
  it('makes the family step by step, each step asked of the door, and ends on Today', async () => {
    const { door } = renderGuardian(guardianStates.fresh());
    await screen.findByText(W.welcome.heading);

    // F1: the box, then Continue.
    fireEvent.press(screen.getByRole('checkbox', { name: W.welcome.tick }));
    await press(W.steps.go);

    // F2: the two must match, and the door says what it will not take.
    expect(await screen.findByText(W.password.heading)).toBeOnTheScreen();
    expect(screen.getByRole('button', { name: W.steps.back })).toBeOnTheScreen();
    await type('password', 'Asha and Mira at home');
    await type('password-again', 'Asha and Mira');
    await press(W.steps.go);
    expect(screen.getByText(W.password.notSame)).toBeOnTheScreen();
    await type('password', 'short');
    await type('password-again', 'short');
    await press(W.steps.go);
    expect(screen.getByText('The password must be at least 8 characters long.')).toBeOnTheScreen();
    await type('password', 'Asha and Mira at home');
    await type('password-again', 'Asha and Mira at home');
    await press(W.steps.go);
    expect(asked(door, 'create_family').at(-1)?.args).toEqual({ password: 'Asha and Mira at home' });

    // F3: the PIN, the two secrets side by side.
    expect(await screen.findByText(W.pin.heading)).toBeOnTheScreen();
    expect(screen.getByText(W.pin.passwordLine)).toBeOnTheScreen();
    expect(screen.getByText(W.pin.pinLine)).toBeOnTheScreen();
    await type('pin', '20481');
    await type('pin-again', '20481');
    await press(W.steps.go);
    expect(screen.getByText('A PIN is 6 to 8 digits, with nothing else in it.')).toBeOnTheScreen();
    await type('pin', '204816');
    await type('pin-again', '204816');
    await press(W.steps.go);
    expect(asked(door, 'set_pin').at(-1)?.args).toEqual({ pin: '204816' });

    // F4: Continue only with a name and an age.
    expect(await screen.findByText(W.child.heading)).toBeOnTheScreen();
    expect(screen.getByRole('button', { name: W.steps.go })).toBeDisabled();
    await type('child-name', 'Asha');
    expect(screen.getByRole('button', { name: W.steps.go })).toBeDisabled();
    await pick('10 to 12');
    await press(W.steps.go);
    expect(asked(door, 'add_child')[0].args).toEqual({ name: 'Asha', age_band: 'ten_to_twelve' });

    // F5: two hours to start; More and Less move it by a quarter hour.
    expect(await screen.findByText('For all of Asha’s devices together.')).toBeOnTheScreen();
    expect(screen.getByTestId('figure-Every day')).toHaveTextContent('2 h');
    fireEvent.press(screen.getByRole('button', { name: 'More, Every day' }));
    expect(screen.getByTestId('figure-Every day')).toHaveTextContent('2 h 15 min');
    fireEvent.press(screen.getByRole('button', { name: 'Less, Every day' }));
    fireEvent.press(screen.getByRole('button', { name: 'Less, Every day' }));
    expect(screen.getByTestId('figure-Every day')).toHaveTextContent('1 h 45 min');
    await press(W.steps.go);
    const budgets = (asked(door, 'replace_rules')[0].args.rules as { budgets: Record<string, unknown> }).budgets;
    expect(Object.values(budgets)).toEqual(Array(7).fill({ minutes: 105 }));

    // F6: school nights only: the nights before a school day.
    expect(await screen.findByText(W.bedtime.line('Asha'))).toBeOnTheScreen();
    expect(screen.getByRole('button', { name: 'From, 9:00 pm' })).toBeOnTheScreen();
    await pick(W.bedtime.school);
    await press(W.steps.go);
    expect((asked(door, 'replace_rules')[1].args.rules as { blocked_hours: unknown }).blocked_hours).toEqual([
      { days: ['sunday', 'monday', 'tuesday', 'wednesday', 'thursday'], start_minute: 1260, end_minute: 420 },
    ]);

    // F7: whose each account is, said at each tap.
    expect(await screen.findByText(W.accounts.heading)).toBeOnTheScreen();
    expect(screen.getByRole('radio', { name: 'Asha: Later' })).toBeChecked();
    await pick('Asha: Asha’s');
    expect(asked(door, 'link_account')[0].args).toEqual({ account: 'S-1-5-21-1002', link: { child: { child_id: 'child-new-1' } } });
    expect(screen.getByRole('radio', { name: 'Asha: Asha’s' })).toBeChecked();
    await pick('Guardian: A guardian’s');
    expect(asked(door, 'link_account')[1].args).toEqual({ account: 'S-1-5-21-1001', link: 'not_limited' });
    expect(screen.queryByTestId('admin-warning')).toBeNull();
    await press(W.steps.go);
    expect(asked(door, 'finish_first_start')).toHaveLength(1);
    expect(door.state.unlinked).toBe('locked');

    // F8: what was made, the phone, and the maker's mark at the end.
    expect(await screen.findByText(W.done.heading)).toBeOnTheScreen();
    inOrder(textsInOrder(), [
      W.done.passwordAndPin,
      'Asha, 10 to 12',
      '1 h 45 min a day',
      'Bedtime from 9:00 pm to 7:00 am on school nights',
      'Asha’s account on this PC',
      'Asha’s phone',
      W.done.phoneLine,
      'Made by',
      'LucidLoom AI',
    ]);
    await press(W.done.later);

    // T1: Today, the first time, with what waits explained once.
    expect(await screen.findByText('When Asha asks for more time, the request shows here.')).toBeOnTheScreen();
    expect(screen.getByText('Asha’s phone is not added yet.')).toBeOnTheScreen();
  });

  it('Back keeps what was typed, and the counter says where a guardian is', async () => {
    renderGuardian(guardianStates.madeFamily(), { route: { at: 'first', initial: { step: 3, ticked: true, familyMade: true, pinSet: true } } });
    await type('child-name', 'Asha');
    await pick('6 to 9');
    await press(W.steps.go);
    expect(await screen.findByText('Step 5 of 8')).toBeOnTheScreen();
    await press(W.steps.back);
    expect(screen.getByTestId('child-name').props.value).toBe('Asha');
    expect(screen.getByRole('radio', { name: '6 to 9' })).toBeChecked();
    // Unchanged, Continue does not add the child twice.
    await press(W.steps.go);
    expect(screen.getByText('Step 5 of 8')).toBeOnTheScreen();
  });

  it('each day on its own: seven figures, each sent as it is', async () => {
    const asha = { childId: 'child-asha', name: 'Asha', ageBand: 'ten_to_twelve' } as const;
    const { door } = renderGuardian(guardianStates.withAsha(), {
      route: { at: 'first', initial: { step: 4, ticked: true, familyMade: true, pinSet: true, name: 'Asha', band: 'ten_to_twelve', child: asha, rules: door0().state.children[0].rules } },
    });
    await pick(W.timeADay.each);
    for (const d of Object.values(W.DAY_WORDS)) expect(screen.getByTestId(`figure-${d}`)).toHaveTextContent('2 h');
    fireEvent.press(screen.getByRole('button', { name: 'More, Saturday' }));
    await press(W.steps.go);
    const budgets = (asked(door, 'replace_rules')[0].args.rules as { budgets: Record<string, unknown> }).budgets;
    expect(budgets.saturday).toEqual({ minutes: 135 });
    expect(budgets.friday).toEqual({ minutes: 120 });
  });

  it('a bedtime the rules will not take is said in the rules’ own words, and the step stays', async () => {
    const asha = { childId: 'child-asha', name: 'Asha', ageBand: 'ten_to_twelve' } as const;
    renderGuardian(guardianStates.withAsha(), {
      route: { at: 'first', initial: { step: 5, ticked: true, familyMade: true, pinSet: true, child: asha, rules: door0().state.children[0].rules, from: 21 * 60, to: 21 * 60 } },
    });
    await press(W.steps.go);
    expect(screen.getByText('Blocked hours number 1 starts and ends at the same time.')).toBeOnTheScreen();
    expect(screen.getByText(W.bedtime.heading)).toBeOnTheScreen();
    // The ends are chosen from a list of half hours.
    await press('To, 9:00 pm');
    await press('7:00 am');
    expect(screen.getByRole('button', { name: 'To, 7:00 am' })).toBeOnTheScreen();
  });

  it('an administrator chosen for a child is said plainly; this window’s own account is refused in the door’s words', async () => {
    const asha = { childId: 'child-asha', name: 'Asha', ageBand: 'ten_to_twelve' } as const;
    renderGuardian(guardianStates.withAsha(true), { route: { at: 'first', initial: { step: 6, ticked: true, familyMade: true, pinSet: true, child: asha } } });
    await screen.findByText(W.accounts.heading);
    await pick('Asha: Asha’s');
    const warning = screen.getByTestId('admin-warning');
    expect(within(warning).getByText(W.accounts.warnText, { exact: false })).toBeOnTheScreen();
    expect(within(warning).getByText('There, choose Asha, then Change account type.')).toBeOnTheScreen();
    expect(within(warning).getByText(W.accounts.changesNone)).toBeOnTheScreen();
    await pick('Guardian: Asha’s');
    expect(
      screen.getByText('LiMo’s settings are open in this account, so it cannot be linked to a child from here: a guardian would be shut out of them. Link it from another account.'),
    ).toBeOnTheScreen();
  });

  it('says the window’s sentence only in the console on the PC', async () => {
    const pc = renderGuardian(guardianStates.fresh(), { place: 'pc' });
    expect(await screen.findByText(W.welcome.window)).toBeOnTheScreen();
    pc.unmount();
    renderGuardian(guardianStates.fresh(), { place: 'phone' });
    expect(await screen.findByText(W.welcome.heading)).toBeOnTheScreen();
    expect(screen.queryByText(W.welcome.window)).toBeNull();
  });

  it('Add the phone now leads to adding a phone, which says plainly that it comes next', async () => {
    renderGuardian(guardianStates.withAsha(false, true), { route: { at: 'first', initial: { step: 7, child: { childId: 'child-asha', name: 'Asha', ageBand: 'ten_to_twelve' } } } });
    await press('Add Asha’s phone now');
    expect(await screen.findByText(W.devices.addComing)).toBeOnTheScreen();
    expect(screen.getByRole('tab', { name: W.tabs.devices })).toBeSelected();
    await press(`Back to ${W.tabs.devices}`);
    expect(await screen.findByTestId('screen-devices')).toBeOnTheScreen();
  });
});

/** The made up door's family, to take its starting rules from. */
function door0() {
  return guardianMock(guardianStates.withAsha());
}

describe('Today (T1 to T3)', () => {
  it('an ordinary afternoon: the time left, used of the day, each device, and nothing waiting (T2)', async () => {
    renderGuardian(guardianStates.oneChild());
    await screen.findByText('1 h 12 min');
    inOrder(textsInOrder(), [
      'Saturday 21 November',
      W.today.nothingWaiting,
      'Asha',
      '1 h 12 min',
      W.today.leftToday,
      '48 min used of 2 h',
      W.today.thisPc,
      W.today.notInUse,
      '17 min',
      'Asha’s phone',
      W.today.inUse,
      '31 min',
    ]);
    expect(screen.queryByText('When Asha asks for more time, the request shows here.')).toBeNull();
    expect(screen.getByRole('button', { name: W.today.lift })).toBeDisabled();
  });

  it('two children, a request waiting, a time used up (T3)', async () => {
    renderGuardian(guardianStates.twoChildren());
    await screen.findByText('Mira asks for 30 min');
    expect(screen.getByText('On this PC · 6:04 pm')).toBeOnTheScreen();
    expect(screen.getByText('“To finish my science project”')).toBeOnTheScreen();
    expect(screen.getByText('0 min')).toBeOnTheScreen();
    expect(screen.getByText('1 h used of 1 h')).toBeOnTheScreen();
    expect(within(screen.getByTestId('kid-child-mira')).getByText(W.today.locked)).toBeOnTheScreen();
    expect(screen.getByText('Last heard 3 min ago')).toBeOnTheScreen();
    // Only right after the first start is a child without a phone offered the way to add one.
    expect(screen.queryByText('Mira’s phone is not added yet.')).toBeNull();
  });

  it('a phone that needs a guardian: a coral mark under it, and a card beside the request in the engine’s sentence (T3)', async () => {
    renderGuardian(guardianStates.twoChildren());
    await screen.findByText('Mira asks for 30 min');
    expect(within(screen.getByTestId('need-phone-asha')).getByText(W.NEED_MARK.accessibility)).toBeOnTheScreen();
    const card = screen.getByTestId('needs-phone-asha');
    expect(within(card).getByText(W.today.needsGuardian)).toBeOnTheScreen();
    expect(within(card).getByText('Asha’s phone')).toBeOnTheScreen();
    expect(within(card).getByText('Accessibility is switched off, so LiMo cannot see which app is open.')).toBeOnTheScreen();
    inOrder(textsInOrder(), [W.today.waitingForYou, 'Mira asks for 30 min', W.today.needsGuardian, 'Asha’s phone', W.today.seeWhatToDo]);
    // What to do is on the phone's card in Devices: its six steps, accessibility not on.
    await press(W.today.seeWhatToDo);
    const phone = await screen.findByTestId('phone-phone-asha');
    expect(screen.getByRole('tab', { name: W.tabs.devices })).toBeSelected();
    expect(within(phone).getByLabelText(`${W.devices.six.accessibility}: ${W.devices.notOn}`)).toBeOnTheScreen();
  });

  it('with nothing said of a device, no mark and no card', async () => {
    renderGuardian(guardianStates.oneChild());
    await screen.findByText('1 h 12 min');
    expect(screen.queryByText(W.today.needsGuardian)).toBeNull();
    expect(screen.queryByText(W.NEED_MARK.accessibility)).toBeNull();
    expect(screen.getByText(W.today.nothingWaiting)).toBeOnTheScreen();
  });

  it('a request answered with one tap: given, gone, and the time left grows', async () => {
    const { door } = renderGuardian(guardianStates.twoChildren());
    await screen.findByText('Mira asks for 30 min');
    await press('Give 30 min');
    expect(asked(door, 'answer_request')[0].args).toEqual({ child: 'child-mira', request: 'pc-mira/ask/1', reply: { grant: { minutes: 30 } } });
    expect(screen.queryByText('Mira asks for 30 min')).toBeNull();
    // Asha's phone still waits for a guardian.
    expect(screen.getByTestId('needs-phone-asha')).toBeOnTheScreen();
    expect(within(screen.getByTestId('kid-child-mira')).getByText('30 min')).toBeOnTheScreen();
    expect(within(screen.getByTestId('kid-child-mira')).queryByText(W.today.locked)).toBeNull();
  });

  it('a request refused with one tap', async () => {
    const { door } = renderGuardian(guardianStates.twoChildren());
    await screen.findByText('Mira asks for 30 min');
    await press(W.today.no);
    expect(asked(door, 'answer_request')[0].args).toMatchObject({ reply: 'refuse' });
    expect(screen.queryByText('Mira asks for 30 min')).toBeNull();
  });

  it('the quick actions: more time, a lock and its lift, open until a time', async () => {
    const { door } = renderGuardian(guardianStates.oneChild());
    await screen.findByText('1 h 12 min');
    await press('+15 min');
    expect(asked(door, 'give_extra')[0].args).toMatchObject({ child: 'child-asha', minutes: 15, tap_id: expect.any(String) });
    expect(screen.getByText('1 h 27 min')).toBeOnTheScreen();

    await press(W.today.lockNow);
    expect(asked(door, 'lock_now')[0].args).toEqual({ child: 'child-asha' });
    expect(screen.getByText(W.today.locked)).toBeOnTheScreen();
    expect(screen.getByRole('button', { name: W.today.lift })).toBeEnabled();
    await press(W.today.lift);
    expect(asked(door, 'lift')).toHaveLength(1);
    expect(screen.queryByText(W.today.locked)).toBeNull();

    await press(W.today.openUntil);
    await press('6:45 pm');
    expect(asked(door, 'unlock_until')[0].args).toEqual({ child: 'child-asha', until: NOW + 33 * 60 });
    expect(screen.getByText('Open until 6:45 pm')).toBeOnTheScreen();
    expect(screen.queryByText(W.today.leftToday)).toBeNull();
  });

  it('asks again every few seconds while it is open and in front, and stops when it is not', async () => {
    jest.useFakeTimers();
    try {
      const addListener = AppState.addEventListener as unknown as jest.Mock;
      const before = addListener.mock.calls.length;
      const { door, unmount } = renderGuardian(guardianStates.oneChild(), { route: { at: 'app', tab: 'today' } });
      await act(async () => {});
      const count = () => asked(door, 'family_today').length;
      const first = count();
      await act(async () => {
        jest.advanceTimersByTime(ASK_AGAIN_MS);
      });
      expect(count()).toBe(first + 1);
      const listeners = addListener.mock.calls.slice(before).filter(([t]) => t === 'change').map(([, l]) => l as (s: AppStateStatus) => void);
      await act(async () => listeners.forEach((l) => l('background')));
      await act(async () => {
        jest.advanceTimersByTime(ASK_AGAIN_MS * 3);
      });
      expect(count()).toBe(first + 1);
      await act(async () => listeners.forEach((l) => l('active')));
      expect(count()).toBe(first + 2);
      unmount();
      await act(async () => {
        jest.advanceTimersByTime(ASK_AGAIN_MS * 3);
      });
      expect(count()).toBe(first + 2);
    } finally {
      jest.useRealTimers();
    }
  });

  it('a session the service ended leads back to sign in, with a plain sentence', async () => {
    jest.useFakeTimers();
    try {
      const { door } = renderGuardian(guardianStates.oneChild(), { route: { at: 'app', tab: 'today' } });
      await act(async () => {});
      door.set({ signedIn: false });
      await act(async () => {
        jest.advanceTimersByTime(ASK_AGAIN_MS);
      });
      await act(async () => {});
      expect(screen.getByText(W.signIn.signedOut)).toBeOnTheScreen();
      expect(screen.getByTestId('password')).toBeOnTheScreen();
    } finally {
      jest.useRealTimers();
    }
  });

  it('at the console’s width: the children at the left, what waits at the right', async () => {
    renderGuardian(guardianStates.twoChildren(), { wide: true });
    await screen.findByText('Mira asks for 30 min');
    inOrder(textsInOrder(), ['Asha', 'Mira', W.today.waitingForYou, 'Mira asks for 30 min']);
  });

  it('at a phone’s width: what waits first', async () => {
    renderGuardian(guardianStates.twoChildren());
    await screen.findByText('Mira asks for 30 min');
    inOrder(textsInOrder(), [W.today.waitingForYou, 'Mira asks for 30 min', 'Asha', 'Mira']);
  });

  it('the tabs open Rules, Devices and More, and Today again', async () => {
    renderGuardian(guardianStates.oneChild());
    await screen.findByText('1 h 12 min');
    for (const [tab, id] of [
      [W.tabs.rules, 'screen-rules'],
      [W.tabs.devices, 'screen-devices'],
      [W.tabs.more, 'screen-more'],
    ] as const) {
      fireEvent.press(screen.getByRole('tab', { name: tab }));
      expect(await screen.findByTestId(id)).toBeOnTheScreen();
      expect(screen.getByRole('tab', { name: tab })).toBeSelected();
    }
    fireEvent.press(screen.getByRole('tab', { name: W.tabs.today }));
    expect(await screen.findByText('1 h 12 min')).toBeOnTheScreen();
  });
});

describe('for a finger and a screen reader', () => {
  const taps = () =>
    (screen.root.findAll((n: TreeNode) => typeof n.type === 'string' && typeof n.props.onResponderRelease === 'function') as TreeNode[]).filter(
      (n) => n.props.accessibilityRole !== 'tab',
    );

  it.each([
    ['at a phone’s width, 48', false, 48],
    ['at the console’s width, 40', true, 40],
  ])('everything to tap has a role, a label and its height: %s', async (_what, wide, least) => {
    renderGuardian(guardianStates.twoChildren(), { wide });
    await screen.findByText('Mira asks for 30 min');
    expect(taps().length).toBeGreaterThan(8);
    for (const t of taps()) {
      const style = StyleSheet.flatten(t.props.style) as { minHeight?: number; height?: number };
      expect(t.props.accessibilityRole).toEqual(expect.stringMatching(/^(button|checkbox|radio|tab)$/));
      expect(String(t.props.accessibilityLabel ?? '').length).toBeGreaterThan(0);
      expect(Math.max(style.minHeight ?? 0, style.height ?? 0)).toBeGreaterThanOrEqual(least);
    }
  });
});

describe('the words', () => {
  const dir = path.resolve(__dirname, '..', 'src', 'guardian');
  // The words themselves: the file without its comments.
  const words = fs
    .readFileSync(path.join(dir, 'words.ts'), 'utf8')
    .replace(/\/\*[\s\S]*?\*\//g, '')
    .replace(/^\s*\/\/.*$/gm, '');
  const all = fs
    .readdirSync(dir)
    .map((f) => fs.readFileSync(path.join(dir, f), 'utf8'))
    .join('\n');

  it('never say "her" or "him", and use no dash punctuation', () => {
    expect(words).not.toMatch(/\b(her|him|his|hers|she|he)\b/i);
    expect(words).not.toMatch(/ - /);
    expect(all).not.toMatch(/[–—]/);
  });

  it('hold no sample names: the child’s name comes from the door', () => {
    expect(fs.readFileSync(path.join(dir, 'words.ts'), 'utf8')).not.toMatch(/\b(Asha|Mira)\b/);
  });
});
