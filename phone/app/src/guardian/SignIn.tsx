// G0, the guardian console's first screen: sign in with the family's
// guardian password. It asks the door first, so a child's Windows account is
// told at once, in the service's own words, that the settings are not for
// it, and is never shown a password field.
//
// Paper and Ink, at any width: one column, never wider than 440, in the
// middle of the page; the console's window is about 1100 by 760, a phone
// 390. The line under the field keeps three lines of room when it is empty,
// and the button keeps its size while a password is checked, so nothing
// moves (design rule 3). Every tap shows a result (rule 4).

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
};

type Shown =
  | { kind: 'asking' }
  | { kind: 'form'; line: string; tone: 'quiet' | 'coral'; busy: boolean }
  | { kind: 'refused'; words: string }
  | { kind: 'problem'; words: string }
  | { kind: 'signed-in' };

/** What shows after the door's answer to a sign in. */
function afterSignIn(outcome: DoorOutcome): Shown {
  switch (outcome.kind) {
    case 'done':
      return { kind: 'signed-in' };
    case 'refused':
      return { kind: 'refused', words: outcome.words };
    default:
      return { kind: 'form', line: outcome.words, tone: 'coral', busy: false };
  }
}

/** What shows after the door's answer to the first question. */
function afterAbout(outcome: DoorOutcome): Shown {
  switch (outcome.kind) {
    case 'refused':
      return { kind: 'refused', words: outcome.words };
    case 'problem':
      return { kind: 'problem', words: outcome.words };
    case 'done':
      return { kind: 'form', line: '', tone: 'quiet', busy: false };
    default:
      return { kind: 'form', line: outcome.words, tone: 'coral', busy: false };
  }
}

export function SignIn({ host }: SignInProps) {
  const [shown, setShown] = useState<Shown>({ kind: 'asking' });
  const [password, setPassword] = useState('');
  const live = useRef(true);

  useEffect(() => {
    live.current = true;
    host.about().then((outcome) => {
      if (live.current) setShown(afterAbout(outcome));
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
      if (live.current) setShown(afterSignIn(outcome));
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
