// The parts the guardian's screens are made of, as design/screens/console.css
// draws them: one set of screens arranged by the width they get. Under 700
// wide a screen takes a phone's arrangement, from 700 up the console's,
// where things to tap are 40 high (48 on a phone), fields and pinned
// buttons 44 (48), and the pinned strip 68 (64, or 122 with two buttons).
//
// What the child's screens draw the same way is used from src/ui/paper
// (the hard shadow, the dashed divider, the icons, the text component).

import { createContext, type ReactNode, useContext, useState } from 'react';
import { Image, Platform, Pressable, StyleSheet, TextInput, type TextInputProps, View, type ViewStyle } from 'react-native';
import { useSafeAreaFrame, useSafeAreaInsets } from 'react-native-safe-area-context';
import Svg, { Circle, Path } from 'react-native-svg';

import logo from '../generated/lucidloom-logo.png';
import { Icon, type IconName } from '../icons/Icon';
import { borderWidth, color, hardShadow, radius, tokens } from '../theme';
import { Dashed, Shadow } from '../ui/paper/parts';
import { Txt } from '../ui/paper/text';
import { gtype } from './text';
import * as W from './words';

type Style = ViewStyle | (ViewStyle | false | null | undefined)[];

// ---------------------------------------------------------------------------
// Where the screens run, and how wide they are

/** The console's window on the family PC, or a guardian's phone. Some words are true of the PC only. */
export type Place = 'pc' | 'phone';
export const PlaceContext = createContext<Place>('phone');
export const usePlace = () => useContext(PlaceContext);

/** From this width up, the console's arrangement. */
export const WIDE_FROM = 700;

/** The width the screens get: the window's or the phone's, less the sides Android keeps for itself. */
export function useScreenWidth(): number {
  const frame = useSafeAreaFrame();
  const insets = useSafeAreaInsets();
  return frame.width - insets.left - insets.right;
}

export function useWide(): boolean {
  return useScreenWidth() >= WIDE_FROM;
}

/** The sizes that change with the width. */
export function useSizes() {
  const wide = useWide();
  return wide ? { wide, tap: 40, field: 44, pin: 68, pin2: 68 } : { wide, tap: 48, field: 48, pin: 64, pin2: 122 };
}

// ---------------------------------------------------------------------------
// Words

/** A mono label in capitals, with a hint beside it in plain type. */
export function Label({ children, hint, style, header = false }: { children: string; hint?: string; style?: Style; header?: boolean }) {
  return (
    <View style={[styles.labelRow, style]}>
      <Txt style={gtype.label} accessibilityRole={header ? 'header' : undefined}>
        {children}
      </Txt>
      {hint ? <Txt style={[gtype.hint, styles.hint]}>{hint}</Txt> : null}
    </View>
  );
}

/** A screen's heading, 28 on a phone and 30 at the console's width, read as a heading. */
export function H1({ children, style }: { children: string; style?: Style }) {
  const wide = useWide();
  return (
    <View style={style}>
      <Txt style={wide ? gtype.h1Wide : gtype.h1} wrap="balance" accessibilityRole="header">
        {children}
      </Txt>
    </View>
  );
}

/** The room under a field for a message: two lines, kept when empty, so nothing moves when one comes. */
export function Room({ words, testID = 'message', done = false }: { words: string; testID?: string; done?: boolean }) {
  return (
    <View style={styles.room} testID={`${testID}-room`}>
      <Txt style={done ? gtype.done : gtype.err} wrap="pretty" testID={testID} accessibilityLiveRegion="polite">
        {words}
      </Txt>
    </View>
  );
}

// ---------------------------------------------------------------------------
// Small marks: plain ink strokes beside text, never icons

function Stroke({ size, d, tint, width = 1.8 }: { size: number; d: string; tint: string; width?: number }) {
  return (
    <Svg width={size} height={size} viewBox={`0 0 ${size} ${size}`} fill="none" importantForAccessibility="no-hide-descendants">
      <Path d={d} stroke={tint} strokeWidth={width} strokeLinecap="round" strokeLinejoin="round" />
    </Svg>
  );
}

export const Mark = {
  left: (tint: string = color.ink) => <Stroke size={16} d="M10 3L5 8l5 5" tint={tint} />,
  down: (tint: string = color.inkSoft) => <Stroke size={16} d="M3.5 6l4.5 4.5L12.5 6" tint={tint} />,
  plus: (tint: string = color.ink) => <Stroke size={18} d="M9 3.5v11M3.5 9h11" tint={tint} width={2} />,
  minus: (tint: string = color.ink) => <Stroke size={18} d="M3.5 9h11" tint={tint} width={2} />,
  tick: (tint: string = color.teal) => <Stroke size={18} d="M3.5 9.5l3.6 3.6L14.5 5" tint={tint} width={2} />,
  cross: (tint: string = color.ink) => <Stroke size={18} d="M5 5l8 8M13 5l-8 8" tint={tint} width={2} />,
  right: (tint: string = color.ink) => <Stroke size={16} d="M6 3l5 5-5 5" tint={tint} />,
  /** A hollow ring: a step not on yet. */
  ring: (tint: string = color.ink) => (
    <Svg width={18} height={18} viewBox="0 0 18 18" fill="none" importantForAccessibility="no-hide-descendants">
      <Circle cx={9} cy={9} r={5.5} stroke={tint} strokeWidth={2} />
    </Svg>
  ),
};

// ---------------------------------------------------------------------------
// The top row, cards, tiles

/** The top row: LiMo's own name first, and what belongs at the right. */
export function TopRow({ right }: { right?: ReactNode }) {
  return (
    <View style={styles.top}>
      <View style={styles.mark}>
        <Icon name="hourglass" size={24} />
        <Txt style={gtype.mark}>LiMo</Txt>
      </View>
      {right ?? null}
    </View>
  );
}

/** A card: lighter paper, an ink border, the hard shadow; its padding grows at the console's width. */
export function Card({ children, style, outer, testID, bare = false }: { children: ReactNode; style?: Style; outer?: Style; testID?: string; bare?: boolean }) {
  const wide = useWide();
  return (
    <View style={outer} testID={testID}>
      <Shadow offset={hardShadow.card.x} corner={radius.lg} />
      <View style={[styles.card, !bare && (wide ? styles.cardPadWide : styles.cardPad), style]}>{children}</View>
    </View>
  );
}

/** An icon on a tile of the quieter paper: 40, or 48 when large. */
export function Tile({ icon, large = false }: { icon: IconName; large?: boolean }) {
  return (
    <View style={[styles.tile, large && styles.tileLarge]}>
      <Icon name={icon} size={32} />
    </View>
  );
}

/** A card's heading row: an icon at 48 beside the heading. */
export function CardHead({ icon, title }: { icon: IconName; title: string }) {
  return (
    <View style={styles.hrow}>
      <Icon name={icon} size={48} />
      <H1 style={styles.hrowTitle}>{title}</H1>
    </View>
  );
}

/** A stamp that says a state, straight. */
export function Stamp({ children, tone }: { children: string; tone: 'coral' | 'teal' }) {
  const c = tone === 'coral' ? color.coral : color.teal;
  return (
    <View style={[styles.stamp, { borderColor: c }]}>
      <Txt style={[gtype.stamp, { color: c }]} numberOfLines={1}>
        {children}
      </Txt>
    </View>
  );
}

// ---------------------------------------------------------------------------
// Buttons and choices

export type BtnLook = 'plain' | 'go' | 'coral';

/**
 * A button at least as high as a tap (48 on a phone, 40 at the console's
 * width). "go" is marigold. Switched off ("off"), a go button is soft and a
 * plain one dashed and faded; neither has a shadow, and both keep their size.
 * Pressing moves the face onto its shadow.
 */
export function Btn({
  label,
  onPress,
  look = 'plain',
  mono = false,
  answer = false,
  off = false,
  height,
  padding = 16,
  style,
  outer,
  children,
  lead,
  testID,
}: {
  label: string;
  onPress: () => void;
  /** "danger" is filled with coral: the last button before something that cannot be undone. */
  look?: BtnLook | 'danger';
  mono?: boolean;
  /** The answers to a request: 15 high type. */
  answer?: boolean;
  off?: boolean;
  /** The pinned buttons and fields' height, where it is not a tap's. */
  height?: number;
  padding?: number;
  style?: Style;
  outer?: Style;
  children?: ReactNode;
  /** A mark before the words, such as a plus. */
  lead?: ReactNode;
  testID?: string;
}) {
  const { tap } = useSizes();
  const [down, setDown] = useState(false);
  const push = down && !off ? 2 : 0;
  const tint = off ? (look === 'go' ? 'rgba(27,38,56,0.5)' : 'rgba(27,38,56,0.45)') : look === 'coral' ? color.coral : look === 'danger' ? color.card : color.ink;
  return (
    <Pressable
      testID={testID}
      onPress={off ? undefined : onPress}
      onPressIn={() => setDown(true)}
      onPressOut={() => setDown(false)}
      disabled={off}
      accessibilityRole="button"
      accessibilityLabel={label}
      accessibilityState={{ disabled: off }}
      style={[{ minHeight: height ?? tap }, outer]}
    >
      {off ? null : <Shadow offset={hardShadow.button.x} corner={radius.md} />}
      <View
        style={[
          styles.btn,
          { minHeight: height ?? tap, paddingHorizontal: padding },
          look === 'go' && styles.go,
          look === 'coral' && styles.coralEdge,
          look === 'danger' && styles.danger,
          off && (look === 'go' ? styles.goOff : styles.dim),
          push ? { transform: [{ translateX: push }, { translateY: push }] } : null,
          style,
        ]}
      >
        {lead ?? null}
        <Txt style={[mono ? gtype.buttonMono : answer ? gtype.buttonAnswer : gtype.button, { color: tint }]} numberOfLines={1}>
          {label}
        </Txt>
        {children}
      </View>
    </Pressable>
  );
}

/** A button pinned to the bottom: as wide as a phone, 48 high; at the console's width 44 high and at least 200 wide. */
export function PinBtn(props: { label: string; onPress: () => void; look?: BtnLook; off?: boolean; testID?: string }) {
  const { wide, field } = useSizes();
  return <Btn {...props} height={field} padding={wide ? 28 : 16} outer={wide ? styles.pinWide : undefined} />;
}

/** One choice of several: ink outlined, mono; the picked one filled with ink, the same size. */
export function Chip({
  label,
  on,
  onPress,
  testID,
  style,
  a11y,
  add = false,
}: {
  label: string;
  on: boolean;
  onPress: () => void;
  testID?: string;
  style?: Style;
  a11y?: string;
  /** A chip that adds what it names: a plus before it, and a button, not a choice. */
  add?: boolean;
}) {
  const { tap } = useSizes();
  return (
    <Pressable
      testID={testID}
      onPress={onPress}
      accessibilityRole={add ? 'button' : 'radio'}
      accessibilityLabel={a11y ?? label}
      accessibilityState={add ? undefined : { checked: on, selected: on }}
      style={[styles.chip, { minHeight: tap }, on && styles.chipOn, add && styles.chipAdd, style]}
    >
      {add ? Mark.plus() : null}
      <Txt style={[gtype.chip, on && { color: color.card }]} numberOfLines={1}>
        {label}
      </Txt>
    </Pressable>
  );
}

// ---------------------------------------------------------------------------
// Fields

/** A field: an ink outline; the one being typed in has a marigold edge at its foot. Switched off, dashed. */
export function Field({ off = false, style, ...input }: TextInputProps & { off?: boolean }) {
  const { field } = useSizes();
  const [focused, setFocused] = useState(false);
  return (
    <View style={[styles.field, { height: field }, off && styles.fieldOff, style as ViewStyle]}>
      <TextInput
        {...input}
        editable={!off && input.editable !== false}
        onFocus={(e) => {
          setFocused(true);
          input.onFocus?.(e);
        }}
        onBlur={(e) => {
          setFocused(false);
          input.onBlur?.(e);
        }}
        placeholderTextColor={color.inkSoft}
        style={[gtype.field, styles.input]}
      />
      {focused && !off ? <View style={styles.focusEdge} pointerEvents="none" /> : null}
    </View>
  );
}

/** A field that opens a choice: its value in mono marigold, and a mark to say it opens. */
export function PickField({ value, onPress, off = false, open = false, label, testID }: { value: string; onPress: () => void; off?: boolean; open?: boolean; label: string; testID?: string }) {
  const { field } = useSizes();
  return (
    <Pressable
      testID={testID}
      onPress={off ? undefined : onPress}
      disabled={off}
      accessibilityRole="button"
      accessibilityLabel={`${label}, ${value}`}
      accessibilityState={{ disabled: off, expanded: open }}
      style={[styles.field, styles.pick, { height: field }, off && styles.fieldOff]}
    >
      <Txt style={[gtype.fieldTime, off && { color: color.inkSoft }]}>{value}</Txt>
      <View style={styles.pickMark}>{Mark.down()}</View>
      {open ? <View style={styles.focusEdge} pointerEvents="none" /> : null}
    </Pressable>
  );
}

/** − figure +: the figure keeps its width, so the buttons never move. Large, the figure is the time a day in Fraunces. */
export function Stepper({ value, onLess, onMore, big = false, name }: { value: string; onLess: () => void; onMore: () => void; big?: boolean; name: string }) {
  const { tap } = useSizes();
  const side = big ? 48 : tap;
  const sq = (label: string, mark: ReactNode, press: () => void, id: string) => (
    <Pressable onPress={press} accessibilityRole="button" accessibilityLabel={`${label}, ${name}`} testID={id} style={[styles.sq, { width: side, height: side }]}>
      {mark}
    </Pressable>
  );
  return (
    <View style={[styles.stepper, big && styles.stepperBig]}>
      {sq(W.timeADay.less, Mark.minus(), onLess, `less-${name}`)}
      <Txt style={big ? [gtype.bigFigure, styles.bigFig] : [gtype.figure, styles.fig]} testID={`figure-${name}`}>
        {value}
      </Txt>
      {sq(W.timeADay.more, Mark.plus(), onMore, `more-${name}`)}
    </View>
  );
}

/** A square button with a mark and no words: its name is said to a screen reader. */
export function Square({ mark, label, onPress, testID }: { mark: ReactNode; label: string; onPress: () => void; testID?: string }) {
  const { tap } = useSizes();
  return (
    <Pressable onPress={onPress} accessibilityRole="button" accessibilityLabel={label} testID={testID} style={[styles.sq, { width: tap, height: tap }]}>
      {mark}
    </Pressable>
  );
}

// ---------------------------------------------------------------------------
// The second sheet's parts

/** A card's head on a screen with tabs: the icon on a tile, the heading at 22, and what stands at its right. */
export function Head2({ icon, title, right }: { icon: IconName; title: string; right?: ReactNode }) {
  return (
    <View style={styles.head2}>
      <Tile icon={icon} />
      <View style={styles.head2Title}>
        <Txt style={gtype.h2} wrap="balance" accessibilityRole="header">
          {title}
        </Txt>
      </View>
      {right ?? null}
    </View>
  );
}

/** Back at the left, and at the right the place it goes back to. */
export function SubHead({ to, onBack }: { to: string; onBack: () => void }) {
  const { tap } = useSizes();
  return (
    <View style={[styles.subHead, { minHeight: tap }]}>
      <Pressable onPress={onBack} accessibilityRole="button" accessibilityLabel={`Back to ${to}`} testID="back" style={[styles.back, { height: tap }]}>
        {Mark.left()}
        <Txt style={gtype.button}>Back</Txt>
      </Pressable>
      <Label>{to}</Label>
    </View>
  );
}

/** Facts: each its name in mono, its value, and what can be done about it, with dashed lines between. */
export function Kvs({ rows, testID }: { rows: readonly { k: string; v: string; after?: ReactNode; teal?: boolean }[]; testID?: string }) {
  const wide = useWide();
  return (
    <View style={styles.kvs} testID={testID}>
      {rows.map((r, i) => (
        <View key={r.k}>
          {i > 0 ? <Dashed /> : null}
          <View style={[styles.kv, i > 0 && styles.kvAfter]} accessible={!r.after} accessibilityLabel={`${r.k}: ${r.v}`}>
            <View style={{ width: wide ? 128 : 104 }}>
              <Txt style={gtype.kvKey}>{r.k}</Txt>
            </View>
            <Txt style={[gtype.kvValue, styles.flex, r.teal && styles.teal]}>{r.v}</Txt>
            {r.after ?? null}
          </View>
        </View>
      ))}
    </View>
  );
}

/** An app's first letter on a tile, standing for its own icon. */
export function Letter({ name, small = false }: { name: string; small?: boolean }) {
  const first = (Array.from(name.trim())[0] ?? '').toUpperCase();
  return (
    <View style={[styles.letter, small && styles.letterSmall]} importantForAccessibility="no-hide-descendants">
      <Txt style={small ? gtype.letterSmall : gtype.letter}>{first}</Txt>
    </View>
  );
}

/** A group's label inside a card: 18 above it, 14 when it comes straight after the card's head. */
export function GroupLabel({ children, first = false }: { children: string; first?: boolean }) {
  return <Label style={first ? styles.groupFirst : styles.group}>{children}</Label>;
}

// ---------------------------------------------------------------------------
// The maker's mark

/** "MADE BY", the LucidLoom AI logo and the name: 22 on a phone, 26 at the console's width. Not a link. */
export function MakerMark() {
  const wide = useWide();
  const size = wide ? 26 : 22;
  return (
    <View style={styles.maker} accessible accessibilityLabel={`${tokens.maker.words} ${tokens.maker.name}`}>
      <Txt style={gtype.makerWords}>{tokens.maker.words}</Txt>
      <Image source={logo} style={{ width: size, height: size, marginLeft: 2 }} accessibilityIgnoresInvertColors />
      <Txt style={gtype.makerName}>{tokens.maker.name}</Txt>
    </View>
  );
}

const styles = StyleSheet.create({
  labelRow: { flexDirection: 'row', alignItems: 'baseline', flexWrap: 'wrap' },
  hint: { marginLeft: 6 },
  room: { minHeight: 40, marginTop: 6 },
  top: { flexDirection: 'row', alignItems: 'center', justifyContent: 'space-between', gap: 12, minHeight: 40, paddingTop: 2, paddingHorizontal: 2 },
  mark: { flexDirection: 'row', alignItems: 'center', gap: 7, flexShrink: 0 },
  card: { backgroundColor: color.card, borderWidth, borderColor: color.ink, borderRadius: radius.lg },
  cardPad: { padding: 16 },
  cardPadWide: { paddingTop: 20, paddingBottom: 22, paddingHorizontal: 24 },
  tile: { width: 40, height: 40, borderRadius: 8, alignItems: 'center', justifyContent: 'center', backgroundColor: color.paper2, flexShrink: 0 },
  tileLarge: { width: 48, height: 48, borderRadius: 10 },
  hrow: { flexDirection: 'row', alignItems: 'center', gap: 12 },
  hrowTitle: { flex: 1 },
  stamp: { flexShrink: 0, paddingTop: 5, paddingBottom: 4, paddingHorizontal: 8, borderWidth, borderRadius: 6 },
  btn: {
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'center',
    gap: 8,
    backgroundColor: color.card,
    borderWidth,
    borderColor: color.ink,
    borderRadius: radius.md,
  },
  pinWide: { minWidth: 200 },
  go: { backgroundColor: color.marigold },
  coralEdge: { borderColor: color.coral },
  danger: { backgroundColor: color.coral },
  goOff: { backgroundColor: '#f5d194', borderColor: 'rgba(27,38,56,0.4)' },
  dim: { borderStyle: 'dashed', borderColor: 'rgba(27,38,56,0.42)' },
  chip: {
    alignItems: 'center',
    justifyContent: 'center',
    paddingHorizontal: 14,
    backgroundColor: color.card,
    borderWidth,
    borderColor: color.ink,
    borderRadius: 8,
  },
  chipOn: { backgroundColor: color.ink },
  chipAdd: { flexDirection: 'row', gap: 6, paddingLeft: 10 },
  field: {
    marginTop: 8,
    flexDirection: 'row',
    alignItems: 'center',
    backgroundColor: color.card,
    borderWidth,
    borderColor: color.ink,
    borderRadius: radius.md,
    overflow: 'hidden',
  },
  fieldOff: { borderStyle: 'dashed', borderColor: 'rgba(27,38,56,0.42)', backgroundColor: 'transparent' },
  input: {
    flex: 1,
    alignSelf: 'stretch',
    paddingHorizontal: 14,
    ...(Platform.OS === 'web' ? ({ outlineStyle: 'none' } as unknown as ViewStyle) : null),
  },
  pick: { paddingHorizontal: 14 },
  pickMark: { marginLeft: 'auto' },
  focusEdge: { position: 'absolute', left: 0, right: 0, bottom: 0, height: 4, backgroundColor: color.marigold },
  maker: { flexDirection: 'row', alignItems: 'center', justifyContent: 'center', gap: 7, marginTop: 'auto', paddingTop: 20 },
  stepper: { flexDirection: 'row', alignItems: 'center', gap: 8 },
  stepperBig: { gap: 18 },
  sq: { alignItems: 'center', justifyContent: 'center', backgroundColor: color.card, borderWidth, borderColor: color.ink, borderRadius: 10 },
  bigFig: { minWidth: 168 },
  fig: { minWidth: 104 },
  flex: { flex: 1, minWidth: 0 },
  teal: { color: color.teal, fontWeight: '600' },
  head2: { flexDirection: 'row', alignItems: 'center', gap: 12 },
  head2Title: { flex: 1, minWidth: 0 },
  subHead: { flexDirection: 'row', alignItems: 'center', justifyContent: 'space-between', marginTop: 12, marginBottom: 14 },
  back: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 4,
    paddingLeft: 10,
    paddingRight: 16,
    backgroundColor: color.card,
    borderWidth,
    borderColor: color.ink,
    borderRadius: 10,
  },
  kvs: { marginTop: 10 },
  kv: { flexDirection: 'row', alignItems: 'center', gap: 12, minHeight: 40, paddingVertical: 5 },
  kvAfter: { minHeight: 38.5 },
  letter: { width: 36, height: 36, alignItems: 'center', justifyContent: 'center', backgroundColor: color.card, borderWidth, borderColor: color.ink, borderRadius: 9, flexShrink: 0 },
  letterSmall: { width: 30, height: 30, borderRadius: 8 },
  group: { marginTop: 18, marginBottom: 4 },
  groupFirst: { marginTop: 14, marginBottom: 4 },
});
