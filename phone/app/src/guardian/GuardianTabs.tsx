// The guardian's tabs: Today, Rules, Devices, More. The floating bar the
// child's app has (src/ui/paper/Tabs.tsx): on a phone set in 14 from the
// sides and the bottom; at the console's width it keeps a width of 440 in
// the middle, 14 above the window's edge. Rules has its own icon, a
// clipboard of ticked rows.

import { StyleSheet, View } from 'react-native';

import { Tabs } from '../ui/paper/Tabs';
import { useScreenWidth, useWide } from './parts';
import * as W from './words';

export type TabKey = 'today' | 'rules' | 'devices' | 'more';

const ITEMS = [
  { key: 'today', label: W.tabs.today, icon: 'sun' },
  { key: 'rules', label: W.tabs.rules, icon: 'rules' },
  { key: 'devices', label: W.tabs.devices, icon: 'devices' },
  { key: 'more', label: W.tabs.more, icon: 'gear' },
] as const;

/** The bar's width at the console's width, and the room the shared bar keeps at each side of it. */
const WIDE_BAR = 440;
const SIDE = 14;

export function GuardianTabs({ picked, onPick }: { picked: TabKey; onPick: (key: TabKey) => void }) {
  const wide = useWide();
  const width = useScreenWidth();
  const bar = <Tabs items={ITEMS} picked={picked} onPick={onPick} />;
  if (!wide) return bar;
  return (
    <View pointerEvents="box-none" style={[styles.middle, { left: (width - WIDE_BAR) / 2 - SIDE, width: WIDE_BAR + 2 * SIDE }]}>
      {bar}
    </View>
  );
}

const styles = StyleSheet.create({
  middle: { position: 'absolute', bottom: 0, height: 120 },
});
