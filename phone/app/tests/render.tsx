// Renders the child form against a mock host, inside a phone's safe areas,
// as the preview does. Tests only.

import { render, screen } from '@testing-library/react-native';
import type { ReactElement } from 'react';
import { SafeAreaFrameContext, SafeAreaInsetsContext } from 'react-native-safe-area-context';

import { ChildApp, type Route } from '../src/child/ChildApp';
import { ChildHostProvider } from '../src/child/HostContext';
import { type MockHost, mockHost, type MockOptions, type MockState } from '../src/mock/childMock';

const FRAME = { x: 0, y: 0, width: 390, height: 844 };
const INSETS = { top: 32, bottom: 24, left: 0, right: 0 };

export function inPhone(ui: ReactElement) {
  return (
    <SafeAreaFrameContext.Provider value={FRAME}>
      <SafeAreaInsetsContext.Provider value={INSETS}>{ui}</SafeAreaInsetsContext.Provider>
    </SafeAreaFrameContext.Provider>
  );
}

/** The child form, asking `host`, starting at `route` or where the host's state says. */
export function renderChild(state: MockState, options: { route?: Route; mock?: MockOptions } = {}): { host: MockHost } & ReturnType<typeof render> {
  const host = mockHost(state, options.mock);
  const result = render(
    inPhone(
      <ChildHostProvider host={host}>
        <ChildApp initial={options.route} />
      </ChildHostProvider>,
    ),
  );
  return { host, ...result };
}

/** A node of the rendered tree, as the tests walk it with findAll. */
export type TreeNode = { type: unknown; props: Record<string, any>; findAll: (match: (n: TreeNode) => boolean) => TreeNode[] };

/** Every string the screen shows, in the order a screen reader meets it. */
export function textsInOrder(): string[] {
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

/** The wanted strings are among `texts`, in this order. */
export function inOrder(texts: string[], wanted: string[]) {
  let at = -1;
  for (const w of wanted) {
    const i = texts.findIndex((t, k) => k > at && t === w);
    if (i < 0) throw new Error(`"${w}" is not after "${texts[at] ?? 'the start'}" in: ${texts.join(' | ')}`);
    at = i;
  }
}
