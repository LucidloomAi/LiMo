// The console's window on the family PC, as design/screens/console.css draws
// it: 1100 by 760, with Windows' own title bar in its top 32, drawn quietly
// in Windows' manner, because Windows draws it, not LiMo. The screen below
// it is 1100 by 728, given to the screens as their frame with no insets, as
// the console's window gives it. Preview only.

import type { ReactNode } from 'react';
import { StyleSheet, View } from 'react-native';
import { SafeAreaFrameContext, SafeAreaInsetsContext } from 'react-native-safe-area-context';
import Svg, { Path, Rect } from 'react-native-svg';

import { Icon } from '../icons/Icon';
import { borderWidth, color } from '../theme';
import { Txt } from '../ui/paper/text';

export const WINDOW = { width: 1100, height: 760, title: 32 } as const;

const NO_INSETS = { top: 0, bottom: 0, left: 0, right: 0 };

function Control({ children }: { children: ReactNode }) {
  return (
    <View style={styles.control}>
      <Svg width={10} height={10} viewBox="0 0 10 10" fill="none">
        {children}
      </Svg>
    </View>
  );
}

export function WindowFrame({ children }: { children: ReactNode }) {
  const screen = { x: 0, y: 0, width: WINDOW.width, height: WINDOW.height - WINDOW.title };
  return (
    <View style={[styles.frame, { width: WINDOW.width + 2 * borderWidth, height: WINDOW.height + 2 * borderWidth }]}>
      <View style={[styles.inside, { width: WINDOW.width, height: WINDOW.height }]}>
        <View style={[styles.screen, { top: WINDOW.title, width: screen.width, height: screen.height }]}>
          <SafeAreaFrameContext.Provider value={screen}>
            <SafeAreaInsetsContext.Provider value={NO_INSETS}>{children}</SafeAreaInsetsContext.Provider>
          </SafeAreaFrameContext.Provider>
        </View>
        <View style={styles.title} pointerEvents="none">
          <View style={styles.name}>
            <Icon name="hourglass" size={16} />
            <Txt style={styles.nameWords}>LiMo</Txt>
          </View>
          <View style={styles.controls}>
            <Control>
              <Path d="M0 5.5h10" stroke="#1f2328" strokeWidth={1} />
            </Control>
            <Control>
              <Rect x={0.5} y={0.5} width={9} height={9} rx={1.5} stroke="#1f2328" strokeWidth={1} />
            </Control>
            <Control>
              <Path d="M.5 .5l9 9M9.5 .5l-9 9" stroke="#1f2328" strokeWidth={1} />
            </Control>
          </View>
        </View>
      </View>
      <View style={styles.outline} pointerEvents="none" />
    </View>
  );
}

const styles = StyleSheet.create({
  frame: { overflow: 'hidden', borderRadius: 10, backgroundColor: color.paper },
  inside: { position: 'absolute', left: borderWidth, top: borderWidth, overflow: 'hidden' },
  screen: { position: 'absolute', left: 0 },
  title: {
    position: 'absolute',
    left: 0,
    right: 0,
    top: 0,
    height: WINDOW.title,
    flexDirection: 'row',
    alignItems: 'center',
    backgroundColor: '#eceef2',
    borderBottomWidth: 1,
    borderBottomColor: '#d3d7de',
  },
  name: { flexDirection: 'row', alignItems: 'center', gap: 8, paddingLeft: 12 },
  nameWords: { fontFamily: 'Segoe UI', fontSize: 12, lineHeight: 12, color: '#1f2328' },
  controls: { flexDirection: 'row', height: '100%', marginLeft: 'auto' },
  control: { width: 46, height: '100%', alignItems: 'center', justifyContent: 'center' },
  outline: { position: 'absolute', left: 0, right: 0, top: 0, bottom: 0, borderWidth, borderColor: color.ink, borderRadius: 10 },
});
