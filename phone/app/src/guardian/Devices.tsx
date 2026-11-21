// D, Devices, as design/screens/console.js draws it (D1 and D2): each
// child's phone in a card of its own, and this PC with its Windows
// accounts and its two settings. On a phone one column, the phones first;
// at the console's width this PC at the left and the phones at the right.
//
// A phone's card says only what the door says of it: whose it is, what it
// said it is (its model and Android, and LiMo's version on it; nothing while
// it has said nothing), when it was paired, when it was last heard, whether it
// is in use, its protection, each of its six setup steps (left out when
// nothing is known of how it stands), whether USB debugging is still on, and
// how far its clock is off. The door has no home and away for a phone, so
// that drawn row is left out, and switching USB debugging off is done over
// the cable, in Add a phone. The family's state is
// asked again every 5 seconds while Devices is in front, as on Today.

import { type ReactNode, useCallback, useEffect, useRef, useState } from 'react';
import { AppState, StyleSheet, View } from 'react-native';

import {
  type AccountRow,
  ask,
  type Asked,
  type ChildToday,
  type DeviceInfo,
  type DeviceToday,
  type GuardianDoor,
  type PcSettings,
  type PhoneStep,
} from '../guardianSeam';
import { Icon } from '../icons/Icon';
import { borderWidth, color } from '../theme';
import { Dashed } from '../ui/paper/parts';
import { Txt } from '../ui/paper/text';
import { useClock } from './clock';
import { GuardianTabs, type TabKey } from './GuardianTabs';
import { Page } from './Page';
import { Btn, Card, Chip, Field, GroupLabel, Head2, Kvs, Label, Mark, Room, Stepper, Tile, TopRow, useSizes } from './parts';
import { gtype } from './text';
import { dateLine, heardAgo, minutesInSentence, shortDate } from './time';
import { ASK_AGAIN_MS } from './Today';
import * as W from './words';

/** A device heard within this long was heard "just now". */
const JUST_HEARD_SECS = 60;
/** A clock off by less than this is not said. */
const CLOCK_SAID_FROM_SECS = 60;
const MIN_IDLE = 1;
const MAX_IDLE = 60;

/** A warning in coral: an alert, the point in bold, and what it means. */
function Warn({ title, text, testID }: { title: string; text: string; testID?: string }) {
  return (
    <View style={styles.warn} testID={testID}>
      <View style={styles.warnTop}>
        <Icon name="alert" size={24} />
        <Txt style={[gtype.warn, styles.flex]} wrap="pretty">
          <Txt style={[gtype.warn, styles.bold]}>{title}</Txt> {text}
        </Txt>
      </View>
    </View>
  );
}

/** The six steps of a phone's setup: a teal tick when on; a hollow ring and "Not on" in coral when off, or "Not yet" while no note said. */
export function Six({ steps, top = 10 }: { steps: readonly PhoneStep[]; top?: number }) {
  return (
    <View style={{ marginTop: top }} testID="six">
      {steps.map(({ permission: p, on }, i) => {
        const word = on === true ? W.devices.on : on === false ? W.devices.notOn : W.devices.notYet;
        return (
          <View key={p}>
            {i > 0 ? <Dashed /> : null}
            <View style={[styles.ok, i > 0 && styles.okAfter]} accessible accessibilityLabel={`${W.devices.six[p]}: ${word}`} testID={`six-${p}`}>
              {on === true ? Mark.tick() : Mark.ring()}
              <Txt style={[gtype.ok, styles.flex]}>{W.devices.six[p]}</Txt>
              <Txt style={[gtype.okState, { color: on === true ? color.teal : on === false ? color.coral : color.inkSoft }]}>{word}</Txt>
            </View>
          </View>
        );
      })}
    </View>
  );
}

function PhoneCard({
  door,
  child,
  device,
  info,
  onRemove,
  onChanged,
  said,
}: {
  door: GuardianDoor;
  child: ChildToday;
  device: DeviceToday;
  info: DeviceInfo | undefined;
  onRemove: () => void;
  onChanged: () => Promise<void>;
  said: (o: Exclude<Asked<unknown>, { kind: 'done' }>) => string;
}) {
  const { now: nowOf, clock } = useClock();
  const now = nowOf();
  const [renaming, setRenaming] = useState(false);
  const [name, setName] = useState(device.name);
  const [err, setErr] = useState('');
  const [busy, setBusy] = useState(false);
  const rows: { k: string; v: string; teal?: boolean }[] = [{ k: W.devices.child, v: child.name }];
  // What the phone said it is: its model and Android; nothing while it has said nothing.
  const facts = device.phoneFacts;
  const what = [facts?.model, facts && facts.android !== null ? W.devices.android(facts.android) : null].filter((x): x is string => Boolean(x));
  if (what.length) rows.push({ k: W.devices.phone, v: what.join(', ') });
  if (info) rows.push({ k: W.devices.paired, v: shortDate(info.pairedAt, clock) });
  if (device.lastHeardAt !== null) {
    rows.push({ k: W.devices.lastHeard, v: now - device.lastHeardAt < JUST_HEARD_SECS ? W.devices.justNow : heardAgo(now, device.lastHeardAt, clock) });
  }
  if (device.inUse) rows.push({ k: W.devices.now, v: W.devices.inUse, teal: true });
  const protection = device.standing?.protection;
  if (protection) rows.push({ k: W.devices.protection, v: W.devices.protectionLevel[protection], teal: protection !== 'off' });
  if (facts?.limoVersion) rows.push({ k: W.devices.limoOnIt, v: facts.limoVersion });
  const off = device.clockOffBySecs;
  const clockSaid = off !== null && Math.abs(off) >= CLOCK_SAID_FROM_SECS;
  const span = minutesInSentence(Math.floor(Math.abs(off ?? 0) / 60));

  const rename = async () => {
    setBusy(true);
    const got = await ask(door, 'rename_device', { device: device.deviceId, name: name.trim() });
    setBusy(false);
    if (got.kind !== 'done') return setErr(said(got));
    setErr('');
    setRenaming(false);
    await onChanged();
  };

  return (
    <Card testID={`phone-${device.deviceId}`}>
      <Head2
        icon="phone"
        title={device.name}
        right={
          <Btn
            label={W.devices.rename}
            mono
            onPress={() => {
              setName(device.name);
              setErr('');
              setRenaming(!renaming);
            }}
            testID="rename"
          />
        }
      />
      {renaming ? (
        <View style={styles.rename} testID="renaming">
          <Label>{W.devices.itsName}</Label>
          <Field testID="device-name" value={name} onChangeText={setName} maxLength={40} accessibilityLabel={W.devices.itsName} />
          <Room words={err} testID="rename-message" />
          <Btn label={W.devices.saveName} look="go" off={busy || name.trim() === ''} onPress={() => void rename()} testID="save-device-name" />
        </View>
      ) : null}
      <Kvs rows={rows} testID="phone-facts" />
      {device.standing ? (
        <>
          <GroupLabel>{W.devices.needs}</GroupLabel>
          <Six steps={device.standing.steps} />
        </>
      ) : null}
      {device.standing?.usbDebuggingOn ? (
        <View style={styles.gap}>
          <Warn title={W.devices.usbTitle} text={W.devices.usbText} testID="usb-warning" />
        </View>
      ) : null}
      {clockSaid ? (
        <View style={styles.gap}>
          <Warn title={(off ?? 0) > 0 ? W.devices.clockAhead(span) : W.devices.clockBehind(span)} text={W.devices.clockLine} testID="clock-warning" />
        </View>
      ) : null}
      <Btn label={W.devices.removePhone} look="coral" onPress={onRemove} outer={styles.gap} testID="remove-phone" />
      <Txt style={[gtype.small, styles.small]} wrap="pretty">
        {W.devices.removeLine}
      </Txt>
    </Card>
  );
}

/** One Windows account, whose it is, and Change, which opens the choices under it. */
function Account({
  row,
  children,
  onPick,
  busy,
}: {
  row: AccountRow;
  children: readonly ChildToday[];
  onPick: (pick: { child: string } | 'not_limited' | 'none') => void;
  busy: boolean;
}) {
  const [open, setOpen] = useState(false);
  const kind = row.isAdministrator ? W.accounts.admin : W.accounts.standard;
  const facts = row.signedIn ? `${kind} · ${W.accounts.signedInNow}` : kind;
  const whose =
    row.link.kind === 'child'
      ? W.possessive(children.find((c) => row.link.kind === 'child' && c.childId === row.link.childId)?.name ?? row.link.childName ?? '')
      : row.link.kind === 'not_limited'
        ? W.devices.guardians
        : W.devices.notSaid;
  const picked = row.link.kind === 'child' ? row.link.childId : row.link.kind;
  const choices: { key: string; label: string; pick: { child: string } | 'not_limited' | 'none' }[] = [
    ...children.map((c) => ({ key: c.childId, label: W.possessive(c.name), pick: { child: c.childId } })),
    { key: 'not_limited', label: W.devices.guardiansChoice, pick: 'not_limited' as const },
    { key: 'none', label: W.devices.notSaidChoice, pick: 'none' as const },
  ];
  return (
    <View testID={`account-${row.name}`}>
      <View style={styles.acc}>
        <Tile icon="account" />
        <View style={styles.flex} accessible accessibilityLabel={`${row.displayName}, ${facts}, ${whose}`}>
          <Txt style={gtype.rowTitle}>{row.displayName}</Txt>
          <Txt style={[gtype.accFacts, styles.accFacts]}>{facts}</Txt>
          <Txt style={[gtype.whose, styles.whose]}>{whose}</Txt>
        </View>
        <Btn label={W.sheet.change} mono onPress={() => setOpen(!open)} testID={`change-${row.name}`} />
      </View>
      {open ? (
        <View style={styles.accChips} accessibilityRole="radiogroup" accessibilityLabel={row.displayName}>
          {choices.map((c) => (
            <Chip
              key={c.key}
              label={c.label}
              a11y={`${row.displayName}: ${c.label}`}
              on={picked === c.key}
              onPress={() => {
                if (busy) return;
                setOpen(false);
                if (picked !== c.key) onPick(c.pick);
              }}
              testID={`account-${row.name}-${c.key}`}
            />
          ))}
        </View>
      ) : null}
    </View>
  );
}

export function Devices({
  door,
  onTab,
  onRemovePhone,
  onAddPhone,
  onSignedOut,
  onRefused,
}: {
  door: GuardianDoor;
  onTab: (tab: TabKey) => void;
  onRemovePhone: (device: DeviceToday) => void;
  onAddPhone: () => void;
  onSignedOut: () => void;
  onRefused: () => void;
}) {
  const { now: nowOf, clock } = useClock();
  const now = nowOf();
  const { wide } = useSizes();
  const [today, setToday] = useState<readonly ChildToday[] | null>(null);
  const [infos, setInfos] = useState<ReadonlyMap<string, DeviceInfo>>(new Map());
  const [accounts, setAccounts] = useState<readonly AccountRow[]>([]);
  const [settings, setSettings] = useState<PcSettings | null>(null);
  const [err, setErr] = useState('');
  const [pcErr, setPcErr] = useState('');
  const [busy, setBusy] = useState(false);
  const live = useRef(true);
  const known = useRef('');

  const said = useCallback(
    (o: Exclude<Asked<unknown>, { kind: 'done' }>): string => {
      if (o.kind === 'not-done' && o.reason === 'signed_out') onSignedOut();
      if (o.kind === 'refused') onRefused();
      return o.words;
    },
    [onSignedOut, onRefused],
  );

  const loadToday = useCallback(async () => {
    const got = await ask(door, 'family_today', {});
    if (!live.current) return;
    if (got.kind !== 'done') return setErr(said(got));
    setErr('');
    setToday(got.value);
    // When each device was paired: asked again only when the devices change.
    const ids = got.value.map((c) => `${c.childId}:${c.devices.map((d) => d.deviceId).join(',')}`).join(';');
    if (ids === known.current) return;
    known.current = ids;
    const lists = await Promise.all(got.value.map((c) => ask(door, 'devices', { child: c.childId })));
    if (!live.current) return;
    setInfos(new Map(lists.flatMap((l) => (l.kind === 'done' ? l.value.map((d) => [d.deviceId, d] as const) : []))));
  }, [door, said]);

  const loadPc = useCallback(async () => {
    const [acc, set] = await Promise.all([ask(door, 'accounts', {}), ask(door, 'pc_settings', {})]);
    if (!live.current) return;
    if (acc.kind === 'done') setAccounts(acc.value);
    else setPcErr(said(acc));
    if (set.kind === 'done') setSettings(set.value);
    else setPcErr(said(set));
  }, [door, said]);

  useEffect(() => {
    live.current = true;
    void loadToday();
    void loadPc();
    let timer: ReturnType<typeof setInterval> | null = setInterval(() => void loadToday(), ASK_AGAIN_MS);
    const sub = AppState.addEventListener('change', (state) => {
      if (state === 'active') {
        if (!timer) {
          void loadToday();
          timer = setInterval(() => void loadToday(), ASK_AGAIN_MS);
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
  }, [loadToday, loadPc]);

  const changeSettings = async (got: Promise<Asked<PcSettings>>) => {
    setBusy(true);
    const a = await got;
    if (!live.current) return;
    setBusy(false);
    if (a.kind === 'done') {
      setSettings(a.value);
      setPcErr('');
    } else setPcErr(said(a));
  };

  const link = async (row: AccountRow, pick: { child: string } | 'not_limited' | 'none') => {
    setBusy(true);
    const got =
      pick === 'none'
        ? await ask(door, 'unlink_account', { account: row.accountId })
        : await ask(door, 'link_account', { account: row.accountId, link: pick === 'not_limited' ? 'not_limited' : { child: { child_id: pick.child } } });
    if (!live.current) return;
    setBusy(false);
    if (got.kind !== 'done') return setPcErr(said(got));
    setPcErr('');
    await loadPc();
  };

  const children = today ?? [];
  const phones = children.flatMap((c) => c.devices.filter((d) => d.kind === 'portable').map((d) => ({ child: c, device: d })));

  const phoneStack: ReactNode = (
    <View style={styles.stack}>
      {phones.map(({ child, device }) => (
        <PhoneCard
          key={device.deviceId}
          door={door}
          child={child}
          device={device}
          info={infos.get(device.deviceId)}
          onRemove={() => onRemovePhone(device)}
          onChanged={loadToday}
          said={said}
        />
      ))}
      <Btn label={W.devices.addPhone} lead={Mark.plus()} onPress={onAddPhone} testID="add-phone" />
    </View>
  );

  const idle = settings?.idleMinutes ?? null;
  const pcCard: ReactNode = (
    <View style={styles.stack}>
      <Card testID="this-pc">
        <Head2 icon="pc" title={W.devices.thisPc} />
        <GroupLabel first>{W.devices.accounts}</GroupLabel>
        <View>
          {accounts.map((row, i) => (
            <View key={row.accountId}>
              {i > 0 ? <Dashed /> : null}
              <Account row={row} children={children} busy={busy} onPick={(pick) => void link(row, pick)} />
            </View>
          ))}
        </View>
        <GroupLabel>{W.devices.pause}</GroupLabel>
        {idle !== null ? (
          <View style={styles.stepline}>
            <Stepper
              value={`${idle} min`}
              name={W.devices.pause}
              onLess={() => (busy || idle <= MIN_IDLE ? undefined : void changeSettings(ask(door, 'set_idle_minutes', { minutes: idle - 1 })))}
              onMore={() => (busy || idle >= MAX_IDLE ? undefined : void changeSettings(ask(door, 'set_idle_minutes', { minutes: idle + 1 })))}
            />
          </View>
        ) : null}
        <Txt style={[gtype.small, styles.small]} wrap="pretty">
          {W.devices.pauseLine}
        </Txt>
        <GroupLabel>{W.devices.unlinked}</GroupLabel>
        <View style={styles.chips} accessibilityRole="radiogroup" accessibilityLabel={W.devices.unlinked}>
          {(
            [
              ['locked', W.devices.locked],
              ['free', W.devices.free],
            ] as const
          ).map(([key, label]) => (
            <Chip
              key={key}
              label={label}
              on={settings?.unlinkedAccounts === key}
              onPress={() => (busy || settings?.unlinkedAccounts === key ? undefined : void changeSettings(ask(door, 'set_unlinked_accounts', { unlinked: key })))}
              testID={`unlinked-${key}`}
            />
          ))}
        </View>
        <Txt style={[gtype.small, styles.small]} wrap="pretty">
          {settings?.unlinkedAccounts === 'free' ? W.devices.freeLine : W.devices.lockedLine}
        </Txt>
        {pcErr ? <Txt style={[gtype.err, styles.problem]}>{pcErr}</Txt> : null}
      </Card>
    </View>
  );

  return (
    <Page testID="screen-devices" tabs={<GuardianTabs picked="devices" onPick={onTab} />}>
      <TopRow right={<Label>{dateLine(now, clock)}</Label>} />
      {err ? <Txt style={[gtype.err, styles.loadErr]}>{err}</Txt> : null}
      {today ? (
        <View style={[styles.devs, wide && styles.devsWide]}>
          {wide ? (
            <>
              <View style={styles.flex}>{pcCard}</View>
              <View style={styles.flex}>{phoneStack}</View>
            </>
          ) : (
            <>
              {phoneStack}
              {pcCard}
            </>
          )}
        </View>
      ) : null}
    </Page>
  );
}

const styles = StyleSheet.create({
  flex: { flex: 1, minWidth: 0 },
  bold: { fontWeight: '600' },
  loadErr: { marginTop: 8, marginHorizontal: 2 },
  detail: { marginTop: 10 },
  devs: { gap: 14, marginTop: 10 },
  devsWide: { flexDirection: 'row', gap: 22, marginTop: 14, alignItems: 'flex-start' },
  stack: { gap: 14 },
  rename: { marginTop: 10 },
  ok: { flexDirection: 'row', alignItems: 'center', gap: 10, minHeight: 36 },
  okAfter: { minHeight: 34.5 },
  gap: { marginTop: 14 },
  small: { marginTop: 12, marginHorizontal: 2 },
  problem: { marginTop: 10, marginHorizontal: 2 },
  warn: { paddingVertical: 12, paddingHorizontal: 14, borderWidth, borderColor: color.coral, borderRadius: 10, backgroundColor: 'rgba(207,67,38,0.06)' },
  warnTop: { flexDirection: 'row', alignItems: 'flex-start', gap: 10 },
  acc: { flexDirection: 'row', alignItems: 'center', gap: 12, paddingVertical: 8 },
  accFacts: { marginTop: 4 },
  whose: { marginTop: 3 },
  accChips: { flexDirection: 'row', flexWrap: 'wrap', gap: 8, paddingBottom: 10 },
  stepline: { flexDirection: 'row', marginTop: 4 },
  chips: { flexDirection: 'row', flexWrap: 'wrap', gap: 8, marginTop: 4 },
});
