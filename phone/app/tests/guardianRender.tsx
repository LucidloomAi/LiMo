// The guardian's screens rendered for a test: the made up door, a phone's
// width (390 by 844, Android's bars) or the console's (1100 by 728 under
// Windows' title bar), the drawings' afternoon on the PC's clock, and the
// helpers the tests press, pick and type with.

import { act, fireEvent, render, screen, waitFor } from '@testing-library/react-native';
import { SafeAreaFrameContext, SafeAreaInsetsContext } from 'react-native-safe-area-context';

import { ClockContext } from '../src/guardian/clock';
import { GuardianApp, type GuardianRoute } from '../src/guardian/GuardianApp';
import { type Place, PlaceContext } from '../src/guardian/parts';
import { type GuardianMock, guardianMock, type GuardianMockState, NOW, PC_CLOCK } from '../src/mock/guardianMock';

export const PHONE = { frame: { x: 0, y: 0, width: 390, height: 844 }, insets: { top: 32, bottom: 24, left: 0, right: 0 } };
export const CONSOLE = { frame: { x: 0, y: 0, width: 1100, height: 728 }, insets: { top: 0, bottom: 0, left: 0, right: 0 } };

export function renderGuardian(
  state: GuardianMockState,
  options: { route?: GuardianRoute; wide?: boolean; place?: Place; now?: () => number } = {},
): { door: GuardianMock; unmount: () => void } {
  const door = guardianMock(state, { now: options.now });
  const at = options.wide ? CONSOLE : PHONE;
  const { unmount } = render(
    <SafeAreaFrameContext.Provider value={at.frame}>
      <SafeAreaInsetsContext.Provider value={at.insets}>
        <ClockContext.Provider value={{ now: options.now ?? (() => NOW), clock: PC_CLOCK }}>
          <PlaceContext.Provider value={options.place ?? (options.wide ? 'pc' : 'phone')}>
            <GuardianApp door={door} initial={options.route} />
          </PlaceContext.Provider>
        </ClockContext.Provider>
      </SafeAreaInsetsContext.Provider>
    </SafeAreaFrameContext.Provider>,
  );
  return { door, unmount };
}

/** A press whose answer comes a moment later: the press and the answer are one act. */
export async function press(name: string | RegExp) {
  await act(async () => {
    fireEvent.press(screen.getByRole('button', { name }));
  });
}

export async function pick(name: string | RegExp) {
  await act(async () => {
    fireEvent.press(screen.getByRole('radio', { name }));
  });
}

/** Presses what has this test id. */
export async function tap(testID: string) {
  await act(async () => {
    fireEvent.press(screen.getByTestId(testID));
  });
}

/** Types into a field once the screen can take it (the sign in asks the door first). */
export async function type(testID: string, words: string) {
  await screen.findByTestId(testID);
  await waitFor(() => expect(screen.getByTestId(testID).props.editable).not.toBe(false));
  await act(async () => {
    fireEvent.changeText(screen.getByTestId(testID), words);
  });
}

/** Every time the screens asked the door for this, in order. */
export const asked = (door: GuardianMock, name: string) => door.calls.filter((c) => c.name === name);
