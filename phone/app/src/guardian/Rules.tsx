// R, a child's rules, as design/screens/console.js draws it (R1 to R3):
// everything a family sets for one child, the child chosen at the top. On a
// phone the cards are one column; at the console's width they run down two
// columns in the same order, the first column filled to about half.
//
// The screen changes a copy of the rules; while it differs from the rules
// kept, the tab bar gives way to Undo and Save, pinned to the bottom. Save
// sends the rules whole (`replace_rules`); a problem is said in the rules'
// own sentence beside the row it is about, the strip says how many, and the
// page scrolls to the first such row as the drawing does. Saved, a notice
// floats above the tab bar for a moment.
//
// The rules keep one list of blocked apps and one of always allowed apps,
// by app id, for all of a child's devices. Each app is shown by the name a
// device gave it, and under each device whose list has it, as drawn
// (`app_names`); its plain id only when no device named it. Apps are offered
// from what the child used today (`usage`), and Add an app offers a device's
// own list (`device_apps`).

import { type ReactNode, useCallback, useEffect, useRef, useState } from 'react';
import { Pressable, type ScrollView, StyleSheet, View } from 'react-native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';

import { type AppName, ask, type DeviceApps, type Asked, type BlockedWindow, type ChildToday, type DeviceUsage, type GuardianDoor, type RuleProblem, type RuleSet, type Weekday, WEEKDAYS } from '../guardianSeam';
import { Icon } from '../icons/Icon';
import { borderWidth, color } from '../theme';
import { Dashed, Shadow } from '../ui/paper/parts';
import { Txt } from '../ui/paper/text';
import { useClock } from './clock';
import { GuardianTabs, type TabKey } from './GuardianTabs';
import { Page } from './Page';
import { Btn, Card, Chip, Field, GroupLabel, Head2, Kvs, Label, Letter, Mark, PickField, Room, Square, Stepper, TopRow, useScreenWidth, useSizes } from './parts';
import { gtype } from './text';
import { minuteOfDay, minutesFigure, usedSpan } from './time';
import * as W from './words';

const STEP_MINUTES = 15;
const MOST_MINUTES = 24 * 60;
/** The nights before a school day. */
const SCHOOL_NIGHTS: readonly Weekday[] = ['sunday', 'monday', 'tuesday', 'wednesday', 'thursday'];
const HALF_HOURS = Array.from({ length: 48 }, (_, i) => i * 30);
/** How long the Saved notice floats, in milliseconds. */
export const SAVED_MS = 4_000;
/** A row with a problem is scrolled to this far below the page's top, as the drawing has it. */
const PROBLEM_AT = 120;

type Bedtime = 'every' | 'school' | 'none';
type AppList = 'blocked_apps' | 'always_allowed_apps';

const minutesOf = (rules: RuleSet, d: Weekday) => {
  const b = rules.budgets[d];
  return b === 'no_limit' ? MOST_MINUTES : b.minutes;
};
const eachOnItsOwn = (rules: RuleSet) => WEEKDAYS.some((d) => minutesOf(rules, d) !== minutesOf(rules, 'monday'));

function bedtimeOf(rules: RuleSet): { kind: Bedtime; from: number; to: number } {
  const w = rules.blocked_hours[0];
  if (!w) return { kind: 'none', from: 21 * 60, to: 7 * 60 };
  return { kind: w.days.length === 7 ? 'every' : 'school', from: w.start_minute, to: w.end_minute };
}

/** The number in a field's path after a list's name: `blocked_apps[2]` is 2. */
function indexIn(field: string, list: AppList): number | null {
  const m = new RegExp(`^${list}\\[(\\d+)\\]`).exec(field);
  return m ? Number(m[1]) : null;
}

/** An app or program in a list: its first letter, its name, and a way to take it off. A row with a problem is outlined in coral, with the rules' own sentence under it. */
function AppRow({
  name,
  onOff,
  problem,
  first,
  fixed,
  rowRef,
}: {
  name: string;
  onOff?: () => void;
  problem?: string;
  first: boolean;
  /** What cannot be taken off says why instead. */
  fixed?: string;
  rowRef?: (v: View | null) => void;
}) {
  return (
    <View testID={`app-${name}`}>
      {first || problem ? null : <Dashed />}
      <View ref={rowRef} style={[styles.app, problem ? styles.appBad : null]}>
        <Letter name={name} />
        <Txt style={[gtype.appName, styles.flex]}>{name}</Txt>
        {fixed ? <Txt style={gtype.fixed}>{fixed}</Txt> : <Square mark={Mark.cross()} label={`${W.rules.takeOff}: ${name}`} onPress={onOff ?? (() => undefined)} testID={`take-off-${name}`} />}
      </View>
      {problem ? (
        <Txt style={[gtype.err, styles.problem]} wrap="pretty" testID="rule-problem">
          {problem}
        </Txt>
      ) : null}
    </View>
  );
}

/**
 * Add an app: one device's own list, a phone's as it sent it or the PC's
 * programs seen running in the child's accounts (`device_apps`), each to add
 * to this list with a tap. The PC's says plainly what it knows.
 */
function AddApp({
  door,
  child,
  list,
  onAdd,
}: {
  door: GuardianDoor;
  child: ChildToday;
  list: readonly string[];
  onAdd: (id: string) => void;
}) {
  const [deviceId, setDeviceId] = useState<string | null>(child.devices[0]?.deviceId ?? null);
  const [apps, setApps] = useState<DeviceApps | null>(null);
  const [err, setErr] = useState('');
  const [find, setFind] = useState('');
  useEffect(() => {
    if (!deviceId) return undefined;
    let live = true;
    setApps(null);
    void ask(door, 'device_apps', { device: deviceId }).then((got) => {
      if (!live) return;
      if (got.kind === 'done') {
        setApps(got.value);
        setErr('');
      } else setErr(got.words);
    });
    return () => {
      live = false;
    };
  }, [door, deviceId]);
  const device = child.devices.find((d) => d.deviceId === deviceId);
  const inList = new Set(list.map((a) => a.toLowerCase()));
  const looking = find.trim().toLowerCase();
  const shown = (apps?.apps ?? []).filter((a) => !inList.has(a.appId.toLowerCase()) && (!looking || a.name.toLowerCase().includes(looking)));
  const deviceName = (d: ChildToday['devices'][number]) => (d.kind === 'on_master' ? W.devices.thisPc : d.name);
  return (
    <View style={styles.adding} testID="add-app">
      {child.devices.length > 1 ? (
        <View style={styles.chips} accessibilityRole="radiogroup" accessibilityLabel={W.rules.addApp}>
          {child.devices.map((d) => (
            <Chip key={d.deviceId} label={deviceName(d)} on={d.deviceId === deviceId} onPress={() => setDeviceId(d.deviceId)} testID={`apps-of-${d.deviceId}`} />
          ))}
        </View>
      ) : null}
      {apps?.source === 'seen_running' ? (
        <Txt style={[gtype.small, styles.small]} wrap="pretty" testID="pc-known">
          {W.rules.pcKnown(child.name)}
        </Txt>
      ) : null}
      {apps && apps.source === 'phone_list' && apps.keptAt === null && device ? (
        <Txt style={[gtype.small, styles.small]} wrap="pretty">
          {W.rules.noList(device.name)}
        </Txt>
      ) : null}
      {apps && apps.apps.length > 8 ? <Field testID="find-app" value={find} onChangeText={setFind} placeholder={W.rules.find} accessibilityLabel={W.rules.find} /> : null}
      <View style={styles.pickList}>
        {shown.map((a, i) => (
          <View key={a.appId}>
            {i > 0 ? <Dashed /> : null}
            <View style={styles.app}>
              <Letter name={a.name} />
              <Txt style={[gtype.appName, styles.flex]}>{a.name}</Txt>
              <Square mark={Mark.plus()} label={W.rules.addThis(a.name)} onPress={() => onAdd(a.appId)} testID={`add-${a.appId}`} />
            </View>
          </View>
        ))}
      </View>
      {apps && apps.apps.length > 0 && shown.length === 0 && !looking ? <Txt style={[gtype.small, styles.small]}>{W.rules.allListed}</Txt> : null}
      {apps?.cut ? <Txt style={[gtype.small, styles.small]}>{W.rules.cut(apps.apps.length, apps.total)}</Txt> : null}
      {err ? <Txt style={[gtype.err, styles.problem]}>{err}</Txt> : null}
    </View>
  );
}

/** Half hours of a day to choose one from. */
function TimeList({ onPick }: { onPick: (minute: number) => void }) {
  const { clock } = useClock();
  return (
    <View style={styles.picker} testID="time-list">
      {HALF_HOURS.map((m) => (
        <Pressable key={m} onPress={() => onPick(m)} accessibilityRole="button" accessibilityLabel={minuteOfDay(m, clock)} style={styles.pickerRow}>
          <Txt style={[gtype.chip, styles.pickerWords]}>{minuteOfDay(m, clock)}</Txt>
        </Pressable>
      ))}
    </View>
  );
}

/** Changing a child's name and age band, in the card. */
function NameAndAge({ start, onSave }: { start: { name: string; band: ChildToday['ageBand'] }; onSave: (name: string, band: ChildToday['ageBand']) => Promise<string> }) {
  const [n, setN] = useState(start.name);
  const [band, setBand] = useState(start.band);
  const [err, setErr] = useState('');
  const [busy, setBusy] = useState(false);
  return (
    <View style={styles.edit} testID="name-and-age">
      <Label>{W.child.name}</Label>
      <Field testID="child-name" value={n} onChangeText={setN} maxLength={40} accessibilityLabel={W.child.name} />
      <Label style={styles.gapLabel}>{W.child.age}</Label>
      <View style={[styles.chips, styles.even]} accessibilityRole="radiogroup" accessibilityLabel={W.child.age}>
        {(['under_six', 'six_to_nine', 'ten_to_twelve', 'thirteen_to_seventeen'] as const).map((b) => (
          <Chip key={b} label={W.AGE_WORDS[b]} on={band === b} onPress={() => setBand(b)} style={styles.evenChip} testID={`age-${b}`} />
        ))}
      </View>
      <Room words={err} testID="name-message" />
      <Btn
        label={W.rules.save}
        look="go"
        off={busy || n.trim() === ''}
        onPress={() => {
          setBusy(true);
          void onSave(n.trim(), band).then((words) => {
            setBusy(false);
            setErr(words);
          });
        }}
        testID="save-name"
      />
    </View>
  );
}

/** Saved: a solid notice that floats above the tab bar for a moment. */
function Saved({ name }: { name: string }) {
  const { wide } = useSizes();
  const width = useScreenWidth();
  const insets = useSafeAreaInsets();
  return (
    <View
      style={[styles.saved, { bottom: insets.bottom + 96 }, wide ? { left: insets.left + (width - 440) / 2, width: 440 } : { left: insets.left + 14, right: insets.right + 14 }]}
      testID="saved"
      accessibilityLiveRegion="polite"
    >
      <Shadow offset={4} corner={14} />
      <View style={styles.savedIn}>
        <View style={styles.savedTile}>
          <Icon name="check" size={32} />
        </View>
        <Txt style={[gtype.notice, styles.flex]} wrap="pretty">
          {W.rules.saved(name)}
        </Txt>
      </View>
    </View>
  );
}

export function Rules({
  door,
  onTab,
  onRemove,
  onSignedOut,
  onRefused,
  initialChild,
  initialDraft,
  saveAtStart = false,
  initialSaved = false,
}: {
  door: GuardianDoor;
  onTab: (tab: TabKey) => void;
  /** Removing a child asks for the password again, on a screen of its own. */
  onRemove: (child: ChildToday) => void;
  onSignedOut: () => void;
  onRefused: () => void;
  initialChild?: string;
  /** A change not saved yet, made on top of the rules kept: the preview's pictures. */
  initialDraft?: (rules: RuleSet) => RuleSet;
  /** Save pressed as soon as the rules are in: the preview's pictures. */
  saveAtStart?: boolean;
  /** The Saved notice showing: the preview's pictures. */
  initialSaved?: boolean;
}) {
  const { clock } = useClock();
  const { wide, field } = useSizes();
  const width = useScreenWidth();
  const [children, setChildren] = useState<readonly ChildToday[] | null>(null);
  const [childId, setChildId] = useState<string | null>(initialChild ?? null);
  const [kept, setKept] = useState<RuleSet | null>(null);
  const [draft, setDraft] = useState<RuleSet | null>(null);
  const [each, setEach] = useState(false);
  const [usage, setUsage] = useState<readonly DeviceUsage[]>([]);
  const [problems, setProblems] = useState<readonly RuleProblem[]>([]);
  const [err, setErr] = useState('');
  const [busy, setBusy] = useState(false);
  const [saved, setSaved] = useState(initialSaved);
  const [picking, setPicking] = useState<'from' | 'to' | 'day' | null>(null);
  const [names, setNames] = useState<ReadonlyMap<string, AppName>>(new Map());
  const [adding, setAdding] = useState<AppList | null>(null);
  const [editName, setEditName] = useState(false);
  const [heights, setHeights] = useState<readonly number[]>([]);
  const scrollRef = useRef<ScrollView | null>(null);
  const contentRef = useRef<View | null>(null);
  const badRef = useRef<View | null>(null);
  const live = useRef(true);
  const started = useRef(false);

  const said = useCallback(
    (o: Exclude<Asked<unknown>, { kind: 'done' }>): string => {
      if (o.kind === 'not-done' && o.reason === 'signed_out') onSignedOut();
      if (o.kind === 'refused') onRefused();
      return o.words;
    },
    [onSignedOut, onRefused],
  );

  const loadChildren = useCallback(async () => {
    const got = await ask(door, 'family_today', {});
    if (!live.current) return;
    if (got.kind !== 'done') return setErr(said(got));
    setChildren(got.value);
    setChildId((id) => (id && got.value.some((c) => c.childId === id) ? id : (got.value[0]?.childId ?? null)));
  }, [door, said]);

  useEffect(() => {
    live.current = true;
    void loadChildren();
    return () => {
      live.current = false;
    };
  }, [loadChildren]);

  const child = children?.find((c) => c.childId === childId) ?? null;
  const day = child?.day;

  async function save(id: string, rules: RuleSet) {
    setBusy(true);
    const got = await ask(door, 'replace_rules', { child: id, rules });
    if (!live.current) return;
    setBusy(false);
    if (got.kind === 'done') {
      setKept(got.value);
      setDraft(got.value);
      setProblems([]);
      setErr('');
      setSaved(true);
      return;
    }
    if (got.kind === 'not-done' && got.ruleProblems.length) {
      setProblems(got.ruleProblems);
      setErr('');
      return;
    }
    setProblems([]);
    setErr(said(got));
  }
  const saveRef = useRef(save);
  saveRef.current = save;

  // The child's rules, and what the child used today.
  useEffect(() => {
    if (!childId || day === undefined) return undefined;
    let current = true;
    void Promise.all([
      ask(door, 'rules', { child: childId }),
      ask(door, 'usage', { child: childId, from_day: day, to_day: day }),
      ask(door, 'app_names', { child: childId }),
    ]).then(([rules, use, named]) => {
      if (!current || !live.current) return;
      if (rules.kind !== 'done') return setErr(said(rules));
      const first = !started.current;
      started.current = true;
      const start = first && initialDraft ? initialDraft(rules.value) : rules.value;
      setKept(rules.value);
      setDraft(start);
      setEach(eachOnItsOwn(start));
      setProblems([]);
      setErr('');
      setUsage(use.kind === 'done' ? use.value : []);
      setNames(new Map(named.kind === 'done' ? named.value.map((a) => [a.appId.toLowerCase(), a] as const) : []));
      if (first && saveAtStart) void saveRef.current(childId, start);
    });
    return () => {
      current = false;
    };
  }, [door, childId, day, said, initialDraft, saveAtStart]);

  // The Saved notice floats for a moment.
  useEffect(() => {
    if (!saved) return undefined;
    const t = setTimeout(() => setSaved(false), SAVED_MS);
    return () => clearTimeout(t);
  }, [saved]);

  // After Save found a problem, the first row it is about is scrolled into sight.
  useEffect(() => {
    if (!problems.length) return;
    const row = badRef.current;
    const content = contentRef.current;
    if (!row || !content) return;
    // Both measured on the screen, so the page's own scroll does not matter.
    content.measureInWindow((_cx, cy) =>
      row.measureInWindow((_rx, ry) => scrollRef.current?.scrollTo({ y: Math.max(0, ry - cy + (wide ? 16 : 4) - PROBLEM_AT), animated: false })),
    );
  }, [problems, wide, heights]);

  const dirty = kept !== null && draft !== null && JSON.stringify(kept) !== JSON.stringify(draft);

  if (!draft || !child || !children) {
    return (
      <Page testID="screen-rules" tabs={<GuardianTabs picked="rules" onPick={onTab} />}>
        <TopRow />
        {err ? <Txt style={[gtype.err, styles.loadErr]}>{err}</Txt> : null}
      </Page>
    );
  }

  const rules = draft;
  const name = child.name;
  const change = (next: RuleSet) => {
    setDraft(next);
    setSaved(false);
    setErr('');
  };
  const undo = () => {
    if (!kept) return;
    setDraft(kept);
    setEach(eachOnItsOwn(kept));
    setProblems([]);
    setErr('');
  };
  const bed = bedtimeOf(rules);
  const setBed = (b: { kind: Bedtime; from: number; to: number }) => {
    const rest = rules.blocked_hours.slice(1);
    const head: BlockedWindow[] = b.kind === 'none' ? [] : [{ days: b.kind === 'every' ? [...WEEKDAYS] : [...SCHOOL_NIGHTS], start_minute: b.from, end_minute: b.to }];
    change({ ...rules, blocked_hours: [...head, ...rest] });
  };
  const setDay = (d: Weekday | null, by: number) => {
    const clamp = (m: number) => ({ minutes: Math.min(MOST_MINUTES, Math.max(0, m + by)) });
    const budgets = { ...rules.budgets };
    if (d === null) for (const w of WEEKDAYS) budgets[w] = clamp(minutesOf(rules, 'monday'));
    else budgets[d] = clamp(minutesOf(rules, d));
    change({ ...rules, budgets });
  };
  const problemsAt = (prefix: string) =>
    problems
      .filter((p) => p.field.startsWith(prefix))
      .map((p) => p.message)
      .join(' ');
  const firstProblem = problems[0];
  const listed = new Set([...rules.blocked_apps, ...rules.always_allowed_apps].map((a) => a.toLowerCase()));
  const deviceKind = (id: string) => child.devices.find((d) => d.deviceId === id)?.kind;
  /** An app by the name a device gave it; its plain id only when no device named it. */
  const nameOf = (id: string) => names.get(id.toLowerCase())?.name ?? id;
  /** Where each device's list is said: "On this PC", "On Asha's phone". */
  const groupOf = (deviceId: string, deviceName: string) => (deviceKind(deviceId) === 'on_master' ? W.rules.onThisPc : W.rules.onDevice(deviceName));
  /** Apps used today on a device, not in either list: offered to block. */
  const offerOn = (deviceId: string) =>
    [...new Set((usage.find((u) => u.deviceId === deviceId)?.apps ?? []).map((a) => a.appId).filter((a) => a !== '' && !listed.has(a.toLowerCase())))];
  // The drawing offers once: under the first device whose use today has something to offer.
  const offerDevice = child.devices.find((d) => offerOn(d.deviceId).length)?.deviceId ?? null;
  let badPlaced = false;

  /** One list, under each of the child's devices whose list has the app; apps no device names, after them. */
  const appGroups = (list: AppList, afterHead: boolean) => {
    const rows = rules[list].map((app, i) => ({ app, i, on: names.get(app.toLowerCase())?.devices.map((d) => d.deviceId) ?? [] }));
    const row = (r: (typeof rows)[number], first: boolean) => {
      const problem = problems.find((p) => indexIn(p.field, list) === r.i)?.message;
      const isFirst = !badPlaced && firstProblem !== undefined && indexIn(firstProblem.field, list) === r.i;
      if (isFirst) badPlaced = true;
      return (
        <AppRow
          key={`${r.app} ${r.i}`}
          name={nameOf(r.app)}
          first={first}
          problem={problem}
          rowRef={
            isFirst
              ? (v) => {
                  badRef.current = v;
                }
              : undefined
          }
          onOff={() => change({ ...rules, [list]: rules[list].filter((_, k) => k !== r.i) })}
        />
      );
    };
    // Each group's label: 14 under the card's head, 18 after anything else.
    let shown = 0;
    const label = (words: string) => <GroupLabel first={afterHead && shown++ === 0}>{words}</GroupLabel>;
    const groups: ReactNode[] = child.devices.map((d) => {
      const mine = rows.filter((r) => r.on.includes(d.deviceId));
      const phoneRow = list === 'always_allowed_apps' && d.kind === 'portable';
      const offer = list === 'blocked_apps' && offerDevice === d.deviceId ? offerOn(d.deviceId) : [];
      if (!mine.length && !phoneRow && !offer.length) return null;
      return (
        <View key={d.deviceId} testID={`${list === 'blocked_apps' ? 'blocked' : 'allowed'}-on-${d.deviceId}`}>
          {label(groupOf(d.deviceId, d.name))}
          {phoneRow ? <AppRow name={W.rules.phoneApp} first fixed={W.rules.neverBlockedMark} /> : null}
          {mine.map((r, k) => row(r, k === 0 && !phoneRow))}
          {offer.length ? (
            <View style={styles.offer} testID="offer">
              <Label>{W.rules.offer(name)}</Label>
              <View style={styles.chips}>
                {offer.map((app) => (
                  <Chip
                    key={app}
                    add
                    label={nameOf(app)}
                    a11y={`${W.rules.offer(name)}: ${nameOf(app)}`}
                    on={false}
                    onPress={() => change({ ...rules, blocked_apps: [...rules.blocked_apps, app] })}
                    testID={`offer-${nameOf(app)}`}
                  />
                ))}
              </View>
            </View>
          ) : null}
        </View>
      );
    });
    const nowhere = rows.filter((r) => !r.on.some((id) => child.devices.some((d) => d.deviceId === id)));
    if (nowhere.length) {
      groups.push(
        <View key="nowhere" testID={`${list === 'blocked_apps' ? 'blocked' : 'allowed'}-on-nowhere`}>
          {label(W.rules.onNoDevice)}
          {nowhere.map((r, k) => row(r, k === 0))}
        </View>,
      );
    }
    return groups;
  };

  const cards: { key: string; node: ReactNode }[] = [
    {
      key: 'time',
      node: (
        <Card testID="time-a-day">
          <Head2 icon="calendar" title={W.rules.timeADay} />
          <View style={[styles.chips, styles.first]} accessibilityRole="radiogroup">
            <Chip
              label={W.timeADay.every}
              on={!each}
              onPress={() => {
                setEach(false);
                setDay(null, 0);
              }}
              testID="every-day"
            />
            <Chip label={W.timeADay.each} on={each} onPress={() => setEach(true)} testID="each-day" />
          </View>
          {each ? (
            <View style={[styles.days, wide && styles.daysWide]}>
              <Dashed />
              {WEEKDAYS.map((d, i) => (
                <View key={d}>
                  {i > 0 ? <Dashed /> : null}
                  <View style={styles.day}>
                    <Txt style={gtype.day}>{W.DAY_WORDS[d]}</Txt>
                    <Stepper value={minutesFigure(minutesOf(rules, d))} name={W.DAY_WORDS[d]} onLess={() => setDay(d, -STEP_MINUTES)} onMore={() => setDay(d, STEP_MINUTES)} />
                  </View>
                </View>
              ))}
            </View>
          ) : (
            <View style={styles.one}>
              <Dashed />
              <View style={styles.oneIn}>
                <Stepper big value={minutesFigure(minutesOf(rules, 'monday'))} name={W.timeADay.every} onLess={() => setDay(null, -STEP_MINUTES)} onMore={() => setDay(null, STEP_MINUTES)} />
              </View>
            </View>
          )}
          {problemsAt('budgets') ? <Txt style={[gtype.err, styles.problem]}>{problemsAt('budgets')}</Txt> : null}
        </Card>
      ),
    },
    {
      key: 'bed',
      node: (
        <Card testID="bedtime">
          <Head2 icon="moon" title={W.rules.bedtime} />
          <View style={[styles.chips, styles.first]} accessibilityRole="radiogroup" accessibilityLabel={W.bedtime.when}>
            {(
              [
                ['every', W.bedtime.every],
                ['school', W.bedtime.school],
                ['none', W.bedtime.none],
              ] as const
            ).map(([key, label]) => (
              <Chip key={key} label={label} on={bed.kind === key} onPress={() => setBed({ ...bed, kind: key })} testID={`bedtime-${key}`} />
            ))}
          </View>
          <View style={styles.times}>
            {(
              [
                ['from', W.bedtime.from, bed.from],
                ['to', W.bedtime.to, bed.to],
              ] as const
            ).map(([key, label, minute]) => (
              <View key={key} style={styles.flex}>
                <Label>{label}</Label>
                <PickField label={label} value={minuteOfDay(minute, clock)} off={bed.kind === 'none'} open={picking === key} onPress={() => setPicking(picking === key ? null : key)} testID={`bedtime-${key}-field`} />
              </View>
            ))}
          </View>
          {picking === 'from' || picking === 'to' ? (
            <TimeList
              onPick={(m) => {
                setBed(picking === 'from' ? { ...bed, from: m } : { ...bed, to: m });
                setPicking(null);
              }}
            />
          ) : null}
          <Room words={problemsAt('blocked_hours')} testID="bedtime-message" />
        </Card>
      ),
    },
    {
      key: 'away',
      node: (
        <Card testID="away">
          <Head2 icon="away" title={W.rules.away} />
          <Txt style={[gtype.detail, styles.detail]} wrap="pretty">
            {W.rules.awayLine(name, rules.away_share_percent)}
          </Txt>
          <View style={[styles.chips, styles.first]} accessibilityRole="radiogroup" accessibilityLabel={W.rules.away}>
            {W.rules.awayChoices.map(([p, label]) => (
              <Chip key={p} label={label} on={rules.away_share_percent === p} onPress={() => change({ ...rules, away_share_percent: p })} testID={`away-${p}`} />
            ))}
          </View>
          {problemsAt('away_share_percent') ? <Txt style={[gtype.err, styles.problem]}>{problemsAt('away_share_percent')}</Txt> : null}
        </Card>
      ),
    },
    {
      key: 'blocked',
      node: (
        <Card testID="blocked">
          <Head2 icon="block" title={W.rules.blocked} />
          {appGroups('blocked_apps', true)}
          <Btn label={W.rules.addApp} mono lead={Mark.plus()} onPress={() => setAdding(adding === 'blocked_apps' ? null : 'blocked_apps')} outer={styles.addApp} testID="add-blocked" />
          {adding === 'blocked_apps' ? <AddApp door={door} child={child} list={rules.blocked_apps} onAdd={(id) => change({ ...rules, blocked_apps: [...rules.blocked_apps, id] })} /> : null}
        </Card>
      ),
    },
    {
      key: 'allowed',
      node: (
        <Card testID="allowed">
          <Head2 icon="star" title={W.rules.allowed} />
          <Txt style={[gtype.detail, styles.detail]} wrap="pretty">
            {W.rules.allowedLine(name)}
          </Txt>
          {appGroups('always_allowed_apps', false)}
          <Txt style={[gtype.small, styles.small]} wrap="pretty">
            {W.rules.neverBlocked}
          </Txt>
          <Btn label={W.rules.addApp} mono lead={Mark.plus()} onPress={() => setAdding(adding === 'always_allowed_apps' ? null : 'always_allowed_apps')} outer={styles.addApp} testID="add-allowed" />
          {adding === 'always_allowed_apps' ? (
            <AddApp door={door} child={child} list={rules.always_allowed_apps} onAdd={(id) => change({ ...rules, always_allowed_apps: [...rules.always_allowed_apps, id] })} />
          ) : null}
        </Card>
      ),
    },
    {
      key: 'use',
      node: (
        <Card testID="use">
          <Head2 icon="chart" title={W.rules.use} />
          {usage
            .filter((d) => !d.removed)
            .map((d, i) => (
              <View key={d.deviceId}>
                <GroupLabel first={i === 0}>{`${deviceKind(d.deviceId) === 'on_master' ? W.rules.onThisPc : W.rules.onDevice(d.name)} · ${usedSpan(d.secs)}`}</GroupLabel>
                {d.apps
                  .filter((a) => a.appId !== '')
                  .map((a, j) => (
                    <View key={a.appId}>
                      {j > 0 ? <Dashed /> : null}
                      <View style={styles.urow} accessible accessibilityLabel={`${nameOf(a.appId)}, ${usedSpan(a.secs)}`}>
                        <Letter name={nameOf(a.appId)} small />
                        <Txt style={[gtype.useName, styles.flex]}>{nameOf(a.appId)}</Txt>
                        <Txt style={gtype.minutes}>{usedSpan(a.secs)}</Txt>
                      </View>
                    </View>
                  ))}
              </View>
            ))}
        </Card>
      ),
    },
    {
      key: 'more',
      node: (
        <Card testID="more-for-child">
          <Head2 icon="gear" title={W.rules.more(name)} />
          <Kvs
            rows={[
              {
                k: W.rules.dayStarts,
                v: rules.day_start_minute === 0 ? W.rules.atMidnight : W.rules.at(minuteOfDay(rules.day_start_minute, clock)),
                after: <Btn label={W.sheet.change} mono onPress={() => setPicking(picking === 'day' ? null : 'day')} testID="change-day-start" />,
              },
              {
                k: W.rules.nameAndAge,
                v: `${name}, ${W.AGE_WORDS[child.ageBand].replace('Under', 'under')}`,
                after: <Btn label={W.sheet.change} mono onPress={() => setEditName(!editName)} testID="change-name" />,
              },
            ]}
          />
          {picking === 'day' ? (
            <TimeList
              onPick={(m) => {
                change({ ...rules, day_start_minute: m });
                setPicking(null);
              }}
            />
          ) : null}
          {problemsAt('day_start_minute') ? <Txt style={[gtype.err, styles.problem]}>{problemsAt('day_start_minute')}</Txt> : null}
          {editName ? (
            <NameAndAge
              start={{ name, band: child.ageBand }}
              onSave={async (n, band) => {
                if (n !== name) {
                  const got = await ask(door, 'rename_child', { child: child.childId, name: n });
                  if (got.kind !== 'done') return said(got);
                }
                if (band !== child.ageBand) {
                  const got = await ask(door, 'set_age_band', { child: child.childId, age_band: band });
                  if (got.kind !== 'done') return said(got);
                }
                await loadChildren();
                if (live.current) setEditName(false);
                return '';
              }}
            />
          ) : null}
          <Btn label={W.rules.remove(name)} look="coral" onPress={() => onRemove(child)} outer={styles.gap} testID="remove-child" />
          <Txt style={[gtype.small, styles.small]}>{W.rules.asksAgain}</Txt>
        </Card>
      ),
    },
  ];

  // Two columns at the console's width: the cards in order, the first
  // column filled until the taller of the two is as short as it can be.
  const colWidth = (width - 64 - 22) / 2;
  let split = Math.ceil(cards.length / 2);
  if (wide && heights.length === cards.length && heights.every((h) => h > 0)) {
    const total = heights.reduce((s, h) => s + h, 0);
    let run = 0;
    let best = { at: cards.length, worst: Infinity };
    for (let k = 1; k < cards.length; k += 1) {
      run += heights[k - 1];
      const worst = Math.max(run, total - run);
      if (worst < best.worst) best = { at: k, worst };
    }
    split = best.at;
  }
  const measured = (i: number) => (e: { nativeEvent: { layout: { height: number } } }) => {
    const h = e.nativeEvent.layout.height;
    setHeights((old) => {
      if (old[i] === h) return old;
      const next = [...old];
      next[i] = h;
      return next;
    });
  };
  const placed = (from: number, to: number) =>
    cards.slice(from, to).map((c, k) => (
      <View key={c.key} onLayout={measured(from + k)} style={wide ? styles.cellWide : null}>
        {c.node}
      </View>
    ));

  const message = problems.length ? W.rules.notSaved(problems.length) : err;
  const strip = dirty
    ? {
        node: (
          <>
            <Txt style={[gtype.err, wide ? styles.flex : null]} numberOfLines={wide ? 2 : 1} testID="not-saved" accessibilityLiveRegion="polite">
              {message}
            </Txt>
            <View style={styles.pinRow}>
              <Btn label={W.rules.undo} onPress={undo} height={field} padding={wide ? 28 : 16} outer={wide ? styles.pinBtnWide : styles.flex} testID="undo" />
              <Btn label={W.rules.save} look="go" off={busy} onPress={() => void save(child.childId, rules)} height={field} padding={wide ? 28 : 16} outer={wide ? styles.pinBtnWide : styles.flex} testID="save" />
            </View>
          </>
        ),
        height: { phone: 96, wide: 68 },
        full: true,
      }
    : undefined;

  return (
    <Page testID="screen-rules" scrollRef={scrollRef} strip={strip} tabs={dirty ? undefined : <GuardianTabs picked="rules" onPick={onTab} />} over={saved && !dirty ? <Saved name={name} /> : undefined}>
      <View ref={contentRef}>
        <TopRow
          right={
            <View style={styles.who} accessibilityRole="radiogroup">
              {children.map((c) => (
                <Chip
                  key={c.childId}
                  label={c.name}
                  on={c.childId === childId}
                  onPress={() => {
                    if (c.childId === childId) return;
                    if (dirty) return setErr(W.rules.saveFirst);
                    setChildId(c.childId);
                  }}
                  testID={`child-${c.childId}`}
                />
              ))}
            </View>
          }
        />
        {err && !dirty ? <Txt style={[gtype.err, styles.loadErr]}>{err}</Txt> : null}
        <View style={[styles.cols, wide && styles.colsWide]}>
          {wide ? (
            <>
              <View style={{ width: colWidth }}>{placed(0, split)}</View>
              <View style={{ width: colWidth }}>{placed(split, cards.length)}</View>
            </>
          ) : (
            placed(0, cards.length)
          )}
        </View>
      </View>
    </Page>
  );
}

const styles = StyleSheet.create({
  flex: { flex: 1, minWidth: 0 },
  loadErr: { marginTop: 8, marginHorizontal: 2 },
  who: { flexDirection: 'row', gap: 8 },
  cols: { gap: 14, marginTop: 10 },
  colsWide: { flexDirection: 'row', gap: 22, marginTop: 14, alignItems: 'flex-start' },
  cellWide: { marginRight: 4, marginBottom: 18 },
  chips: { flexDirection: 'row', flexWrap: 'wrap', gap: 8, marginTop: 8 },
  first: { marginTop: 14 },
  even: { flexWrap: 'nowrap' },
  evenChip: { flex: 1, paddingHorizontal: 2 },
  detail: { marginTop: 10 },
  small: { marginTop: 12, marginHorizontal: 2 },
  gap: { marginTop: 14 },
  gapLabel: { marginTop: 18 },
  days: { marginTop: 14 },
  daysWide: { marginTop: 12 },
  day: { flexDirection: 'row', alignItems: 'center', justifyContent: 'space-between', gap: 12, paddingVertical: 2 },
  one: { marginTop: 22 },
  oneIn: { alignItems: 'center', paddingTop: 14, paddingBottom: 6 },
  times: { flexDirection: 'row', gap: 12, marginTop: 18 },
  picker: { marginTop: 8, maxHeight: 220, overflow: 'scroll', borderWidth, borderColor: color.ink, borderRadius: 10, backgroundColor: color.card },
  pickerRow: { minHeight: 44, justifyContent: 'center', paddingHorizontal: 14 },
  pickerWords: { textAlign: 'left' },
  list: { marginTop: 10 },
  app: { flexDirection: 'row', alignItems: 'center', gap: 12, paddingVertical: 5 },
  appBad: { marginTop: 4, paddingTop: 5, paddingBottom: 5, paddingLeft: 8, paddingRight: 5, borderWidth, borderColor: color.coral, borderRadius: 10, backgroundColor: 'rgba(207,67,38,0.06)' },
  problem: { marginTop: 6, marginBottom: 4, marginHorizontal: 2 },
  offer: { marginTop: 10, paddingTop: 10, paddingBottom: 12, paddingHorizontal: 12, borderWidth, borderStyle: 'dashed', borderColor: 'rgba(27,38,56,0.42)', borderRadius: 10 },
  addApp: { marginTop: 10 },
  adding: { marginTop: 4 },
  pickList: { marginTop: 6, maxHeight: 320, overflow: 'scroll' },
  urow: { flexDirection: 'row', alignItems: 'center', gap: 12, paddingVertical: 6 },
  edit: { marginTop: 14 },
  pinRow: { flexDirection: 'row', gap: 10 },
  pinBtnWide: { minWidth: 180 },
  saved: { position: 'absolute', zIndex: 5 },
  savedIn: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 12,
    paddingTop: 10,
    paddingBottom: 10,
    paddingLeft: 10,
    paddingRight: 14,
    backgroundColor: color.card,
    borderWidth,
    borderColor: color.ink,
    borderRadius: 14,
  },
  savedTile: { width: 40, height: 40, borderRadius: 8, alignItems: 'center', justifyContent: 'center', backgroundColor: color.paper2 },
});
