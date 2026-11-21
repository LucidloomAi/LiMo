// The guardian's screens, and which one shows:
//
//   no family yet                     the first start (F1 to F8)
//   a family, this window signed out  sign in (C0)
//   signed in                         Today, Rules, Devices and More, under the tabs
//   a child's Windows account         sign in says so, with no field (C0d)
//
// Where to start is asked of the door when the screens open
// (family_state). A first start left before its PIN or its child goes on
// from there after sign in. A session the service ended (15 minutes with
// nothing done) comes back to sign in with a plain sentence; a refusal, at
// any time, shows the sign in's refusal. Under a tab, a screen of its own
// opens with Back (a one time code, the password and the PIN, About, and
// each screen that asks for the password again). Deleting the family goes
// back to the first start. Adding a phone is still being drawn: it says so
// in one sentence.

import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { StyleSheet, View } from 'react-native';

import { type AccountRow, ask, type GuardianDoor, hostOf, type RuleSet } from '../guardianSeam';
import { AddPhoneComing, Devices } from './Devices';
import { FirstStart, type FirstStartState } from './FirstStart';
import type { TabKey } from './GuardianTabs';
import { About, Code, DeleteFamily, MoreList, RemoveChild, RemoveLimo, RemovePhone, Secrets } from './More';
import { Page } from './Page';
import { Rules } from './Rules';
import { SignIn, type SignInStart } from './SignIn';
import { Today } from './Today';
import * as W from './words';

/** A screen of its own under a tab. */
export type GuardianSub =
  | { readonly at: 'code' }
  | { readonly at: 'secrets' }
  | { readonly at: 'about' }
  | { readonly at: 'delete-family' }
  | { readonly at: 'remove-limo' }
  | { readonly at: 'remove-child'; readonly child: { readonly childId: string; readonly name: string } }
  | { readonly at: 'remove-phone'; readonly device: { readonly deviceId: string; readonly name: string } }
  | { readonly at: 'add-phone' };

/** For the preview's pictures only: a screen in a state a guardian reaches by tapping. */
export type GuardianShow = {
  readonly rulesDraft?: (rules: RuleSet) => RuleSet;
  readonly rulesSave?: boolean;
  readonly rulesSaved?: boolean;
  readonly codeMade?: boolean;
};

export type GuardianRoute =
  | { readonly at: 'sign-in'; readonly start?: SignInStart }
  | { readonly at: 'first'; readonly initial?: Partial<FirstStartState>; readonly accounts?: readonly AccountRow[] }
  | { readonly at: 'app'; readonly tab: TabKey; readonly fromFirstStart?: boolean; readonly sub?: GuardianSub; readonly show?: GuardianShow };

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
  const open = useCallback((t: TabKey, sub: GuardianSub) => setRoute({ at: 'app', tab: t, sub }), []);
  const familyDeleted = useCallback(() => {
    startRoute(door).then((r) => {
      if (live.current) setRoute(r);
    });
  }, [door]);

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
          onDone={(addPhone) => setRoute(addPhone ? { at: 'app', tab: 'devices', sub: { at: 'add-phone' } } : { at: 'app', tab: 'today', fromFirstStart: true })}
        />
      );
    case 'app': {
      const common = { door, onTab: tab, onSignedOut: signedOut, onRefused: refused };
      const sub = route.sub;
      const show = route.show ?? {};
      let screen;
      if (sub) {
        const back = () => tab(route.tab);
        switch (sub.at) {
          case 'code':
            screen = <Code {...common} onBack={back} makeAtStart={show.codeMade} />;
            break;
          case 'secrets':
            screen = <Secrets {...common} onBack={back} />;
            break;
          case 'about':
            screen = <About door={door} onTab={tab} onBack={back} onRemoveLimo={() => open('more', { at: 'remove-limo' })} onDeleteFamily={() => open('more', { at: 'delete-family' })} />;
            break;
          case 'delete-family':
            screen = <DeleteFamily {...common} onBack={() => open('more', { at: 'about' })} onDeleted={familyDeleted} />;
            break;
          case 'remove-limo':
            screen = <RemoveLimo {...common} onBack={() => open('more', { at: 'about' })} />;
            break;
          case 'remove-child':
            screen = <RemoveChild {...common} child={sub.child} onBack={back} onRemoved={back} />;
            break;
          case 'remove-phone':
            screen = <RemovePhone {...common} device={sub.device} onBack={back} onRemoved={back} />;
            break;
          case 'add-phone':
            screen = <AddPhoneComing onBack={back} onTab={tab} />;
            break;
        }
      } else {
        switch (route.tab) {
          case 'today':
            screen = <Today {...common} fromFirstStart={route.fromFirstStart} onAddPhone={() => open('devices', { at: 'add-phone' })} />;
            break;
          case 'rules':
            screen = (
              <Rules
                {...common}
                onRemove={(c) => open('rules', { at: 'remove-child', child: { childId: c.childId, name: c.name } })}
                initialDraft={show.rulesDraft}
                saveAtStart={show.rulesSave}
                initialSaved={show.rulesSaved}
              />
            );
            break;
          case 'devices':
            screen = (
              <Devices
                {...common}
                onRemovePhone={(d) => open('devices', { at: 'remove-phone', device: { deviceId: d.deviceId, name: d.name } })}
                onAddPhone={() => open('devices', { at: 'add-phone' })}
              />
            );
            break;
          case 'more':
            screen = <MoreList onTab={tab} onOpen={(key) => open('more', { at: key })} />;
            break;
        }
      }
      // Marked as signed in, for the console's self check (src/console/selfCheck.ts).
      return (
        <View style={styles.fill} testID="signed-in">
          {screen}
        </View>
      );
    }
  }
}

const styles = StyleSheet.create({ fill: { flex: 1 } });
