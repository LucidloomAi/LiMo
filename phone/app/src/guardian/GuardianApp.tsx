// The guardian's screens, and which one shows:
//
//   no family yet                     the first start (F1 to F8)
//   a family, this window signed out  sign in (C0)
//   signed in                         Today, under the tabs
//   a child's Windows account         sign in says so, with no field (C0d)
//
// Where to start is asked of the door when the screens open
// (family_state). A first start left before its PIN or its child goes on
// from there after sign in. A session the service ended (15 minutes with
// nothing done) comes back to sign in with a plain sentence; a refusal, at
// any time, shows the sign in's refusal. Rules, Devices and More, and adding
// a phone, are still being drawn: they say so in one sentence.

import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { StyleSheet, View } from 'react-native';

import { type AccountRow, ask, type GuardianDoor, hostOf } from '../guardianSeam';
import { FirstStart, type FirstStartState } from './FirstStart';
import type { TabKey } from './GuardianTabs';
import { Page } from './Page';
import { SignIn, type SignInStart } from './SignIn';
import { Coming, Today } from './Today';
import * as W from './words';

export type GuardianRoute =
  | { readonly at: 'sign-in'; readonly start?: SignInStart }
  | { readonly at: 'first'; readonly initial?: Partial<FirstStartState>; readonly accounts?: readonly AccountRow[] }
  | { readonly at: 'app'; readonly tab: TabKey; readonly fromFirstStart?: boolean };

/** Where a signed in guardian goes: on with a first start left before its PIN or child, else Today. */
async function afterSignIn(door: GuardianDoor): Promise<GuardianRoute> {
  const state = await ask(door, 'family_state', {});
  if (state.kind !== 'done') return { at: 'app', tab: 'today' };
  if (!state.value.hasPin) return { at: 'first', initial: { step: 2, ticked: true, familyMade: true } };
  if (state.value.children === 0) return { at: 'first', initial: { step: 3, ticked: true, familyMade: true, pinSet: true } };
  return { at: 'app', tab: 'today' };
}

/** Where the screens start, from what the door says now. */
export async function startRoute(door: GuardianDoor): Promise<GuardianRoute> {
  const state = await ask(door, 'family_state', {});
  // A refusal or a service that cannot be asked: the sign in asks again and says so.
  if (state.kind !== 'done') return { at: 'sign-in' };
  if (!state.value.hasFamily) return { at: 'first' };
  if (!state.value.signedIn) return { at: 'sign-in' };
  return afterSignIn(door);
}

export function GuardianApp({ door, initial }: { door: GuardianDoor; initial?: GuardianRoute }) {
  const [route, setRoute] = useState<GuardianRoute | null>(initial ?? null);
  // A new sign in each time one is shown, so it asks the door afresh.
  const [signInRound, setSignInRound] = useState(0);
  const live = useRef(true);
  const host = useMemo(() => hostOf(door), [door]);

  useEffect(() => {
    live.current = true;
    if (!initial) {
      startRoute(door).then((r) => {
        if (live.current) setRoute(r);
      });
    }
    return () => {
      live.current = false;
    };
    // Asked once, when the screens open.
  }, [door]);

  const toSignIn = useCallback((start?: SignInStart) => {
    setSignInRound((n) => n + 1);
    setRoute({ at: 'sign-in', start });
  }, []);
  const signedOut = useCallback(() => toSignIn({ line: W.signIn.signedOut }), [toSignIn]);
  const refused = useCallback(() => toSignIn(), [toSignIn]);
  const onSignedIn = useCallback(() => {
    afterSignIn(door).then((r) => {
      if (live.current) setRoute(r);
    });
  }, [door]);
  const tab = useCallback((t: TabKey) => setRoute({ at: 'app', tab: t }), []);

  if (!route) return <Page>{null}</Page>;
  switch (route.at) {
    case 'sign-in':
      return <SignIn key={signInRound} host={host} start={route.start} onSignedIn={onSignedIn} />;
    case 'first':
      return (
        <FirstStart
          door={door}
          initial={route.initial}
          accountsAtFirst={route.accounts}
          onSignedOut={signedOut}
          onRefused={refused}
          onDone={(addPhone) => setRoute({ at: 'app', tab: addPhone ? 'devices' : 'today', fromFirstStart: true })}
        />
      );
    case 'app':
      // Marked as signed in, for the console's self check (src/console/selfCheck.ts).
      return (
        <View style={styles.fill} testID="signed-in">
          {route.tab === 'today' ? (
            <Today
              door={door}
              fromFirstStart={route.fromFirstStart}
              onSignedOut={signedOut}
              onRefused={refused}
              onTab={tab}
              onAddPhone={() => tab('devices')}
            />
          ) : (
            <Coming tab={route.tab} onTab={tab} />
          )}
        </View>
      );
  }
}

const styles = StyleSheet.create({ fill: { flex: 1 } });
