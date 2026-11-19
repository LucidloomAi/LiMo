// For a screen reader and a finger: everything to tap has a role, a label and
// at least 48 of height, and each screen reads in the order it is drawn.

import { screen } from '@testing-library/react-native';
import { StyleSheet } from 'react-native';

import type { Route } from '../src/child/ChildApp';
import { type MockState, states, stepsOnUpTo } from '../src/mock/childMock';
import { renderChild } from './render';

const SCREENS: [string, MockState, Route | undefined, string][] = [
  ['S1', states.fresh, undefined, 'This phone is now Mira’s.'],
  ['S2', states.fresh, { at: 'agree' }, 'What LiMo does here'],
  ['S4', { ...states.agreed, steps: stepsOnUpTo(1) }, { at: 'step', step: 'usage' }, 'Usage access'],
  ['S6', { ...states.agreed, steps: stepsOnUpTo(3) }, { at: 'step', step: 'battery' }, 'Battery'],
  ['S8', states.allOn, undefined, 'This phone is set up.'],
  ['C3', states.allOn, { at: 'intro', index: 2 }, 'How to ask for more time'],
  ['T', states.guardianNeeded, undefined, '1 h 12 min'],
  ['H', states.childApp, { at: 'app', tab: 'how' }, 'Your rules'],
];

/** Every string the screen shows, in the order a screen reader meets it. */
function textsInOrder(): string[] {
  const out: string[] = [];
  const walk = (node: ReturnType<typeof screen.toJSON>) => {
    if (!node) return;
    if (typeof node === 'string') {
      out.push(node);
      return;
    }
    if (Array.isArray(node)) {
      node.forEach(walk);
      return;
    }
    if (node.type === 'Text') {
      const text = flat(node);
      if (text) out.push(text);
      return;
    }
    (node.children ?? []).forEach(walk);
  };
  const flat = (node: ReturnType<typeof screen.toJSON>): string =>
    !node ? '' : typeof node === 'string' ? node : Array.isArray(node) ? node.map(flat).join('') : (node.children ?? []).map(flat).join('');
  walk(screen.toJSON());
  return out.map((t) => t.replace(/\s+/g, ' '));
}

function inOrder(texts: string[], wanted: string[]) {
  let at = -1;
  for (const w of wanted) {
    const i = texts.findIndex((t, k) => k > at && t === w);
    if (i < 0) throw new Error(`"${w}" is not after "${texts[at] ?? 'the start'}" in: ${texts.join(' | ')}`);
    at = i;
  }
}

describe.each(SCREENS)('%s', (_id, state, route, heading) => {
  it('gives everything to tap a role, a label and at least 48 of height', async () => {
    renderChild(state, { route });
    await screen.findByText(heading);
    // Everything that can be pressed: the views React Native gives its press handling to.
    const taps = screen.root.findAll(
      (n: { type: unknown; props: Record<string, unknown> }) => typeof n.type === 'string' && typeof n.props.onResponderRelease === 'function',
    );
    expect(taps.length).toBeGreaterThan(0);
    for (const t of taps) {
      const style = StyleSheet.flatten(t.props.style) as { minHeight?: number; height?: number };
      expect(t.props.accessibilityRole).toEqual(expect.stringMatching(/^(button|checkbox|tab)$/));
      expect(t.props.accessibilityLabel).toEqual(expect.any(String));
      expect(t.props.accessibilityLabel.length).toBeGreaterThan(0);
      expect(Math.max(style.minHeight ?? 0, style.height ?? 0)).toBeGreaterThanOrEqual(48);
    }
  });

  it('has one heading', async () => {
    renderChild(state, { route });
    await screen.findByText(heading);
    expect(screen.getAllByRole('header').length).toBeGreaterThanOrEqual(1);
  });
});

describe('reading order', () => {
  it('S4: LiMo, the step, why, what happens, where it stands, then the button', async () => {
    renderChild({ ...states.agreed, steps: stepsOnUpTo(1) }, { route: { at: 'step', step: 'usage' } });
    await screen.findByText('Usage access');
    inOrder(textsInOrder(), ['LiMo', 'Step 2 of 5', 'Usage access', 'Why', 'What happens', 'Not on yet', 'Open Android’s settings']);
  });

  it('T: LiMo, the name, the banner, the time left, the bedtime line, asking, the apps, then the tabs', async () => {
    renderChild(states.guardianNeeded);
    await screen.findByText('1 h 12 min');
    inOrder(textsInOrder(), [
      'LiMo',
      'Mira',
      'Needs a guardian',
      'Accessibility is switched off, so LiMo cannot see which app is open.',
      'Show a guardian what to do',
      '1 h 12 min',
      'Left today',
      '48 min used of 2 h',
      'Screens are off from 9:00 pm to 7:00 am.',
      'Ask for more time',
      'Today on this phone',
      'Maps',
      'Clock',
      'Today',
      'How it works',
    ]);
  });
});
