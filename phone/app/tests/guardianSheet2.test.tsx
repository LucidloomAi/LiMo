// The guardian's screens from the second sheet (design/screens/console.html):
// Rules (R1 to R3), Devices (D1 and D2), a one time code (K1 and K2), and
// More (M1 to M4), with the screens that ask for the password again, against
// the made up family behind the made up door (src/mock/guardianMock.ts). At a
// phone's width unless a test says the console's.

import { act, fireEvent, screen, within } from '@testing-library/react-native';
import { StyleSheet } from 'react-native';

import { SAVED_MS } from '../src/guardian/Rules';
import * as W from '../src/guardian/words';
import { guardianStates } from '../src/mock/guardianMock';
import { asked, pick, press, renderGuardian, tap, type } from './guardianRender';
import { inOrder, textsInOrder, type TreeNode } from './render';
import type { GuardianRoute } from '../src/guardian/GuardianApp';

const RIGHT = 'Asha and Mira at home';

/** Picks a time from the list of half hours that is open. */
async function pickTime(time: string) {
  await act(async () => {
    fireEvent.press(within(screen.getByTestId('time-list')).getByRole('button', { name: time }));
  });
}
const rulesOf = (door: { state: { children: { child_id: string; rules: unknown }[] } }, id: string) => door.state.children.find((c) => c.child_id === id)?.rules;

describe('Rules (R1 to R3)', () => {
  it('a child’s rules, as the family’s file keeps them, with today’s use (R1)', async () => {
    const { door } = renderGuardian(guardianStates.family(), { route: { at: 'app', tab: 'rules' } });
    await screen.findByTestId('time-a-day');
    expect(screen.getByRole('radio', { name: 'Asha' })).toBeChecked();
    expect(screen.getByRole('radio', { name: W.timeADay.each })).toBeChecked();
    expect(screen.getByTestId('figure-Monday')).toHaveTextContent('1 h 30 min');
    expect(screen.getByTestId('figure-Friday')).toHaveTextContent('2 h');
    expect(screen.getByTestId('figure-Sunday')).toHaveTextContent('3 h');
    expect(within(screen.getByTestId('bedtime')).getByRole('radio', { name: W.bedtime.every })).toBeChecked();
    expect(screen.getByRole('button', { name: `${W.bedtime.from}, 9:00 pm` })).toBeOnTheScreen();
    expect(screen.getByRole('button', { name: `${W.bedtime.to}, 7:00 am` })).toBeOnTheScreen();
    expect(screen.getByText('When Asha’s phone cannot reach this PC, it may use up to half of the day.')).toBeOnTheScreen();
    expect(screen.getByRole('radio', { name: 'Half' })).toBeChecked();
    expect(within(screen.getByTestId('blocked')).getByText('Game Launcher')).toBeOnTheScreen();
    // Offered from what Asha used today, less what is in a list already.
    expect(within(screen.getByTestId('offer')).getAllByRole('button').map((b) => b.props.accessibilityLabel)).toEqual([
      'Add from what Asha used today: Chrome',
      'Add from what Asha used today: Paint',
      'Add from what Asha used today: Gallery',
    ]);
    // The phone call app, never blocked, first among the always allowed on a child with a phone.
    inOrder(textsInOrder(), [W.rules.allowed, W.rules.phoneApp, W.rules.neverBlockedMark, 'Calculator', 'Messages', 'Clock', 'Maps', W.rules.neverBlocked]);
    inOrder(textsInOrder(), [W.rules.use, 'On this PC · 17 min', 'Chrome', '11 min', 'Paint', '6 min', 'On Asha’s phone · 31 min', 'Maps', '12 min']);
    expect(screen.getByText(W.rules.atMidnight)).toBeOnTheScreen();
    expect(screen.getByText('Asha, 10 to 12')).toBeOnTheScreen();
    // Nothing changed: the tabs, no Undo or Save.
    expect(screen.getByRole('tab', { name: W.tabs.rules })).toBeSelected();
    expect(screen.queryByTestId('save')).toBeNull();
    expect(asked(door, 'usage')[0].args).toEqual({ child: 'child-asha', from_day: expect.any(Number), to_day: expect.any(Number) });
    expect(asked(door, 'usage')[0].args.from_day).toBe(asked(door, 'usage')[0].args.to_day);
  });

  it('saving with a problem: nothing saved, the rules’ own sentence by its row, and the strip says how many; then saved without', async () => {
    const { door } = renderGuardian(guardianStates.family(), { route: { at: 'app', tab: 'rules' } });
    await screen.findByTestId('bedtime');
    const before = rulesOf(door, 'child-asha');
    // Bedtime to end when it starts.
    await press(`${W.bedtime.to}, 7:00 am`);
    await pickTime('9:00 pm');
    expect(screen.queryByRole('tab', { name: W.tabs.rules })).toBeNull();
    expect(screen.getByTestId('undo')).toBeOnTheScreen();
    await tap('save');
    expect(asked(door, 'replace_rules')).toHaveLength(1);
    // The rules are sent whole.
    expect(asked(door, 'replace_rules')[0].args).toMatchObject({ child: 'child-asha', rules: { budgets: { monday: { minutes: 90 } }, blocked_hours: [{ start_minute: 1260, end_minute: 1260 }] } });
    expect(rulesOf(door, 'child-asha')).toEqual(before);
    expect(screen.getByTestId('not-saved')).toHaveTextContent('Not saved. One thing to fix.');
    expect(screen.getByTestId('bedtime-message')).toHaveTextContent('Blocked hours number 1 starts and ends at the same time.');
    // Fixed and saved: the notice floats a moment, the tabs come back.
    await press(`${W.bedtime.to}, 9:00 pm`);
    await pickTime('6:30 am');
    await tap('save');
    expect(asked(door, 'replace_rules')).toHaveLength(2);
    expect(rulesOf(door, 'child-asha')).toMatchObject({ blocked_hours: [{ start_minute: 1260, end_minute: 390 }] });
    expect(screen.getByTestId('saved')).toHaveTextContent('Saved. Asha’s devices have the new rules within a minute.');
    expect(screen.getByRole('tab', { name: W.tabs.rules })).toBeSelected();
    expect(screen.queryByTestId('save')).toBeNull();
  });

  it('a problem with an app is outlined on its row, as drawn (R2)', async () => {
    renderGuardian(guardianStates.family(), {
      route: { at: 'app', tab: 'rules', show: { rulesDraft: (r) => ({ ...r, blocked_apps: [...r.blocked_apps, 'Maps'] }), rulesSave: true } },
    });
    expect(await screen.findByTestId('rule-problem')).toHaveTextContent('Maps is both always allowed and blocked. Choose one.');
    expect(within(screen.getByTestId('blocked')).getByTestId('rule-problem')).toBeOnTheScreen();
    expect(screen.getByTestId('not-saved')).toHaveTextContent('Not saved. One thing to fix.');
    // Taking it off again: the problem goes with it, and nothing is left to save.
    const rows = within(screen.getByTestId('blocked')).getAllByRole('button', { name: `${W.rules.takeOff}: Maps` });
    await act(async () => {
      fireEvent.press(rows[0]);
    });
    expect(screen.queryByTestId('rule-problem')).toBeNull();
    expect(screen.queryByTestId('save')).toBeNull();
  });

  it('Undo changes puts back the rules kept', async () => {
    const { door } = renderGuardian(guardianStates.family(), { route: { at: 'app', tab: 'rules' } });
    await screen.findByTestId('time-a-day');
    await press('More, Monday');
    expect(screen.getByTestId('figure-Monday')).toHaveTextContent('1 h 45 min');
    await tap('undo');
    expect(screen.getByTestId('figure-Monday')).toHaveTextContent('1 h 30 min');
    expect(screen.queryByTestId('undo')).toBeNull();
    expect(asked(door, 'replace_rules')).toHaveLength(0);
  });

  it('an app from today’s use blocked, and one taken off a list, sent whole on Save', async () => {
    const { door } = renderGuardian(guardianStates.family(), { route: { at: 'app', tab: 'rules' } });
    await screen.findByTestId('offer');
    await press('Add from what Asha used today: Chrome');
    await press(`${W.rules.takeOff}: Calculator`);
    await pick('A quarter');
    await tap('save');
    expect(asked(door, 'replace_rules')[0].args).toMatchObject({
      rules: { blocked_apps: ['Game Launcher', 'Chrome'], always_allowed_apps: ['Messages', 'Clock', 'Maps'], away_share_percent: 25 },
    });
    expect(screen.getByText('When Asha’s phone cannot reach this PC, it may use up to a quarter of the day.')).toBeOnTheScreen();
  });

  it('another child’s rules, and not before the changes are saved or undone', async () => {
    const { door } = renderGuardian(guardianStates.family(), { route: { at: 'app', tab: 'rules' } });
    await screen.findByTestId('time-a-day');
    await pick('Mira');
    expect(await screen.findByText('Mira, 6 to 9')).toBeOnTheScreen();
    expect(asked(door, 'rules').at(-1)?.args).toEqual({ child: 'child-mira' });
    await press('Less, Every day');
    await pick('Asha');
    expect(screen.getByTestId('not-saved')).toHaveTextContent(W.rules.saveFirst);
    expect(screen.getByRole('radio', { name: 'Mira' })).toBeChecked();
  });

  it('the name and age changed in the card, each by its own request', async () => {
    const { door } = renderGuardian(guardianStates.family(), { route: { at: 'app', tab: 'rules' } });
    await screen.findByTestId('more-for-child');
    await tap('change-name');
    await type('child-name', 'Asha R');
    await pick(W.AGE_WORDS.thirteen_to_seventeen);
    await tap('save-name');
    expect(asked(door, 'rename_child')[0].args).toEqual({ child: 'child-asha', name: 'Asha R' });
    expect(asked(door, 'set_age_band')[0].args).toEqual({ child: 'child-asha', age_band: 'thirteen_to_seventeen' });
    expect(await screen.findByText('Asha R, 13 to 17')).toBeOnTheScreen();
  });

  it('a new day starting at another time', async () => {
    const { door } = renderGuardian(guardianStates.family(), { route: { at: 'app', tab: 'rules' } });
    await screen.findByTestId('more-for-child');
    await tap('change-day-start');
    await pickTime('4:00 am');
    expect(screen.getByText('At 4:00 am')).toBeOnTheScreen();
    await tap('save');
    expect(asked(door, 'replace_rules')[0].args).toMatchObject({ rules: { day_start_minute: 240 } });
  });

  it('the Saved notice floats a moment, then goes', async () => {
    jest.useFakeTimers();
    try {
      renderGuardian(guardianStates.family(), { route: { at: 'app', tab: 'rules', show: { rulesSaved: true } } });
      await act(async () => {});
      expect(screen.getByTestId('saved')).toBeOnTheScreen();
      await act(async () => {
        jest.advanceTimersByTime(SAVED_MS);
      });
      expect(screen.queryByTestId('saved')).toBeNull();
    } finally {
      jest.useRealTimers();
    }
  });

  it('at the console’s width, two columns in the same order', async () => {
    renderGuardian(guardianStates.family(), { route: { at: 'app', tab: 'rules' }, wide: true });
    await screen.findByTestId('time-a-day');
    inOrder(textsInOrder(), [W.rules.timeADay, W.rules.bedtime, W.rules.away, W.rules.blocked, W.rules.allowed, W.rules.use, 'More for Asha']);
  });

  it('removing a child asks for the password again, as drawn; Back, or nothing typed, removes nothing', async () => {
    const { door } = renderGuardian(guardianStates.family(), { route: { at: 'app', tab: 'rules' } });
    await screen.findByTestId('more-for-child');
    await tap('remove-child');
    expect(await screen.findByText('Remove Asha?')).toBeOnTheScreen();
    expect(screen.getByText('Everything LiMo keeps for Asha is deleted: the rules, the time and the devices. It cannot be undone.')).toBeOnTheScreen();
    // Pressed with nothing typed: nothing asked.
    await tap('confirm');
    expect(screen.getByTestId('confirm-message')).toHaveTextContent(W.signIn.empty);
    expect(asked(door, 'remove_child')).toHaveLength(0);
    // Back: Asha is still there.
    await press(`Back to ${W.tabs.rules}`);
    expect(await screen.findByText('Asha, 10 to 12')).toBeOnTheScreen();
    await tap('remove-child');
    await type('confirm-password', 'not it');
    await tap('confirm');
    expect(screen.getByTestId('confirm-message')).toHaveTextContent('That password is not right. 3 more tries before a wait.');
    expect(screen.getByTestId('confirm-password').props.value).toBe('');
    expect(door.state.children).toHaveLength(2);
    await type('confirm-password', RIGHT);
    await tap('confirm');
    expect(asked(door, 'remove_child').at(-1)?.args).toEqual({ child: 'child-asha', password: RIGHT });
    expect(door.state.children.map((c) => c.name)).toEqual(['Mira']);
    // Back on Rules, with Mira's.
    expect(await screen.findByText('Mira, 6 to 9')).toBeOnTheScreen();
  });
});

describe('Devices (D1 and D2)', () => {
  it('each phone, and this PC with its accounts and settings (D1)', async () => {
    renderGuardian(guardianStates.family(), { route: { at: 'app', tab: 'devices' } });
    const phone = await screen.findByTestId('phone-phone-asha');
    expect(within(phone).getByText('Asha’s phone')).toBeOnTheScreen();
    for (const fact of [`${W.devices.child}: Asha`, `${W.devices.paired}: 21 Nov`, `${W.devices.lastHeard}: ${W.devices.justNow}`, `${W.devices.now}: ${W.devices.inUse}`, `${W.devices.protection}: On`]) {
      expect(within(within(phone).getByTestId('phone-facts')).getByLabelText(fact)).toBeOnTheScreen();
    }
    // All six on, said by the phone.
    for (const p of ['notifications', 'phone', 'usage_access', 'accessibility', 'battery', 'protection'] as const) {
      expect(within(within(phone).getByTestId('six')).getByLabelText(`${W.devices.six[p]}: ${W.devices.on}`)).toBeOnTheScreen();
    }
    expect(within(phone).queryByTestId('clock-warning')).toBeNull();
    expect(within(phone).queryByTestId('usb-warning')).toBeNull();
    const pc = screen.getByTestId('this-pc');
    expect(within(pc).getByLabelText(`Guardian, ${W.accounts.admin} · ${W.accounts.signedInNow}, ${W.devices.guardians}`)).toBeOnTheScreen();
    expect(within(pc).getByLabelText(`Asha, ${W.accounts.standard}, Asha’s`)).toBeOnTheScreen();
    expect(within(pc).getByLabelText(`Mira, ${W.accounts.standard}, Mira’s`)).toBeOnTheScreen();
    expect(screen.getByTestId(`figure-${W.devices.pause}`)).toHaveTextContent('3 min');
    expect(screen.getByRole('radio', { name: W.devices.locked })).toBeChecked();
    // On a phone, the phones first.
    inOrder(textsInOrder(), ['Asha’s phone', W.devices.addPhone, W.devices.thisPc]);
  });

  it('a phone that needs a guardian: not heard for a while, a step not on, USB debugging on, its clock off (D2)', async () => {
    renderGuardian(guardianStates.family(true), { route: { at: 'app', tab: 'devices' } });
    const phone = await screen.findByTestId('phone-phone-asha');
    expect(within(phone).getByLabelText(`${W.devices.lastHeard}: 2 h ago`)).toBeOnTheScreen();
    expect(within(phone).queryByLabelText(`${W.devices.now}: ${W.devices.inUse}`)).toBeNull();
    expect(within(phone).getByLabelText(`${W.devices.six.accessibility}: ${W.devices.notOn}`)).toBeOnTheScreen();
    expect(within(phone).getByLabelText(`${W.devices.six.battery}: ${W.devices.on}`)).toBeOnTheScreen();
    expect(within(phone).getByTestId('usb-warning')).toHaveTextContent(`${W.devices.usbTitle} ${W.devices.usbText}`);
    expect(within(phone).getByTestId('clock-warning')).toHaveTextContent('Its clock is 9 min ahead of this PC’s. LiMo keeps its own count, so no time is gained.');
    // The steps, then the warnings, then Remove.
    const ids = (phone.findAll((n: TreeNode) => ['six', 'usb-warning', 'clock-warning', 'remove-phone'].includes(String(n.props.testID ?? ''))) as TreeNode[]).map((n) => n.props.testID);
    expect([...new Set(ids)]).toEqual(['six', 'usb-warning', 'clock-warning', 'remove-phone']);
  });

  it('a phone paired and not set up yet: its steps not yet, none claimed on', async () => {
    const state = guardianStates.family();
    const phone = state.children[0].devices[1];
    const standing = phone.standing as { steps: { on: boolean | null }[] };
    standing.steps = standing.steps.map((s) => ({ ...s, on: null }));
    renderGuardian(state, { route: { at: 'app', tab: 'devices' } });
    const card = await screen.findByTestId('phone-phone-asha');
    expect(within(card).getAllByLabelText(new RegExp(`: ${W.devices.notYet}$`))).toHaveLength(6);
    expect(within(within(card).getByTestId('six')).queryByLabelText(new RegExp(`: ${W.devices.on}$`))).toBeNull();
  });

  it('with nothing known of how a phone stands, none of its six is claimed', async () => {
    renderGuardian(guardianStates.oneChild(), { route: { at: 'app', tab: 'devices' } });
    const phone = await screen.findByTestId('phone-phone-asha');
    expect(within(phone).queryByText(W.devices.needs)).toBeNull();
    expect(within(phone).queryByTestId('six')).toBeNull();
  });

  it('at the console’s width, this PC at the left and the phones at the right', async () => {
    renderGuardian(guardianStates.family(), { route: { at: 'app', tab: 'devices' }, wide: true });
    await screen.findByTestId('phone-phone-asha');
    inOrder(textsInOrder(), [W.devices.thisPc, W.devices.accounts, 'Asha’s phone']);
  });

  it('the PC’s two settings, each sent as it changes', async () => {
    const { door } = renderGuardian(guardianStates.family(), { route: { at: 'app', tab: 'devices' } });
    await screen.findByTestId('this-pc');
    await press(`More, ${W.devices.pause}`);
    expect(asked(door, 'set_idle_minutes')[0].args).toEqual({ minutes: 4 });
    expect(screen.getByTestId(`figure-${W.devices.pause}`)).toHaveTextContent('4 min');
    await pick(W.devices.free);
    expect(asked(door, 'set_unlinked_accounts')[0].args).toEqual({ unlinked: 'free' });
    expect(screen.getByText(W.devices.freeLine)).toBeOnTheScreen();
  });

  it('the pause never goes under a minute', async () => {
    const state = { ...guardianStates.family(), idleMinutes: 1 };
    const { door } = renderGuardian(state, { route: { at: 'app', tab: 'devices' } });
    await screen.findByTestId('this-pc');
    expect(await screen.findByText('1 min')).toBeOnTheScreen();
    await press(`Less, ${W.devices.pause}`);
    expect(asked(door, 'set_idle_minutes')).toHaveLength(0);
  });

  it('whose an account is, changed under its row', async () => {
    const { door } = renderGuardian(guardianStates.family(), { route: { at: 'app', tab: 'devices' } });
    await screen.findByTestId('this-pc');
    await tap('change-mira');
    await pick(`Mira: ${W.devices.notSaidChoice}`);
    expect(asked(door, 'unlink_account')[0].args).toEqual({ account: 'S-1-5-21-1003' });
    expect(await screen.findByLabelText(`Mira, ${W.accounts.standard}, ${W.devices.notSaid}`)).toBeOnTheScreen();
    await tap('change-mira');
    await pick('Mira: Mira’s');
    expect(asked(door, 'link_account')[0].args).toEqual({ account: 'S-1-5-21-1003', link: { child: { child_id: 'child-mira' } } });
  });

  it('a phone renamed', async () => {
    const { door } = renderGuardian(guardianStates.family(), { route: { at: 'app', tab: 'devices' } });
    await screen.findByTestId('phone-phone-asha');
    await tap('rename');
    await type('device-name', 'Asha’s tablet');
    await tap('save-device-name');
    expect(asked(door, 'rename_device')[0].args).toEqual({ device: 'phone-asha', name: 'Asha’s tablet' });
    expect(await screen.findByText('Asha’s tablet')).toBeOnTheScreen();
  });

  it('removing a phone asks for the password again, as drawn', async () => {
    const { door } = renderGuardian(guardianStates.family(), { route: { at: 'app', tab: 'devices' } });
    await screen.findByTestId('phone-phone-asha');
    await tap('remove-phone');
    expect(await screen.findByText('Remove Asha’s phone?')).toBeOnTheScreen();
    expect(screen.getByText(W.confirm.removePhone.line)).toBeOnTheScreen();
    await type('confirm-password', 'not it');
    await tap('confirm');
    expect(screen.getByTestId('confirm-message')).toHaveTextContent('That password is not right. 3 more tries before a wait.');
    expect(door.state.children[0].devices.map((d) => d.device_id)).toContain('phone-asha');
    await type('confirm-password', RIGHT);
    await tap('confirm');
    expect(asked(door, 'remove_device').at(-1)?.args).toEqual({ device: 'phone-asha', password: RIGHT });
    expect(await screen.findByTestId('screen-devices')).toBeOnTheScreen();
    expect(screen.queryByTestId('phone-phone-asha')).toBeNull();
  });

  it('Add a phone leads to one screen that says plainly it comes next', async () => {
    renderGuardian(guardianStates.family(), { route: { at: 'app', tab: 'devices' } });
    await screen.findByTestId('this-pc');
    await tap('add-phone');
    expect(await screen.findByText(W.devices.addComing)).toBeOnTheScreen();
  });
});

describe('a one time code (K1 and K2)', () => {
  it('for whom, on what, how much; then the code to read out', async () => {
    const { door } = renderGuardian(guardianStates.family(), { route: { at: 'app', tab: 'more' } });
    await tap('more-code');
    await screen.findByTestId('make-code');
    expect(screen.getByRole('radio', { name: 'Asha' })).toBeChecked();
    expect(screen.getByRole('radio', { name: 'Asha’s phone' })).toBeChecked();
    expect(screen.getByRole('radio', { name: '30 min' })).toBeChecked();
    await tap('make');
    expect(asked(door, 'make_code')[0].args).toEqual({ device: 'phone-asha', minutes: 30 });
    expect(screen.getByText('Asha’s phone · 30 min')).toBeOnTheScreen();
    expect(screen.getByText('4718')).toBeOnTheScreen();
    expect(screen.getByText('2093')).toBeOnTheScreen();
    expect(screen.getByText('Read it to Asha. It works once, today only, on Asha’s phone, when its time is used up.')).toBeOnTheScreen();
    // Another, for this PC.
    await tap('another');
    await pick(W.devices.thisPc);
    await pick('1 hour');
    await tap('make');
    expect(asked(door, 'make_code')[1].args).toEqual({ device: 'pc-asha', minutes: 60 });
    expect(screen.getByText('This PC · 1 hour')).toBeOnTheScreen();
    expect(screen.getByText('Read it to Asha. It works once, today only, on this PC, when its time is used up.')).toBeOnTheScreen();
    await tap('code-done');
    expect(await screen.findByTestId('screen-more')).toBeOnTheScreen();
  });

  it('a child without a phone: this PC', async () => {
    const { door } = renderGuardian(guardianStates.family(), { route: { at: 'app', tab: 'more', sub: { at: 'code' } } });
    await screen.findByTestId('make-code');
    await pick('Mira');
    expect(screen.getByRole('radio', { name: W.devices.thisPc })).toBeChecked();
    await tap('make');
    expect(asked(door, 'make_code')[0].args).toEqual({ device: 'pc-mira', minutes: 30 });
  });
});

describe('More (M1 to M4)', () => {
  it('the list, each row opening its screen, and Back to More (M1)', async () => {
    renderGuardian(guardianStates.family(), { route: { at: 'app', tab: 'more' } });
    await screen.findByTestId('screen-more');
    inOrder(textsInOrder(), W.more.rows.flatMap((r) => [r.title, r.text]));
    for (const [row, id] of [
      ['more-code', 'screen-code'],
      ['more-secrets', 'screen-secrets'],
      ['more-about', 'screen-about'],
    ] as const) {
      await tap(row);
      expect(await screen.findByTestId(id)).toBeOnTheScreen();
      expect(screen.getByRole('tab', { name: W.tabs.more })).toBeSelected();
      await press(`Back to ${W.more.back}`);
      expect(await screen.findByTestId('screen-more')).toBeOnTheScreen();
    }
  });

  it('the password changed: twice the same, the old one asked for, and the door’s words when it is wrong (M2)', async () => {
    const { door } = renderGuardian(guardianStates.family(), { route: { at: 'app', tab: 'more', sub: { at: 'secrets' } } });
    await screen.findByTestId('change-password');
    await type('old-password', RIGHT);
    await type('new-password', 'a longer one now');
    await type('new-password-again', 'a longer one');
    await tap('change-the-password');
    expect(screen.getByTestId('password-message')).toHaveTextContent(W.password.notSame);
    expect(asked(door, 'change_password')).toHaveLength(0);
    await type('old-password', 'not it');
    await type('new-password-again', 'a longer one now');
    await tap('change-the-password');
    expect(screen.getByTestId('password-message')).toHaveTextContent('That password is not right. 3 more tries before a wait.');
    await type('old-password', RIGHT);
    await tap('change-the-password');
    expect(asked(door, 'change_password').at(-1)?.args).toEqual({ old: RIGHT, new: 'a longer one now' });
    expect(screen.getByTestId('password-message')).toHaveTextContent(W.secrets.passwordChanged);
    expect(door.state.password).toBe('a longer one now');
  });

  it('the PIN changed', async () => {
    const { door } = renderGuardian(guardianStates.family(), { route: { at: 'app', tab: 'more', sub: { at: 'secrets' } } });
    await screen.findByTestId('change-pin');
    await type('new-pin', '135790');
    await type('new-pin-again', '135791');
    await tap('change-the-pin');
    expect(screen.getByTestId('pin-message')).toHaveTextContent(W.pin.notSame);
    await type('new-pin-again', '135790');
    await tap('change-the-pin');
    expect(asked(door, 'set_pin')[0].args).toEqual({ pin: '135790' });
    expect(screen.getByTestId('pin-message')).toHaveTextContent(W.secrets.pinChanged);
  });

  it('About: the version, the licences, and removing (M3)', async () => {
    renderGuardian(guardianStates.family(), { route: { at: 'app', tab: 'more', sub: { at: 'about' } } });
    expect(await screen.findByLabelText(`${W.about.forWindows}: 0.1.0`)).toBeOnTheScreen();
    expect(screen.getByText(W.about.licence)).toBeOnTheScreen();
    await act(async () => {
      fireEvent.press(screen.getAllByRole('button', { name: W.about.read })[0]);
    });
    expect(screen.getByText(W.about.readComing)).toBeOnTheScreen();
    expect(screen.getByText(W.about.removeLimoLine)).toBeOnTheScreen();
    expect(screen.getByLabelText('Made by LucidLoom AI')).toBeOnTheScreen();
  });

  it('removing LiMo asks for the password, then says until when it may be removed', async () => {
    const { door } = renderGuardian(guardianStates.family(), { route: { at: 'app', tab: 'more', sub: { at: 'about' } } });
    await tap('remove-limo');
    expect(await screen.findByText(W.confirm.removeLimo.heading)).toBeOnTheScreen();
    await type('confirm-password', RIGHT);
    await tap('confirm');
    expect(asked(door, 'allow_removal')[0].args).toEqual({ password: RIGHT });
    expect(screen.getByTestId('confirm-message')).toHaveTextContent('LiMo can be removed until 6:22 pm. Remove it in Windows Settings, Apps, Installed apps.');
    await press(`Back to ${W.about.heading}`);
    expect(await screen.findByText('LiMo can be removed until 6:22 pm. Remove it in Windows Settings, Apps, Installed apps.')).toBeOnTheScreen();
  });

  it('deleting the family asks for the password again, as drawn, then starts again from the first step (M4)', async () => {
    const { door } = renderGuardian(guardianStates.family(), { route: { at: 'app', tab: 'more', sub: { at: 'about' } } });
    await tap('delete-family');
    expect(await screen.findByText(W.confirm.deleteFamily.heading)).toBeOnTheScreen();
    expect(await screen.findByText('Everything LiMo keeps on this PC is deleted: Asha and Mira, their rules, their time and their devices. It cannot be undone.')).toBeOnTheScreen();
    expect(
      screen.getByText('Asha’s phone is told at its next contact, lets go of its limits, and says LiMo can be removed. LiMo stays on this PC and starts again from the first step.'),
    ).toBeOnTheScreen();
    expect(screen.getByRole('button', { name: W.confirm.deleteFamily.button })).toBeOnTheScreen();
    // Nothing typed: nothing deleted.
    await tap('confirm');
    expect(asked(door, 'delete_family')).toHaveLength(0);
    await type('confirm-password', 'not it');
    await tap('confirm');
    expect(screen.getByTestId('confirm-message')).toHaveTextContent('That password is not right. 3 more tries before a wait.');
    expect(door.state.hasFamily).toBe(true);
    await type('confirm-password', RIGHT);
    await tap('confirm');
    expect(asked(door, 'delete_family').at(-1)?.args).toEqual({ password: RIGHT });
    expect(await screen.findByText(W.welcome.heading)).toBeOnTheScreen();
    expect(screen.getByText('Step 1 of 8')).toBeOnTheScreen();
  });

  it('too many wrong tries, counted with sign in’s: the wait, at a time on the PC’s clock', async () => {
    const state = { ...guardianStates.family(), wrongGuesses: 4 };
    const { door } = renderGuardian(state, { route: { at: 'app', tab: 'more', sub: { at: 'delete-family' } } });
    await type('confirm-password', 'not it');
    await tap('confirm');
    expect(screen.getByTestId('confirm-message')).toHaveTextContent('Too many wrong tries. Try again at 6:13 pm.');
    // While it lasts, even the right one waits.
    await type('confirm-password', RIGHT);
    await tap('confirm');
    expect(screen.getByTestId('confirm-message')).toHaveTextContent('Too many wrong tries. Try again at 6:13 pm.');
    expect(door.state.hasFamily).toBe(true);
  });
});

describe('for a finger and a screen reader', () => {
  const taps = () =>
    (screen.root.findAll((n: TreeNode) => typeof n.type === 'string' && typeof n.props.onResponderRelease === 'function') as TreeNode[]).filter(
      (n) => n.props.accessibilityRole !== 'tab',
    );
  const SCREENS: [string, GuardianRoute, string][] = [
    ['R1', { at: 'app', tab: 'rules' }, 'time-a-day'],
    ['R2', { at: 'app', tab: 'rules', show: { rulesDraft: (r) => ({ ...r, blocked_apps: [...r.blocked_apps, 'Maps'] }), rulesSave: true } }, 'rule-problem'],
    ['D1', { at: 'app', tab: 'devices' }, 'phone-phone-asha'],
    ['K1', { at: 'app', tab: 'more', sub: { at: 'code' } }, 'make-code'],
    ['K2', { at: 'app', tab: 'more', sub: { at: 'code' }, show: { codeMade: true } }, 'the-code'],
    ['M1', { at: 'app', tab: 'more' }, 'more-code'],
    ['M2', { at: 'app', tab: 'more', sub: { at: 'secrets' } }, 'change-pin'],
    ['M3', { at: 'app', tab: 'more', sub: { at: 'about' } }, 'licences'],
    ['M4', { at: 'app', tab: 'more', sub: { at: 'delete-family' } }, 'confirm'],
  ];
  const cases = SCREENS.flatMap(([id, route, ready]) => [[id, 'a phone’s width, 48', false, 48, route, ready] as const, [id, 'the console’s width, 40', true, 40, route, ready] as const]);
  it.each(cases)('%s: everything to tap has a role, a label and its height at %s', async (_id, _what, wide, least, route, ready) => {
    renderGuardian(guardianStates.family(), { route, wide });
    await screen.findByTestId(ready);
    expect(taps().length).toBeGreaterThan(1);
    for (const t of taps()) {
      const style = StyleSheet.flatten(t.props.style) as { minHeight?: number; height?: number };
      expect(t.props.accessibilityRole).toEqual(expect.stringMatching(/^(button|checkbox|radio|tab)$/));
      expect(String(t.props.accessibilityLabel ?? '').length).toBeGreaterThan(0);
      expect(Math.max(style.minHeight ?? 0, style.height ?? 0)).toBeGreaterThanOrEqual(least);
    }
  });
});