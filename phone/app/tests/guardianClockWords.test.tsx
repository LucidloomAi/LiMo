// Times of day said on the guardian's screens, with moments that have
// seconds: a moment from which something becomes possible is said rounded
// up, a moment until which something is allowed rounded down
// (src/guardian/time.ts, `Way`). The owner saw a wait that ended at
// 8:40:33 pm said as "8:40 pm" while the field was still off.

import { act, screen } from '@testing-library/react-native';

import * as W from '../src/guardian/words';
import { guardianStates, NOW } from '../src/mock/guardianMock';
import { renderGuardian } from './guardianRender';

describe('a wait’s end, rounded up', () => {
  it('sign in: the time said is never before the field opens, and the field opens at the real end', async () => {
    jest.useFakeTimers();
    try {
      // 6:12:00 pm now; the wait ends at 6:12:33 pm.
      const until = NOW + 33;
      renderGuardian({ ...guardianStates.signedOut(), wrongGuesses: 5, waitUntil: until }, { route: { at: 'sign-in', start: { waitUntil: until } } });
      await act(async () => {});
      expect(screen.getByText('Too many wrong tries. Try again at 6:13 pm.')).toBeOnTheScreen();
      expect(screen.getByTestId('password').props.editable).toBe(false);
      await act(async () => {
        jest.advanceTimersByTime(32_000);
      });
      expect(screen.getByTestId('password').props.editable).toBe(false);
      await act(async () => {
        jest.advanceTimersByTime(1_000);
      });
      expect(screen.getByTestId('password').props.editable).not.toBe(false);
    } finally {
      jest.useRealTimers();
    }
  });

  it('sign in: a wait that ends on a whole minute is said as that minute', async () => {
    renderGuardian(guardianStates.waiting(), { route: { at: 'sign-in', start: { waitUntil: NOW + 5 * 60 } } });
    expect(await screen.findByText('Too many wrong tries. Try again at 6:17 pm.')).toBeOnTheScreen();
  });
});

describe('a moment until which something is allowed, rounded down', () => {
  it('open until: a guardian’s unlock that ends at 7:12:50 pm says 7:12 pm', async () => {
    const state = guardianStates.oneChild();
    state.children[0].guardian = { unlock: { until: NOW + 3_600 + 50 } };
    renderGuardian(state, { route: { at: 'app', tab: 'today' } });
    expect(await screen.findByText('Open until 7:12 pm')).toBeOnTheScreen();
  });

  it('LiMo allowed to be removed until 6:22:45 pm says 6:22 pm', async () => {
    renderGuardian({ ...guardianStates.family(), removalUntil: NOW + 600 + 45 }, { route: { at: 'app', tab: 'more', sub: { at: 'about' } } });
    expect(await screen.findByText(W.confirm.removeLimo.allowed('6:22 pm'))).toBeOnTheScreen();
  });
});
