// The time now and the clock the guardian's screens show times on. In the
// app it is the PC's (or the phone's) own; the preview and the tests give a
// fixed afternoon, so a picture or a test says the same every time.

import { createContext, useContext } from 'react';

import { type Clock, clockHere } from './time';

export type ScreenClock = {
  /** Now, in unix seconds. */
  readonly now: () => number;
  readonly clock: Clock;
};

export const ClockContext = createContext<ScreenClock>({ now: () => Math.floor(Date.now() / 1000), clock: clockHere() });

export const useClock = () => useContext(ClockContext);
