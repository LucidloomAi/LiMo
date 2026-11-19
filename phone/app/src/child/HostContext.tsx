// Which ChildHost the child's screens ask. In the app it is always the real
// one (src/host.ts). Only the preview and the tests pass another, through
// ChildHostProvider, and only they import the mock (src/mock/).

import { createContext, type ReactNode, useContext } from 'react';

import { type ChildHost, childHost } from '../host';

const HostContext = createContext<ChildHost>(childHost);

export function ChildHostProvider({ host, children }: { host: ChildHost; children: ReactNode }) {
  return <HostContext.Provider value={host}>{children}</HostContext.Provider>;
}

export function useChildHost(): ChildHost {
  return useContext(HostContext);
}
