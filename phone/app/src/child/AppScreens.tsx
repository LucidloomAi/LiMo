// The child's own app, after setup: Today (design/screens/phone.html, E) with
// its banner when a guardian is needed (phone-setup.html, T), and How it
// works (H). Both sit under the floating tab bar, which ChildApp draws.
//
// Today shows the engine's words as they come (TodayView): it never makes a
// sentence about time, or about what is switched off. With more than one
// step off, the banner says the first step's sentence and, under it, how many
// more there are; its button walks a guardian through all of them. It never
// says "counting": looking at LiMo itself does not use time, and the screen
// says so.

import { StyleSheet, View } from 'react-native';

import type { RulesView, TodayView } from '../host';
import { borderWidth, color } from '../theme';
import { Button, Card, Dashed, Label, Stamp, Tail, Tile, TopRow, Who } from '../ui/paper/parts';
import { Txt, type } from '../ui/paper/text';
import { SetupRows } from './SetupScreens';
import * as W from './words';

/**
 * A time never breaks between its digits and am or pm, and a span never
 * between its number and its unit ("37 min"): the space between becomes one
 * that does not break.
 */
export function keepTimesWhole(text: string): string {
  return text.replace(/(\d) (am|pm|min|minutes?|h|hours?)\b/g, '$1\u00a0$2');
}

/** The word joiner: no line may break beside it. It is never drawn. */
export const WORD_JOINER = '⁠';

/**
 * An address never breaks inside: with a word joiner between each two of its
 * characters, a line can break only before it, so on a phone too narrow for
 * the whole line it moves to a line of its own.
 */
export function unbroken(text: string): string {
  return Array.from(text).join(WORD_JOINER);
}

// ---------- E and T. Today ----------

export function Today({
  name,
  view,
  onAsk,
  onShowGuardian,
}: {
  name: string;
  /** Null until the host has answered. */
  view: TodayView | null;
  onAsk: () => void;
  onShowGuardian: () => void;
}) {
  return (
    <>
      <TopRow right={<Who name={name} />} />
      {view ? (
        <>
          {view.guardianNeeded ? (
            <Card outer={styles.afterTop} testID="banner">
              <View style={styles.bannerTop}>
                <Tile icon="alert" large />
                <Stamp tone="coral">{W.today.needsGuardian}</Stamp>
              </View>
              {/* The engine's sentence for the first step that is off, and how many more there are. */}
              <Txt style={[type.line, styles.bannerWords]} wrap="pretty" testID="banner-words">
                {view.guardianNeeded.words[0]}
              </Txt>
              {view.guardianNeeded.off.length > 1 ? (
                <View style={styles.bannerMore} testID="banner-more">
                  <Txt style={type.where}>{W.today.more(view.guardianNeeded.off.length - 1)}</Txt>
                </View>
              ) : null}
              <Button label={W.today.showGuardian} onPress={onShowGuardian} outer={styles.bannerButton} testID="show-guardian" />
            </Card>
          ) : null}
          <Card outer={view.guardianNeeded ? styles.between : styles.afterTop} testID="time-left">
            <Txt style={type.big} accessibilityRole="header">
              {keepTimesWhole(view.left.words)}
            </Txt>
            {view.left.kind === 'left' ? <Label style={styles.bigLabel}>{W.today.left}</Label> : null}
            <Txt style={[type.quiet, styles.quiet]}>{W.today.quiet}</Txt>
            <View style={styles.used}>
              <Dashed />
              <View style={styles.usedInside}>
                {view.usedPart !== null ? (
                  <View style={styles.bar} importantForAccessibility="no-hide-descendants">
                    <View style={[styles.barFill, { width: `${Math.round(view.usedPart * 1000) / 10}%` }]} />
                  </View>
                ) : null}
                <Txt style={[type.line, view.usedPart !== null && styles.usedLine]}>{keepTimesWhole(view.used)}</Txt>
                <Txt style={[type.where, styles.where]}>{keepTimesWhole(view.where)}</Txt>
              </View>
            </View>
          </Card>
          {view.bedtime ? (
            <Card outer={styles.between} style={styles.hours}>
              <Tile icon="moon" large />
              <Txt style={[type.line, styles.hoursWords]} wrap="balance">
                {keepTimesWhole(view.bedtime)}
              </Txt>
            </Card>
          ) : null}
          <Button label={W.today.ask} onPress={onAsk} outer={styles.between} testID="ask-for-more-time" />
          {view.apps.length ? (
            <>
              <Label style={styles.sec} header>{W.today.apps}</Label>
              <Card style={styles.list}>
                {view.apps.map((app, i) => (
                  <View key={`${app.name} ${i}`}>
                    {i > 0 ? <Dashed /> : null}
                    <View style={styles.li} accessible accessibilityLabel={`${app.name}, ${app.minutes}`}>
                      <View style={styles.letter}>
                        <Txt style={type.letter}>{firstLetter(app.name)}</Txt>
                      </View>
                      <Txt style={[type.app, styles.appName]}>{app.name}</Txt>
                      <Txt style={type.minutes}>{app.minutes}</Txt>
                    </View>
                  </View>
                ))}
              </Card>
            </>
          ) : null}
        </>
      ) : null}
    </>
  );
}

/** The letter on an app's tile: its first letter or digit, in capitals. */
function firstLetter(name: string): string {
  const ch = Array.from(name.trim())[0] ?? '';
  return ch.toUpperCase();
}

// ---------- H. How it works ----------

export function HowItWorks({ name, rules, version }: { name: string; rules: RulesView | null; version: string | null }) {
  const rows: [string, string][] = [];
  if (rules) {
    rows.push([W.how.timeADay, rules.timeADay]);
    if (rules.screensOff) rows.push([W.how.screensOff, rules.screensOff]);
    if (rules.alwaysOpen.length) rows.push([W.how.alwaysOpen, rules.alwaysOpen.join(', ')]);
    if (rules.notAllowed.length) rows.push([W.how.notAllowed, rules.notAllowed.join(', ')]);
  }
  return (
    <>
      <TopRow right={<Who name={name} />} />
      <Label style={styles.sec} header>{W.how.rules}</Label>
      <Card style={styles.list} testID="rules">
        {rows.map(([label, value], i) => (
          <View key={label}>
            {i > 0 ? <Dashed /> : null}
            <View style={styles.rule} accessible accessibilityLabel={`${label}: ${value}`}>
              <View style={styles.ruleLabel}>
                <Txt style={type.label}>{label}</Txt>
              </View>
              <Txt style={[type.ruleValue, styles.ruleValue]} wrap="balance">
                {keepTimesWhole(value)}
              </Txt>
            </View>
          </View>
        ))}
      </Card>
      <Label style={styles.sec} header>{W.how.does}</Label>
      <SetupRows rows={W.intro.map((c) => ({ icon: c.icon, title: c.title, text: c.text }))} />
      <Label style={styles.sec} header>{W.how.about}</Label>
      <Card testID="about">
        <Txt style={type.aboutTitle}>{W.how.name(version ?? '')}</Txt>
        {/* The address is plain text: nothing on this screen opens a browser. */}
        <View style={styles.aboutText} accessible accessibilityLabel={`${W.how.more}${W.how.site}`}>
          <Txt style={type.aboutText}>
            {W.how.more}
            <Txt style={type.address}>{unbroken(W.how.site)}</Txt>
          </Txt>
        </View>
      </Card>
      <Tail />
    </>
  );
}

const styles = StyleSheet.create({
  afterTop: { marginTop: 10 },
  between: { marginTop: 14 },
  sec: { marginTop: 16, marginBottom: 8, marginHorizontal: 2 },
  // With very large text the stamp goes under the tile instead of past the card's edge.
  bannerTop: { flexDirection: 'row', flexWrap: 'wrap', alignItems: 'center', justifyContent: 'space-between', columnGap: 12, rowGap: 8 },
  bannerWords: { marginTop: 12 },
  bannerMore: { marginTop: 8 },
  bannerButton: { marginTop: 14 },
  bigLabel: { marginTop: 9 },
  quiet: { marginTop: 10 },
  used: { marginTop: 12 },
  usedInside: { paddingTop: 14 },
  bar: {
    height: 10,
    borderWidth,
    borderColor: color.ink,
    borderRadius: 999,
    overflow: 'hidden',
    backgroundColor: color.paper,
  },
  barFill: { height: '100%', backgroundColor: color.marigold, borderRightWidth: borderWidth, borderRightColor: color.ink },
  usedLine: { marginTop: 10 },
  where: { marginTop: 8 },
  hours: { flexDirection: 'row', alignItems: 'center', gap: 12, paddingTop: 12, paddingBottom: 12, paddingLeft: 12, paddingRight: 14 },
  hoursWords: { flex: 1 },
  list: { paddingVertical: 2, paddingHorizontal: 14 },
  li: { flexDirection: 'row', alignItems: 'center', gap: 12, paddingVertical: 8 },
  letter: {
    width: 36,
    height: 36,
    alignItems: 'center',
    justifyContent: 'center',
    backgroundColor: color.card,
    borderWidth,
    borderColor: color.ink,
    borderRadius: 9,
  },
  appName: { flex: 1 },
  rule: { flexDirection: 'row', alignItems: 'baseline', justifyContent: 'space-between', gap: 14, paddingVertical: 12 },
  // The label keeps its size until it would take more than half the row (very
  // large text); then it wraps, and the value always gives way first.
  ruleLabel: { flexShrink: 0, maxWidth: '50%' },
  ruleValue: { flexShrink: 1, minWidth: 0 },
  aboutText: { marginTop: 4 },
});
