// T, Today, the guardian's home screen, as design/screens/console.js draws
// it (T1 to T3): what is happening now, and the answers a guardian gives
// most, each one tap. At the console's width two columns, the children at
// the left and what waits for a guardian at the right; at a phone's width
// one column, what waits first. Marigold is time, teal is in use or done,
// coral is locked or needs a guardian.
//
// Everything on it comes from the door as numbers (family_today,
// waiting_requests) and is written by src/guardian/time.ts in the engine's
// words. It asks again every few seconds while it is open and the window is
// in front, and stops when it is not. A tap asks the door, switches its
// button off until the answer is in, and asks for the day again, so its
// result shows: the time left grows, a lock is stamped, a request goes.

import { useCallback, useEffect, useRef, useState } from 'react';
import { AppState, StyleSheet, View } from 'react-native';

import { ask, type Asked, type ChildToday, type DeviceToday, type GuardianDoor, type WaitingRequest } from '../guardianSeam';
import { Icon } from '../icons/Icon';
import { color } from '../theme';
import { Dashed } from '../ui/paper/parts';
import { Txt } from '../ui/paper/text';
import { useClock } from './clock';
import { GuardianTabs, type TabKey } from './GuardianTabs';
import { Page } from './Page';
import { Btn, Card, Label, Mark, Stamp, Tile, TopRow, useScreenWidth, useSizes } from './parts';
import { gtype } from './text';
import { dateLine, heardAgo, leftWords, minutesInSentence, timeAt, timeOfDay, usedLine, usedSpan } from './time';
import * as W from './words';

/** How often Today asks again while it is open, in milliseconds. */
export const ASK_AGAIN_MS = 5_000;
/** The right column's width at the console's width. */
const ASIDE = 340;
/** From this width up, a child's card puts the time beside the devices. */
const KID_WIDE_FROM = 540;
/** A device heard within this long is not said to be "last heard". */
const JUST_HEARD_SECS = 60;

type Data = { today: readonly ChildToday[]; requests: readonly WaitingRequest[] };

/** A device's state, under its name. */
function deviceState(d: DeviceToday, now: number, clock: ReturnType<typeof useClock>['clock']): string {
  if (d.inUse) return W.today.inUse;
  if (d.kind === 'on_master') return W.today.notInUse;
  if (d.lastHeardAt === null) return W.today.notHeard;
  if (now - d.lastHeardAt < JUST_HEARD_SECS) return W.today.notInUse;
  return W.today.lastHeard(heardAgo(now, d.lastHeardAt, clock));
}

/** What a device needs a guardian for; nothing when the door says nothing of it. */
const needsOf = (d: DeviceToday) => d.needs ?? [];

/** The mark under a device that needs a guardian: the first thing switched off, and how many more. */
function needWords(d: DeviceToday): string[] {
  if (!needsOf(d).length) return [];
  const first = W.NEED_MARK[needsOf(d)[0].permission];
  return needsOf(d).length > 1 ? [first, W.today.more(needsOf(d).length - 1)] : [first];
}

/** A device that needs a guardian, beside what waits: the engine's sentence for the first thing switched off. */
function Need({ child, d, onSee }: { child: ChildToday; d: DeviceToday; onSee: () => void }) {
  return (
    <Card testID={`needs-${d.deviceId}`}>
      <View style={styles.needTop}>
        <Tile icon="alert" large />
        <Stamp tone="coral">{W.today.needsGuardian}</Stamp>
      </View>
      <Txt style={[gtype.rowTitle, styles.needName]}>{d.kind === 'on_master' ? W.today.thisPc : d.name}</Txt>
      <Txt style={[gtype.needWords, styles.needWords]} wrap="pretty">
        {needsOf(d)[0].words}
      </Txt>
      {needsOf(d).length > 1 ? <Label style={styles.needMore}>{W.today.more(needsOf(d).length - 1)}</Label> : null}
      <Btn label={W.today.seeWhatToDo} onPress={onSee} outer={styles.needButton} testID={`see-${child.childId}-${d.deviceId}`} />
    </Card>
  );
}

/** "Open until" offers: half an hour, an hour and two hours on, each up to the next quarter hour. */
function openChoices(now: number): number[] {
  return [30, 60, 120].map((m) => Math.ceil((now + m * 60) / 900) * 900);
}

function Kid({
  k,
  cardWidth,
  afterFirstStart,
  busy,
  err,
  choosing,
  onAct,
}: {
  k: ChildToday;
  cardWidth: number;
  afterFirstStart: boolean;
  busy: string | null;
  err: string;
  choosing: boolean;
  onAct: (act: 'plus15' | 'plus30' | 'lock' | 'choose' | 'lift' | 'phone' | number) => void;
}) {
  const { now, clock } = useClock();
  const { tap } = useSizes();
  const t = now();
  const kidWide = cardWidth - 3 >= KID_WIDE_FROM;
  const unlockedUntil = k.guardian.mode === 'unlock' ? k.guardian.until : null;
  const left = leftWords(k.leftSecs, unlockedUntil, t, clock);
  const used = usedLine(k.usedSecs, k.budgetSecs);
  const locked =
    (k.guardian.mode === 'lock' && (k.guardian.until === null || k.guardian.until > t)) ||
    (left.kind === 'left' && k.leftSecs === 0);
  const inForce = k.guardian.mode !== 'nothing';
  // Right after the first start, a child with no phone yet is offered the way to add one.
  const offerPhone = afterFirstStart && !k.devices.some((d) => d.kind === 'portable');
  // The quick actions: three a row on a phone (Open until across two), all in one row at the console's width.
  const inner = cardWidth - 3 - 32;
  const col = (inner - 16) / 3;
  const actOff = (key: string) => busy !== null && busy !== key;
  const act = (key: 'plus15' | 'plus30' | 'lock' | 'choose' | 'lift', label: string, look: 'plain' | 'coral' = 'plain', span = 1, off = false) => (
    <Btn
      key={key}
      testID={`${key}-${k.childId}`}
      label={label}
      look={look}
      mono
      padding={kidWide ? 14 : 8}
      off={off || actOff(key) || busy === key}
      onPress={() => onAct(key)}
      outer={kidWide ? undefined : { width: span === 2 ? 2 * col + 8 : col }}
    >
      {key === 'choose' ? Mark.down(color.ink) : null}
    </Btn>
  );

  const time = (
    <View style={[styles.time, kidWide && styles.timeWide]}>
      <Txt style={gtype.big} numberOfLines={1} testID={`left-${k.childId}`}>
        {left.words}
      </Txt>
      {left.kind === 'left' ? <Label style={styles.leftLabel}>{W.today.leftToday}</Label> : null}
      {used.part !== null ? (
        <View style={styles.bar} importantForAccessibility="no-hide-descendants">
          <View style={[styles.barFill, { width: `${Math.round(used.part * 1000) / 10}%` }, used.part > 0 && used.part < 1 && styles.barPart]} />
        </View>
      ) : null}
      <Txt style={[gtype.used, styles.used]}>{used.words}</Txt>
    </View>
  );

  const devices = (
    <View style={[styles.devs, kidWide && styles.devsWide]}>
      {kidWide ? null : <Dashed />}
      {k.devices.map((d, i) => (
        <View key={d.deviceId}>
          {i > 0 ? <Dashed /> : null}
          <View
            style={styles.dev}
            accessible
            accessibilityLabel={[d.kind === 'on_master' ? W.today.thisPc : d.name, deviceState(d, t, clock), ...needWords(d), usedSpan(d.usedSecs)].join(', ')}
          >
            <Tile icon={d.kind === 'on_master' ? 'pc' : 'phone'} />
            <View style={styles.devName}>
              <Txt style={gtype.rowTitle}>{d.kind === 'on_master' ? W.today.thisPc : d.name}</Txt>
              <Txt style={[gtype.state, styles.devState, d.inUse && styles.live]}>{deviceState(d, t, clock)}</Txt>
              {needsOf(d).length ? (
                <View style={styles.need} testID={`need-${d.deviceId}`}>
                  <Icon name="alert" size={20} />
                  <Txt style={[gtype.need, styles.flex]}>{needWords(d).join(' · ')}</Txt>
                </View>
              ) : null}
            </View>
            <Txt style={[gtype.minutes, styles.devMin]}>{usedSpan(d.usedSecs)}</Txt>
          </View>
        </View>
      ))}
      {!offerPhone ? null : (
        <View style={styles.missing} testID={`no-phone-${k.childId}`}>
          <Tile icon="phone" />
          <Txt style={[gtype.missing, styles.missingWords]} wrap="pretty">
            {W.today.notAdded(k.name)}
          </Txt>
          <Btn label={W.today.addPhone(k.name)} onPress={() => onAct('phone')} outer={kidWide ? undefined : styles.missingButton} testID={`add-phone-${k.childId}`} />
        </View>
      )}
    </View>
  );

  return (
    <Card bare testID={`kid-${k.childId}`}>
      <View style={kidWide ? styles.kidInWide : styles.kidIn}>
        <View style={styles.kidHead}>
          <Txt style={gtype.kidName} accessibilityRole="header">
            {k.name}
          </Txt>
          {locked ? <Stamp tone="coral">{W.today.locked}</Stamp> : null}
        </View>
        <Dashed />
        {kidWide ? (
          <View style={styles.kidBody}>
            {time}
            {devices}
          </View>
        ) : (
          <>
            {time}
            {devices}
          </>
        )}
        <View style={[styles.acts, kidWide && styles.actsWide]}>
          <Dashed />
          <View style={[styles.actsIn, kidWide ? styles.actsRow : styles.actsGrid, { minHeight: tap }]}>
            {act('plus15', W.today.plus(15))}
            {act('plus30', W.today.plus(30))}
            {act('lock', W.today.lockNow, 'coral')}
            {act('choose', W.today.openUntil, 'plain', 2)}
            {act('lift', W.today.lift, 'plain', 1, !inForce)}
          </View>
          {choosing ? (
            <View style={styles.choices} testID={`open-choices-${k.childId}`}>
              {openChoices(t).map((until) => (
                <Btn key={until} label={timeAt(until, 'until', clock)} mono padding={14} onPress={() => onAct(until)} testID={`open-${until}`} />
              ))}
            </View>
          ) : null}
          {err ? (
            <Txt style={[gtype.err, styles.kidErr]} testID={`err-${k.childId}`}>
              {err}
            </Txt>
          ) : null}
        </View>
      </View>
    </Card>
  );
}

function Request({ r, today, busy, onAnswer }: { r: WaitingRequest; today: readonly ChildToday[]; busy: string | null; onAnswer: (minutes: number | null) => void }) {
  const { clock } = useClock();
  const device = today.flatMap((c) => c.devices).find((d) => d.deviceId === r.deviceId);
  const where = device?.kind === 'on_master' ? W.today.thisPcInSentence : r.deviceName;
  const mine = busy === r.requestId;
  return (
    <Card testID={`request-${r.requestId}`}>
      <View style={styles.reqTop}>
        <Tile icon="hand" large />
        <View style={styles.flex}>
          <Txt style={gtype.ask}>{W.today.asks(r.childName, minutesInSentence(r.minutes))}</Txt>
          <Txt style={[gtype.askWhere, styles.askWhere]}>{W.today.onDevice(where, timeOfDay(r.askedAt, clock))}</Txt>
        </View>
      </View>
      {r.reason ? (
        <View style={styles.quote}>
          <Txt style={gtype.quote} wrap="pretty">
            {W.today.quote(r.reason)}
          </Txt>
        </View>
      ) : null}
      <View style={styles.answers}>
        <Btn label={W.today.give(minutesInSentence(r.minutes))} look="go" answer padding={8} off={mine} onPress={() => onAnswer(r.minutes)} outer={styles.flex} testID={`give-${r.requestId}`} />
        {r.minutes > 15 ? <Btn label={W.today.give(minutesInSentence(15))} answer padding={8} off={mine} onPress={() => onAnswer(15)} outer={styles.flex} testID={`give15-${r.requestId}`} /> : null}
        <Btn label={W.today.no} answer padding={8} off={mine} onPress={() => onAnswer(null)} outer={styles.no} testID={`no-${r.requestId}`} />
      </View>
    </Card>
  );
}

let taps = 0;
/** The screen's own id for a tap that gives time: the door gives it once, however often it arrives. */
function tapId(): string {
  taps += 1;
  return `console-${Date.now().toString(36)}-${taps}-${Math.random().toString(36).slice(2, 8)}`;
}

export function Today({
  door,
  fromFirstStart = false,
  onSignedOut,
  onRefused,
  onTab,
  onAddPhone,
}: {
  door: GuardianDoor;
  /** Opened straight from the first start: what waits is explained once. */
  fromFirstStart?: boolean;
  onSignedOut: () => void;
  onRefused: () => void;
  onTab: (tab: TabKey) => void;
  onAddPhone: () => void;
}) {
  const { now, clock } = useClock();
  const { wide } = useSizes();
  const width = useScreenWidth();
  const [data, setData] = useState<Data | null>(null);
  const [problem, setProblem] = useState('');
  const [busy, setBusy] = useState<string | null>(null);
  const [errs, setErrs] = useState<Record<string, string>>({});
  const [choosing, setChoosing] = useState<string | null>(null);
  const live = useRef(true);

  /** What the door said, where it is said: the sign in for an ended session or a refusal, else on the screen. */
  const said = useCallback(
    (o: Exclude<Asked<unknown>, { kind: 'done' }>): string => {
      if (o.kind === 'not-done' && o.reason === 'signed_out') onSignedOut();
      if (o.kind === 'refused') onRefused();
      return o.words;
    },
    [onSignedOut, onRefused],
  );

  const load = useCallback(async () => {
    const [today, requests] = await Promise.all([ask(door, 'family_today', {}), ask(door, 'waiting_requests', {})]);
    if (!live.current) return;
    if (today.kind !== 'done') return setProblem(said(today));
    if (requests.kind !== 'done') return setProblem(said(requests));
    setProblem('');
    setData({ today: today.value, requests: requests.value });
  }, [door, said]);

  // Asked when it opens, again every few seconds while the window is in front, and never once it is closed.
  useEffect(() => {
    live.current = true;
    void load();
    let timer: ReturnType<typeof setInterval> | null = setInterval(() => void load(), ASK_AGAIN_MS);
    const sub = AppState.addEventListener('change', (state) => {
      if (state === 'active') {
        if (!timer) {
          void load();
          timer = setInterval(() => void load(), ASK_AGAIN_MS);
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
    };
  }, [load]);

  /** One tap: the door asked, its button off meanwhile, then the day asked again. */
  const tap = async (key: string, childId: string, work: () => Promise<Asked<unknown>>) => {
    if (busy) return;
    setBusy(key);
    setErrs((e) => ({ ...e, [childId]: '' }));
    const got = await work();
    if (!live.current) return;
    if (got.kind !== 'done') setErrs((e) => ({ ...e, [childId]: said(got) }));
    await load();
    if (live.current) setBusy(null);
  };

  const onAct = (k: ChildToday) => (act: 'plus15' | 'plus30' | 'lock' | 'choose' | 'lift' | 'phone' | number) => {
    const child = k.childId;
    if (act === 'phone') return onAddPhone();
    if (act === 'choose') return setChoosing(choosing === child ? null : child);
    setChoosing(null);
    if (typeof act === 'number') return void tap('choose', child, () => ask(door, 'unlock_until', { child, until: act }));
    if (act === 'plus15' || act === 'plus30') {
      const minutes = act === 'plus15' ? 15 : 30;
      return void tap(act, child, () => ask(door, 'give_extra', { child, minutes, tap_id: tapId() }));
    }
    if (act === 'lock') return void tap(act, child, () => ask(door, 'lock_now', { child }));
    return void tap(act, child, () => ask(door, 'lift', { child }));
  };

  const answer = (r: WaitingRequest) => (minutes: number | null) =>
    void tap(r.requestId, r.childId, () =>
      ask(door, 'answer_request', { child: r.childId, request: r.requestId, reply: minutes === null ? 'refuse' : { grant: { minutes } } }),
    );

  const kidWidth = wide ? width - 64 - ASIDE - 24 : width - 32;
  const today = data?.today ?? [];
  const requests = data?.requests ?? [];
  // A request and something that needs a guardian both wait for a guardian, so they share one heading.
  const needs = today.flatMap((child) => child.devices.filter((d) => needsOf(d).length).map((d) => ({ child, d })));

  const aside = (
    <View style={[styles.column, wide && styles.aside]}>
      {data === null ? null : requests.length === 0 && needs.length === 0 ? (
        <View style={styles.quiet} testID="nothing-waiting">
          <Txt style={gtype.quietBold}>{W.today.nothingWaiting}</Txt>
          {fromFirstStart && today.length === 1 ? (
            <Txt style={gtype.quiet} wrap="pretty">
              {W.today.whenAsks(today[0].name)}
            </Txt>
          ) : null}
        </View>
      ) : (
        <>
          <Label style={styles.sec} header>
            {W.today.waitingForYou}
          </Label>
          {requests.map((r) => (
            <Request key={r.requestId} r={r} today={today} busy={busy} onAnswer={answer(r)} />
          ))}
          {/* What to do is on the phone's own page in Devices, still being drawn. */}
          {needs.map(({ child, d }) => (
            <Need key={d.deviceId} child={child} d={d} onSee={() => onTab('devices')} />
          ))}
        </>
      )}
    </View>
  );
  const kids = (
    <View style={[styles.column, wide && styles.kids]}>
      {today.map((k) => (
        <Kid
          key={k.childId}
          k={k}
          cardWidth={kidWidth}
          afterFirstStart={fromFirstStart}
          busy={busy}
          err={errs[k.childId] ?? ''}
          choosing={choosing === k.childId}
          onAct={onAct(k)}
        />
      ))}
    </View>
  );

  return (
    <Page testID="screen-today" tabs={<GuardianTabs picked="today" onPick={onTab} />}>
      <TopRow right={<Label>{dateLine(now(), clock)}</Label>} />
      {problem ? (
        <Txt style={[gtype.err, styles.problem]} testID="today-problem" accessibilityLiveRegion="polite">
          {problem}
        </Txt>
      ) : null}
      <View style={[styles.today, wide && styles.todayWide]}>
        {wide ? (
          <>
            {kids}
            {aside}
          </>
        ) : (
          <>
            {aside}
            {kids}
          </>
        )}
      </View>
    </Page>
  );
}

/** Rules, Devices and More: their screens are still being drawn. */
export function Coming({ tab, onTab }: { tab: TabKey; onTab: (tab: TabKey) => void }) {
  return (
    <Page testID={`screen-${tab}`} tabs={<GuardianTabs picked={tab} onPick={onTab} />}>
      <TopRow />
      <View style={[styles.quiet, styles.coming]} testID="coming">
        <Txt style={gtype.quiet} wrap="pretty">
          {W.coming}
        </Txt>
      </View>
    </Page>
  );
}

const styles = StyleSheet.create({
  flex: { flex: 1, minWidth: 0 },
  today: { gap: 14, marginTop: 8 },
  todayWide: { flexDirection: 'row', alignItems: 'flex-start', gap: 24, marginTop: 14 },
  column: { gap: 14, minWidth: 0 },
  kids: { flex: 1 },
  aside: { width: ASIDE },
  sec: { marginTop: 4, marginHorizontal: 2, marginBottom: -4 },
  problem: { marginTop: 8, marginHorizontal: 2 },
  kidIn: { paddingTop: 14, paddingHorizontal: 16, paddingBottom: 16 },
  kidInWide: { paddingTop: 12, paddingHorizontal: 20, paddingBottom: 14 },
  kidHead: { flexDirection: 'row', alignItems: 'center', justifyContent: 'space-between', gap: 10, minHeight: 32, paddingBottom: 8 },
  kidBody: { flexDirection: 'row', gap: 26 },
  time: { paddingTop: 12 },
  timeWide: { width: 210 },
  leftLabel: { marginTop: 8 },
  bar: { height: 10, marginTop: 14, borderWidth: 1.5, borderColor: color.ink, borderRadius: 999, overflow: 'hidden', backgroundColor: color.paper },
  barFill: { height: '100%', backgroundColor: color.marigold },
  barPart: { borderRightWidth: 1.5, borderRightColor: color.ink },
  used: { marginTop: 8 },
  devs: { marginTop: 12 },
  devsWide: { flex: 1, minWidth: 0, marginTop: 4 },
  dev: { flexDirection: 'row', alignItems: 'center', gap: 12, paddingVertical: 9 },
  devName: { flexShrink: 1, minWidth: 0 },
  devState: { marginTop: 4 },
  live: { color: color.teal },
  devMin: { marginLeft: 'auto', paddingLeft: 8 },
  missing: {
    flexDirection: 'row',
    flexWrap: 'wrap',
    alignItems: 'center',
    gap: 12,
    marginTop: 6,
    marginBottom: 2,
    paddingVertical: 10,
    paddingHorizontal: 12,
    borderWidth: 1.5,
    borderStyle: 'dashed',
    borderColor: 'rgba(27,38,56,0.42)',
    borderRadius: 10,
  },
  missingWords: { flexGrow: 1, flexShrink: 1, flexBasis: 160 },
  missingButton: { flexBasis: '100%', flexGrow: 1 },
  acts: { marginTop: 12 },
  actsWide: { marginTop: 8 },
  actsIn: { paddingTop: 12, gap: 8 },
  actsGrid: { flexDirection: 'row', flexWrap: 'wrap' },
  actsRow: { flexDirection: 'row' },
  choices: { flexDirection: 'row', flexWrap: 'wrap', gap: 8, marginTop: 8 },
  kidErr: { marginTop: 8 },
  reqTop: { flexDirection: 'row', alignItems: 'flex-start', gap: 12 },
  askWhere: { marginTop: 6 },
  quote: { marginTop: 12, paddingVertical: 10, paddingHorizontal: 12, borderWidth: 1.5, borderStyle: 'dashed', borderColor: 'rgba(27,38,56,0.42)', borderRadius: 10 },
  answers: { flexDirection: 'row', gap: 8, marginTop: 12 },
  no: { minWidth: 64 },
  need: { flexDirection: 'row', alignItems: 'center', gap: 6, marginTop: 5 },
  needTop: { flexDirection: 'row', alignItems: 'center', justifyContent: 'space-between' },
  needName: { marginTop: 12 },
  needWords: { marginTop: 4 },
  needMore: { marginTop: 8 },
  needButton: { marginTop: 12 },
  quiet: { paddingVertical: 14, paddingHorizontal: 16, borderWidth: 1.5, borderStyle: 'dashed', borderColor: 'rgba(27,38,56,0.42)', borderRadius: 12 },
  coming: { marginTop: 14 },
});
