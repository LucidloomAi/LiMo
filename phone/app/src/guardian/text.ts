// The guardian screens' type, as design/screens/console.css sets it, with
// CSS's em turned into points and line heights written out. Reading text
// 16, small text 14, mono labels 12 and never smaller, headings 28 in
// Fraunces (30 at the console's width). The shared styles of the child's
// screens are used where they are the same (src/ui/paper/text.tsx).

import { StyleSheet, type TextStyle } from 'react-native';

import { bodyFamily, color, display, monoFamily } from '../theme';

const em = (ems: number, size: number) => Math.round(ems * size * 1000) / 1000;

const mono = (weight: TextStyle['fontWeight'], size: number, lineHeight: number, spacing = 0): TextStyle => ({
  fontFamily: monoFamily,
  fontWeight: weight,
  fontSize: size,
  lineHeight,
  letterSpacing: em(spacing, size),
});

export const gtype = StyleSheet.create({
  /** .cn-h1 at a phone's width. */
  h1: { fontFamily: display(28), fontWeight: '600', fontSize: 28, lineHeight: 30.8, letterSpacing: em(-0.012, 28) },
  /** .cn-h1 at the console's width. */
  h1Wide: { fontFamily: display(30), fontWeight: '600', fontSize: 30, lineHeight: 33, letterSpacing: em(-0.012, 30) },
  /** .cn-label. */
  label: { ...mono('500', 12, 12, 0.12), textTransform: 'uppercase', color: color.inkSoft },
  /** .cn-label em: a hint beside a label. */
  hint: { fontFamily: bodyFamily, fontWeight: '400', fontSize: 14, lineHeight: 14, color: color.inkSoft },
  /** .cn-detail. */
  detail: { fontSize: 16, lineHeight: 22.4, color: color.inkSoft },
  /** .cn-small. */
  small: { fontSize: 14, lineHeight: 20.3, color: color.inkSoft },
  /** .cn-err: a message under a field. */
  err: { fontSize: 14, lineHeight: 20, fontWeight: '600', color: color.coral },
  /** .cn-btn. */
  button: { fontWeight: '600', fontSize: 16, lineHeight: 16, color: color.ink, textAlign: 'center' },
  /** .cn-btn.mono. */
  buttonMono: { ...mono('600', 14, 14), color: color.ink, textAlign: 'center' },
  /** .cn-answers .cn-btn. */
  buttonAnswer: { fontWeight: '600', fontSize: 15, lineHeight: 15, color: color.ink, textAlign: 'center' },
  /** .cn-chip. */
  chip: { ...mono('500', 14, 14), color: color.ink, textAlign: 'center' },
  /** A field's text. */
  field: { fontFamily: bodyFamily, fontSize: 16, color: color.ink },
  /** .cn-field .val: a time in a field. */
  fieldTime: { ...mono('600', 18, 18), color: color.marigoldDeep },
  /** .cn-mark: LiMo's name. */
  mark: { fontFamily: display(22), fontWeight: '600', fontSize: 22, lineHeight: 22, letterSpacing: em(-0.01, 22) },
  /** .cn-li b, .cn-acct .who b, .cn-dev .nm b, .cn-next b, .cn-two b, .cn-need b. */
  rowTitle: { fontSize: 16, fontWeight: '600', lineHeight: 20, color: color.ink },
  /** .cn-li i, .cn-next i. */
  rowText: { fontSize: 14, lineHeight: 18.9, color: color.inkSoft },
  /** .cn-two p. */
  twoText: { fontSize: 14, lineHeight: 19.6, color: color.inkSoft },
  /** .cn-tick. */
  tick: { fontSize: 16, lineHeight: 21.6, color: color.ink },
  /** .cn-refuse p. */
  refuse: { fontSize: 16, lineHeight: 23.2, color: color.ink },
  /** .cn-forgot p. */
  forgot: { fontSize: 14, lineHeight: 20.3, color: color.inkSoft },
  /** .cn-ok: a thing made, on Done. */
  ok: { fontSize: 16, lineHeight: 22.4, color: color.ink },
  /** .cn-bigfig: the time a day, large. */
  bigFigure: { fontFamily: display(48), fontWeight: '600', fontSize: 48, lineHeight: 48, letterSpacing: em(-0.02, 48), color: color.marigoldDeep, textAlign: 'center' },
  /** .cn-fig: a day's time. */
  figure: { ...mono('600', 15, 15), color: color.marigoldDeep, textAlign: 'center' },
  /** .cn-day span: a weekday's name. */
  day: { fontSize: 16, lineHeight: 22.4, color: color.ink },
  /** .cn-acct .who .cn-label: an account's facts. */
  facts: { ...mono('500', 12, 17.4, 0.08), textTransform: 'uppercase', color: color.inkSoft },
  /** .cn-warn p, li. */
  warn: { fontSize: 15, lineHeight: 21, color: color.ink },
  warnStep: { fontSize: 15, lineHeight: 21.75, color: color.ink },
  /** .cn-kid-head b: a child's name. */
  kidName: { fontFamily: display(24), fontWeight: '600', fontSize: 24, lineHeight: 26.4, letterSpacing: em(-0.01, 24), color: color.ink },
  /** .cn-big: the time left. */
  big: { fontFamily: display(40), fontWeight: '600', fontSize: 40, lineHeight: 40, letterSpacing: em(-0.02, 40), color: color.marigoldDeep },
  /** .cn-used: the line under the bar. */
  used: { fontSize: 15, lineHeight: 20.25, color: color.ink },
  /** .cn-dev .st: a device's state. */
  state: { ...mono('500', 12, 15.6, 0.08), textTransform: 'uppercase', color: color.inkSoft },
  /** .cn-dev .need: what needs a guardian on a device. */
  need: { ...mono('600', 12, 15.6, 0.08), textTransform: 'uppercase', color: color.coral },
  /** .cn-need p: the sentence for what is switched off. */
  needWords: { fontSize: 15, lineHeight: 21, color: color.inkSoft },
  /** .cn-dev .min: a device's use today. */
  minutes: { ...mono('500', 14, 14), color: color.ink },
  /** .cn-dev.missing p. */
  missing: { fontSize: 15, lineHeight: 20.25, color: color.ink },
  /** .cn-st: a stamp. */
  stamp: { ...mono('600', 12, 12, 0.1), textTransform: 'uppercase' },
  /** .cn-req-top b: who asks for how long. */
  ask: { fontSize: 17, fontWeight: '600', lineHeight: 21.25, color: color.ink },
  /** .cn-req-top .cn-label: where and when. */
  askWhere: { ...mono('500', 12, 15.6, 0.12), textTransform: 'uppercase', color: color.inkSoft },
  /** .cn-quote: the child's own reason. */
  quote: { fontSize: 16, lineHeight: 22.4, color: color.ink },
  /** .cn-quiet. */
  quiet: { fontSize: 15, lineHeight: 21.75, color: color.inkSoft },
  quietBold: { fontSize: 15, lineHeight: 21.75, fontWeight: '600', color: color.ink },
  /** .cn-h2: a card's heading on a screen with tabs. */
  h2: { fontFamily: display(22), fontWeight: '600', fontSize: 22, lineHeight: 25.3, letterSpacing: em(-0.008, 22), color: color.ink },
  /** .cn-kv .k: a fact's name. */
  kvKey: { ...mono('500', 12, 16.2, 0.1), textTransform: 'uppercase', color: color.inkSoft },
  /** .cn-kv .v: a fact. */
  kvValue: { fontSize: 16, lineHeight: 21.6, color: color.ink },
  /** .cn-letter: an app's first letter on its tile. */
  letter: { ...mono('600', 15, 15), color: color.ink },
  letterSmall: { ...mono('600', 13, 13), color: color.ink },
  /** .cn-app .nm: an app's name in a list. */
  appName: { fontSize: 16, lineHeight: 20.8, color: color.ink },
  /** .cn-urow .nm. */
  useName: { fontSize: 16, lineHeight: 22.4, color: color.ink },
  /** .cn-fixed: what cannot be taken off, and why. */
  fixed: { ...mono('600', 12, 12, 0.08), textTransform: 'uppercase', color: color.teal },
  /** .cn-okst: a step's state at the end of its row. */
  okState: { ...mono('600', 12, 12, 0.1), textTransform: 'uppercase' },
  /** .cn-acc .who .cn-label: an account's facts on Devices. */
  accFacts: { ...mono('500', 12, 16.8, 0.08), textTransform: 'uppercase', color: color.inkSoft },
  /** .cn-acc .whose: whose an account is. */
  whose: { fontSize: 15, lineHeight: 19.5, color: color.ink },
  /** .cn-saved p, .cn-update p. */
  notice: { fontSize: 15, lineHeight: 21, color: color.ink },
  /** .cn-code: a one time code, large. */
  code: { ...mono('600', 44, 44, 0.08), color: color.ink },
  /** .cn-addr: a web address in a sentence. */
  address: { ...mono('500', 14, 19.6), color: color.ink },
  /** A line that says a change was made, in teal. */
  done: { fontSize: 14, lineHeight: 20, fontWeight: '600', color: color.teal },
  /** .cn-foot b, i: the cable's state at the foot of Add a phone. */
  footLine: { fontSize: 15, lineHeight: 19.5, fontWeight: '600', color: color.ink },
  footMore: { fontSize: 14, lineHeight: 18.2, color: color.inkSoft },
  /** .cn-ol li, .sub, and its number. */
  olItem: { fontSize: 16, lineHeight: 22.4, color: color.ink },
  olSub: { fontSize: 14, lineHeight: 19.6, color: color.inkSoft },
  olNumber: { ...mono('600', 13, 13), color: color.ink },
  /** .cn-note p. */
  note: { fontSize: 14, lineHeight: 19.6, color: color.ink },
  /** .cn-finger: the key's fingerprint, to compare with the phone's. */
  finger: { ...mono('600', 17, 27.2, 0.03), color: color.ink },
  /** .cn-statusrow. */
  status: { fontSize: 15, lineHeight: 21, color: color.inkSoft },
  /** .cn-maker span: "MADE BY". */
  makerWords: { ...mono('500', 12, 12, 0.12), textTransform: 'uppercase', color: color.inkSoft },
  /** .cn-maker b. */
  makerName: { fontFamily: bodyFamily, fontWeight: '600', fontSize: 14, lineHeight: 14, letterSpacing: em(-0.005, 14), color: color.ink },
});
