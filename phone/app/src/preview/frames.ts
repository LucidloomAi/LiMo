// Every screen the preview draws, in every state, each from the mock host
// (src/mock/childMock.ts). The ids are the drawings' own (S0 to S8, C1 to C3,
// E, H, T), with a suffix for a state the drawings do not show.

import type { Route } from '../child/ChildApp';
import { STEP_IDS } from '../childSeam';
import { type MockState, states, stepsOnUpTo } from '../mock/childMock';

export type Frame = {
  readonly id: string;
  readonly title: string;
  readonly note: string;
  readonly state: MockState;
  /** Where the screen starts; without it, the child form works it out from the mock, as on a phone. */
  readonly route?: Route;
};

const STEP_NAMES = ['Notifications', 'Usage access', 'Accessibility', 'Battery', 'Protection'];

function stepFrames(): Frame[] {
  const out: Frame[] = [];
  STEP_IDS.forEach((step, i) => {
    const n = 3 + i;
    out.push({
      id: `S${n}`,
      title: `S${n} · ${STEP_NAMES[i]}`,
      note: `step ${i + 1} of 5 · not on yet`,
      state: { ...states.agreed, steps: stepsOnUpTo(i) },
      route: { at: 'step', step },
    });
    out.push({
      id: n === 4 ? 'S4b' : `S${n}-on`,
      title: `S${n}${n === 4 ? 'b' : ''} · ${STEP_NAMES[i]}, done`,
      note: `step ${i + 1} of 5 · on`,
      state: { ...states.agreed, steps: stepsOnUpTo(i + 1) },
      route: { at: 'step', step },
    });
  });
  return out;
}

/** The drawings' screens, each state. */
export const FRAMES: readonly Frame[] = [
  { id: 'S0', title: 'S0 · Not set up', note: 'not paired · no button', state: states.notPaired },
  { id: 'S1', title: 'S1 · Welcome', note: 'Start pinned', state: states.fresh },
  { id: 'S2', title: 'S2 · What LiMo does here', note: 'Continue pinned, switched off', state: states.fresh, route: { at: 'agree' } },
  { id: 'S2b', title: 'S2b · Ticked', note: 'Continue switched on', state: states.fresh, route: { at: 'agree', ticked: true } },
  ...stepFrames(),
  { id: 'S8', title: 'S8 · Done', note: 'button pinned', state: states.allOn },
  { id: 'C1', title: 'C1 · What LiMo counts', note: '1 of 3', state: states.allOn, route: { at: 'intro', index: 0 } },
  { id: 'C2', title: 'C2 · What LiMo never looks at', note: '2 of 3', state: states.allOn, route: { at: 'intro', index: 1 } },
  { id: 'C3', title: 'C3 · How to ask for more time', note: '3 of 3', state: states.allOn, route: { at: 'intro', index: 2 } },
  { id: 'E', title: 'E · Today', note: 'tab bar floats', state: states.childApp },
  { id: 'T', title: 'T · Today, something switched off', note: 'the banner', state: states.guardianNeeded },
  { id: 'H', title: 'H · How it works', note: 'second tab picked', state: states.childApp, route: { at: 'app', tab: 'how' } },
  { id: 'E-14min', title: 'E · 14 min left', note: 'not drawn: another time left', state: states.littleLeft },
  { id: 'E-42s', title: 'E · under a minute', note: 'not drawn: another time left', state: states.underAMinute },
  { id: 'E-nolimit', title: 'E · no limit today', note: 'not drawn: no bar, no label', state: states.noLimit },
  { id: 'E-open', title: 'E · open until', note: 'not drawn: a guardian unlocked', state: states.openUntil },
];

/** Long names and large text, where layouts break if they are going to. */
export const STRESS: readonly Frame[] = [
  { id: 'L-S1', title: 'S1 · a long name', note: '', state: { ...states.longNames, agreed: false, introSeen: false }, route: { at: 'welcome' } },
  { id: 'L-S2b', title: 'S2b · a long name', note: '', state: { ...states.longNames, agreed: false }, route: { at: 'agree', ticked: true } },
  { id: 'L-S4', title: 'S4 · a long name', note: '', state: { ...states.longNames, steps: stepsOnUpTo(1) }, route: { at: 'step', step: 'usage' } },
  { id: 'L-S6', title: 'S6 · a long name', note: '', state: { ...states.longNames, steps: stepsOnUpTo(3) }, route: { at: 'step', step: 'battery' } },
  { id: 'L-S8', title: 'S8 · a long name', note: '', state: { ...states.longNames, introSeen: false, steps: stepsOnUpTo(5) }, route: { at: 'done' } },
  { id: 'L-C1', title: 'C1 · a long name', note: '', state: states.longNames, route: { at: 'intro', index: 0 } },
  { id: 'L-T', title: 'T · long names', note: '', state: states.longNames, route: { at: 'app', tab: 'today' } },
  { id: 'L-H', title: 'H · long names', note: '', state: states.longNames, route: { at: 'app', tab: 'how' } },
];

export function findFrame(id: string): Frame | undefined {
  return FRAMES.find((f) => f.id === id) ?? STRESS.find((f) => f.id === id);
}
