// Renders the child form against a mock host, inside a phone's safe areas,
// as the preview does. Tests only.

import { render } from '@testing-library/react-native';
import type { ReactElement } from 'react';
import { SafeAreaFrameContext, SafeAreaInsetsContext } from 'react-native-safe-area-context';

import { ChildApp, type Route } from '../src/child/ChildApp';
import { ChildHostProvider } from '../src/child/HostContext';
import { type MockHost, mockHost, type MockOptions, type MockState } from '../src/mock/childMock';

const FRAME = { x: 0, y: 0, width: 390, height: 844 };
const INSETS = { top: 32, bottom: 24, left: 0, right: 0 };

export function inPhone(ui: ReactElement) {
  return (
    <SafeAreaFrameContext.Provider value={FRAME}>
      <SafeAreaInsetsContext.Provider value={INSETS}>{ui}</SafeAreaInsetsContext.Provider>
    </SafeAreaFrameContext.Provider>
  );
}

/** The child form, asking `host`, starting at `route` or where the host's state says. */
export function renderChild(state: MockState, options: { route?: Route; mock?: MockOptions } = {}): { host: MockHost } & ReturnType<typeof render> {
  const host = mockHost(state, options.mock);
  const result = render(
    inPhone(
      <ChildHostProvider host={host}>
        <ChildApp initial={options.route} />
      </ChildHostProvider>,
    ),
  );
  return { host, ...result };
}
