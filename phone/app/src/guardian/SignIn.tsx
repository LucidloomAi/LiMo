// G0, the guardian console's first screen: sign in with the family's
// guardian password. It asks the door who it is first, so a child's Windows
// account is told at once that the settings are not for it, and is never
// shown a password field.
//
// Paper and Ink, at any width: one column, never wider than 440, in the
// middle of the page; the console's window is about 1100 by 760, a phone
// 390. The line under the field keeps its room when it is empty, and the
// button keeps its size and words' room while a password is checked, so
// nothing moves (design rule 3). Every tap shows a result (rule 4).

import { useEffect, useRef, useState } from 'react';
import { StyleSheet, TextInput, View } from 'react-native';

import type { DoorOutcome, GuardianHost } from '../guardianSeam';
import { bodyFamily, borderWidth, color, radius } from '../theme';
import { Page } from '../ui/paper/Page';
import { Button, Card, Heading, Label, Stamp, Tail, Tile, TopRow } from '../ui/paper/parts';
import { Txt, type } from '../ui/paper/text';
import * as W from './words';

export type SignInProps = {
  host: GuardianHost;
  /** How a wait's end is written; the PC's own way by default. */
  timeOfDay?: (unixSeconds: number) => string;
};

type Shown =
  | { kind: 'asking' }
  | { kind: 'form'; line: string; tone: 'quiet' | 'coral'; busy: boolean }
  | { kind: 'refused'; words: string }
  | { kind: 'problem'; words: string }
  | { kind: 'signed-in' };

/** What the line under the field says after an answer, or null when the screen changes instead. */
function lineFor(outcome: DoorOutcome, timeOfDay: (s: number) => string): string | null {
  switch (outcome.kind) {
    case 'wrong':
      return W.signIn.wrong(outcome.triesBeforeWait);
    case 'wait':
      return W.signIn.wait(timeOfDay(outcome.until));
    case 'problem':
      return outcome.words;
    default:
      return null;
  }
}

export function SignIn({ host, timeOfDay = W.timeOfDay }: SignInProps) {
  const [shown, setShown] = useState<Shown>({ kind: 'asking' });
  const [password, setPassword] = useState('');
  const live = useRef(true);

  useEffect(() => {
    live.current = true;
    host.about().then((outcome) => {
      if (!live.current) return;
      if (outcome.kind === 'child-account') setShown({ kind: 'refused', words: W.notForThisAccount.child(outcome.child) });
      else if (outcome.kind === 'unknown-account') setShown({ kind: 'refused', words: W.notForThisAccount.unknown });
      else if (outcome.kind === 'problem') setShown({ kind: 'problem', words: outcome.words });
      else setShown({ kind: 'form', line: '', tone: 'quiet', busy: false });
    });
    return () => {
      live.current = false;
    };
  }, [host]);

  const submit = () => {
    if (shown.kind !== 'form' || shown.busy) return;
    if (password === '') {
      setShown({ kind: 'form', line: W.signIn.empty, tone: 'coral', busy: false });
      return;
    }
    const typed = password;
    // The field is emptied at once: the password stays nowhere on the page.
    setPassword('');
    setShown({ kind: 'form', line: W.signIn.checking, tone: 'quiet', busy: true });
    host.signIn(typed).then((outcome) => {
      if (!live.current) return;
      if (outcome.kind === 'signed-in') setShown({ kind: 'signed-in' });
      else if (outcome.kind === 'child-account') setShown({ kind: 'refused', words: W.notForThisAccount.child(outcome.child) });
      else if (outcome.kind === 'unknown-account') setShown({ kind: 'refused', words: W.notForThisAccount.unknown });
      else setShown({ kind: 'form', line: lineFor(outcome, timeOfDay) ?? '', tone: 'coral', busy: false });
    });
  };

  return (
    <Page testID="sign-in">
      <View style={styles.column}>
        <TopRow />
        <View style={styles.middle}>
          {shown.kind === 'signed-in' ? (
            <Card testID="signed-in">
              <Tile icon="key" large />
              <View style={styles.stampRow}>
                <Stamp tone="teal">{W.signedIn.stamp}</Stamp>
              </View>
              <Heading style={styles.heading}>{W.signedIn.heading}</Heading>
              <Txt style={[type.detail, styles.text]} wrap="pretty">
                {W.signedIn.text}
              </Txt>
            </Card>
          ) : shown.kind === 'refused' ? (
            <Card testID="refused">
              <Tile icon="key" large />
              <View style={styles.stampRow}>
                <Stamp tone="coral">{W.notForThisAccount.stamp}</Stamp>
              </View>
              <Heading style={styles.heading}>{W.notForThisAccount.heading}</Heading>
              <Txt style={[type.detail, styles.text]} wrap="pretty" testID="refusal">
                {shown.words}
              </Txt>
            </Card>
          ) : (
            <Card>
              <Tile icon="key" large />
              <Heading style={styles.heading}>{W.signIn.heading}</Heading>
              <Txt style={[type.detail, styles.text]} wrap="pretty">
                {W.signIn.text}
              </Txt>
              <Label style={styles.fieldLabel}>{W.signIn.field}</Label>
              <TextInput
                testID="password"
                style={[styles.field, shown.kind !== 'form' && styles.fieldOff]}
                value={password}
                onChangeText={setPassword}
                onSubmitEditing={submit}
                secureTextEntry
                autoComplete="off"
                autoCorrect={false}
                autoCapitalize="none"
                editable={shown.kind === 'form' && !shown.busy}
                accessibilityLabel={W.signIn.field}
              />
              <View style={styles.lineRoom} testID="message-room">
                <Txt
                  testID="message"
                  style={[type.state, shown.kind === 'form' && shown.tone === 'coral' && styles.coral, shown.kind === 'problem' && styles.coral]}
                  accessibilityLiveRegion="polite"
                >
                  {shown.kind === 'form' ? shown.line : shown.kind === 'problem' ? shown.words : ''}
                </Txt>
              </View>
              <Button
                testID="open"
                label={W.signIn.button}
                kind="go"
                off={shown.kind !== 'form' || shown.busy}
                onPress={submit}
                outer={styles.button}
              />
            </Card>
          )}
          {shown.kind === 'form' || shown.kind === 'asking' ? (
            <Txt style={[type.quiet, styles.forgotten]} wrap="pretty" testID="forgotten">
              {W.signIn.forgotten}
            </Txt>
          ) : null}
        </View>
        <Tail />
      </View>
    </Page>
  );
}

const styles = StyleSheet.create({
  column: { flexGrow: 1, width: '100%', maxWidth: 440, alignSelf: 'center', paddingBottom: 14 },
  middle: { flexGrow: 1, justifyContent: 'center', paddingTop: 20 },
  heading: { marginTop: 14 },
  text: { marginTop: 8 },
  stampRow: { flexDirection: 'row', marginTop: 14 },
  fieldLabel: { marginTop: 22 },
  field: {
    marginTop: 8,
    height: 48,
    paddingHorizontal: 12,
    fontFamily: bodyFamily,
    fontSize: 16,
    color: color.ink,
    backgroundColor: color.card,
    borderWidth,
    borderColor: color.ink,
    borderRadius: radius.md,
  },
  fieldOff: { borderColor: color.inkSoft },
  // Three lines' room, kept when empty, so nothing below moves when a
  // message comes, the longest at phone width too.
  lineRoom: { minHeight: 60, marginTop: 8 },
  coral: { color: color.coral },
  button: { marginTop: 4 },
  forgotten: { marginTop: 16, paddingHorizontal: 2 },
});
