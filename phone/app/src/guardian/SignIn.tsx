// C0, sign in, as design/screens/console.js draws it (C0 to C0d): LiMo's
// settings open with the guardian password. It asks the door first, so a
// child's Windows account is told at once, in the service's own words, that
// the settings are not for it, and is never shown a password field.
//
// The message under the field has two lines of room kept, so nothing moves
// when one appears (design rule 3); the button keeps its size while a
// password is checked. After too many wrong tries the field and the button
// are switched off until the time the door gave, said on the PC's clock.
// What to do about a forgotten password is said on the screen, because there
// is nothing to tap for it.

import { useEffect, useRef, useState } from 'react';
import { StyleSheet, View } from 'react-native';

import type { DoorOutcome, GuardianHost } from '../guardianSeam';
import { Icon } from '../icons/Icon';
import { Dashed } from '../ui/paper/parts';
import { Txt } from '../ui/paper/text';
import { useClock } from './clock';
import { Page } from './Page';
import { Btn, Card, Field, H1, Label, Room, Tile, TopRow } from './parts';
import { gtype } from './text';
import { timeOfDay } from './time';
import * as W from './words';

export type SignInStart = {
  /** A sentence to show at first, such as why the guardian was signed out. */
  line?: string;
  /** A wait already in force, until this moment (unix seconds). */
  waitUntil?: number;
  /** What is in the field at first: the preview's pictures only. */
  typed?: string;
};

type Shown =
  | { kind: 'asking' }
  | { kind: 'form'; line: string; busy: boolean; waitUntil: number | null }
  | { kind: 'refused'; words: string }
  | { kind: 'problem'; words: string }
  | { kind: 'signed-in' };

export function SignIn({ host, onSignedIn, start }: { host: GuardianHost; onSignedIn?: () => void; start?: SignInStart }) {
  const { now, clock } = useClock();
  const [shown, setShown] = useState<Shown>({ kind: 'asking' });
  const [password, setPassword] = useState(start?.typed ?? '');
  const live = useRef(true);

  const waitWords = (until: number) => W.signIn.wait(timeOfDay(until, clock));
  const formAfter = (outcome: DoorOutcome): Shown => {
    switch (outcome.kind) {
      case 'done':
        return { kind: 'signed-in' };
      case 'refused':
        return { kind: 'refused', words: outcome.words };
      case 'wait':
        return { kind: 'form', line: waitWords(outcome.until), busy: false, waitUntil: outcome.until };
      case 'wrong':
        return outcome.waitUntil !== null
          ? { kind: 'form', line: waitWords(outcome.waitUntil), busy: false, waitUntil: outcome.waitUntil }
          : { kind: 'form', line: outcome.words, busy: false, waitUntil: null };
      default:
        return { kind: 'form', line: outcome.words, busy: false, waitUntil: null };
    }
  };

  useEffect(() => {
    live.current = true;
    host.about().then((outcome) => {
      if (!live.current) return;
      if (outcome.kind === 'refused') setShown({ kind: 'refused', words: outcome.words });
      else if (outcome.kind === 'problem') setShown({ kind: 'problem', words: outcome.words });
      else if (start?.waitUntil && start.waitUntil > now()) setShown({ kind: 'form', line: waitWords(start.waitUntil), busy: false, waitUntil: start.waitUntil });
      else setShown({ kind: 'form', line: outcome.kind === 'done' ? (start?.line ?? '') : outcome.words, busy: false, waitUntil: null });
    });
    return () => {
      live.current = false;
    };
    // Asked once, when the screen opens.
  }, [host]);

  // The field comes back when the wait is over.
  const waitUntil = shown.kind === 'form' ? shown.waitUntil : null;
  useEffect(() => {
    if (waitUntil === null) return undefined;
    const ms = Math.max(0, (waitUntil - now()) * 1000);
    const t = setTimeout(() => {
      if (live.current) setShown({ kind: 'form', line: '', busy: false, waitUntil: null });
    }, ms);
    return () => clearTimeout(t);
  }, [waitUntil, now]);

  useEffect(() => {
    if (shown.kind === 'signed-in') onSignedIn?.();
  }, [shown.kind, onSignedIn]);

  const waiting = shown.kind === 'form' && shown.waitUntil !== null;
  const usable = shown.kind === 'form' && !shown.busy && !waiting;

  const submit = () => {
    if (!usable) return;
    if (password === '') {
      setShown({ kind: 'form', line: W.signIn.empty, busy: false, waitUntil: null });
      return;
    }
    const typed = password;
    // The field is emptied at once: the password stays nowhere on the page.
    setPassword('');
    setShown({ kind: 'form', line: '', busy: true, waitUntil: null });
    host.signIn(typed).then((outcome) => {
      if (live.current) setShown(formAfter(outcome));
    });
  };

  const child = shown.kind === 'refused';
  return (
    <Page testID="sign-in">
      <TopRow />
      <View style={styles.middle}>
        <Card outer={styles.card}>
          <View style={styles.icon}>
            <Icon name={child ? 'lock' : 'key'} size={64} />
          </View>
          <H1>{W.signIn.heading}</H1>
          {shown.kind === 'refused' ? (
            <View testID="refused" style={styles.gap}>
              <Dashed />
              <View style={styles.refuse}>
                <Tile icon="account" />
                <Txt style={[gtype.refuse, styles.refuseWords]} wrap="pretty" testID="refusal">
                  {shown.words}
                </Txt>
              </View>
            </View>
          ) : shown.kind === 'signed-in' && !onSignedIn ? (
            <View testID="signed-in" />
          ) : (
            <>
              <Label style={styles.gap}>{W.signIn.field}</Label>
              <Field
                testID="password"
                value={password}
                onChangeText={setPassword}
                onSubmitEditing={submit}
                secureTextEntry
                autoComplete="off"
                autoCorrect={false}
                autoCapitalize="none"
                autoFocus
                off={!usable && !(shown.kind === 'form' && shown.busy)}
                editable={usable}
                accessibilityLabel={W.signIn.field}
              />
              <Room words={shown.kind === 'form' ? shown.line : shown.kind === 'problem' ? shown.words : ''} />
              <Btn testID="open" label={W.signIn.button} look="go" off={!usable} onPress={submit} outer={styles.button} />
              <View style={styles.forgot}>
                <Dashed />
                <Label style={styles.forgotLabel}>{W.signIn.forgottenLabel}</Label>
                <Txt style={[gtype.forgot, styles.forgotWords]} wrap="pretty" testID="forgotten">
                  {W.signIn.forgotten}
                </Txt>
              </View>
            </>
          )}
        </Card>
      </View>
    </Page>
  );
}

const styles = StyleSheet.create({
  middle: { flexGrow: 1, justifyContent: 'center', paddingTop: 16, paddingBottom: 8 },
  card: { width: '100%', maxWidth: 440, alignSelf: 'center' },
  icon: { marginBottom: 12 },
  gap: { marginTop: 18 },
  button: { marginTop: 2 },
  forgot: { marginTop: 18 },
  forgotLabel: { marginTop: 14 },
  forgotWords: { marginTop: 8 },
  refuse: { flexDirection: 'row', alignItems: 'flex-start', gap: 12, marginTop: 16 },
  refuseWords: { flex: 1 },
});
