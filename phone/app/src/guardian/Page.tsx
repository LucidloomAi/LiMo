// A guardian screen in Paper and Ink: warm paper with a faint dot grid,
// what scrolls, and the main button pinned to the bottom on a strip of the
// page itself (its dots lined up with the page's), so nothing shows through
// it. On a phone a pinned button is as wide as the screen, and two stand one
// above the other; at the console's width they stand at the right of a
// column of 560. A screen with the floating tab bar keeps room at its end so
// its last row scrolls clear of it.

import { type ReactNode, type RefObject, useLayoutEffect, useRef, useState } from 'react';
import { type LayoutChangeEvent, ScrollView, StyleSheet, View } from 'react-native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import Svg, { Circle, Defs, Pattern, Rect } from 'react-native-svg';

import { color } from '../theme';
import { useSizes } from './parts';

let grids = 0;

/** The page's dots: one every 18, in ink at 13 percent, drawn from `originY` so a strip can line its dots up with the page's. */
function DotGrid({ originY = 0 }: { originY?: number }) {
  const [id] = useState(() => {
    grids += 1;
    return `guardian-dots-${grids}`;
  });
  return (
    <Svg width="100%" height="100%" style={StyleSheet.absoluteFill} pointerEvents="none" importantForAccessibility="no-hide-descendants">
      <Defs>
        <Pattern id={id} x={0} y={originY} width={18} height={18} patternUnits="userSpaceOnUse">
          <Circle cx={9} cy={9} r={1.1} fill="rgba(27,38,56,0.13)" />
        </Pattern>
      </Defs>
      <Rect x={0} y={0} width="100%" height="100%" fill={`url(#${id})`} />
    </Svg>
  );
}

/** Room the tab bar takes at a page's end: it floats 14 above the bottom, 69 high, and the page keeps a little more. */
const TABBED_END = 104;

/** A strip of its own pinned to the bottom: its height at each width, and whether it spans the page or the reading column. */
export type Strip = { readonly node: ReactNode; readonly height: { readonly phone: number; readonly wide: number }; readonly full?: boolean };

export function Page({
  children,
  pinned,
  strip,
  tabs,
  over,
  scrollRef,
  testID,
}: {
  /** The page: what scrolls. */
  children: ReactNode;
  /** One button pinned to the bottom, or two (the second is the main one). */
  pinned?: ReactNode | readonly [ReactNode, ReactNode];
  /** Or a strip of its own, such as a line over Undo and Save. */
  strip?: Strip;
  /** The floating tab bar. */
  tabs?: ReactNode;
  /** Something that floats over the page for a moment, such as a notice. */
  over?: ReactNode;
  /** For a screen that scrolls itself to a row. */
  scrollRef?: RefObject<ScrollView | null>;
  testID?: string;
}) {
  const insets = useSafeAreaInsets();
  const { wide, pin, pin2 } = useSizes();
  const [stripTop, setStripTop] = useState(0);
  // The page's own ref, always given: react-native-web takes a ScrollView's ref once, when it is first drawn,
  // and a screen may ask for it only later. Handed on after every drawing.
  const scroll = useRef<ScrollView | null>(null);
  useLayoutEffect(() => {
    if (scrollRef) scrollRef.current = scroll.current;
  });
  const two = Array.isArray(pinned);
  const stripHeight = strip ? (wide ? strip.height.wide : strip.height.phone) : pinned ? (two ? pin2 : pin) : 0;

  const onStrip = (e: LayoutChangeEvent) => {
    const y = e.nativeEvent.layout.y;
    if (y !== stripTop) setStripTop(y);
  };

  return (
    <View style={styles.root} testID={testID}>
      <DotGrid />
      <ScrollView
        ref={scroll}
        style={[styles.scroll, { marginTop: insets.top, marginBottom: insets.bottom + stripHeight }]}
        contentContainerStyle={[
          styles.content,
          wide ? styles.contentWide : styles.contentPhone,
          { paddingLeft: (wide ? 32 : 16) + insets.left, paddingRight: (wide ? 32 : 16) + insets.right },
          tabs ? { paddingBottom: TABBED_END } : null,
        ]}
        showsVerticalScrollIndicator={false}
      >
        {children}
      </ScrollView>
      {pinned || strip ? (
        <View
          style={[styles.strip, wide ? styles.stripWide : styles.stripPhone, { height: stripHeight, bottom: insets.bottom, paddingLeft: (wide ? 32 : 16) + insets.left, paddingRight: (wide ? 32 : 16) + insets.right }]}
          onLayout={onStrip}
          testID="pinned"
        >
          <DotGrid originY={-stripTop} />
          {strip ? (
            <View style={wide ? [styles.pinRowWide, styles.stripRowWide, strip.full && styles.full] : styles.pinColumn}>{strip.node}</View>
          ) : (
            <View style={wide ? styles.pinRowWide : styles.pinColumn}>
              {two ? (pinned as readonly ReactNode[]).map((p, i) => <View key={i} style={wide ? null : styles.pinFull}>{p}</View>) : <View style={wide ? null : styles.pinFull}>{pinned as ReactNode}</View>}
            </View>
          )}
        </View>
      ) : null}
      {over ?? null}
      {tabs ?? null}
    </View>
  );
}

/** One column of reading width, in the middle: the forms. */
export function Column({ children }: { children: ReactNode }) {
  return <View style={styles.column}>{children}</View>;
}

const styles = StyleSheet.create({
  root: { flex: 1, backgroundColor: color.paper, overflow: 'hidden' },
  scroll: { flex: 1 },
  content: { flexGrow: 1 },
  contentPhone: { paddingTop: 4, paddingBottom: 24 },
  contentWide: { paddingTop: 16, paddingBottom: 28 },
  strip: { position: 'absolute', left: 0, right: 0, backgroundColor: color.paper, overflow: 'hidden' },
  stripPhone: { paddingTop: 8 },
  stripWide: { paddingTop: 12 },
  pinColumn: { flexDirection: 'column', gap: 10 },
  pinFull: { alignSelf: 'stretch' },
  pinRowWide: { flexDirection: 'row', justifyContent: 'flex-end', gap: 12, width: '100%', maxWidth: 560, alignSelf: 'center' },
  stripRowWide: { alignItems: 'center' },
  full: { maxWidth: '100%' },
  column: { width: '100%', maxWidth: 560, alignSelf: 'center' },
});
