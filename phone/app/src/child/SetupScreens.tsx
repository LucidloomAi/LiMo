// Setting up a child's phone, as design/screens/phone-setup.html draws it:
// S0 not set up, S1 welcome, S2 what LiMo does here (and S2b, ticked), S3 to
// S7 the five steps from one layout, S8 done, and C1 to C3, the child's own
// introduction.
//
// These are plain screens: they show what they are given and say what was
// tapped. ChildApp decides which one shows and asks the host.

import { Pressable, StyleSheet, View } from 'react-native';

import { STEP_IDS, type StepId, type StepState, type StepStates } from '../host';
import { Icon } from '../icons/Icon';
import { borderWidth, color, hardShadow, radius } from '../theme';
import { Page } from '../ui/paper/Page';
import {
  Button,
  Card,
  Dashed,
  Dots,
  Heading,
  Label,
  plain,
  type Rich,
  RichText,
  Shadow,
  Stamp,
  Tail,
  Tick,
  Tile,
  TopRow,
} from '../ui/paper/parts';
import { Txt, type } from '../ui/paper/text';
import * as W from './words';

/** A card that opens a screen: the icon, then the words. */
function IconCard({ icon, eyebrow, heading, text }: { icon: W.IntroWords['icon']; eyebrow?: string; heading: string; text: Rich }) {
  return (
    <Card outer={styles.afterTop}>
      <Icon name={icon} size={64} />
      {eyebrow ? <Label style={styles.eyebrow}>{eyebrow}</Label> : null}
      <Heading style={eyebrow ? styles.headingAfterEyebrow : styles.heading}>{heading}</Heading>
      <View style={styles.detail} accessible accessibilityLabel={plain(text)}>
        <RichText words={text} style={type.detail} wrap="pretty" />
      </View>
    </Card>
  );
}

// ---------- S0. Not set up ----------

/** Only on a phone the PC has not paired. Nothing on the phone can set it up alone: no button. */
export function NotSetUp() {
  return (
    <Page testID="screen-not-set-up">
      <TopRow />
      <IconCard icon="usb" heading={W.notSetUp.heading} text={W.notSetUp.text} />
      <Tail />
    </Page>
  );
}

// ---------- S1. Welcome ----------

export function Welcome({ name, onStart }: { name: string; onStart: () => void }) {
  return (
    <Page testID="screen-welcome" pinned={<Button kind="go" label={W.welcome.button} onPress={onStart} testID="main-button" />}>
      <TopRow />
      <IconCard icon="phone" eyebrow={W.welcome.eyebrow} heading={W.welcome.heading(name)} text={[W.welcome.text]} />
    </Page>
  );
}

// ---------- S2. What LiMo does here ----------

/** Plain rows inside one card, with dashed dividers. */
function Rows({ rows }: { rows: readonly { icon: W.IntroWords['icon']; title: string; text: Rich | string }[] }) {
  return (
    <Card style={styles.list}>
      {rows.map((r, i) => (
        <View key={r.title}>
          {i > 0 ? <Dashed /> : null}
          <View style={styles.li} accessible accessibilityLabel={`${r.title}. ${plain(r.text)}`}>
            <Tile icon={r.icon} />
            <View style={styles.liWords}>
              <Txt style={type.rowTitle}>{r.title}</Txt>
              <RichText words={r.text} style={type.rowText} wrap="pretty" />
            </View>
          </View>
        </View>
      ))}
    </Card>
  );
}
export { Rows as SetupRows };

/** The tick box row: the whole row can be tapped. */
function TickRow({ name, ticked, onTick }: { name: string; ticked: boolean; onTick: () => void }) {
  const words = W.agree.tick(name);
  return (
    <View style={styles.tickRow}>
      <Shadow offset={hardShadow.button.x} corner={radius.md} />
      <Pressable
        testID="agree-box"
        onPress={onTick}
        accessibilityRole="checkbox"
        accessibilityLabel={words}
        accessibilityState={{ checked: ticked }}
        style={styles.tick}
      >
        <View style={[styles.box, ticked && styles.boxOn]}>{ticked ? <Tick tint={color.card} /> : null}</View>
        <Txt style={[type.tick, styles.tickWords]}>{words}</Txt>
      </Pressable>
    </View>
  );
}

export function Agree({
  name,
  ticked,
  onTick,
  onContinue,
}: {
  name: string;
  ticked: boolean;
  onTick: () => void;
  onContinue: () => void;
}) {
  return (
    <Page
      testID="screen-agree"
      pinned={<Button kind="go" off={!ticked} label={W.agree.button} onPress={onContinue} testID="main-button" />}
    >
      <TopRow />
      <View style={styles.head}>
        <Label>{W.agree.label}</Label>
        <Heading style={styles.headHeading}>{W.agree.heading}</Heading>
      </View>
      <Rows rows={W.agree.rows} />
      <Txt style={[type.line, styles.line]} wrap="pretty">
        {W.agree.line}
      </Txt>
      <TickRow name={name} ticked={ticked} onTick={onTick} />
    </Page>
  );
}

// ---------- S3 to S7. One permission each ----------

/** "Step 2 of 5", and the dots: done ones teal, this one marigold. */
function StepCount({ at }: { at: number }) {
  return (
    <View style={styles.count}>
      <Label>{W.step.count(at + 1, STEP_IDS.length)}</Label>
      <Dots count={STEP_IDS.length} at={at} />
    </View>
  );
}

export function Step({
  id,
  name,
  state,
  notOpened,
  onOpen,
  onNext,
}: {
  id: StepId;
  name: string;
  /** As Android reports it. Shown as on only when it is "on". */
  state: StepState;
  /** Android's own screen could not be opened just now. */
  notOpened: boolean;
  onOpen: () => void;
  onNext: () => void;
}) {
  const s = W.steps[id];
  const at = STEP_IDS.indexOf(id);
  const on = state === 'on';
  // The line under the stamp keeps its room, so nothing moves when it comes.
  const line = on ? W.step.onLine : notOpened ? W.step.notOpened : '';
  return (
    <Page
      testID={`screen-step-${id}`}
      pinned={<Button kind="go" label={on ? W.step.next : s.button} onPress={on ? onNext : onOpen} testID="main-button" />}
    >
      <TopRow right={<StepCount at={at} />} />
      <Card outer={styles.afterTop}>
        <Icon name={s.icon} size={64} />
        <Heading style={styles.heading}>{s.title}</Heading>
        <Label style={styles.gap}>{W.step.why}</Label>
        <Txt style={[type.detail, styles.detail]} wrap="pretty">
          {s.why(name)}
        </Txt>
        <View style={styles.what}>
          <Dashed />
          <View style={styles.whatInside}>
            <Label>{W.step.what}</Label>
            <View style={styles.detail} accessible accessibilityLabel={plain(s.what)}>
              <RichText words={s.what} style={type.detail} wrap="pretty" />
            </View>
            {s.note ? (
              <View style={styles.note}>
                <Label>{s.note.label}</Label>
                <Txt style={[type.note, styles.noteWords]} wrap="pretty">
                  {s.note.text}
                </Txt>
              </View>
            ) : null}
          </View>
        </View>
        <View style={styles.state} accessible accessibilityLabel={on ? `${W.step.on}. ${W.step.onLine}` : W.step.notOnYet} testID="step-state">
          {on ? (
            <Stamp tone="teal" testID="stamp-on">
              {W.step.on}
            </Stamp>
          ) : (
            <Stamp tone="coral" testID="stamp-not-on-yet">
              {W.step.notOnYet}
            </Stamp>
          )}
          <Txt style={[type.state, styles.stateLine]} testID="step-line">
            {line}
          </Txt>
        </View>
      </Card>
    </Page>
  );
}

// ---------- S8. Done ----------

export function Done({ name, steps, onShow }: { name: string; steps: StepStates | null; onShow: () => void }) {
  return (
    <Page testID="screen-done" pinned={<Button kind="go" label={W.done.button(name)} onPress={onShow} testID="main-button" />}>
      <TopRow />
      <Card outer={styles.afterTop}>
        <Icon name="check" size={64} />
        <Heading style={styles.heading}>{W.done.heading}</Heading>
        <View style={styles.oks}>
          {STEP_IDS.map((id, i) => {
            // A tick only where Android says the step is on now.
            const on = steps?.[id] === 'on';
            return (
              <View key={id} style={styles.okRow}>
                {i > 0 ? <Dashed /> : null}
                <View style={styles.ok} accessible accessibilityLabel={on ? `${W.steps[id].title}, ${W.step.on}` : W.steps[id].title}>
                  {on ? <Tick tint={color.teal} /> : <View style={styles.noTick} />}
                  <Txt style={type.app}>{W.steps[id].title}</Txt>
                </View>
              </View>
            );
          })}
        </View>
        <Txt style={[type.detail, styles.detail, styles.after]} wrap="pretty">
          {W.done.after(name)}
        </Txt>
      </Card>
      <Tail />
    </Page>
  );
}

// ---------- C1 to C3. The child's own introduction ----------

export function Intro({ at, name, onNext }: { at: number; name: string; onNext: () => void }) {
  const c = W.intro[at];
  const of = W.intro.length;
  return (
    <Page testID={`screen-intro-${at + 1}`} pinned={<Button kind="go" label={c.button} onPress={onNext} testID="main-button" />}>
      <TopRow
        right={
          <View style={styles.countThree}>
            <Dots count={of} at={at} label={W.introCount(at + 1, of)} />
          </View>
        }
      />
      <IconCard icon={c.icon} eyebrow={c.eyebrow?.(name)} heading={c.title} text={c.text} />
    </Page>
  );
}

const styles = StyleSheet.create({
  afterTop: { marginTop: 10 },
  eyebrow: { marginTop: 10 },
  heading: { marginTop: 6 },
  headingAfterEyebrow: { marginTop: 6 },
  detail: { marginTop: 8 },
  head: { marginTop: 16, marginBottom: 12, marginHorizontal: 2 },
  headHeading: { marginTop: 8 },
  list: { paddingVertical: 2, paddingHorizontal: 14 },
  li: { flexDirection: 'row', alignItems: 'flex-start', gap: 12, paddingVertical: 12 },
  liWords: { flex: 1, gap: 2 },
  line: { marginTop: 14, marginHorizontal: 2 },
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
    borderRadius: radius.md,
  },
  box: {
    width: 26,
    height: 26,
    alignItems: 'center',
    justifyContent: 'center',
    borderWidth: 2,
    borderColor: color.ink,
    borderRadius: radius.sm,
    backgroundColor: color.card,
  },
  boxOn: { backgroundColor: color.ink },
  tickWords: { flex: 1 },
  count: { alignItems: 'flex-end', gap: 7, paddingTop: 3, marginLeft: 12 },
  countThree: { paddingTop: 9, marginLeft: 12 },
  gap: { marginTop: 14 },
  what: { marginTop: 14 },
  whatInside: { paddingTop: 12 },
  note: {
    marginTop: 12,
    paddingTop: 10,
    paddingBottom: 11,
    paddingHorizontal: 12,
    borderWidth,
    borderStyle: 'dashed',
    borderColor: 'rgba(27,38,56,0.42)',
    borderRadius: radius.md,
  },
  noteWords: { marginTop: 6 },
  state: { marginTop: 16, alignItems: 'flex-start' },
  stateLine: { minHeight: 20, marginTop: 8 },
  oks: { marginTop: 12 },
  okRow: { minHeight: 38 },
  ok: { flex: 1, flexDirection: 'row', alignItems: 'center', gap: 10, minHeight: 36.5 },
  noTick: { width: 18, height: 18 },
  after: { paddingTop: 4 },
});
