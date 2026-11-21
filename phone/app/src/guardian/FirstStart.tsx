// F1 to F8, the first start: making the family, one step a screen, as
// design/screens/console.js draws it. Back and a counter above the card (on
// the first step Back's room is kept empty, so the counter never moves), the
// main button pinned to the bottom.
//
// Each step asks the door when its button is pressed, and goes on only when
// the door has done it: the password makes the family (and signs this
// window in), then the PIN, the child, the time a day and bedtime (the
// child's rules, sent whole), then whose each Windows account is, said at
// each tap, and the end of the first start, after which an account left for
// later is locked. A step the door refuses says why under its fields, in the
// door's own words, or the rules' own.
//
// Back keeps what was typed, except a password or a PIN, which is kept on
// the page only until the door has it.

import { type ReactElement, useEffect, useRef, useState } from 'react';
import { Pressable, ScrollView, StyleSheet, View } from 'react-native';

import {
  type AccountRow,
  type AgeBand,
  AGE_BANDS,
  ask,
  type Asked,
  type BlockedWindow,
  type ChildView,
  type DailyBudget,
  type GuardianDoor,
  type RuleSet,
  type Weekday,
  WEEKDAYS,
} from '../guardianSeam';
import { Icon, type IconName } from '../icons/Icon';
import { borderWidth, color } from '../theme';
import { Dashed, Shadow } from '../ui/paper/parts';
import { Txt } from '../ui/paper/text';
import { useClock } from './clock';
import { Column, Page } from './Page';
import { Card, CardHead, Chip, Field, H1, Label, MakerMark, Mark, PickField, PinBtn, Room, Stepper, Tile, TopRow, usePlace, useSizes } from './parts';
import { gtype } from './text';
import { minutesFigure, minuteOfDay, timeADayWords } from './time';
import * as W from './words';

export const STEP_COUNT = 8;

export type Bedtime = 'every' | 'school' | 'none';

/** Everything the first start holds while it is open. */
export type FirstStartState = {
  /** 0 to 7: F1 to F8. */
  step: number;
  ticked: boolean;
  password: string;
  passwordAgain: string;
  familyMade: boolean;
  pin: string;
  pinAgain: string;
  pinSet: boolean;
  name: string;
  band: AgeBand | null;
  child: ChildView | null;
  rules: RuleSet | null;
  /** Each day on its own, or one figure for every day. */
  each: boolean;
  every: number;
  /** Each day's minutes. */
  days: Readonly<Record<Weekday, number>>;
  bedtime: Bedtime;
  /** Minutes of the day. */
  from: number;
  to: number;
};

/** Two hours a day, as the family's file starts a child, and a bedtime from 9:00 pm to 7:00 am to start from. */
export const FIRST_START: FirstStartState = {
  step: 0,
  ticked: false,
  password: '',
  passwordAgain: '',
  familyMade: false,
  pin: '',
  pinAgain: '',
  pinSet: false,
  name: '',
  band: null,
  child: null,
  rules: null,
  each: false,
  every: 120,
  days: Object.fromEntries(WEEKDAYS.map((d) => [d, 120])) as Record<Weekday, number>,
  bedtime: 'every',
  from: 21 * 60,
  to: 7 * 60,
};

/** The nights before a school day. */
const SCHOOL_NIGHTS: readonly Weekday[] = ['sunday', 'monday', 'tuesday', 'wednesday', 'thursday'];
const STEP_MINUTES = 15;
const MOST_MINUTES = 24 * 60;

/** What the stepper shows a day's figure from: the rules' budget, a no limit as the most. */
function minutesOf(b: DailyBudget): number {
  return b === 'no_limit' ? MOST_MINUTES : b.minutes;
}

/** The rules' budgets and bedtime, as the first start holds them. */
function fromRules(s: FirstStartState, rules: RuleSet): FirstStartState {
  const days = Object.fromEntries(WEEKDAYS.map((d) => [d, minutesOf(rules.budgets[d])])) as Record<Weekday, number>;
  const each = WEEKDAYS.some((d) => days[d] !== days.monday);
  return { ...s, rules, days, every: days.monday, each };
}

/** The rules to send for the time a day. */
function withBudgets(s: FirstStartState): RuleSet {
  const rules = s.rules!;
  const budgets = Object.fromEntries(WEEKDAYS.map((d) => [d, { minutes: s.each ? s.days[d] : s.every }])) as Record<Weekday, DailyBudget>;
  return { ...rules, budgets };
}

/** The rules to send for bedtime. */
function withBedtime(s: FirstStartState): RuleSet {
  const rules = s.rules!;
  const blocked: BlockedWindow[] =
    s.bedtime === 'none' ? [] : [{ days: s.bedtime === 'every' ? [...WEEKDAYS] : [...SCHOOL_NIGHTS], start_minute: s.from, end_minute: s.to }];
  return { ...rules, blocked_hours: blocked };
}

// ---------------------------------------------------------------------------

/** Back at the left, its room kept empty on the first step; the counter at the right. */
function StepHead({ at, onBack }: { at: number; onBack: () => void }) {
  const { tap } = useSizes();
  return (
    <View style={[styles.steps, { minHeight: tap }]}>
      {at > 0 ? (
        <Pressable onPress={onBack} accessibilityRole="button" accessibilityLabel={W.steps.back} testID="back" style={[styles.back, { height: tap }]}>
          {Mark.left()}
          <Txt style={gtype.button}>{W.steps.back}</Txt>
        </Pressable>
      ) : (
        <View />
      )}
      <View style={styles.count}>
        <Label>{W.steps.count(at + 1, STEP_COUNT)}</Label>
        <View style={styles.dots} accessible accessibilityLabel={W.steps.dots(at + 1, STEP_COUNT)}>
          {Array.from({ length: STEP_COUNT }, (_, i) => (
            <View key={i} style={[styles.dot, i < at && styles.dotDone, i === at && styles.dotAt]} />
          ))}
        </View>
      </View>
    </View>
  );
}

/** Half hours of a day, for a bedtime's ends. */
const HALF_HOURS = Array.from({ length: 48 }, (_, i) => i * 30);

/** One Windows account, and whose it is. */
function AccountView({
  row,
  child,
  busy,
  err,
  onPick,
}: {
  row: AccountRow;
  child: ChildView;
  busy: boolean;
  err: string;
  onPick: (pick: 'child' | 'guardian' | 'later') => void;
}) {
  const { wide } = useSizes();
  const pick = row.link.kind === 'child' ? (row.link.childId === child.childId ? 'child' : null) : row.link.kind === 'not_limited' ? 'guardian' : 'later';
  const kind = row.isAdministrator ? W.accounts.admin : W.accounts.standard;
  const choices: ['child' | 'guardian' | 'later', string][] = [
    ['child', W.possessive(child.name)],
    ['guardian', W.accounts.guardians],
    ['later', W.accounts.later],
  ];
  const facts = (
    <View style={wide ? null : styles.factsLine} accessible accessibilityLabel={row.signedIn ? `${kind}, ${W.accounts.signedInNow}` : kind}>
      <Txt style={[gtype.facts, row.isAdministrator && pick === 'child' && styles.coral]}>{kind}</Txt>
      {row.signedIn ? (
        <>
          {wide ? null : <Txt style={gtype.facts}>{' · '}</Txt>}
          <Txt style={gtype.facts}>{W.accounts.signedInNow}</Txt>
        </>
      ) : null}
    </View>
  );
  const chips = (
    <View style={[styles.acctChips, wide && styles.acctChipsWide]} accessibilityRole="radiogroup" accessibilityLabel={row.displayName}>
      {choices.map(([key, label]) => (
        <Chip
          key={key}
          label={label}
          a11y={`${row.displayName}: ${label}`}
          on={pick === key}
          onPress={() => (busy ? undefined : onPick(key))}
          style={wide ? styles.acctChipWide : styles.acctChip}
          testID={`account-${row.name}-${key}`}
        />
      ))}
    </View>
  );
  // A grid of two columns on a phone (the tile, then the name; the choices
  // and any warning across both), of three at the console's width (the
  // choices at the right of the name), 8 between its rows.
  return (
    <View testID={`account-${row.name}`} style={[styles.acct, wide && styles.acctWide]}>
      <View style={styles.acctHead}>
        <Tile icon="account" />
        <View style={styles.who}>
          <Txt style={gtype.rowTitle}>{row.displayName}</Txt>
          <View style={styles.facts}>{facts}</View>
        </View>
        {wide ? chips : null}
      </View>
      {wide ? null : chips}
      {row.isAdministrator && pick === 'child' ? (
        <View style={styles.warn} testID="admin-warning">
          <View style={styles.warnTop}>
            <Icon name="alert" size={24} />
            <Txt style={[gtype.warn, styles.flex]} wrap="pretty">
              <Txt style={[gtype.warn, styles.bold]}>{W.accounts.warnBold(child.name)}</Txt> {W.accounts.warnText}
            </Txt>
          </View>
          {W.accounts.warnSteps(row.displayName).map((s, i) => (
            <View key={s} style={[styles.olRow, i === 0 && styles.olFirst]}>
              <Txt style={[gtype.warnStep, styles.olNumber]}>{`${i + 1}.`}</Txt>
              <Txt style={[gtype.warnStep, styles.flex]}>{s}</Txt>
            </View>
          ))}
          <Txt style={[gtype.small, styles.warnSmall]}>{W.accounts.changesNone}</Txt>
        </View>
      ) : null}
      {err ? <Txt style={gtype.err}>{err}</Txt> : null}
    </View>
  );
}

// ---------------------------------------------------------------------------

export function FirstStart({
  door,
  initial,
  accountsAtFirst,
  onDone,
  onSignedOut,
  onRefused,
}: {
  door: GuardianDoor;
  initial?: Partial<FirstStartState>;
  /** The accounts as already listed: the preview's pictures only. */
  accountsAtFirst?: readonly AccountRow[];
  onDone: (addPhone: boolean, child: ChildView | null) => void;
  onSignedOut: () => void;
  onRefused: () => void;
}) {
  const place = usePlace();
  const { clock } = useClock();
  const { wide } = useSizes();
  const [s, setS] = useState<FirstStartState>({ ...FIRST_START, ...initial });
  const [err, setErr] = useState('');
  const [busy, setBusy] = useState(false);
  const [accounts, setAccounts] = useState<readonly AccountRow[] | null>(accountsAtFirst ?? null);
  const [rowBusy, setRowBusy] = useState<string | null>(null);
  const [rowErr, setRowErr] = useState<Record<string, string>>({});
  const [picking, setPicking] = useState<'from' | 'to' | null>(null);
  const live = useRef(true);
  useEffect(() => {
    live.current = true;
    return () => {
      live.current = false;
    };
  }, []);

  const set = (change: Partial<FirstStartState>) => setS((old) => ({ ...old, ...change }));
  const go = (step: number) => {
    setErr('');
    setPicking(null);
    set({ step });
  };

  /** What the door said, where it is said: the sign in for a session that ended, else under the fields. */
  const said = (o: Exclude<Asked<unknown>, { kind: 'done' }>): string => {
    if (o.kind === 'not-done' && o.reason === 'signed_out') onSignedOut();
    if (o.kind === 'refused') onRefused();
    if (o.kind === 'not-done' && o.ruleProblems.length) return o.ruleProblems.map((p) => p.message).join(' ');
    return o.words;
  };

  /** Runs one step's asking, with the button switched off meanwhile. */
  const run = async (work: () => Promise<void>) => {
    if (busy) return;
    setBusy(true);
    setErr('');
    try {
      await work();
    } finally {
      if (live.current) setBusy(false);
    }
  };

  const loadAccounts = async () => {
    const got = await ask(door, 'accounts', {});
    if (!live.current) return;
    if (got.kind === 'done') setAccounts(got.value.filter((a) => a.onThisPc));
    else setErr(said(got));
  };
  useEffect(() => {
    if (s.step === 6 && accounts === null) void loadAccounts();
    // Asked when the step opens.
  }, [s.step]);

  const pickAccount = async (row: AccountRow, pick: 'child' | 'guardian' | 'later') => {
    if (!s.child || rowBusy) return;
    setRowBusy(row.accountId);
    setRowErr((e) => ({ ...e, [row.accountId]: '' }));
    const got =
      pick === 'later'
        ? row.link.kind === 'none'
          ? ({ kind: 'done', value: {} } as const)
          : await ask(door, 'unlink_account', { account: row.accountId })
        : await ask(door, 'link_account', { account: row.accountId, link: pick === 'guardian' ? 'not_limited' : { child: { child_id: s.child.childId } } });
    if (!live.current) return;
    setRowBusy(null);
    if (got.kind !== 'done') {
      setRowErr((e) => ({ ...e, [row.accountId]: said(got) }));
      return;
    }
    await loadAccounts();
  };

  const next = () =>
    run(async () => {
      switch (s.step) {
        case 0:
          if (s.ticked) go(1);
          return;
        case 1: {
          if (s.familyMade && s.password === '' && s.passwordAgain === '') return go(2);
          if (s.password !== s.passwordAgain) return setErr(W.password.notSame);
          const got = await ask(door, 'create_family', { password: s.password });
          if (got.kind !== 'done') return setErr(said(got));
          set({ familyMade: true, password: '', passwordAgain: '' });
          return go(2);
        }
        case 2: {
          if (s.pinSet && s.pin === '' && s.pinAgain === '') return go(3);
          if (s.pin !== s.pinAgain) return setErr(W.pin.notSame);
          const got = await ask(door, 'set_pin', { pin: s.pin });
          if (got.kind !== 'done') return setErr(said(got));
          set({ pinSet: true, pin: '', pinAgain: '' });
          return go(3);
        }
        case 3: {
          if (!s.band || s.name.trim() === '') return;
          let child = s.child;
          if (!child) {
            const got = await ask(door, 'add_child', { name: s.name.trim(), age_band: s.band });
            if (got.kind !== 'done') return setErr(said(got));
            child = got.value;
          } else {
            if (child.name !== s.name.trim()) {
              const got = await ask(door, 'rename_child', { child: child.childId, name: s.name.trim() });
              if (got.kind !== 'done') return setErr(said(got));
              child = got.value;
            }
            if (child.ageBand !== s.band) {
              const got = await ask(door, 'set_age_band', { child: child.childId, age_band: s.band });
              if (got.kind !== 'done') return setErr(said(got));
              child = got.value;
            }
          }
          let rules = s.rules;
          if (!rules) {
            const got = await ask(door, 'rules', { child: child.childId });
            if (got.kind !== 'done') return setErr(said(got));
            rules = got.value;
            setS((old) => fromRules({ ...old, child }, rules!));
          } else {
            set({ child });
          }
          return go(4);
        }
        case 4:
        case 5: {
          const got = await ask(door, 'replace_rules', { child: s.child!.childId, rules: s.step === 4 ? withBudgets(s) : withBedtime(s) });
          if (got.kind !== 'done') return setErr(said(got));
          set({ rules: got.value });
          return go(s.step + 1);
        }
        case 6: {
          const got = await ask(door, 'finish_first_start', {});
          if (got.kind !== 'done') return setErr(said(got));
          return go(7);
        }
      }
    });

  const name = s.child?.name ?? s.name.trim();
  const at = s.step;

  // ---- The steps' cards

  let card: ReactElement;
  switch (at) {
    case 0:
      card = (
        <>
          <View style={styles.head}>
            <Label>{W.welcome.label}</Label>
            <H1 style={styles.headTitle}>{W.welcome.heading}</H1>
          </View>
          <Card bare style={[styles.list, wide && styles.listWide]}>
            {W.welcome.rows.map((r, i) => (
              <View key={r.title}>
                {i > 0 ? <Dashed /> : null}
                <View style={[styles.li, wide && styles.liWide]} accessible accessibilityLabel={`${r.title}. ${r.text}`}>
                  <Tile icon={r.icon as IconName} />
                  <View style={styles.flex}>
                    <Txt style={gtype.rowTitle}>{r.title}</Txt>
                    <Txt style={[gtype.rowText, styles.liText]} wrap="pretty">
                      {r.text}
                    </Txt>
                  </View>
                </View>
              </View>
            ))}
          </Card>
          {place === 'pc' ? (
            <Txt style={[gtype.small, styles.small]} wrap="pretty" testID="window-line">
              {W.welcome.window}
            </Txt>
          ) : null}
          <View style={styles.tickRow}>
            <Shadow offset={3} corner={10} />
            <Pressable
              testID="agree"
              onPress={() => set({ ticked: !s.ticked })}
              accessibilityRole="checkbox"
              accessibilityLabel={W.welcome.tick}
              accessibilityState={{ checked: s.ticked }}
              style={styles.tick}
            >
              <View style={[styles.box, s.ticked && styles.boxOn]}>{s.ticked ? Mark.tick(color.card) : null}</View>
              <Txt style={[gtype.tick, styles.flex]} wrap="balance">
                {W.welcome.tick}
              </Txt>
            </Pressable>
          </View>
        </>
      );
      break;
    case 1:
      card = (
        <Card>
          <CardHead icon="key" title={W.password.heading} />
          <Txt style={[gtype.detail, styles.detail]} wrap="pretty">
            {W.password.line}
          </Txt>
          <Label style={styles.gap} hint={W.password.hint}>
            {W.password.field}
          </Label>
          <Field testID="password" value={s.password} onChangeText={(v) => set({ password: v })} secureTextEntry autoComplete="off" autoCorrect={false} autoCapitalize="none" accessibilityLabel={W.password.field} />
          <Label style={styles.gap}>{W.password.again}</Label>
          <Field testID="password-again" value={s.passwordAgain} onChangeText={(v) => set({ passwordAgain: v })} onSubmitEditing={next} secureTextEntry autoComplete="off" autoCorrect={false} autoCapitalize="none" accessibilityLabel={W.password.again} />
          <Room words={err} />
        </Card>
      );
      break;
    case 2:
      card = (
        <Card>
          <CardHead icon="pin" title={W.pin.heading} />
          <View style={[styles.two, wide && styles.twoWide]}>
            {[
              ['key', W.pin.passwordTitle, W.pin.passwordLine, false],
              ['pin', W.pin.pinTitle, W.pin.pinLine, true],
            ].map(([icon, title, line, now]) => (
              <View key={title as string} style={[styles.twoBox, now ? styles.twoNow : null, wide && styles.flex]}>
                <Tile icon={icon as IconName} />
                <View style={styles.flex}>
                  <Txt style={gtype.rowTitle}>{title as string}</Txt>
                  <Txt style={[gtype.twoText, styles.twoLine]} wrap="pretty">
                    {line as string}
                  </Txt>
                </View>
              </View>
            ))}
          </View>
          <Txt style={[gtype.detail, styles.detail]} wrap="pretty">
            {W.pin.choose}
          </Txt>
          <Label style={styles.gap} hint={W.pin.hint}>
            {W.pin.field}
          </Label>
          <Field testID="pin" value={s.pin} onChangeText={(v) => set({ pin: v })} secureTextEntry keyboardType="number-pad" autoComplete="off" accessibilityLabel={W.pin.field} />
          <Label style={styles.gap}>{W.pin.again}</Label>
          <Field testID="pin-again" value={s.pinAgain} onChangeText={(v) => set({ pinAgain: v })} onSubmitEditing={next} secureTextEntry keyboardType="number-pad" autoComplete="off" accessibilityLabel={W.pin.again} />
          <Room words={err} />
        </Card>
      );
      break;
    case 3:
      card = (
        <Card>
          <CardHead icon="child" title={W.child.heading} />
          <Label style={styles.gap}>{W.child.name}</Label>
          <Field testID="child-name" value={s.name} onChangeText={(v) => set({ name: v })} autoCorrect={false} maxLength={40} accessibilityLabel={W.child.name} />
          <Label style={styles.gap}>{W.child.age}</Label>
          <View style={styles.even} accessibilityRole="radiogroup" accessibilityLabel={W.child.age}>
            {AGE_BANDS.map((b) => (
              <Chip key={b} label={W.AGE_WORDS[b]} on={s.band === b} onPress={() => set({ band: b })} style={styles.evenChip} testID={`age-${b}`} />
            ))}
          </View>
          <Txt style={[gtype.small, styles.small]} wrap="pretty">
            {W.child.later}
          </Txt>
          {err ? <Room words={err} /> : null}
        </Card>
      );
      break;
    case 4: {
      const change = (day: Weekday | null, by: number) => {
        const clamp = (m: number) => Math.min(MOST_MINUTES, Math.max(0, m + by));
        if (day === null) set({ every: clamp(s.every) });
        else set({ days: { ...s.days, [day]: clamp(s.days[day]) } });
      };
      card = (
        <Card>
          <CardHead icon="calendar" title={W.timeADay.heading} />
          <Txt style={[gtype.detail, styles.detail]} wrap="pretty">
            {W.timeADay.forAll(name)}
          </Txt>
          <View style={[styles.chips, styles.chipsTime]} accessibilityRole="radiogroup">
            <Chip label={W.timeADay.every} on={!s.each} onPress={() => set({ each: false })} testID="every-day" />
            <Chip label={W.timeADay.each} on={s.each} onPress={() => set({ each: true, days: s.each ? s.days : (Object.fromEntries(WEEKDAYS.map((d) => [d, s.every])) as Record<Weekday, number>) })} testID="each-day" />
          </View>
          {s.each ? (
            <View style={[styles.days, wide && styles.daysWide]}>
              <Dashed />
              {WEEKDAYS.map((d, i) => (
                <View key={d}>
                  {i > 0 ? <Dashed /> : null}
                  <View style={styles.day}>
                    <Txt style={gtype.day}>{W.DAY_WORDS[d]}</Txt>
                    <Stepper value={minutesFigure(s.days[d])} onLess={() => change(d, -STEP_MINUTES)} onMore={() => change(d, STEP_MINUTES)} name={W.DAY_WORDS[d]} />
                  </View>
                </View>
              ))}
            </View>
          ) : (
            <View style={styles.one}>
              <Dashed />
              <View style={styles.oneIn}>
                <Stepper big value={minutesFigure(s.every)} onLess={() => change(null, -STEP_MINUTES)} onMore={() => change(null, STEP_MINUTES)} name={W.timeADay.every} />
              </View>
            </View>
          )}
          {err ? <Room words={err} /> : null}
        </Card>
      );
      break;
    }
    case 5: {
      const off = s.bedtime === 'none';
      card = (
        <Card>
          <CardHead icon="moon" title={W.bedtime.heading} />
          <Txt style={[gtype.detail, styles.detail]} wrap="pretty">
            {W.bedtime.line(name)}
          </Txt>
          <Label style={styles.gap}>{W.bedtime.when}</Label>
          <View style={styles.chips} accessibilityRole="radiogroup" accessibilityLabel={W.bedtime.when}>
            {(
              [
                ['every', W.bedtime.every],
                ['school', W.bedtime.school],
                ['none', W.bedtime.none],
              ] as const
            ).map(([key, label]) => (
              <Chip key={key} label={label} on={s.bedtime === key} onPress={() => set({ bedtime: key })} testID={`bedtime-${key}`} />
            ))}
          </View>
          <View style={styles.times}>
            {(
              [
                ['from', W.bedtime.from, s.from],
                ['to', W.bedtime.to, s.to],
              ] as const
            ).map(([key, label, minute]) => (
              <View key={key} style={styles.flexMin}>
                <Label>{label}</Label>
                <PickField
                  testID={`bedtime-${key}-field`}
                  label={label}
                  value={minuteOfDay(minute, clock)}
                  off={off}
                  open={picking === key}
                  onPress={() => setPicking(picking === key ? null : key)}
                />
              </View>
            ))}
          </View>
          {picking ? (
            <View style={styles.picker} testID="time-list">
              <ScrollView style={styles.pickerScroll} nestedScrollEnabled>
                {HALF_HOURS.map((m) => (
                  <Pressable
                    key={m}
                    onPress={() => {
                      set(picking === 'from' ? { from: m } : { to: m });
                      setPicking(null);
                    }}
                    accessibilityRole="button"
                    accessibilityLabel={minuteOfDay(m, clock)}
                    style={styles.pickerRow}
                  >
                    <Txt style={[gtype.chip, styles.pickerWords]}>{minuteOfDay(m, clock)}</Txt>
                  </Pressable>
                ))}
              </ScrollView>
            </View>
          ) : null}
          <Room words={err} />
        </Card>
      );
      break;
    }
    case 6:
      card = (
        <Card>
          <CardHead icon="account" title={W.accounts.heading} />
          <Txt style={[gtype.detail, styles.detail]} wrap="pretty">
            {W.accounts.line}
          </Txt>
          <View style={styles.accts}>
            <Dashed />
            {(accounts ?? []).map((row, i) => (
              <View key={row.accountId}>
                {i > 0 ? <Dashed /> : null}
                {s.child ? (
                  <AccountView row={row} child={s.child} busy={rowBusy !== null} err={rowErr[row.accountId] ?? ''} onPick={(p) => void pickAccount(row, p)} />
                ) : null}
              </View>
            ))}
          </View>
          <Txt style={[gtype.small, styles.small]} wrap="pretty">
            {W.accounts.laterLine}
          </Txt>
          {err ? <Room words={err} /> : null}
        </Card>
      );
      break;
    default: {
      const rules = s.rules;
      const budgets = rules?.budgets;
      const same = budgets && WEEKDAYS.every((d) => JSON.stringify(budgets[d]) === JSON.stringify(budgets.monday));
      const timeLine = !budgets
        ? null
        : same && budgets.monday !== 'no_limit'
          ? W.done.aDay(minutesFigure(budgets.monday.minutes))
          : timeADayWords(budgets);
      const window = rules?.blocked_hours[0];
      const bedLine = !rules
        ? null
        : !window
          ? W.done.noBedtime
          : W.done.bedtime(minuteOfDay(window.start_minute, clock), minuteOfDay(window.end_minute, clock)) + (window.days.length < 7 ? W.done.schoolNights : '');
      const linked = s.child && accounts?.some((a) => a.link.kind === 'child' && a.link.childId === s.child!.childId);
      const made = [
        W.done.passwordAndPin,
        s.child ? W.done.child(s.child.name, W.AGE_WORDS[s.child.ageBand].replace('Under', 'under')) : null,
        timeLine,
        bedLine,
        linked && s.child ? W.done.account(s.child.name) : null,
      ].filter((m): m is string => m !== null);
      card = (
        <Card>
          <CardHead icon="check" title={W.done.heading} />
          <View style={styles.oks}>
            {made.map((m, i) => (
              <View key={m}>
                {i > 0 ? <Dashed /> : null}
                <View style={[styles.ok, i > 0 && styles.okAfter]}>
                  {Mark.tick()}
                  <Txt style={[gtype.ok, styles.flex]}>{m}</Txt>
                </View>
              </View>
            ))}
          </View>
          <View style={styles.next}>
            <Tile icon="phone" />
            <View style={styles.flex}>
              <Txt style={gtype.rowTitle}>{W.done.phone(name)}</Txt>
              <Txt style={[gtype.rowText, styles.liText]} wrap="balance">
                {W.done.phoneLine}
              </Txt>
            </View>
          </View>
        </Card>
      );
    }
  }

  const pinned =
    at === 7
      ? ([
          <PinBtn key="later" label={W.done.later} onPress={() => onDone(false, s.child)} testID="later" />,
          <PinBtn key="add" label={W.done.addNow(name)} look="go" onPress={() => onDone(true, s.child)} testID="add-phone" />,
        ] as const)
      : (
        <PinBtn
          label={W.steps.go}
          look="go"
          off={busy || (at === 0 && !s.ticked) || (at === 3 && (!s.band || s.name.trim() === ''))}
          onPress={() => void next()}
          testID="main-button"
        />
      );

  return (
    <Page testID={`first-${at + 1}`} pinned={pinned}>
      <TopRow />
      <Column>
        <StepHead at={at} onBack={() => go(at - 1)} />
        {card}
      </Column>
      {at === 7 ? <MakerMark /> : null}
    </Page>
  );
}

const styles = StyleSheet.create({
  flex: { flex: 1 },
  flexMin: { flex: 1, minWidth: 0 },
  bold: { fontWeight: '600' },
  coral: { color: color.coral },
  steps: { flexDirection: 'row', alignItems: 'center', justifyContent: 'space-between', marginTop: 12, marginBottom: 14 },
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
  count: { alignItems: 'flex-end', gap: 7 },
  dots: { flexDirection: 'row', gap: 6 },
  dot: { width: 10, height: 10, borderRadius: 5, borderWidth, borderColor: color.inkSoft },
  dotDone: { backgroundColor: color.teal, borderColor: color.teal },
  dotAt: { backgroundColor: color.marigold, borderColor: color.marigoldDeep },
  head: { marginBottom: 12, marginHorizontal: 2 },
  headTitle: { marginTop: 8 },
  list: { paddingVertical: 2, paddingHorizontal: 14 },
  listWide: { paddingHorizontal: 18 },
  li: { flexDirection: 'row', alignItems: 'flex-start', gap: 12, paddingVertical: 12 },
  liWide: { alignItems: 'center' },
  liText: { marginTop: 2 },
  small: { marginTop: 12, marginHorizontal: 2 },
  tickRow: { marginTop: 14 },
  tick: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 12,
    minHeight: 56,
    paddingTop: 10,
    paddingBottom: 10,
    paddingLeft: 12,
    paddingRight: 14,
    backgroundColor: color.card,
    borderWidth,
    borderColor: color.ink,
    borderRadius: 10,
  },
  box: { width: 26, height: 26, alignItems: 'center', justifyContent: 'center', borderWidth: 2, borderColor: color.ink, borderRadius: 7, backgroundColor: color.card },
  boxOn: { backgroundColor: color.ink },
  detail: { marginTop: 10 },
  gap: { marginTop: 18 },
  two: { gap: 10, marginTop: 16 },
  twoWide: { flexDirection: 'row' },
  twoBox: { flexDirection: 'row', alignItems: 'flex-start', gap: 12, paddingVertical: 10, paddingHorizontal: 12, borderWidth, borderStyle: 'dashed', borderColor: 'rgba(27,38,56,0.42)', borderRadius: 10 },
  twoNow: { borderStyle: 'solid', borderColor: color.ink },
  twoLine: { marginTop: 2 },
  even: { flexDirection: 'row', gap: 8, marginTop: 8 },
  evenChip: { flex: 1, paddingHorizontal: 2 },
  chips: { flexDirection: 'row', flexWrap: 'wrap', gap: 8, marginTop: 8 },
  chipsTime: { marginTop: 16 },
  one: { marginTop: 22 },
  oneIn: { alignItems: 'center', paddingTop: 14, paddingBottom: 6 },
  days: { marginTop: 14 },
  daysWide: { marginTop: 12 },
  day: { flexDirection: 'row', alignItems: 'center', justifyContent: 'space-between', gap: 12, paddingVertical: 2 },
  times: { flexDirection: 'row', gap: 12, marginTop: 18 },
  picker: { marginTop: 8, borderWidth, borderColor: color.ink, borderRadius: 10, backgroundColor: color.card, overflow: 'hidden' },
  pickerScroll: { maxHeight: 220 },
  pickerRow: { minHeight: 44, justifyContent: 'center', paddingHorizontal: 14 },
  pickerWords: { textAlign: 'left' },
  accts: { marginTop: 12 },
  acct: { gap: 8, paddingVertical: 10 },
  acctWide: { paddingVertical: 12 },
  acctHead: { flexDirection: 'row', alignItems: 'center', gap: 12 },
  who: { flex: 1, minWidth: 0 },
  facts: { marginTop: 5 },
  factsLine: { flexDirection: 'row', flexWrap: 'wrap' },
  acctChips: { flexDirection: 'row', gap: 8 },
  acctChipsWide: { flexShrink: 0 },
  acctChip: { flexGrow: 1, flexShrink: 1, paddingHorizontal: 10 },
  acctChipWide: { paddingHorizontal: 12 },
  warn: { paddingVertical: 12, paddingHorizontal: 14, borderWidth, borderColor: color.coral, borderRadius: 10, backgroundColor: 'rgba(207,67,38,0.06)' },
  warnTop: { flexDirection: 'row', alignItems: 'flex-start', gap: 10 },
  olRow: { flexDirection: 'row', paddingLeft: 34 },
  olFirst: { marginTop: 8 },
  olNumber: { width: 20 },
  warnSmall: { marginTop: 8, marginLeft: 34 },
  oks: { marginTop: 14 },
  ok: { flexDirection: 'row', alignItems: 'center', gap: 10, minHeight: 40 },
  okAfter: { minHeight: 38.5 },
  next: { flexDirection: 'row', alignItems: 'flex-start', gap: 12, marginTop: 12, paddingVertical: 12, paddingHorizontal: 14, borderWidth, borderStyle: 'dashed', borderColor: 'rgba(27,38,56,0.42)', borderRadius: 10 },
});
