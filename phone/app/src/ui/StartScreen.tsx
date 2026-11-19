// The plain start screen both forms show until setup is built: the app's
// name, which form this is, a card saying setup is not built yet, a line
// saying who answered the host, and the maker's mark at the end.
//
// Paper and Ink, plainly: the real screens are the lead's design and replace
// this one. The page fills the whole display (Android draws apps edge to
// edge), and the safe area insets keep every word clear of the status bar,
// the navigation bar and any cut out.

import { useEffect, useState } from 'react';
import { Image, ScrollView, StyleSheet, Text, View } from 'react-native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';

import logo from '../generated/lucidloom-logo.png';
import { type About, host, HostError } from '../host';
import { Icon, type IconName } from '../icons/Icon';
import { bodyFamily, borderWidth, color, display, hardShadow, monoFamily, radius, space, tokens } from '../theme';

export type StartScreenProps = {
  /** The app's name, as the launcher shows it. */
  title: string;
  /** A short state in capitals, shown as a stamp. */
  stamp: string;
  icon: IconName;
  heading: string;
  paragraphs: string[];
  /** The line under the card, from what the host answered. */
  checkLine: (about: About) => string;
};

type Check = { kind: 'waiting' } | { kind: 'answered'; about: About } | { kind: 'failed'; text: string };

export function StartScreen({ title, stamp, icon, heading, paragraphs, checkLine }: StartScreenProps) {
  const insets = useSafeAreaInsets();
  const [check, setCheck] = useState<Check>({ kind: 'waiting' });

  useEffect(() => {
    let live = true;
    host
      .request('about', {})
      .then((about) => live && setCheck({ kind: 'answered', about }))
      .catch((e: unknown) => {
        const text = e instanceof HostError ? `${e.code}: ${e.message}` : String(e);
        if (live) setCheck({ kind: 'failed', text });
      });
    return () => {
      live = false;
    };
  }, []);

  // The line keeps its room while the answer comes, so nothing moves.
  const line =
    check.kind === 'answered' ? checkLine(check.about) : check.kind === 'failed' ? `The host did not answer. ${check.text}` : ' ';

  return (
    <ScrollView
      style={styles.page}
      contentContainerStyle={[
        styles.content,
        {
          paddingTop: insets.top + space(8),
          paddingBottom: insets.bottom + space(6),
          paddingLeft: insets.left + space(5),
          paddingRight: insets.right + space(5),
        },
      ]}
    >
      <View style={styles.column}>
        <View style={styles.stamp}>
          <Text style={styles.stampText}>{stamp}</Text>
        </View>
        <Text style={styles.title} accessibilityRole="header">
          {title}
        </Text>

        <View style={styles.cardWrap}>
          <View style={styles.cardShadow} />
          <View style={styles.card}>
            <View style={styles.cardHead}>
              <Icon name={icon} size={tokens.icon.heading} />
              <Text style={styles.heading} accessibilityRole="header">
                {heading}
              </Text>
            </View>
            {paragraphs.map((p) => (
              <Text key={p} style={styles.paragraph}>
                {p}
              </Text>
            ))}
          </View>
        </View>

        <Text style={[styles.check, check.kind === 'failed' && styles.checkFailed]}>{line}</Text>
      </View>

      <View style={styles.maker} accessible accessibilityLabel={`${tokens.maker.words} ${tokens.maker.name}`}>
        <Text style={styles.makerWords}>{tokens.maker.words.toUpperCase()}</Text>
        <Image source={logo} style={styles.makerLogo} accessibilityIgnoresInvertColors />
        <Text style={styles.makerName}>{tokens.maker.name}</Text>
      </View>
    </ScrollView>
  );
}

const TITLE_SIZE = 44;
const HEADING_SIZE = 24;
const SHADOW = hardShadow.card;

const styles = StyleSheet.create({
  page: { flex: 1, backgroundColor: color.paper },
  content: { flexGrow: 1, justifyContent: 'space-between', alignItems: 'center' },
  column: { width: '100%', maxWidth: 520 },
  stamp: {
    alignSelf: 'flex-start',
    borderWidth,
    borderColor: color.ink,
    borderRadius: radius.sm,
    paddingHorizontal: space(2),
    paddingVertical: space(1),
  },
  stampText: { fontFamily: monoFamily, fontWeight: '500', fontSize: 12, letterSpacing: 1.4, color: color.ink },
  title: {
    marginTop: space(4),
    fontFamily: display(TITLE_SIZE),
    fontWeight: '600',
    fontSize: TITLE_SIZE,
    lineHeight: Math.round(TITLE_SIZE * 1.1),
    color: color.ink,
  },
  cardWrap: { marginTop: space(6), marginRight: SHADOW.x, marginBottom: SHADOW.y },
  cardShadow: {
    position: 'absolute',
    top: SHADOW.y,
    left: SHADOW.x,
    right: -SHADOW.x,
    bottom: -SHADOW.y,
    borderRadius: radius.lg,
    backgroundColor: SHADOW.color,
  },
  card: {
    backgroundColor: color.card,
    borderWidth,
    borderColor: color.ink,
    borderRadius: radius.lg,
    padding: space(5),
  },
  cardHead: { flexDirection: 'row', alignItems: 'center', gap: space(3), marginBottom: space(2) },
  heading: {
    flex: 1,
    fontFamily: display(HEADING_SIZE),
    fontWeight: '600',
    fontSize: HEADING_SIZE,
    lineHeight: Math.round(HEADING_SIZE * 1.2),
    color: color.ink,
  },
  paragraph: { marginTop: space(2), fontFamily: bodyFamily, fontSize: 16, lineHeight: 23, color: color.inkSoft },
  check: { marginTop: space(5), fontFamily: monoFamily, fontWeight: '500', fontSize: 12, lineHeight: 17, color: color.inkSoft },
  checkFailed: { color: color.coral },
  maker: { marginTop: space(10), flexDirection: 'row', alignItems: 'center', gap: space(2) },
  makerWords: { fontFamily: monoFamily, fontWeight: '500', fontSize: 11, letterSpacing: 1.3, color: color.inkSoft },
  makerLogo: { width: tokens.maker.logoSize, height: tokens.maker.logoSize },
  makerName: { fontFamily: bodyFamily, fontWeight: '600', fontSize: 14, color: color.ink },
});
