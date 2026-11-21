// A phone drawn as the design draws it (design/screens/phone.css, .ph-frame):
// a plain ink outline, Android's status bar (32, only the time) at the top
// and its gesture bar (24) at the bottom, given to the screen inside as its
// safe area insets, as Android gives them on a phone. Preview only.

import type { ReactNode } from 'react';
import { StyleSheet, View } from 'react-native';
import { SafeAreaFrameContext, SafeAreaInsetsContext } from 'react-native-safe-area-context';

import { borderWidth, color } from '../theme';
import { Txt, TextScale, type } from '../ui/paper/text';

export type Size = { readonly width: number; readonly height: number; readonly corner: number };

export const SIZES = {
  390: { width: 390, height: 844, corner: 40 },
  360: { width: 360, height: 640, corner: 34 },
} as const satisfies Record<string, Size>;

const STATUS = 32;
const GESTURE = 24;

export function PhoneFrame({ size, textScale = 1, time = '9:42', children }: { size: Size; textScale?: number; time?: string; children: ReactNode }) {
  const frame = { x: 0, y: 0, width: size.width, height: size.height };
  const insets = { top: STATUS, bottom: GESTURE, left: 0, right: 0 };
  // The screen gets exactly its size, inside the outline, as the design's
  // frame has its border outside its 390 by 844. (Drawn as a separate ring:
  // a browser rounds a 1.5 border to whole pixels, which would eat into the
  // screen at some scales.)
  return (
    <View
      style={[
        styles.frame,
        { width: size.width + 2 * borderWidth, height: size.height + 2 * borderWidth, borderRadius: size.corner },
      ]}
    >
      <View style={[styles.screen, { width: size.width, height: size.height }]}>
        <SafeAreaFrameContext.Provider value={frame}>
          <SafeAreaInsetsContext.Provider value={insets}>
            <TextScale.Provider value={textScale}>{children}</TextScale.Provider>
          </SafeAreaInsetsContext.Provider>
        </SafeAreaFrameContext.Provider>
        <View style={styles.status} pointerEvents="none">
          <Txt style={type.status}>{time}</Txt>
        </View>
        <View style={styles.gesture} pointerEvents="none">
          <View style={styles.pill} />
        </View>
      </View>
      <View style={[styles.outline, { borderRadius: size.corner }]} pointerEvents="none" />
    </View>
  );
}

const styles = StyleSheet.create({
  frame: { overflow: 'hidden', backgroundColor: color.paper },
  screen: { position: 'absolute', left: borderWidth, top: borderWidth, overflow: 'hidden' },
  outline: { position: 'absolute', left: 0, right: 0, top: 0, bottom: 0, borderWidth, borderColor: color.ink },
  status: {
    position: 'absolute',
    left: 0,
    right: 0,
    top: 0,
    height: STATUS,
    flexDirection: 'row',
    alignItems: 'center',
    paddingTop: 2,
    paddingHorizontal: 30,
  },
  gesture: { position: 'absolute', left: 0, right: 0, bottom: 0, height: GESTURE, alignItems: 'center', justifyContent: 'center' },
  pill: { width: 108, height: 4, borderRadius: 2, backgroundColor: 'rgba(27,38,56,0.3)' },
});
