// The floating tab bar (design/README.md, "The tab bar"): set in from the
// sides and the bottom by 14, corners of 24, a pale rim and then a hairline
// of ink, and the theme's one soft shadow, because it hangs above the page.
//
// Android cannot blur what is behind a view, so the bar takes the design's
// own answer for that case: the card colour at nearly full strength
// (tokens.tabBar.withoutBlur) instead of glass.
//
// The picked tab has a faint ink pill behind it and its label in full ink.
// Every tab keeps the same size, picked or not: only colours change.

import { Pressable, StyleSheet, View } from 'react-native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';

import { Icon, type IconName } from '../../icons/Icon';
import { color, tokens } from '../../theme';
import { TAB_INSET } from './Page';
import { Txt, type } from './text';

export type TabItem<K extends string> = { key: K; label: string; icon: IconName };

const bar = tokens.tabBar;

export function Tabs<K extends string>({
  items,
  picked,
  onPick,
  onHeight,
}: {
  items: readonly TabItem<K>[];
  picked: K;
  onPick: (key: K) => void;
  onHeight?: (height: number) => void;
}) {
  const insets = useSafeAreaInsets();
  return (
    <View
      style={[styles.hairline, { left: TAB_INSET - 1 + insets.left, right: TAB_INSET - 1 + insets.right, bottom: insets.bottom + TAB_INSET - 1 }]}
      onLayout={(e) => onHeight?.(e.nativeEvent.layout.height - 2)}
      accessibilityRole="tablist"
      testID="tab-bar"
    >
      <View style={styles.bar}>
        {items.map((item) => {
          const on = item.key === picked;
          return (
            <Pressable
              key={item.key}
              testID={`tab-${item.key}`}
              onPress={() => onPick(item.key)}
              accessibilityRole="tab"
              accessibilityLabel={item.label}
              accessibilityState={{ selected: on }}
              style={[styles.tab, on && styles.tabOn]}
            >
              <Icon name={item.icon} size={tokens.icon.row} />
              <Txt style={[type.tab, styles.label, { color: on ? color.ink : color.inkSoft }]} numberOfLines={2}>
                {item.label}
              </Txt>
            </Pressable>
          );
        })}
      </View>
    </View>
  );
}

export const tabStyles = StyleSheet.create({
  // Exposed for the test that every tab keeps its size.
  tab: {
    flex: 1,
    flexBasis: 0,
    alignItems: 'center',
    gap: 4,
    paddingTop: 7,
    paddingBottom: 7,
    paddingHorizontal: 4,
    minHeight: 54,
    borderRadius: 18,
  },
  tabOn: { backgroundColor: bar.picked },
  // Very large text wraps to a second line rather than being cut; both tabs
  // grow together, so they keep one size.
  label: { textAlign: 'center' },
});

const styles = StyleSheet.create({
  // The hairline of ink just outside the rim: a ring one wide, drawn as a
  // border so it shows on every Android, with the soft shadow where Android
  // draws one (9.0 and later).
  hairline: {
    position: 'absolute',
    borderWidth: 1,
    borderColor: bar.hairline,
    borderRadius: bar.radius + 1,
    boxShadow: bar.shadow,
  },
  bar: {
    flexDirection: 'row',
    gap: 4,
    padding: 6,
    borderRadius: bar.radius,
    borderWidth: 1.5,
    borderColor: bar.rim,
    backgroundColor: bar.withoutBlur,
  },
  ...tabStyles,
});
