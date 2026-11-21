// A, Add a phone, as design/screens/console.js draws it (A1 to A9): one step
// a screen, as in the first start, with the cable's state at the foot of
// every step, beside the main button at the console's width and above it on
// a phone.
//
// The console's window answers these screens itself (console/README.md,
// "Adding a phone"): `phone.look` is asked once a second while the flow is
// open, and at no other time; it never asks the phone anything. The phone is
// asked to allow this PC (`phone.ask`) only when the guardian taps Ask the
// phone on step 3, with the phone unlocked: a Samsung phone shows no Allow on
// a locked screen, and none later. `phone.install`, `phone.pair` and
// `phone.debugging_off` each start a step and answer at once; `phone.done`
// when the flow closes, whichever way. Every line the console sends is shown
// as given. The phone's own six steps (A6) are the service's, from `device`.
//
// A step that waits for the phone keeps its button switched off and moves on
// by itself once the look says so, when it was come to going forward; come
// back to with Back, its button is on once the look says it may go on.

import { type ReactNode, useCallback, useEffect, useRef, useState } from 'react';
import { AppState, StyleSheet, View } from 'react-native';

import { ask, type Asked, type ChildToday, type GuardianDoor, type PhoneLook, type Standing } from '../guardianSeam';
import type { IconName } from '../icons/Icon';
import { borderWidth, color } from '../theme';
import { Dashed } from '../ui/paper/parts';
import { Txt } from '../ui/paper/text';
import { Six } from './Devices';
import { Column, Page } from './Page';
import { Btn, Card, CardHead, Chip, Field, Label, Mark, PinBtn, Stamp, StepHead, Tile, TopRow, useSizes } from './parts';
import { gtype } from './text';
import * as W from './words';

/** How often the screens ask the console where adding the phone stands, in milliseconds. */
export const LOOK_EVERY_MS = 1_000;
/** After this long asking with no Allow, step 3 says what to do when the phone shows nothing. */
export const HELP_AFTER_SECS = 20;

const A = W.addPhone;
const LAST = A.steps - 1;

/** The cable's state at the foot of a step: its tile, the console's line and what to do. Teal when connected, coral when it is a problem. */
function Foot({ look, problem }: { look: PhoneLook | null; problem: string }) {
  const { wide } = useSizes();
  const c = look?.cable;
  const tone = problem || c?.case === 'driver' || c?.case === 'trouble' ? color.coral : c?.case === 'ok' ? color.teal : color.ink;
  const line = problem || c?.line || '';
  const more = problem ? '' : (c?.more ?? '');
  return (
    <View style={[styles.foot, wide && styles.footWide]} testID="cable" accessibilityLiveRegion="polite" accessible accessibilityLabel={more ? `${line} ${more}` : line}>
      <Tile icon="usb" />
      <View style={styles.flex}>
        <Txt style={[gtype.footLine, { color: tone }]} testID="cable-line">
          {line}
        </Txt>
        {more ? (
          <Txt style={gtype.footMore} wrap="pretty" testID="cable-more">
            {more}
          </Txt>
        ) : null}
      </View>
    </View>
  );
}

/** Ticks: teal when done, a hollow ring when not. */
function Ticks({ rows, testID }: { rows: readonly { ok: boolean; words: string }[]; testID?: string }) {
  return (
    <View style={styles.oks} testID={testID}>
      {rows.map((r, i) => (
        <View key={`${r.words} ${i}`}>
          {i > 0 ? <Dashed /> : null}
          <View style={[styles.ok, i > 0 && styles.okAfter]} accessible accessibilityLabel={r.ok ? r.words : `${r.words}: ${W.devices.notYet}`}>
            {r.ok ? Mark.tick() : Mark.ring(color.coral)}
            <Txt style={[gtype.ok, styles.flex, !r.ok && styles.coral]}>{r.words}</Txt>
          </View>
        </View>
      ))}
    </View>
  );
}

/** What went wrong, in the console's words, and Try again. */
function Trouble({ line, advice, onAgain, testID }: { line: string | null; advice: string | null; onAgain?: () => void; testID: string }) {
  return (
    <View style={styles.trouble} testID={testID}>
      {line ? <Txt style={gtype.err}>{line}</Txt> : null}
      {advice ? (
        <Txt style={[gtype.small, styles.advice]} wrap="pretty">
          {advice}
        </Txt>
      ) : null}
      {onAgain ? <Btn label={A.tryAgain} onPress={onAgain} outer={styles.again} testID="try-again" /> : null}
    </View>
  );
}

export function AddPhone({
  door,
  initialStep = 0,
  initialChild,
  onLeave,
  onAdded,
  onSignedOut,
  onRefused,
}: {
  door: GuardianDoor;
  /** 0 to 8: A1 to A9. */
  initialStep?: number;
  initialChild?: string;
  /** Back from the first step. */
  onLeave: () => void;
  /** Done on the last step: Devices, with the new phone. */
  onAdded: () => void;
  onSignedOut: () => void;
  onRefused: () => void;
}) {
  const [step, setStep] = useState(initialStep);
  const [children, setChildren] = useState<readonly ChildToday[] | null>(null);
  const [childId, setChildId] = useState<string | null>(initialChild ?? null);
  const [look, setLook] = useState<PhoneLook | null>(null);
  const [lookProblem, setLookProblem] = useState('');
  const [standing, setStanding] = useState<Standing | null>(null);
  const [name, setName] = useState<string | null>(null);
  const [err, setErr] = useState('');
  const [busy, setBusy] = useState(false);
  const forward = useRef(true);
  const stepRef = useRef(step);
  stepRef.current = step;
  const pairedRef = useRef<string | null>(null);
  pairedRef.current = look?.pairing.device?.deviceId ?? null;
  const askedInstall = useRef(false);
  const askedPair = useRef(false);
  const live = useRef(true);

  const said = useCallback(
    (o: Exclude<Asked<unknown>, { kind: 'done' }>): string => {
      if (o.kind === 'not-done' && o.reason === 'signed_out') onSignedOut();
      if (o.kind === 'refused') onRefused();
      return o.words;
    },
    [onSignedOut, onRefused],
  );

  // The children, and whose phone it is.
  useEffect(() => {
    void ask(door, 'family_today', {}).then((got) => {
      if (!live.current) return;
      if (got.kind !== 'done') return setErr(said(got));
      setChildren(got.value);
      setChildId((id) => id ?? got.value[0]?.childId ?? null);
    });
  }, [door, said]);

  // The look, once a second while the flow is open and in front; the phone's
  // own steps on A6 with it. Closing the flow, by any way, lets go of the phone.
  useEffect(() => {
    live.current = true;
    let asking = false;
    const tick = async () => {
      if (asking) return;
      asking = true;
      const got = await ask(door, 'phone.look', {});
      if (live.current) {
        if (got.kind === 'done') {
          setLook(got.value);
          setLookProblem('');
        } else setLookProblem(said(got));
        const device = pairedRef.current ?? (got.kind === 'done' ? (got.value.pairing.device?.deviceId ?? null) : null);
        // The phone's own steps, on A6 and for A9's ticks.
        if ((stepRef.current === 5 || stepRef.current === LAST) && device) {
          const d = await ask(door, 'device', { device });
          if (live.current && d.kind === 'done') setStanding(d.value.standing);
        }
      }
      asking = false;
    };
    void tick();
    let timer: ReturnType<typeof setInterval> | null = setInterval(() => void tick(), LOOK_EVERY_MS);
    const sub = AppState.addEventListener('change', (state) => {
      if (state === 'active') {
        if (!timer) {
          void tick();
          timer = setInterval(() => void tick(), LOOK_EVERY_MS);
        }
      } else if (timer) {
        clearInterval(timer);
        timer = null;
      }
    });
    return () => {
      live.current = false;
      if (timer) clearInterval(timer);
      sub.remove();
      void ask(door, 'phone.done', {});
    };
  }, [door, said]);

  const go = (to: number) => {
    forward.current = to > stepRef.current;
    setErr('');
    setStep(to);
  };

  const start = useCallback(
    async (name: 'phone.ask' | 'phone.install' | 'phone.debugging_off') => {
      setBusy(true);
      const got = await ask(door, name, {});
      if (!live.current) return;
      setBusy(false);
      if (got.kind !== 'done') return setErr(said(got));
      setErr('');
      if (name === 'phone.debugging_off' && !got.value.started) go(LAST);
    },
    [door, said],
  );
  const pair = useCallback(async () => {
    if (!childId) return;
    const got = await ask(door, 'phone.pair', { child: childId });
    if (!live.current) return;
    if (got.kind !== 'done') return setErr(said(got));
    setErr('');
  }, [door, said, childId]);

  // What each waiting step waits for.
  const c = look?.cable.case;
  const installed = look?.install.stage === 'done' || look?.install.stage === 'not_needed';
  const paired = look?.pairing.stage === 'paired';
  const setUp = standing?.setupFinished === true;
  const home = look?.pairing.ways.includes('home_network') ?? false;
  const debuggingOff = look?.debugging.stage === 'off';
  const canGoOn = [true, c === 'unknown' || c === 'allow' || c === 'ok', c === 'ok', installed, paired, setUp, true, true, true][step];
  // Step 3: the phone does not know this PC and was not asked (the button asks it), or it was asked and the window waits.
  const asking = step === 2 && c === 'allow';
  // Asked and not answered in time, the strip says so as trouble, and the guardian may ask again.
  const canAsk = step === 2 && (c === 'unknown' || c === 'trouble');

  // Moving on by what the look says, when the step was come to going forward;
  // and starting the install and the pairing once each.
  useEffect(() => {
    if (!look) return;
    if (step === 3 && look.phone?.limo.looked && look.phone.recentEnough && look.install.stage === 'not_asked' && !askedInstall.current) {
      askedInstall.current = true;
      void start('phone.install');
    }
    if (step === 4 && look.pairing.stage === 'not_asked' && !askedPair.current && childId) {
      askedPair.current = true;
      void pair();
    }
    if (step === 4 && look.pairing.device && name === null) setName(look.pairing.device.name);
    if (!forward.current) return;
    // On to step 3 when the phone is there and does not know this PC; past it when it does.
    if (step === 1 && (c === 'unknown' || c === 'allow')) go(2);
    else if ((step === 1 || step === 2) && c === 'ok') go(3);
    else if (step === 3 && installed) go(4);
    else if (step === 5 && setUp) go(6);
    else if (step === 7 && debuggingOff) go(LAST);
  }, [look, step, standing]);

  const child = children?.find((k) => k.childId === childId) ?? null;
  const who = child?.name ?? '';

  const next = async () => {
    if (step === 0) {
      // A phone may be connected already: on to the first step it needs.
      return go(c === 'ok' ? 3 : c === 'unknown' || c === 'allow' ? 2 : 1);
    }
    if (step === 4 && look?.pairing.device && name !== null && name.trim() !== '' && name.trim() !== look.pairing.device.name) {
      setBusy(true);
      const got = await ask(door, 'rename_device', { device: look.pairing.device.deviceId, name: name.trim() });
      if (!live.current) return;
      setBusy(false);
      if (got.kind !== 'done') return setErr(said(got));
    }
    // The phone is asked only by this button, never by itself.
    if (step === 2 && canAsk) return void start('phone.ask');
    if (step === 7) return void start('phone.debugging_off');
    if (step === LAST) return onAdded();
    go(step + 1);
  };

  const heads: [IconName, string][] = [
    ['phone', A.a1.heading],
    ['usb', A.a2.heading],
    ['key', A.a3.heading],
    ['install', A.a4.heading(who)],
    ['link', paired ? A.a5.heading(who) : A.a5.pairing(who)],
    ['phone', A.a6.heading],
    ['wifi', A.a7.heading],
    ['usb', A.a8.heading],
    ['check', A.a9.heading(who)],
  ];

  let body: ReactNode = null;
  switch (step) {
    case 0:
      body = (
        <>
          {children && children.length > 1 ? (
            <>
              <Label style={styles.gap}>{A.a1.whose}</Label>
              <View style={styles.chips} accessibilityRole="radiogroup" accessibilityLabel={A.a1.whose}>
                {children.map((k) => (
                  <Chip key={k.childId} label={k.name} on={k.childId === childId} onPress={() => setChildId(k.childId)} testID={`whose-${k.childId}`} />
                ))}
              </View>
            </>
          ) : null}
          <Label style={styles.gap}>{A.a1.youNeed}</Label>
          <View style={styles.needs}>
            {(
              [
                ['phone', A.phoneOf(who), null],
                ['usb', A.a1.cable, A.a1.cableText],
                ['clock', A.a1.time, null],
              ] as const
            ).map(([icon, title, text], i) => (
              <View key={title}>
                {i > 0 ? <Dashed /> : null}
                <View style={styles.need}>
                  <Tile icon={icon} />
                  <View style={styles.flex}>
                    <Txt style={gtype.rowTitle}>{title}</Txt>
                    {text ? (
                      <Txt style={[gtype.rowText, styles.needText]} wrap="pretty">
                        {text}
                      </Txt>
                    ) : null}
                  </View>
                </View>
              </View>
            ))}
          </View>
        </>
      );
      break;
    case 1:
      body = (
        <>
          <Txt style={[gtype.detail, styles.detail]} wrap="pretty">
            {A.a2.line}
          </Txt>
          <Steps items={A.a2.steps} />
          <View style={styles.note}>
            <Label>{A.a2.noteLabel}</Label>
            <Txt style={[gtype.note, styles.noteText]} wrap="pretty">
              {A.a2.note}
            </Txt>
          </View>
        </>
      );
      break;
    case 2:
      body = (
        <>
          {asking ? (
            <Txt style={[gtype.detail, styles.detail]} wrap="pretty">
              {A.a3.line}
            </Txt>
          ) : (
            <>
              <Txt style={[gtype.detail, styles.detail]} wrap="pretty" testID="unlock">
                {A.a3.unlock}
              </Txt>
              <Txt style={[gtype.small, styles.small]} wrap="pretty">
                {A.a3.once}
              </Txt>
            </>
          )}
          <Label style={styles.gap}>{A.a3.key}</Label>
          <View style={styles.finger} accessible accessibilityLabel={look?.fingerprint ?? ''} testID="fingerprint">
            <Txt style={gtype.finger}>{fingerprintLines(look?.fingerprint ?? null)}</Txt>
          </View>
          <Txt style={[gtype.small, styles.small]} wrap="pretty">
            {A.a3.same}
          </Txt>
          {asking && (look?.askedSecs ?? 0) >= HELP_AFTER_SECS ? (
            <View style={styles.note} testID="nothing-on-the-phone">
              <Label>{A.a3.helpLabel}</Label>
              <Txt style={[gtype.note, styles.noteText]} wrap="pretty">
                {A.a3.help}
              </Txt>
            </View>
          ) : null}
        </>
      );
      break;
    case 3: {
      const i = look?.install;
      const failed = i && (i.stage === 'failed' || i.stage === 'refused' || i.stage === 'no_app');
      body = (
        <>
          <Ticks rows={look?.checks ?? []} testID="checks" />
          <View style={styles.progress}>
            <Dashed />
            <Label style={styles.progressLabel}>{A.a4.putting(who)}</Label>
            {failed ? (
              <Trouble
                line={i.line}
                advice={i.advice}
                onAgain={() => {
                  askedInstall.current = true;
                  void start('phone.install');
                }}
                testID="install-trouble"
              />
            ) : i?.stage === 'installing' && i.line ? (
              // Most of the wait is Android installing, which says nothing until it is done: no figure.
              <Txt style={[gtype.status, styles.installing]} testID="installing">
                {i.line}
              </Txt>
            ) : (
              <View style={styles.bar} accessibilityRole="progressbar" accessibilityValue={{ min: 0, max: 100, now: Math.round((i?.sent ?? 0) * 100) }} testID="sent">
                <View style={[styles.fill, { width: `${Math.round((i?.sent ?? 0) * 100)}%` }, !(i?.sent ?? 0) && styles.fillNone]} />
              </View>
            )}
          </View>
          <Txt style={[gtype.small, styles.small]}>{A.a4.asksNothing}</Txt>
        </>
      );
      break;
    }
    case 4: {
      const p = look?.pairing;
      body = (
        <>
          {p?.stage === 'failed' ? (
            <Trouble
              line={p.line}
              advice={p.advice}
              onAgain={() => {
                askedPair.current = true;
                void pair();
              }}
              testID="pair-trouble"
            />
          ) : (
            <View style={styles.status}>
              {paired ? <Stamp tone="teal">{A.a5.paired}</Stamp> : null}
              {p?.line ? (
                <Txt style={[gtype.status, styles.flexText]} testID="pairing-line">
                  {p.line}
                </Txt>
              ) : null}
            </View>
          )}
          {paired ? (
            <>
              <Txt style={[gtype.detail, styles.detail]} wrap="pretty">
                {A.a5.opened(who)}
              </Txt>
              <Label style={styles.gap}>{A.a5.itsName}</Label>
              <Field testID="phone-name" value={name ?? ''} onChangeText={setName} maxLength={40} accessibilityLabel={A.a5.itsName} />
            </>
          ) : null}
        </>
      );
      break;
    }
    case 5:
      body = (
        <>
          <Txt style={[gtype.detail, styles.detail]} wrap="pretty">
            {A.a6.line}
          </Txt>
          {standing ? <Six steps={standing.steps} top={14} /> : null}
          <Txt style={[gtype.small, styles.small]} wrap="pretty">
            {A.a6.byItself}
          </Txt>
        </>
      );
      break;
    case 6:
      body = (
        <>
          <Txt style={[gtype.detail, styles.detail]} wrap="pretty">
            {A.a7.line(who)}
          </Txt>
          <View style={styles.status}>
            {home ? (
              <Stamp tone="teal">{A.a7.reached}</Stamp>
            ) : (
              <Txt style={gtype.status} testID="not-home">
                {A.a7.notYet}
              </Txt>
            )}
          </View>
        </>
      );
      break;
    case 7: {
      const d = look?.debugging;
      body = (
        <>
          <Txt style={[gtype.detail, styles.detail]} wrap="pretty">
            {A.a8.line(who)}
          </Txt>
          <Btn label={A.a8.leave} onPress={() => go(LAST)} outer={styles.gapBtn} testID="leave-it-on" />
          <Txt style={[gtype.small, styles.small]}>{A.a8.then}</Txt>
          {d?.stage === 'still_on' ? <Trouble line={d.line} advice={d.advice} testID="debugging-trouble" /> : null}
          {d?.stage === 'switching_off' && d.line ? <Txt style={[gtype.status, styles.installing]}>{d.line}</Txt> : null}
          <View style={styles.note}>
            <Label>{A.a8.byHandLabel}</Label>
            <Txt style={[gtype.note, styles.noteText]} wrap="pretty">
              {A.a8.byHand}
            </Txt>
          </View>
        </>
      );
      break;
    }
    default: {
      const t = A.a9.ticks;
      body = (
        <>
          <Ticks
            testID="added"
            rows={[
              { ok: installed, words: t.installed },
              { ok: paired, words: t.paired },
              { ok: setUp, words: setUp ? t.setUp : t.setUpNot },
              { ok: home, words: home ? t.home : t.homeNot },
              { ok: debuggingOff, words: debuggingOff ? t.off : t.on },
            ]}
          />
          <Txt style={[gtype.detail, styles.detail]}>{A.a9.unplug}</Txt>
        </>
      );
    }
  }

  const button = [A.start, A.next, canAsk ? A.a3.ask : asking ? A.a3.waiting : A.next, A.next, A.next, A.next, A.next, A.a8.button, A.done][step];
  const off =
    busy ||
    (step === 0 && (!children || !childId)) ||
    ([1, 3, 5].includes(step) && !canGoOn) ||
    (step === 2 && !canAsk && !canGoOn) ||
    (step === 4 && !paired) ||
    (step === 7 && (look?.debugging.stage === 'switching_off' || busy));
  const strip = {
    node: (
      <>
        <Foot look={look} problem={lookProblem} />
        <PinBtn label={button} look="go" off={off} onPress={() => void next()} testID="go" />
      </>
    ),
    height: { phone: 132, wide: 88 },
  };

  return (
    <Page testID={`screen-add-phone-${step + 1}`} strip={strip}>
      <TopRow />
      <Column>
        <StepHead at={step} total={A.steps} backOnFirst onBack={() => (step === 0 ? onLeave() : go(step - 1))} />
        <Card>
          <CardHead icon={heads[step][0]} title={heads[step][1]} />
          {body}
          {err ? (
            <Txt style={[gtype.err, styles.err]} testID="add-phone-message">
              {err}
            </Txt>
          ) : null}
        </Card>
      </Column>
    </Page>
  );
}

/** The key's fingerprint as the phone shows it: sixteen pairs, eight a line. */
function fingerprintLines(f: string | null): string {
  if (!f) return '';
  const pairs = f.split(':');
  if (pairs.length <= 8) return f;
  return `${pairs.slice(0, 8).join(':')}:\n${pairs.slice(8).join(':')}`;
}

/** Steps on the phone, numbered in mono circles. */
function Steps({ items }: { items: readonly (readonly [string, string])[] }) {
  const { wide } = useSizes();
  return (
    <View style={styles.ol}>
      {items.map(([text, sub], i) => (
        <View key={text}>
          {i > 0 ? <Dashed /> : null}
          <View style={[styles.li, wide && styles.liWide]} accessible accessibilityLabel={`${i + 1}. ${text} ${sub}`}>
            <View style={[styles.number, wide && styles.numberWide]}>
              <Txt style={gtype.olNumber}>{String(i + 1)}</Txt>
            </View>
            <Txt style={gtype.olItem}>{text}</Txt>
            <Txt style={[gtype.olSub, styles.sub]} wrap="pretty">
              {sub}
            </Txt>
          </View>
        </View>
      ))}
    </View>
  );
}

const styles = StyleSheet.create({
  flex: { flex: 1, minWidth: 0 },
  flexText: { flexShrink: 1 },
  coral: { color: color.coral },
  gap: { marginTop: 18 },
  gapBtn: { marginTop: 14 },
  detail: { marginTop: 10 },
  small: { marginTop: 12, marginHorizontal: 2 },
  err: { marginTop: 12, marginHorizontal: 2 },
  chips: { flexDirection: 'row', flexWrap: 'wrap', gap: 8, marginTop: 8 },
  needs: { marginTop: 6 },
  need: { flexDirection: 'row', alignItems: 'center', gap: 12, paddingVertical: 8 },
  needText: { marginTop: 2 },
  ol: { marginTop: 10 },
  li: { paddingTop: 9, paddingBottom: 9, paddingLeft: 40 },
  liWide: { paddingTop: 7, paddingBottom: 7 },
  number: { position: 'absolute', left: 0, top: 8, width: 28, height: 28, borderRadius: 14, borderWidth, borderColor: color.ink, alignItems: 'center', justifyContent: 'center' },
  numberWide: { top: 6 },
  sub: { marginTop: 2 },
  note: { marginTop: 12, paddingTop: 10, paddingBottom: 11, paddingHorizontal: 12, borderWidth, borderStyle: 'dashed', borderColor: 'rgba(27,38,56,0.42)', borderRadius: 10 },
  noteText: { marginTop: 6 },
  finger: { marginTop: 8, paddingVertical: 12, paddingHorizontal: 14, borderWidth, borderColor: color.ink, borderRadius: 10, backgroundColor: color.paper },
  oks: { marginTop: 14 },
  ok: { flexDirection: 'row', alignItems: 'center', gap: 10, minHeight: 40 },
  okAfter: { minHeight: 38.5 },
  progress: { marginTop: 16 },
  progressLabel: { marginTop: 14 },
  bar: { height: 12, marginTop: 10, borderWidth, borderColor: color.ink, borderRadius: 999, overflow: 'hidden', backgroundColor: color.paper },
  fill: { height: '100%', backgroundColor: color.teal, borderRightWidth: borderWidth, borderRightColor: color.ink },
  fillNone: { borderRightWidth: 0 },
  installing: { marginTop: 10 },
  trouble: { marginTop: 10 },
  advice: { marginTop: 4, marginHorizontal: 0 },
  again: { marginTop: 10 },
  status: { flexDirection: 'row', flexWrap: 'wrap', alignItems: 'center', gap: 10, marginTop: 16 },
  foot: { flexDirection: 'row', alignItems: 'center', gap: 12, minWidth: 0 },
  footWide: { flexGrow: 1, flexShrink: 1 },
});
