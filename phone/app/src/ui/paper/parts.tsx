// The parts every phone screen is built from, as the drawings draw them
// (design/screens/phone.css and phone-setup.css): the card with its hard
// shadow, buttons, labels, stamps, tiles, the dashed divider, the top row
// with LiMo's mark, the dot counter and the maker's mark.
//
// Rule 3 of the theme holds in each: nothing changes size when it is picked,
// pressed or switched off.

import { type ReactNode, useState } from 'react';
import { Image, Pressable, StyleSheet, Text, View, type ViewStyle } from 'react-native';
import Svg, { Line, Path } from 'react-native-svg';

import logo from '../../generated/lucidloom-logo.png';
import { Icon, type IconName } from '../../icons/Icon';
import { borderWidth, color, hardShadow, radius, tokens } from '../../theme';
import { Txt, type Wrap, type } from './text';

type Style = ViewStyle | (ViewStyle | false | null | undefined)[];

// ---------------------------------------------------------------------------
// The card, and the hard shadow behind it

/**
 * A card: lighter paper, an ink border, and a hard shadow set down and to
 * the right. React Native has no shadow without blur on every Android LiMo
 * runs on, so the shadow is a second shape behind the card, which takes no
 * room, as a CSS shadow takes none.
 */
export function Card({ children, style, outer, testID }: { children: ReactNode; style?: Style; outer?: Style; testID?: string }) {
  return (
    <View style={outer} testID={testID}>
      <Shadow offset={hardShadow.card.x} corner={radius.lg} />
      <View style={[styles.card, style]}>{children}</View>
    </View>
  );
}

/** The hard shadow alone, for a shape that is not a Card: put it first inside the shape's own box. */
export function Shadow({ offset, corner }: { offset: number; corner: number }) {
  return (
    <View
      pointerEvents="none"
      style={[styles.shadow, { top: offset, left: offset, right: -offset, bottom: -offset, borderRadius: corner }]}
    />
  );
}

// ---------------------------------------------------------------------------
// Buttons

export type ButtonKind = 'go' | 'plain';

/** The design's switched off marigold (phone-setup.css, .ph-btn.go.off): solid, so the page does not show through. */
const OFF = { background: '#f5d194', border: 'rgba(27,38,56,0.4)', text: 'rgba(27,38,56,0.5)' };

/**
 * A button, at least 48 high. "go" is marigold, LiMo's own main button;
 * "plain" is the card's paper. A switched off button is soft and has no
 * shadow; it keeps its size. Pressing moves the face onto its shadow.
 */
export function Button({
  label,
  onPress,
  kind = 'plain',
  off = false,
  hint,
  outer,
  testID,
}: {
  label: string;
  onPress: () => void;
  kind?: ButtonKind;
  off?: boolean;
  hint?: string;
  outer?: Style;
  testID?: string;
}) {
  const [down, setDown] = useState(false);
  const push = down && !off ? 2 : 0;
  return (
    <Pressable
      testID={testID}
      onPress={off ? undefined : onPress}
      onPressIn={() => setDown(true)}
      onPressOut={() => setDown(false)}
      disabled={off}
      accessibilityRole="button"
      accessibilityLabel={label}
      accessibilityHint={hint}
      accessibilityState={{ disabled: off }}
      style={[styles.press, outer]}
    >
      {off ? null : <Shadow offset={hardShadow.button.x} corner={radius.md} />}
      <View
        style={[
          styles.button,
          kind === 'go' && styles.go,
          off && { backgroundColor: OFF.background, borderColor: OFF.border },
          push ? { transform: [{ translateX: push }, { translateY: push }] } : null,
        ]}
      >
        <Txt style={[type.button, off && { color: OFF.text }]}>{label}</Txt>
      </View>
    </Pressable>
  );
}

// ---------------------------------------------------------------------------
// Words

/** A mono label in capitals. A section's label is a header, so a screen reader can move by sections. */
export function Label({ children, style, header = false }: { children: string; style?: Style; header?: boolean }) {
  return (
    <View style={style}>
      <Txt style={type.label} accessibilityRole={header ? 'header' : undefined}>
        {children}
      </Txt>
    </View>
  );
}

/** Words with the names of things to tap in bold: "Choose **Allow**." */
export type Rich = readonly (string | { readonly strong: string })[];

export function RichText({ words, style, wrap }: { words: Rich | string; style: (typeof type)[keyof typeof type]; wrap?: Wrap }) {
  const parts = typeof words === 'string' ? [words] : words;
  return (
    <Txt style={style} wrap={wrap}>
      {parts.map((p, i) =>
        typeof p === 'string' ? (
          p
        ) : (
          <Text key={i} style={type.name}>
            {p.strong}
          </Text>
        ),
      )}
    </Txt>
  );
}

/** Plain words of a Rich, for a screen reader's label. */
export function plain(words: Rich | string): string {
  return typeof words === 'string' ? words : words.map((p) => (typeof p === 'string' ? p : p.strong)).join('');
}

/** A heading, read as one by a screen reader. */
export function Heading({ children, style }: { children: string; style?: Style }) {
  return (
    <View style={style}>
      <Txt style={type.h1} wrap="balance" accessibilityRole="header">
        {children}
      </Txt>
    </View>
  );
}

export type Tone = 'ink' | 'teal' | 'coral';

/** A small stamp that says a state, straight. */
export function Stamp({ children, tone = 'ink', testID }: { children: string; tone?: Tone; testID?: string }) {
  const c = tone === 'teal' ? color.teal : tone === 'coral' ? color.coral : color.ink;
  return (
    <View style={[styles.stamp, { borderColor: c }]} testID={testID}>
      <Txt style={[type.stamp, { color: c }]} numberOfLines={1}>
        {children}
      </Txt>
    </View>
  );
}

// ---------------------------------------------------------------------------
// Small things

/** An icon on a tile of the quieter paper, so a pale drawing keeps an edge on a card. */
export function Tile({ icon, large = false }: { icon: IconName; large?: boolean }) {
  return (
    <View style={[styles.tile, large && styles.tileLarge]}>
      <Icon name={icon} size={tokens.icon.tile} />
    </View>
  );
}

/** A dashed divider, as the top border of what follows it. */
export function Dashed() {
  return (
    <Svg width="100%" height={borderWidth} style={styles.dashed} importantForAccessibility="no-hide-descendants">
      <Line
        x1={0}
        y1={borderWidth / 2}
        x2="100%"
        y2={borderWidth / 2}
        stroke={color.border}
        strokeWidth={borderWidth}
        strokeDasharray={[3, 1]}
      />
    </Svg>
  );
}

/** A tick: a plain ink stroke beside text, never an icon. */
export function Tick({ tint, size = 18 }: { tint: string; size?: number }) {
  return (
    <Svg width={size} height={size} viewBox="0 0 18 18" fill="none" importantForAccessibility="no-hide-descendants">
      <Path d="M3.5 9.5l3.6 3.6L14.5 5" stroke={tint} strokeWidth={2} strokeLinecap="round" strokeLinejoin="round" />
    </Svg>
  );
}

/** The top row: LiMo's own mark first, and what belongs at the right. */
export function TopRow({ right }: { right?: ReactNode }) {
  return (
    <View style={styles.top}>
      <View style={styles.mark}>
        <Icon name="hourglass" size={tokens.icon.row} />
        <Txt style={type.mark}>LiMo</Txt>
      </View>
      {right ?? null}
    </View>
  );
}

/** The child's name at the top right, on one line however long it is. */
export function Who({ name }: { name: string }) {
  return (
    <View style={styles.who}>
      <Txt style={type.who} numberOfLines={1} ellipsizeMode="tail">
        {name}
      </Txt>
    </View>
  );
}

/** A counter: done ones teal, this one marigold, later ones outlined. Every dot keeps its size. */
export function Dots({ count, at, label }: { count: number; at: number; label?: string }) {
  const dots = [];
  for (let i = 0; i < count; i += 1) {
    dots.push(<View key={i} style={[styles.dot, i < at && styles.dotDone, i === at && styles.dotAt]} />);
  }
  return (
    <View
      style={styles.dots}
      accessible={Boolean(label)}
      accessibilityLabel={label}
      importantForAccessibility={label ? 'yes' : 'no-hide-descendants'}
    >
      {dots}
    </View>
  );
}

/**
 * The maker's mark: said once, small, at the end of a screen that fills the
 * display. Not a link. The logo keeps its own colours, 22 high on a phone.
 */
export function Maker() {
  return (
    <View style={styles.maker} accessible accessibilityLabel={`${tokens.maker.words} ${tokens.maker.name}`}>
      <Txt style={type.makerWords}>{tokens.maker.words}</Txt>
      <Image source={logo} style={styles.logo} accessibilityIgnoresInvertColors />
      <Txt style={type.makerName}>{tokens.maker.name}</Txt>
    </View>
  );
}

/** The end of a screen: the maker's mark, pushed to the bottom when there is room. */
export function Tail() {
  return (
    <View style={styles.tail}>
      <Maker />
    </View>
  );
}

/** The phone drawings' maker logo size. */
export const PHONE_LOGO = 22;

const styles = StyleSheet.create({
  card: {
    paddingVertical: 14,
    paddingHorizontal: 16,
    backgroundColor: color.card,
    borderWidth,
    borderColor: color.ink,
    borderRadius: radius.lg,
  },
  shadow: { position: 'absolute', backgroundColor: hardShadow.card.color },
  press: { minHeight: 48 },
  button: {
    minHeight: 48,
    paddingHorizontal: 16,
    paddingVertical: 8,
    alignItems: 'center',
    justifyContent: 'center',
    backgroundColor: color.card,
    borderWidth,
    borderColor: color.ink,
    borderRadius: radius.md,
  },
  go: { backgroundColor: color.marigold },
  stamp: {
    flexShrink: 0,
    paddingTop: 5,
    paddingBottom: 4,
    paddingHorizontal: 8,
    borderWidth,
    borderRadius: 6,
  },
  tile: { width: 40, height: 40, borderRadius: 8, alignItems: 'center', justifyContent: 'center', backgroundColor: color.paper2 },
  tileLarge: { width: 48, height: 48, borderRadius: radius.md },
  dashed: { flexShrink: 0 },
  top: { flexDirection: 'row', alignItems: 'flex-start', justifyContent: 'space-between', paddingTop: 2, paddingHorizontal: 2 },
  mark: { flexDirection: 'row', alignItems: 'center', gap: 7, paddingTop: 2, flexShrink: 0 },
  who: {
    flexShrink: 1,
    marginLeft: 12,
    marginTop: 1,
    paddingTop: 7,
    paddingBottom: 6,
    paddingHorizontal: 10,
    borderWidth,
    borderColor: color.ink,
    borderRadius: 8,
  },
  dots: { flexDirection: 'row', gap: 6 },
  dot: { width: 10, height: 10, borderRadius: 5, borderWidth, borderColor: color.inkSoft },
  dotDone: { backgroundColor: color.teal, borderColor: color.teal },
  dotAt: { backgroundColor: color.marigold, borderColor: color.marigoldDeep },
  maker: { flexDirection: 'row', alignItems: 'center', justifyContent: 'center', gap: 7, marginTop: 8 },
  logo: { width: PHONE_LOGO, height: PHONE_LOGO, marginLeft: 2 },
  tail: { marginTop: 'auto', paddingTop: 18 },
});
