// A phone screen in Paper and Ink: warm paper with a faint dot grid, filling
// the display edge to edge, with every word kept clear of Android's status
// bar, navigation bar and any cut out (the safe area insets). What does not
// fit scrolls between them.
//
// A screen may pin its main button to the bottom, on a strip of the page
// itself with its dots lined up with the page's, so nothing shows through
// it; the page then scrolls above it. A screen with tabs keeps room at its
// end as tall as the floating tab bar, so the last row can be scrolled
// clear of it.

import { type ReactNode, useState } from 'react';
import { type LayoutChangeEvent, ScrollView, StyleSheet, View } from 'react-native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import Svg, { Circle, Defs, Pattern, Rect } from 'react-native-svg';

import { color } from '../../theme';

/** The page's side margins, and the strip that holds a pinned button: 8 above it, 48 for it, 8 below. */
const SIDE = 16;
const PIN_STRIP = 64;
/** The tab bar is set in from the bottom by this much, and the page keeps this much more below its last row. */
export const TAB_INSET = 14;
const TAB_ROOM_EXTRA = 7;
/** The tab bar's height before it is measured: 6 + 54 + 6 inside a 1.5 border. */
export const TAB_BAR_HEIGHT = 69;

let grids = 0;

/**
 * The page's dot grid: a dot every 18, 1 across, in ink at 13 percent,
 * drawn from `originY` so a strip can line its dots up with the page's.
 */
function DotGrid({ originY = 0 }: { originY?: number }) {
  const [id] = useState(() => {
    grids += 1;
    return `limo-dots-${grids}`;
  });
  return (
    <Svg
      width="100%"
      height="100%"
      style={StyleSheet.absoluteFill}
      pointerEvents="none"
      importantForAccessibility="no-hide-descendants"
    >
      <Defs>
        <Pattern id={id} x={0} y={originY} width={18} height={18} patternUnits="userSpaceOnUse">
          <Circle cx={9} cy={9} r={1.1} fill="rgba(27,38,56,0.13)" />
        </Pattern>
      </Defs>
      <Rect x={0} y={0} width="100%" height="100%" fill={`url(#${id})`} />
    </Svg>
  );
}

export type PageProps = {
  /** What scrolls: the top row, the cards, and the maker's mark at the end. */
  children: ReactNode;
  /** The main button, pinned to the bottom. */
  pinned?: ReactNode;
  /** The floating tab bar, given its height when it is measured. */
  tabs?: (onHeight: (height: number) => void) => ReactNode;
  testID?: string;
};

export function Page({ children, pinned, tabs, testID }: PageProps) {
  const insets = useSafeAreaInsets();
  const [strip, setStrip] = useState({ height: PIN_STRIP, top: 0 });
  const [barHeight, setBarHeight] = useState(TAB_BAR_HEIGHT);

  const onStrip = (e: LayoutChangeEvent) => {
    const { height, y } = e.nativeEvent.layout;
    if (height !== strip.height || y !== strip.top) setStrip({ height, top: y });
  };

  const sides = { paddingLeft: SIDE + insets.left, paddingRight: SIDE + insets.right };
  return (
    <View style={styles.root} testID={testID}>
      <DotGrid />
      <ScrollView
        style={[styles.scroll, { marginTop: insets.top, marginBottom: insets.bottom + (pinned ? strip.height : 0) }]}
        contentContainerStyle={[
          styles.content,
          sides,
          { paddingBottom: tabs ? barHeight + TAB_INSET + TAB_ROOM_EXTRA : 6 },
        ]}
        showsVerticalScrollIndicator={false}
      >
        {children}
      </ScrollView>
      {pinned ? (
        <View style={[styles.strip, sides, { bottom: insets.bottom }]} onLayout={onStrip}>
          <DotGrid originY={-strip.top} />
          {pinned}
        </View>
      ) : null}
      {tabs ? tabs(setBarHeight) : null}
    </View>
  );
}

const styles = StyleSheet.create({
  root: { flex: 1, backgroundColor: color.paper, overflow: 'hidden' },
  scroll: { flex: 1 },
  content: { flexGrow: 1, paddingTop: 4 },
  strip: {
    position: 'absolute',
    left: 0,
    right: 0,
    paddingTop: 8,
    paddingBottom: 8,
    backgroundColor: color.paper,
    overflow: 'hidden',
  },
});
