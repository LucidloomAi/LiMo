// The guardian console's page: what the console's Tauri window shows on the
// PC (../../console): the guardian's screens (src/guardian), the sign in,
// the first start and Today, at the window's own width.
//
// The page holds no state of its own and keeps nothing in the browser's
// storage: every answer comes from LiMo's service through the console's
// Rust side (src/host.ts, place "console"), each request by the door's own
// name.

import { useEffect, useState } from 'react';
import { SafeAreaFrameContext, SafeAreaInsetsContext } from 'react-native-safe-area-context';

import { GuardianApp } from '../guardian/GuardianApp';
import { PlaceContext } from '../guardian/parts';
import { guardianHost } from '../host';
import { guardThePage } from './pageGuard';
import { startSelfCheckIfAsked } from './selfCheck';

guardThePage();

/** A window has no notch and no system bars over it: no insets, and the window's own size. */
const NO_INSETS = { top: 0, bottom: 0, left: 0, right: 0 };
function windowFrame() {
  const w = typeof window === 'undefined' ? 1100 : window.innerWidth;
  const h = typeof window === 'undefined' ? 760 : window.innerHeight;
  return { x: 0, y: 0, width: w, height: h };
}

export function ConsoleApp() {
  const [frame, setFrame] = useState(windowFrame);
  useEffect(() => {
    startSelfCheckIfAsked();
    if (typeof window === 'undefined') return;
    const resize = () => setFrame(windowFrame());
    window.addEventListener('resize', resize);
    return () => window.removeEventListener('resize', resize);
  }, []);
  return (
    <SafeAreaFrameContext.Provider value={frame}>
      <SafeAreaInsetsContext.Provider value={NO_INSETS}>
        {/* The console's window on the family's PC, which Windows draws. */}
        <PlaceContext.Provider value="pc">
          <GuardianApp door={guardianHost} />
        </PlaceContext.Provider>
      </SafeAreaInsetsContext.Provider>
    </SafeAreaFrameContext.Provider>
  );
}
