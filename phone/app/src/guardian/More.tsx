// More, as design/screens/console.js draws it: the list (M1), a one time
// code (K1 and K2), the password and the PIN (M2), About (M3), and the
// screens that ask for the password again before something that cannot be
// undone (M4 draws deleting the family; removing a child, a phone or LiMo
// itself are made the same way). Each opens under the tabs with Back at its
// top, and says where Back goes.

import { type ReactNode, useCallback, useEffect, useRef, useState } from 'react';
import { Pressable, StyleSheet, View } from 'react-native';

import { ask, type Asked, type ChildToday, type DeviceToday, type GuardianDoor, type OneTimeCode } from '../guardianSeam';
import { Dashed } from '../ui/paper/parts';
import { Txt } from '../ui/paper/text';
import { useClock } from './clock';
import { GuardianTabs, type TabKey } from './GuardianTabs';
import { Column, Page } from './Page';
import { Btn, Card, CardHead, Chip, Field, Head2, Kvs, Label, MakerMark, Mark, Room, SubHead, Tile, TopRow, useSizes, useWide } from './parts';
import { gtype } from './text';
import { dateLine, laterInSentence } from './time';
import * as W from './words';

export type MoreScreen = (typeof W.more.rows)[number]['key'];

type Said = (o: Exclude<Asked<unknown>, { kind: 'done' }>) => string;

/** What the door said when it did not do it, as a sentence; a session ended or a refusal also leaves the screen. */
export function useSaid(onSignedOut: () => void, onRefused: () => void): Said {
  const { now, clock } = useClock();
  return useCallback(
    (o) => {
      if (o.kind === 'not-done' && o.reason === 'signed_out') onSignedOut();
      if (o.kind === 'refused') onRefused();
      if (o.kind === 'wait') return W.confirm.wait(laterInSentence(now(), o.until, 'from', clock));
      if (o.kind === 'wrong' && o.waitUntil !== null) return W.confirm.wait(laterInSentence(now(), o.waitUntil, 'from', clock));
      return o.words;
    },
    [onSignedOut, onRefused, now, clock],
  );
}

/** A screen under More: today's date at the top, Back, and the screen, in a column of reading width or across the page. */
function Under({ to, onBack, onTab, children, across = false, end, testID }: { to: string; onBack: () => void; onTab: (tab: TabKey) => void; children: ReactNode; across?: boolean; end?: ReactNode; testID: string }) {
  const { now: nowOf, clock } = useClock();
  const now = nowOf();
  const inner = (
    <>
      <SubHead to={to} onBack={onBack} />
      {children}
    </>
  );
  return (
    <Page testID={testID} tabs={<GuardianTabs picked="more" onPick={onTab} />}>
      <TopRow right={<Label>{dateLine(now, clock)}</Label>} />
      {across ? inner : <Column>{inner}</Column>}
      {end ?? null}
    </Page>
  );
}

// ---------------------------------------------------------------------------
// M1, the list

export function MoreList({ onOpen, onTab }: { onOpen: (screen: MoreScreen) => void; onTab: (tab: TabKey) => void }) {
  const { now: nowOf, clock } = useClock();
  const now = nowOf();
  const wide = useWide();
  return (
    <Page testID="screen-more" tabs={<GuardianTabs picked="more" onPick={onTab} />}>
      <TopRow right={<Label>{dateLine(now, clock)}</Label>} />
      <View style={styles.moreTop}>
        <Column>
          <Card bare style={wide ? styles.listWide : styles.list}>
            {W.more.rows.map((r, i) => (
              <View key={r.key}>
                {i > 0 ? <Dashed /> : null}
                <Pressable onPress={() => onOpen(r.key)} accessibilityRole="button" accessibilityLabel={`${r.title}. ${r.text}`} style={[styles.li, i > 0 && styles.liAfter]} testID={`more-${r.key}`}>
                  <Tile icon={r.icon} />
                  <View style={styles.flex}>
                    <Txt style={gtype.rowTitle}>{r.title}</Txt>
                    <Txt style={[gtype.rowText, styles.liText]} wrap="pretty">
                      {r.text}
                    </Txt>
                  </View>
                  {Mark.right()}
                </Pressable>
              </View>
            ))}
          </Card>
        </Column>
      </View>
    </Page>
  );
}

// ---------------------------------------------------------------------------
// K1 and K2, a one time code

export function Code({
  door,
  onBack,
  onTab,
  onSignedOut,
  onRefused,
  makeAtStart = false,
}: {
  door: GuardianDoor;
  onBack: () => void;
  onTab: (tab: TabKey) => void;
  onSignedOut: () => void;
  onRefused: () => void;
  /** Make a code with the first choices as soon as they are known: the preview's pictures. */
  makeAtStart?: boolean;
}) {
  const { field } = useSizes();
  const said = useSaid(onSignedOut, onRefused);
  const [children, setChildren] = useState<readonly ChildToday[] | null>(null);
  const [childId, setChildId] = useState<string | null>(null);
  const [deviceId, setDeviceId] = useState<string | null>(null);
  const [minutes, setMinutes] = useState(30);
  const [made, setMade] = useState<OneTimeCode | null>(null);
  const [err, setErr] = useState('');
  const [busy, setBusy] = useState(false);
  const live = useRef(true);

  const firstDevice = (c: ChildToday | undefined) => (c ? (c.devices.find((d) => d.kind === 'portable') ?? c.devices[0])?.deviceId ?? null : null);

  const make = useCallback(
    async (device: string, m: number) => {
      setBusy(true);
      const got = await ask(door, 'make_code', { device, minutes: m });
      if (!live.current) return;
      setBusy(false);
      if (got.kind !== 'done') return setErr(said(got));
      setErr('');
      setMade(got.value);
    },
    [door, said],
  );

  useEffect(() => {
    live.current = true;
    void ask(door, 'family_today', {}).then((got) => {
      if (!live.current) return;
      if (got.kind !== 'done') return setErr(said(got));
      setChildren(got.value);
      const c = got.value[0];
      const d = firstDevice(c);
      setChildId(c?.childId ?? null);
      setDeviceId(d);
      if (makeAtStart && d) void make(d, 30);
    });
    return () => {
      live.current = false;
    };
    // Asked once, when the screen opens.
  }, [door]);

  const child = children?.find((c) => c.childId === childId);
  const device = child?.devices.find((d) => d.deviceId === deviceId);
  const deviceName = (d: DeviceToday) => (d.kind === 'on_master' ? W.devices.thisPc : d.name);
  const howMuch = (m: number) => W.code.howMuchChoices.find(([n]) => n === m)?.[1] ?? `${m} min`;

  let card: ReactNode;
  if (made && child && device) {
    const groups = made.shown.split(' ');
    card = (
      <Card testID="the-code">
        <CardHead icon="code" title={W.code.made} />
        <Label style={styles.gapLabel}>{W.code.what(deviceName(device), howMuch(made.minutes))}</Label>
        <View style={styles.code} accessible accessibilityLabel={Array.from(made.shown.replace(/ /g, '')).join(' ')} testID="code">
          <Dashed />
          <View style={styles.codeIn}>
            {groups.map((g, i) => (
              <Txt key={i} style={gtype.code}>
                {g}
              </Txt>
            ))}
          </View>
          <Dashed />
        </View>
        <Txt style={[gtype.detail, styles.detail]} wrap="pretty">
          {W.code.read(child.name, device.kind === 'on_master' ? W.code.thisPcInSentence : device.name)}
        </Txt>
        {made.codesLeftToday !== null ? (
          <Txt style={[gtype.small, styles.small]} testID="codes-left">
            {W.code.left(made.codesLeftToday, device.kind === 'on_master')}
          </Txt>
        ) : null}
        <View style={[styles.pinRow, styles.gap]}>
          <Btn label={W.code.another} onPress={() => setMade(null)} height={field} outer={styles.flex} testID="another" />
          <Btn label={W.code.done} look="go" onPress={onBack} height={field} outer={styles.flex} testID="code-done" />
        </View>
      </Card>
    );
  } else {
    card = (
      <Card testID="make-code">
        <CardHead icon="code" title={W.code.heading} />
        <Txt style={[gtype.detail, styles.detail]} wrap="pretty">
          {W.code.line}
        </Txt>
        {children ? (
          <>
            <Label style={styles.gapLabel}>{W.code.for}</Label>
            <View style={styles.chips} accessibilityRole="radiogroup" accessibilityLabel={W.code.for}>
              {children.map((c) => (
                <Chip
                  key={c.childId}
                  label={c.name}
                  on={c.childId === childId}
                  onPress={() => {
                    setChildId(c.childId);
                    setDeviceId(firstDevice(c));
                  }}
                  testID={`code-child-${c.childId}`}
                />
              ))}
            </View>
            <Label style={styles.gapLabel}>{W.code.on}</Label>
            <View style={styles.chips} accessibilityRole="radiogroup" accessibilityLabel={W.code.on}>
              {(child?.devices ?? []).map((d) => (
                <Chip key={d.deviceId} label={deviceName(d)} on={d.deviceId === deviceId} onPress={() => setDeviceId(d.deviceId)} testID={`code-device-${d.deviceId}`} />
              ))}
            </View>
            <Label style={styles.gapLabel}>{W.code.howMuch}</Label>
            <View style={styles.chips} accessibilityRole="radiogroup" accessibilityLabel={W.code.howMuch}>
              {W.code.howMuchChoices.map(([m, label]) => (
                <Chip key={m} label={label} on={m === minutes} onPress={() => setMinutes(m)} testID={`code-minutes-${m}`} />
              ))}
            </View>
          </>
        ) : null}
        <Btn label={W.code.make} look="go" off={busy || !deviceId} onPress={() => (deviceId ? void make(deviceId, minutes) : undefined)} outer={styles.gap} testID="make" />
        {err ? <Txt style={[gtype.err, styles.err]}>{err}</Txt> : null}
        <Txt style={[gtype.small, styles.small]} wrap="pretty">
          {W.code.later}
        </Txt>
      </Card>
    );
  }
  return (
    <Under to={W.more.back} onBack={onBack} onTab={onTab} testID="screen-code">
      {card}
    </Under>
  );
}

// ---------------------------------------------------------------------------
// M2, the password and the PIN

const secret = { secureTextEntry: true, autoComplete: 'off', autoCorrect: false, autoCapitalize: 'none' } as const;

export function Secrets({ door, onBack, onTab, onSignedOut, onRefused }: { door: GuardianDoor; onBack: () => void; onTab: (tab: TabKey) => void; onSignedOut: () => void; onRefused: () => void }) {
  const wide = useWide();
  const said = useSaid(onSignedOut, onRefused);
  const [old, setOld] = useState('');
  const [fresh, setFresh] = useState('');
  const [again, setAgain] = useState('');
  const [pin, setPin] = useState('');
  const [pinAgain, setPinAgain] = useState('');
  const [pwSaid, setPwSaid] = useState({ words: '', done: false });
  const [pinSaid, setPinSaid] = useState({ words: '', done: false });
  const [busy, setBusy] = useState(false);
  const live = useRef(true);
  useEffect(() => {
    live.current = true;
    return () => {
      live.current = false;
    };
  }, []);

  const changePassword = async () => {
    if (old === '' || fresh === '') return setPwSaid({ words: W.signIn.empty, done: false });
    if (fresh !== again) return setPwSaid({ words: W.password.notSame, done: false });
    setBusy(true);
    const got = await ask(door, 'change_password', { old, new: fresh });
    if (!live.current) return;
    setBusy(false);
    setOld('');
    if (got.kind !== 'done') return setPwSaid({ words: said(got), done: false });
    setFresh('');
    setAgain('');
    setPwSaid({ words: W.secrets.passwordChanged, done: true });
  };
  const changePin = async () => {
    if (pin !== pinAgain) return setPinSaid({ words: W.pin.notSame, done: false });
    setBusy(true);
    const got = await ask(door, 'set_pin', { pin });
    if (!live.current) return;
    setBusy(false);
    if (got.kind !== 'done') return setPinSaid({ words: said(got), done: false });
    setPin('');
    setPinAgain('');
    setPinSaid({ words: W.secrets.pinChanged, done: true });
  };

  return (
    <Under to={W.more.back} onBack={onBack} onTab={onTab} across testID="screen-secrets">
      <View style={[styles.pair, wide && styles.pairWide]}>
        <Card outer={wide ? styles.flex : undefined} testID="change-password">
          <Head2 icon="key" title={W.secrets.password} />
          <Txt style={[gtype.detail, styles.detail]} wrap="pretty">
            {W.password.line}
          </Txt>
          <Label style={styles.gapLabel}>{W.secrets.now}</Label>
          <Field testID="old-password" value={old} onChangeText={setOld} {...secret} accessibilityLabel={W.secrets.now} />
          <Label style={styles.gapLabel} hint={W.password.hint}>
            {W.secrets.newOne}
          </Label>
          <Field testID="new-password" value={fresh} onChangeText={setFresh} {...secret} accessibilityLabel={W.secrets.newOne} />
          <Label style={styles.gapLabel}>{W.secrets.again}</Label>
          <Field testID="new-password-again" value={again} onChangeText={setAgain} onSubmitEditing={() => void changePassword()} {...secret} accessibilityLabel={W.secrets.again} />
          <Room words={pwSaid.words} done={pwSaid.done} testID="password-message" />
          <Btn label={W.secrets.changePassword} off={busy} onPress={() => void changePassword()} outer={styles.pairBtn} testID="change-the-password" />
        </Card>
        <Card outer={wide ? styles.flex : undefined} testID="change-pin">
          <Head2 icon="pin" title={W.secrets.pin} />
          <Txt style={[gtype.detail, styles.detail]} wrap="pretty">
            {W.pin.pinLine}
          </Txt>
          <Txt style={[gtype.small, styles.small]} wrap="pretty">
            {W.secrets.choose}
          </Txt>
          <Label style={styles.gapLabel} hint={W.pin.hint}>
            {W.secrets.newPin}
          </Label>
          <Field testID="new-pin" value={pin} onChangeText={setPin} keyboardType="number-pad" {...secret} accessibilityLabel={W.secrets.newPin} />
          <Label style={styles.gapLabel}>{W.secrets.again}</Label>
          <Field testID="new-pin-again" value={pinAgain} onChangeText={setPinAgain} onSubmitEditing={() => void changePin()} keyboardType="number-pad" {...secret} accessibilityLabel={W.pin.again} />
          <Room words={pinSaid.words} done={pinSaid.done} testID="pin-message" />
          <Btn label={W.secrets.changePin} off={busy} onPress={() => void changePin()} outer={styles.pairBtn} testID="change-the-pin" />
        </Card>
      </View>
    </Under>
  );
}

// ---------------------------------------------------------------------------
// M3, About LiMo

export function About({
  door,
  onBack,
  onTab,
  onRemoveLimo,
  onDeleteFamily,
}: {
  door: GuardianDoor;
  onBack: () => void;
  onTab: (tab: TabKey) => void;
  onRemoveLimo: () => void;
  onDeleteFamily: () => void;
}) {
  const { now: nowOf, clock } = useClock();
  const now = nowOf();
  const [version, setVersion] = useState<string | null>(null);
  const [removalUntil, setRemovalUntil] = useState<number | null>(null);
  const [reading, setReading] = useState(false);
  useEffect(() => {
    let live = true;
    void Promise.all([ask(door, 'about', {}), ask(door, 'family_state', {})]).then(([a, s]) => {
      if (!live) return;
      if (a.kind === 'done') setVersion(a.value.version);
      if (s.kind === 'done') setRemovalUntil(s.value.removalAllowedUntil);
    });
    return () => {
      live = false;
    };
  }, [door]);
  const read = <Btn label={W.about.read} mono onPress={() => setReading(true)} />;
  return (
    <Under to={W.more.back} onBack={onBack} onTab={onTab} end={<MakerMark />} testID="screen-about">
      <Card testID="about-limo">
        <Head2 icon="info" title={W.about.heading} />
        {version ? <Kvs rows={[{ k: W.about.forWindows, v: version }]} /> : null}
        <Txt style={[gtype.small, styles.small]} wrap="pretty">
          {W.about.carries}
        </Txt>
      </Card>
      <Card outer={styles.gap} testID="licences">
        <Head2 icon="list" title={W.about.licences} />
        <Kvs
          rows={[
            { k: W.about.limo, v: W.about.licence, after: read },
            { k: W.about.others, v: W.about.notices, after: read },
          ]}
        />
        {reading ? (
          <Txt style={[gtype.small, styles.small]} wrap="pretty" testID="read-coming">
            {W.about.readComing}
          </Txt>
        ) : null}
        <Txt style={[gtype.detail, styles.detail]}>
          {`${W.about.moreInfo} `}
          <Txt style={gtype.address}>{W.about.address}</Txt>
        </Txt>
      </Card>
      <Card outer={styles.gap} testID="remove">
        <Head2 icon="trash" title={W.about.remove} />
        <Btn label={W.about.removeLimo} onPress={onRemoveLimo} outer={styles.gap} testID="remove-limo" />
        <Txt style={[gtype.small, styles.small]} wrap="pretty">
          {removalUntil !== null && removalUntil > now ? W.confirm.removeLimo.allowed(laterInSentence(now, removalUntil, 'until', clock)) : W.about.removeLimoLine}
        </Txt>
        <Btn label={W.about.deleteFamily} look="coral" onPress={onDeleteFamily} outer={styles.gap} testID="delete-family" />
        <Txt style={[gtype.small, styles.small]}>{W.about.asksAgain}</Txt>
      </Card>
    </Under>
  );
}

// ---------------------------------------------------------------------------
// M4, and the others like it: the password again

/**
 * A screen that asks for the password again before something that cannot
 * be undone: what it does, the field, room for a message, and the button in
 * coral. Asked with the password; done, it hands the answer on.
 */
export function Confirm<T>({
  to,
  onBack,
  onTab,
  tab,
  heading,
  line,
  small,
  button,
  act,
  onDone,
  onSignedOut,
  onRefused,
  doneWords,
  testID,
}: {
  to: string;
  onBack: () => void;
  onTab: (tab: TabKey) => void;
  tab: TabKey;
  heading: string;
  line: string;
  small?: string;
  button: string;
  act: (password: string) => Promise<Asked<T>>;
  /** What happens once it is done; when it leaves the screen open, `doneWords` says what was done. */
  onDone: (value: T) => void;
  onSignedOut: () => void;
  onRefused: () => void;
  doneWords?: (value: T) => string;
  testID: string;
}) {
  const { now: nowOf, clock } = useClock();
  const now = nowOf();
  const said = useSaid(onSignedOut, onRefused);
  const [password, setPassword] = useState('');
  const [message, setMessage] = useState({ words: '', done: false });
  const [busy, setBusy] = useState(false);
  const live = useRef(true);
  useEffect(() => {
    live.current = true;
    return () => {
      live.current = false;
    };
  }, []);
  const submit = async () => {
    if (password === '') return setMessage({ words: W.signIn.empty, done: false });
    setBusy(true);
    const got = await act(password);
    if (!live.current) return;
    setBusy(false);
    setPassword('');
    if (got.kind !== 'done') return setMessage({ words: said(got), done: false });
    setMessage({ words: doneWords ? doneWords(got.value) : '', done: true });
    onDone(got.value);
  };
  return (
    <Page testID={testID} tabs={<GuardianTabs picked={tab} onPick={onTab} />}>
      <TopRow right={<Label>{dateLine(now, clock)}</Label>} />
      <Column>
        <SubHead to={to} onBack={onBack} />
        <Card>
          <CardHead icon="trash" title={heading} />
          <Txt style={[gtype.detail, styles.detail]} wrap="pretty">
            {line}
          </Txt>
          {small ? (
            <Txt style={[gtype.small, styles.small]} wrap="pretty">
              {small}
            </Txt>
          ) : null}
          <Label style={styles.gapLabel}>{W.confirm.password}</Label>
          <Field testID="confirm-password" value={password} onChangeText={setPassword} onSubmitEditing={() => void submit()} autoFocus {...secret} accessibilityLabel={W.confirm.password} />
          <Room words={message.words} done={message.done} testID="confirm-message" />
          <Btn label={button} look="danger" off={busy} onPress={() => void submit()} testID="confirm" />
        </Card>
      </Column>
    </Page>
  );
}

export function DeleteFamily({ door, onBack, onTab, onDeleted, onSignedOut, onRefused }: { door: GuardianDoor; onBack: () => void; onTab: (tab: TabKey) => void; onDeleted: () => void; onSignedOut: () => void; onRefused: () => void }) {
  const [today, setToday] = useState<readonly ChildToday[]>([]);
  useEffect(() => {
    let live = true;
    void ask(door, 'family_today', {}).then((got) => {
      if (live && got.kind === 'done') setToday(got.value);
    });
    return () => {
      live = false;
    };
  }, [door]);
  const phones = today.flatMap((c) => c.devices.filter((d) => d.kind === 'portable').map((d) => d.name));
  return (
    <Confirm
      to={W.about.heading}
      tab="more"
      onBack={onBack}
      onTab={onTab}
      heading={W.confirm.deleteFamily.heading}
      line={W.confirm.deleteFamily.line(today.map((c) => c.name))}
      small={W.confirm.deleteFamily.phones(phones) + W.confirm.deleteFamily.stays}
      button={W.confirm.deleteFamily.button}
      act={(password) => ask(door, 'delete_family', { password })}
      onDone={onDeleted}
      onSignedOut={onSignedOut}
      onRefused={onRefused}
      testID="screen-delete-family"
    />
  );
}

export function RemoveChild({ door, child, onBack, onTab, onRemoved, onSignedOut, onRefused }: { door: GuardianDoor; child: { childId: string; name: string }; onBack: () => void; onTab: (tab: TabKey) => void; onRemoved: () => void; onSignedOut: () => void; onRefused: () => void }) {
  return (
    <Confirm
      to={W.tabs.rules}
      tab="rules"
      onBack={onBack}
      onTab={onTab}
      heading={W.confirm.removeChild.heading(child.name)}
      line={W.confirm.removeChild.line(child.name)}
      button={W.confirm.removeChild.button(child.name)}
      act={(password) => ask(door, 'remove_child', { child: child.childId, password })}
      onDone={onRemoved}
      onSignedOut={onSignedOut}
      onRefused={onRefused}
      testID="screen-remove-child"
    />
  );
}

export function RemovePhone({ door, device, onBack, onTab, onRemoved, onSignedOut, onRefused }: { door: GuardianDoor; device: { deviceId: string; name: string }; onBack: () => void; onTab: (tab: TabKey) => void; onRemoved: () => void; onSignedOut: () => void; onRefused: () => void }) {
  return (
    <Confirm
      to={W.tabs.devices}
      tab="devices"
      onBack={onBack}
      onTab={onTab}
      heading={W.confirm.removePhone.heading(device.name)}
      line={W.confirm.removePhone.line}
      button={W.confirm.removePhone.button}
      act={(password) => ask(door, 'remove_device', { device: device.deviceId, password })}
      onDone={onRemoved}
      onSignedOut={onSignedOut}
      onRefused={onRefused}
      testID="screen-remove-phone"
    />
  );
}

export function RemoveLimo({ door, onBack, onTab, onSignedOut, onRefused }: { door: GuardianDoor; onBack: () => void; onTab: (tab: TabKey) => void; onSignedOut: () => void; onRefused: () => void }) {
  const { now: nowOf, clock } = useClock();
  const now = nowOf();
  return (
    <Confirm
      to={W.about.heading}
      tab="more"
      onBack={onBack}
      onTab={onTab}
      heading={W.confirm.removeLimo.heading}
      line={W.confirm.removeLimo.line}
      button={W.confirm.removeLimo.button}
      act={(password) => ask(door, 'allow_removal', { password })}
      onDone={() => undefined}
      doneWords={(v) => W.confirm.removeLimo.allowed(laterInSentence(now, v.until, 'until', clock))}
      onSignedOut={onSignedOut}
      onRefused={onRefused}
      testID="screen-remove-limo"
    />
  );
}

const styles = StyleSheet.create({
  flex: { flex: 1, minWidth: 0 },
  moreTop: { marginTop: 12 },
  list: { paddingVertical: 2, paddingHorizontal: 14 },
  listWide: { paddingVertical: 2, paddingHorizontal: 18 },
  li: { flexDirection: 'row', alignItems: 'center', gap: 12, minHeight: 64, paddingVertical: 12 },
  liAfter: { minHeight: 62.5 },
  liText: { marginTop: 2 },
  detail: { marginTop: 10 },
  small: { marginTop: 12, marginHorizontal: 2 },
  gap: { marginTop: 14 },
  gapLabel: { marginTop: 18 },
  err: { marginTop: 8, marginHorizontal: 2 },
  chips: { flexDirection: 'row', flexWrap: 'wrap', gap: 8, marginTop: 8 },
  // The drawing's 4 under the code meets the 10 over the sentence: 10 between.
  code: { marginTop: 10 },
  codeIn: { flexDirection: 'row', justifyContent: 'center', gap: 24, paddingVertical: 16 },
  pinRow: { flexDirection: 'row', gap: 10 },
  pair: { gap: 14 },
  pairWide: { flexDirection: 'row', gap: 22, alignItems: 'flex-start' },
  pairBtn: { marginTop: 2 },
});
