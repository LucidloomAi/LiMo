// Text in Paper and Ink, as the phone drawings set it (design/screens/phone.css
// and phone-setup.css): every style here is one of theirs, with CSS's em
// turned into points and line heights written out. Reading text 16, small
// text 14, mono labels 12 and never smaller, headings 28 in Fraunces.

import { createContext, type ReactNode, useContext } from 'react';
import { Platform, StyleSheet, Text, type TextProps, type TextStyle } from 'react-native';

import { bodyFamily, color, display, monoFamily } from '../../theme';

/**
 * A multiplier on every size of text, for the preview on the PC only. On a
 * phone Android applies the person's own font size itself, so the app never
 * sets this and it stays 1.
 */
export const TextScale = createContext(1);

/**
 * How lines break: "balance" evens the lines of a heading, "pretty" keeps a
 * last word from standing alone. Android's own line breaking does the
 * nearest it has (balanced, or its high quality breaking).
 */
export type Wrap = 'balance' | 'pretty';

export type TxtProps = Omit<TextProps, 'style'> & {
  style?: TextStyle | (TextStyle | false | null | undefined)[];
  wrap?: Wrap;
  children?: ReactNode;
};

const SCALED = ['fontSize', 'lineHeight', 'letterSpacing'] as const;

export function Txt({ style, wrap, ...rest }: TxtProps) {
  const scale = useContext(TextScale);
  const flat = StyleSheet.flatten(style) ?? {};
  let scaled: TextStyle = flat;
  if (scale !== 1) {
    scaled = { ...flat };
    for (const key of SCALED) {
      const v = flat[key];
      if (typeof v === 'number') scaled[key] = v * scale;
    }
  }
  const wrapStyle = wrap && Platform.OS === 'web' ? ({ textWrap: wrap } as unknown as TextStyle) : null;
  return (
    <Text
      {...rest}
      textBreakStrategy={wrap === 'balance' ? 'balanced' : 'highQuality'}
      style={[base.text, scaled, wrapStyle]}
    />
  );
}

const base = StyleSheet.create({
  text: { includeFontPadding: false, color: color.ink, fontFamily: bodyFamily },
});

/** CSS letter-spacing in em, as points. */
const em = (ems: number, size: number) => Math.round(ems * size * 1000) / 1000;

const LABEL: TextStyle = {
  fontFamily: monoFamily,
  fontWeight: '500',
  fontSize: 12,
  lineHeight: 12,
  letterSpacing: em(0.12, 12),
  textTransform: 'uppercase',
  color: color.inkSoft,
};

export const type = StyleSheet.create({
  /** .ph-mark: LiMo's name beside its hourglass. */
  mark: { fontFamily: display(22), fontWeight: '600', fontSize: 22, lineHeight: 22, letterSpacing: em(-0.01, 22) },
  /** .ph-label: a mono label in capitals. */
  label: LABEL,
  /** .ph-h1: a screen's heading. */
  h1: { fontFamily: display(28), fontWeight: '600', fontSize: 28, lineHeight: 30.8, letterSpacing: em(-0.012, 28) },
  /** .ph-detail: the words under a heading. */
  detail: { fontSize: 16, lineHeight: 21.6, color: color.inkSoft },
  /** .su-name: the name of a thing to tap, inside a sentence. */
  name: { fontWeight: '600', color: color.ink },
  /** .su-li b, .ph-row b: a row's title. */
  rowTitle: { fontSize: 16, fontWeight: '600', lineHeight: 20 },
  /** .su-li i: a row's words. */
  rowText: { fontSize: 14, lineHeight: 18.9, color: color.inkSoft },
  /** .su-line, .su-banner p, .ph-used p, .ph-hours p: reading text in ink. */
  line: { fontSize: 16, lineHeight: 22.4 },
  /** .su-tick: the words beside the tick box. */
  tick: { fontSize: 16, lineHeight: 21.6 },
  /** .su-state p: the line under a step's stamp. */
  state: { fontSize: 14, lineHeight: 20, color: color.inkSoft },
  /** .su-note p: the words of a note. */
  note: { fontSize: 14, lineHeight: 19.6 },
  /** .ph-st: a small stamp. */
  stamp: { fontFamily: monoFamily, fontWeight: '600', fontSize: 12, lineHeight: 12, letterSpacing: em(0.1, 12), textTransform: 'uppercase' },
  /** .ph-who: the child's name at the top right. */
  who: { fontFamily: monoFamily, fontWeight: '600', fontSize: 12, lineHeight: 12, letterSpacing: em(0.12, 12), textTransform: 'uppercase' },
  /** .ph-btn: a button's words. */
  button: { fontWeight: '600', fontSize: 16, lineHeight: 16, textAlign: 'center' },
  /** .ph-big: the time left. */
  big: { fontFamily: display(44), fontWeight: '600', fontSize: 44, lineHeight: 44, letterSpacing: em(-0.02, 44), color: color.marigoldDeep },
  /** .ph-quiet: the quiet line under it. */
  quiet: { fontSize: 14, lineHeight: 19.6, color: color.inkSoft },
  /** .ph-used .ph-label: where the time went, under the bar. */
  where: { ...LABEL, lineHeight: 17.4, letterSpacing: em(0.08, 12) },
  /** .ph-li: an app on Today's list. */
  app: { fontSize: 16, lineHeight: 22.4 },
  /** .ph-li .min: its minutes. */
  minutes: { fontFamily: monoFamily, fontWeight: '500', fontSize: 14, lineHeight: 14 },
  /** .ph-letter.sm: the letter on an app's tile. */
  letter: { fontFamily: monoFamily, fontWeight: '600', fontSize: 15, lineHeight: 15 },
  /** .ph-tab: a tab's label. */
  tab: { fontFamily: monoFamily, fontWeight: '600', fontSize: 12, lineHeight: 12, letterSpacing: em(0.06, 12), textTransform: 'uppercase' },
  /** .su-rule .v: a rule's value. */
  ruleValue: { fontSize: 16, lineHeight: 21.6, textAlign: 'right' },
  /** .su-about b. */
  aboutTitle: { fontSize: 16, fontWeight: '600', lineHeight: 20.8 },
  /** .su-about p. */
  aboutText: { fontSize: 14, lineHeight: 19.6, color: color.inkSoft },
  /** .su-addr: an address, as plain text in mono, inside a line of .su-about p. */
  address: { fontFamily: monoFamily, fontWeight: '500', fontSize: 14, lineHeight: 19.6, color: color.ink },
  /** .ph-maker span: "MADE BY". */
  makerWords: LABEL,
  /** .ph-maker b: the maker's name. */
  makerName: { fontWeight: '600', fontSize: 14, lineHeight: 14, letterSpacing: em(-0.005, 14) },
  /** The phone's status bar time, in the preview's drawn phones only. */
  status: { fontFamily: monoFamily, fontWeight: '500', fontSize: 12, lineHeight: 12, letterSpacing: em(0.04, 12), color: color.inkSoft },
});
