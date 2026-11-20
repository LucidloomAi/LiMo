// Reading one answer from the child's host, and reading it again when it
// may have changed: when the host says so, and, for what Android decides,
// whenever the app comes back to the front. An older answer that arrives
// after a newer one is dropped.

import { useCallback, useEffect, useRef, useState } from 'react';
import { AppState } from 'react-native';

import type { ChildChange, ChildHost } from '../host';
import { useChildHost } from './HostContext';

export type Loaded<T> =
  | { readonly state: 'waiting' }
  | { readonly state: 'ready'; readonly value: T }
  | { readonly state: 'failed'; readonly error: unknown };

export type HostValue<T> = Loaded<T> & {
  /**
   * Ask again now. Resolves with that answer once it is in, or with null when
   * it was dropped for a newer one or the screen has gone.
   */
  again: () => Promise<Loaded<T> | null>;
};

export function useHostValue<T>(
  read: (host: ChildHost) => Promise<T>,
  on: readonly ChildChange[],
  options: { whenBackInFront?: boolean } = {},
): HostValue<T> {
  const host = useChildHost();
  const [loaded, setLoaded] = useState<Loaded<T>>({ state: 'waiting' });
  const readRef = useRef(read);
  readRef.current = read;
  const asked = useRef(0);
  const live = useRef(true);

  const again = useCallback(() => {
    asked.current += 1;
    const mine = asked.current;
    const settle = (now: Loaded<T>): Loaded<T> | null => {
      if (!live.current || mine !== asked.current) return null;
      setLoaded(now);
      return now;
    };
    return readRef.current(host).then(
      (value) => settle({ state: 'ready', value }),
      (error: unknown) => settle({ state: 'failed', error }),
    );
  }, [host]);

  const changes = on.join(' ');
  useEffect(() => {
    live.current = true;
    again();
    const wanted = changes.split(' ');
    const stop = host.subscribe((change) => {
      if (wanted.includes(change)) again();
    });
    return () => {
      live.current = false;
      stop();
    };
  }, [host, again, changes]);

  const whenBack = options.whenBackInFront === true;
  useEffect(() => {
    if (!whenBack) return undefined;
    const sub = AppState.addEventListener('change', (state) => {
      if (state === 'active') again();
    });
    return () => sub.remove();
  }, [whenBack, again]);

  return { ...loaded, again };
}
