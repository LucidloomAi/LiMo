// The child form: which screen shows, and what each tap asks of the host.
//
//   not paired                      S0, and nothing else
//   paired, no guardian agreed yet  S1 welcome, S2 what LiMo does here
//   agreed, introduction not seen   the first step not on (S3 to S7), or S8
//   introduction seen               Today and How it works, under the tab bar
//
// Where to start is worked out once, from what the host says when the app
// opens, and again only when the pairing changes; after that the person
// moves through the screens and Android's back button goes back. A step is
// never taken on trust: its screen asks the host again whenever the app
// comes back to the front, and after Android's own screen was opened.

import { useCallback, useEffect, useRef, useState } from 'react';
import { BackHandler } from 'react-native';

import { type ChildHost, STEP_IDS, type StepId, type StepStates } from '../host';
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
  /** `fixing`: the steps a guardian is putting right from Today's banner. */
  | { readonly at: 'step'; readonly step: StepId; readonly fixing?: readonly StepId[] }
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

  const go = useCallback((r: Route) => setStack((s) => [...s, r]), []);
  const replace = useCallback((r: Route) => setStack((s) => [...s.slice(0, -1), r]), []);
  const reset = useCallback((r: Route) => setStack([r]), []);

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
    case 'step':
      return (
        <StepRoute
          key={route.step}
          step={route.step}
          name={name}
          onNext={(steps) => {
            if (route.fixing) {
              const next = route.fixing.find((id) => id !== route.step && steps[id] !== 'on');
              if (next) replace({ at: 'step', step: next, fixing: route.fixing });
              else reset({ at: 'app', tab: 'today' });
              return;
            }
            const i = STEP_IDS.indexOf(route.step);
            go(i + 1 < STEP_IDS.length ? { at: 'step', step: STEP_IDS[i + 1] } : { at: 'done' });
          }}
        />
      );
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

function StepRoute({ step, name, onNext }: { step: StepId; name: string; onNext: (steps: StepStates) => void }) {
  const host = useChildHost();
  const steps = useHostValue((h) => h.steps(), ['steps'], { whenBackInFront: true });
  const [notOpened, setNotOpened] = useState(false);
  const state = steps.state === 'ready' && steps.value[step] === 'on' ? 'on' : 'off';
  return (
    <Step
      id={step}
      name={name}
      state={state}
      notOpened={notOpened && state !== 'on'}
      onOpen={() => {
        setNotOpened(false);
        host.openStep(step).then(
          () => steps.again(),
          () => {
            setNotOpened(true);
            steps.again();
          },
        );
      }}
      onNext={() => onNext(steps.state === 'ready' ? steps.value : ({} as StepStates))}
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
