// The child form: which screen shows, and what each tap asks of the host.
//
//   not paired                      S0, and nothing else
//   paired, no guardian agreed yet  S1 welcome, S2 what LiMo does here
//   agreed, introduction not seen   the first of the six steps not on, or done
//   introduction seen               Today and How it works, under the tab bar
//
// Where to start is worked out once, from what the host says when the app
// opens, and again only when the pairing changes; after that the person
// moves through the screens and Android's back button goes back. A step is
// never taken on trust: its screen asks the host again whenever the app
// comes back to the front, and after Android's own screen was opened.
//
// A step never traps a guardian: once Android's screen for it has been
// opened twice on one visit and the step is still off, the screen offers to
// go on without it. Setup then goes on, and Today's banner names the step,
// because the engine says what is off.

import { useCallback, useEffect, useRef, useState } from 'react';
import { BackHandler } from 'react-native';

import { type ChildHost, isSamsung, STEP_IDS, type StepId, type StepStates } from '../host';
import { Page } from '../ui/paper/Page';
import { Tabs } from '../ui/paper/Tabs';
import { HowItWorks, Today } from './AppScreens';
import { useChildHost } from './HostContext';
import { Agree, Done, Intro, NotSetUp, Step, Welcome } from './SetupScreens';
import { useHostValue } from './useHostValue';
import * as W from './words';

export type Tab = 'today' | 'how';

export type Route =
  | { readonly at: 'not-set-up' }
  | { readonly at: 'welcome' }
  | { readonly at: 'agree'; readonly ticked?: boolean }
  /**
   * `fixing`: the steps a guardian is putting right from Today's banner.
   * `triedOff`: how many times, on this visit, Android's screen for the step
   * was opened and the step came back still off.
   */
  | { readonly at: 'step'; readonly step: StepId; readonly fixing?: readonly StepId[]; readonly triedOff?: number }
  | { readonly at: 'done' }
  | { readonly at: 'intro'; readonly index: number }
  | { readonly at: 'app'; readonly tab: Tab };

/** Where the child form starts, from what the host says now. */
export async function startRoute(host: ChildHost): Promise<Route> {
  const pairing = await host.pairing();
  if (!pairing.paired) return { at: 'not-set-up' };
  const [agreed, introSeen] = await Promise.all([host.agreed(), host.introSeen()]);
  if (!agreed) return { at: 'welcome' };
  if (introSeen) return { at: 'app', tab: 'today' };
  const steps = await host.steps();
  const firstOff = STEP_IDS.find((id) => steps[id] !== 'on');
  return firstOff ? { at: 'step', step: firstOff } : { at: 'done' };
}

/** After this many tries that came back off, a step's screen offers to go on without it. */
export const TRIES_BEFORE_GOING_ON = 2;

const TAB_ITEMS = [
  { key: 'today', label: W.tabs.today, icon: 'sun' },
  { key: 'how', label: W.tabs.how, icon: 'help' },
] as const;

export function ChildApp({ initial }: { initial?: Route }) {
  const host = useChildHost();
  const [stack, setStack] = useState<readonly Route[]>(initial ? [initial] : []);
  const route = stack.length ? stack[stack.length - 1] : null;

  // Where to start: when the app opens, and again whenever the pairing changes
  // (the PC pairs this phone while S0 shows, or removes it).
  const pairing = useHostValue((h) => h.pairing(), ['pairing']);
  const paired = pairing.state === 'ready' ? pairing.value.paired : null;
  const lastPaired = useRef<boolean | null>(initial ? true : null);
  useEffect(() => {
    if (paired === null) return;
    if (lastPaired.current === paired && stack.length) return;
    lastPaired.current = paired;
    let live = true;
    startRoute(host).then(
      (r) => live && setStack([r]),
      // A host that cannot answer shows "not set up yet": nothing else can be shown safely.
      () => live && setStack([{ at: 'not-set-up' }]),
    );
    return () => {
      live = false;
    };
    // Runs again only when the pairing changes; stack.length is read only to
    // know whether a start was already chosen (the preview and the tests
    // give one).
  }, [paired, host]);

  // Who made the phone, asked once: it never changes.
  const maker = useHostValue((h) => h.maker(), []);

  const go = useCallback((r: Route) => setStack((s) => [...s, r]), []);
  const replace = useCallback((r: Route) => setStack((s) => [...s.slice(0, -1), r]), []);
  const reset = useCallback((r: Route) => setStack([r]), []);
  // One more try that came back off, kept on the step's own place in the
  // stack, so going back to a step finds its tries still there.
  const triedOff = useCallback(
    (step: StepId) =>
      setStack((s) => {
        const top = s[s.length - 1];
        if (top?.at !== 'step' || top.step !== step) return s;
        return [...s.slice(0, -1), { ...top, triedOff: (top.triedOff ?? 0) + 1 }];
      }),
    [],
  );

  // Android's back button goes back a screen; on the first screen it leaves.
  useEffect(() => {
    const sub = BackHandler.addEventListener('hardwareBackPress', () => {
      if (stack.length > 1) {
        setStack((s) => s.slice(0, -1));
        return true;
      }
      const top = stack[0];
      if (top?.at === 'app' && top.tab === 'how') {
        reset({ at: 'app', tab: 'today' });
        return true;
      }
      return false;
    });
    return () => sub.remove();
  }, [stack, reset]);

  const name = pairing.state === 'ready' && pairing.value.paired ? pairing.value.childName : '';

  // Plain paper until it is known where to start, and until the child's name
  // is known for a screen that says it.
  if (!route || (route.at !== 'not-set-up' && !name)) return <Page>{null}</Page>;
  switch (route.at) {
    case 'not-set-up':
      return <NotSetUp />;
    case 'welcome':
      return <Welcome name={name} onStart={() => go({ at: 'agree' })} />;
    case 'agree':
      return <AgreeRoute name={name} initiallyTicked={route.ticked} onAgreed={() => go({ at: 'step', step: STEP_IDS[0] })} />;
    case 'step': {
      const { step, fixing } = route;
      // Onward, after Next or going on without the step: the next step, or
      // done; from Today's banner, the next step after this one that is
      // still off, or back to Today.
      const onward = (steps: StepStates) => {
        if (fixing) {
          const next = fixing.slice(fixing.indexOf(step) + 1).find((id) => steps[id] !== 'on');
          if (next) replace({ at: 'step', step: next, fixing });
          else reset({ at: 'app', tab: 'today' });
          return;
        }
        const i = STEP_IDS.indexOf(step);
        go(i + 1 < STEP_IDS.length ? { at: 'step', step: STEP_IDS[i + 1] } : { at: 'done' });
      };
      // A note for one maker's phones waits for the maker, so it never comes in late and moves the stamp.
      if (W.steps[step].note?.only === 'samsung' && maker.state === 'waiting') return <Page>{null}</Page>;
      return (
        <StepRoute
          key={step}
          step={step}
          name={name}
          samsung={maker.state === 'ready' && isSamsung(maker.value)}
          stuck={(route.triedOff ?? 0) >= TRIES_BEFORE_GOING_ON}
          onTriedOff={() => triedOff(step)}
          onNext={onward}
          onGoOn={onward}
        />
      );
    }
    case 'done':
      return <DoneRoute name={name} onShow={() => go({ at: 'intro', index: 0 })} />;
    case 'intro':
      return (
        <Intro
          at={route.index}
          name={name}
          onNext={() => {
            if (route.index + 1 < W.intro.length) {
              go({ at: 'intro', index: route.index + 1 });
              return;
            }
            host.markIntroSeen().then(
              () => reset({ at: 'app', tab: 'today' }),
              () => reset({ at: 'app', tab: 'today' }),
            );
          }}
        />
      );
    case 'app':
      return (
        <AppRoute
          name={name}
          tab={route.tab}
          onTab={(tab) => replace({ at: 'app', tab })}
          onFix={(off) => go({ at: 'step', step: off[0], fixing: off })}
        />
      );
  }
}

function AgreeRoute({ name, initiallyTicked, onAgreed }: { name: string; initiallyTicked?: boolean; onAgreed: () => void }) {
  const host = useChildHost();
  const agreed = useHostValue((h) => h.agreed(), ['agreement']);
  const [ticked, setTicked] = useState<boolean | null>(initiallyTicked ?? null);
  // Coming back to this screen after agreeing, the box shows ticked.
  const shown = ticked ?? (agreed.state === 'ready' && agreed.value);
  return (
    <Agree
      name={name}
      ticked={shown}
      onTick={() => setTicked(!shown)}
      onContinue={() => {
        if (!shown) return;
        host.agree().then(onAgreed, onAgreed);
      }}
    />
  );
}

function StepRoute({
  step,
  name,
  samsung,
  stuck,
  onTriedOff,
  onNext,
  onGoOn,
}: {
  step: StepId;
  name: string;
  samsung: boolean;
  stuck: boolean;
  onTriedOff: () => void;
  onNext: (steps: StepStates) => void;
  onGoOn: (steps: StepStates) => void;
}) {
  const host = useChildHost();
  const steps = useHostValue((h) => h.steps(), ['steps'], { whenBackInFront: true });
  const [notOpened, setNotOpened] = useState(false);
  const state = steps.state === 'ready' && steps.value[step] === 'on' ? 'on' : 'off';
  const now = () => (steps.state === 'ready' ? steps.value : ({} as StepStates));
  // Android is asked again once its screen is open, or could not be opened;
  // a try counts once that answer is in and the step is still off, so the
  // way on never shows for a moment before the step turns on.
  const askAgain = () =>
    steps.again().then((answer) => {
      if (!(answer?.state === 'ready' && answer.value[step] === 'on')) onTriedOff();
    });
  return (
    <Step
      id={step}
      name={name}
      state={state}
      samsung={samsung}
      notOpened={notOpened && state !== 'on'}
      stuck={stuck}
      onOpen={() => {
        setNotOpened(false);
        host.openStep(step).then(askAgain, () => {
          setNotOpened(true);
          return askAgain();
        });
      }}
      onNext={() => onNext(now())}
      onGoOn={() => onGoOn(now())}
    />
  );
}

function DoneRoute({ name, onShow }: { name: string; onShow: () => void }) {
  const steps = useHostValue((h) => h.steps(), ['steps'], { whenBackInFront: true });
  return <Done name={name} steps={steps.state === 'ready' ? steps.value : null} onShow={onShow} />;
}

function AppRoute({
  name,
  tab,
  onTab,
  onFix,
}: {
  name: string;
  tab: Tab;
  onTab: (tab: Tab) => void;
  onFix: (off: readonly StepId[]) => void;
}) {
  const host = useChildHost();
  const today = useHostValue((h) => h.today(), ['today', 'steps'], { whenBackInFront: true });
  const rules = useHostValue((h) => h.rules(), ['rules']);
  const version = useHostValue((h) => h.version(), []);
  const view = today.state === 'ready' ? today.value : null;
  return (
    <Page
      testID={`screen-${tab}`}
      tabs={(onHeight) => <Tabs items={TAB_ITEMS} picked={tab} onPick={onTab} onHeight={onHeight} />}
    >
      {tab === 'today' ? (
        <Today
          name={name}
          view={view}
          onAsk={() => {
            host.askForMoreTime().catch(() => undefined);
          }}
          onShowGuardian={() => {
            if (view?.guardianNeeded) onFix(view.guardianNeeded.off);
          }}
        />
      ) : (
        <HowItWorks
          name={name}
          rules={rules.state === 'ready' ? rules.value : null}
          version={version.state === 'ready' ? version.value : null}
        />
      )}
    </Page>
  );
}
