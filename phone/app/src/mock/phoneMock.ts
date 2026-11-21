// The console's desk for a phone, made up, for the guardian screens' preview
// and tests: the five `phone.*` requests answered as the console answers
// them (console/README.md, "Adding a phone"), in its own shapes and words.
// DEVELOPMENT ONLY; nothing in the app imports it.
//
// The cable plays a plan: each `phone.look` takes the next case from it, as
// a person plugging the phone in and switching USB debugging on would. The
// phone is asked to allow this PC only by `phone.ask`; it taps Allow a few
// looks later, unless it was locked when asked, when it shows nothing until
// it goes and comes back. Installing, pairing and switching USB debugging off then go
// on by themselves, one stage a look, unless the cable is `frozen` (the
// preview's pictures). A case can be set at any time, as pulling the cable.

import type { CableCase } from '../guardianSeam';

export type MockPhoneFacts = {
  name: string;
  model: string;
  maker: string;
  android: string;
  recent_enough: boolean;
  limo: { looked: boolean; on_it: boolean; version: string | null };
  this_pc_carries: string;
};

export type MockCable = {
  /** True: every look stays as it is (the preview). */
  frozen: boolean;
  case: CableCase;
  /** The cases still to come, one a look. */
  plan: CableCase[];
  /** What went wrong, while the case is `trouble`. */
  trouble: { line: string; more: string } | null;
  /**
   * Asked while locked: a Samsung phone puts up no Allow on a locked screen,
   * and none later until it goes and comes back. It stays `allow` for ever.
   */
  locked: boolean;
  /** Looks after it is asked before the guardian taps Allow on the phone. */
  allowAfter: number;
  /** Seconds since it was asked, while `allow` (a look a second). */
  askedSecs: number | null;
  /** The phone that plugs in when the case comes to `ok`. */
  plugs: MockPhoneFacts;
  /** The phone connected, once it was. */
  phone: MockPhoneFacts | null;
  install: { stage: string; sent: number | null; line: string | null; advice: string | null };
  /** How many installs fail before one works. */
  installFails: number;
  pairing: { stage: string; device: { device_id: string; name: string } | null; ways: string[]; line: string | null; advice: string | null };
  /** How many pairings find their offer run out before one pairs. */
  offerRunsOut: number;
  /** The child being paired. */
  pairingChild: string | null;
  /** Looks since pairing, for when the phone reaches the home Wi-Fi. */
  sincePaired: number;
  /** Looks after pairing before the phone reaches the PC over the home Wi-Fi; null: never. */
  homeAfter: number | null;
  debugging: { stage: string; line: string | null; advice: string | null };
  debuggingFails: boolean;
  busy: string | null;
  held: boolean;
};

/** How long the window waits for Allow after asking, in seconds. */
export const ASK_WAITS_SECS = 600;

export const FINGERPRINT = '3C:91:5A:07:E2:4B:D8:16:AF:62:0C:B9:75:18:4E:D3';

/** The drawings' phone: a Galaxy S20 Ultra with Android 13, LiMo not on it yet. */
export const GALAXY: MockPhoneFacts = {
  name: 'Galaxy S20 Ultra',
  model: 'SM-G988B',
  maker: 'samsung',
  android: '13',
  recent_enough: true,
  limo: { looked: true, on_it: false, version: null },
  this_pc_carries: '0.1.0',
};

/** A phone too old for LiMo. */
export const OLD_PHONE: MockPhoneFacts = { ...GALAXY, name: 'Galaxy S7', model: 'SM-G930F', android: '7.1', recent_enough: false, limo: { looked: false, on_it: false, version: null } };

/** The console's words (console/README.md, "The look"). */
export const CABLE_WORDS: Record<Exclude<CableCase, 'ok' | 'trouble'>, [string, string]> = {
  looking: ['Looking at this PC’s USB ports.', ''],
  none: ['No phone on a USB port.', 'If it is plugged in, the cable may only charge it. Try the one that came with the phone.'],
  off: ['A phone is plugged in.', 'USB debugging is off on it.'],
  driver: ['No working driver for this phone.', 'Install its maker’s USB driver for Windows, such as Samsung’s Android USB Driver.'],
  connecting: ['A phone is plugged in.', 'Connecting to it.'],
  unknown: ['A phone is plugged in.', 'It does not know this PC yet.'],
  allow: ['The phone is asking.', 'Waiting for Allow on the phone.'],
};

export const PHONE_WORDS = {
  heldElsewhere: { line: 'Another program on this PC holds the phone’s connection (Android Studio, or Google’s adb).', more: 'Close it, then unplug the phone and plug it in again.' },
  tooOld: (android: string) => ({ line: `This phone has Android ${android}. LiMo needs Android 8.0 or later.`, more: '' }),
  noPhone: 'No phone is connected yet. Plug it in, unlocked, with USB debugging on.',
  busy: 'The phone is busy with another step. Wait a moment, then try again.',
  alreadyConnected: 'The phone is connected already: it knows this PC.',
  notAllowedInTime: { line: 'The phone did not allow this PC in time.', more: 'Unlock the phone and ask again. If the phone shows no question, switch USB debugging off and on again on the phone first.' },
  notYet: 'LiMo is not on the phone yet.',
  sending: 'Sending LiMo to the phone.',
  installing: 'The phone is installing LiMo.',
  installed: 'LiMo is on the phone.',
  lost: ['The phone’s connection ended while LiMo was being put on it.', 'Plug the phone in again, unlocked, then try again.'] as const,
  notInTime: ['The phone did not finish installing LiMo in time.', 'Look at the phone’s screen for a question, then try again.'] as const,
  asking: 'Asking LiMo’s service for a pairing.',
  handing: 'Handing the pairing to LiMo on the phone.',
  waiting: 'Waiting for LiMo on the phone to pair with this PC.',
  overCable: 'Over the cable, with this PC.',
  overWifi: 'Over the home Wi-Fi, with this PC.',
  ranOut: ['The pairing offer ran out before LiMo on the phone paired.', 'Try again.'] as const,
  switchingOff: 'Switching USB debugging off.',
  isOff: 'USB debugging is off.',
  stillOn: ['USB debugging is still on.', 'Switch it off on the phone: Settings, Developer options, USB debugging.'] as const,
};

const NOT_ASKED = { stage: 'not_asked', line: null, advice: null };

/** No phone yet: the cable shows none, and plays nothing until a test or the preview says. */
export function freshCable(change: Partial<MockCable> = {}): MockCable {
  return {
    frozen: false,
    case: 'none',
    plan: [],
    trouble: null,
    locked: false,
    allowAfter: 2,
    askedSecs: null,
    plugs: GALAXY,
    phone: null,
    install: { ...NOT_ASKED, sent: null },
    installFails: 0,
    pairing: { ...NOT_ASKED, device: null, ways: [] },
    offerRunsOut: 0,
    pairingChild: null,
    sincePaired: 0,
    homeAfter: 2,
    debugging: { stage: 'on', line: null, advice: null },
    debuggingFails: false,
    busy: null,
    held: false,
    ...change,
  };
}

/** A phone connected: the strip says so, and the checks are ticked. */
export function connected(phone: MockPhoneFacts = GALAXY, change: Partial<MockCable> = {}): MockCable {
  return freshCable({ case: 'ok', phone, held: true, ...change });
}

export type PhoneAnswer = { ok: true; value: unknown } | { ok: false; code: string; words: string };

/** What the desk for a phone needs of the family: a new phone's device, paired with a child. */
export type FamilySide = {
  /** Adds the paired phone to the child's devices; its id. */
  pairPhone(child: string, name: string): string;
  /** The phone's own setup on the phone: one more of its six steps on. */
  setUpOneMore(device: string): void;
  /** The name the service gave a device. */
  nameOf(device: string): string;
};

function strip(c: MockCable): { case: string; line: string; more: string } {
  if (c.case === 'ok') return { case: 'ok', line: `Connected: ${(c.phone ?? c.plugs).name}.`, more: `Android ${(c.phone ?? c.plugs).android}.` };
  if (c.case === 'trouble') return { case: 'trouble', line: c.trouble?.line ?? PHONE_WORDS.heldElsewhere.line, more: c.trouble?.more ?? '' };
  const [line, more] = CABLE_WORDS[c.case];
  return { case: c.case, line, more };
}

function checks(p: MockPhoneFacts | null): { ok: boolean; words: string }[] {
  if (!p) return [];
  const out = [
    { ok: true, words: 'One phone plugged in' },
    p.recent_enough ? { ok: true, words: `Android ${p.android}, recent enough for LiMo` } : { ok: false, words: `Android ${p.android}, older than the Android 8.0 LiMo needs` },
  ];
  if (p.limo.looked) {
    const v = p.limo.version;
    out.push({
      ok: true,
      words: !p.limo.on_it ? 'LiMo is not on it yet' : v === p.this_pc_carries ? `LiMo ${v} is on it already` : v ? `LiMo ${v} is on it, and this PC carries ${p.this_pc_carries}` : 'LiMo is on it',
    });
  }
  return out;
}

/** The look, as `phone.look` answers it. */
export function lookOf(c: MockCable): unknown {
  return {
    cable: strip(c),
    fingerprint: c.case === 'allow' || c.case === 'unknown' ? FINGERPRINT : null,
    asked_secs: c.case === 'allow' ? c.askedSecs : null,
    phone: c.phone,
    checks: checks(c.phone),
    install: { stage: c.install.stage, sent: c.install.sent, line: c.install.line, advice: c.install.advice },
    pairing: { stage: c.pairing.stage, device: c.pairing.device, ways: c.pairing.ways, line: c.pairing.line, advice: c.pairing.advice },
    debugging: { stage: c.debugging.stage, line: c.debugging.line, advice: c.debugging.advice },
    busy: c.busy,
    held: c.held,
  };
}

/** One look's worth of the world going on. */
function tick(c: MockCable, family: FamilySide): void {
  const next = c.plan.shift();
  if (next) {
    c.case = next;
    // Gone and come back: whatever was asked is forgotten.
    if (next !== 'allow') c.askedSecs = null;
  }
  // Asked: a second more of waiting, and Allow tapped in time unless the phone was locked when asked.
  if (c.case === 'allow' && c.askedSecs !== null) {
    c.askedSecs += 1;
    if (!c.locked && c.askedSecs >= c.allowAfter) {
      c.case = 'ok';
      c.askedSecs = null;
      c.busy = null;
    } else if (c.askedSecs >= ASK_WAITS_SECS) {
      // Nobody answered in ten minutes: said until the phone goes or the guardian asks again.
      c.case = 'trouble';
      c.trouble = PHONE_WORDS.notAllowedInTime;
      c.askedSecs = null;
      c.busy = null;
    }
  }
  // A phone LiMo can open is connected by itself as soon as it allows this PC.
  if (c.case === 'ok' && !c.phone) {
    c.phone = structuredClone(c.plugs);
    c.held = true;
    if (!c.phone.recent_enough) {
      c.case = 'trouble';
      c.trouble = PHONE_WORDS.tooOld(c.phone.android);
    }
  }
  const pulled = c.case !== 'ok';
  // Installing.
  if (c.install.stage === 'sending' || c.install.stage === 'installing') {
    if (pulled) {
      c.install = { stage: 'failed', sent: null, line: PHONE_WORDS.lost[0], advice: PHONE_WORDS.lost[1] };
      c.busy = null;
    } else if (c.install.stage === 'sending') {
      const sent = Math.min(1, Math.round(((c.install.sent ?? 0) + 0.5) * 100) / 100);
      c.install = sent >= 1 ? { stage: 'installing', sent: 1, line: PHONE_WORDS.installing, advice: null } : { stage: 'sending', sent, line: PHONE_WORDS.sending, advice: null };
    } else if (c.installFails > 0) {
      c.installFails -= 1;
      c.install = { stage: 'failed', sent: null, line: PHONE_WORDS.notInTime[0], advice: PHONE_WORDS.notInTime[1] };
      c.busy = null;
    } else {
      c.install = { stage: 'done', sent: null, line: PHONE_WORDS.installed, advice: null };
      if (c.phone) c.phone.limo = { looked: true, on_it: true, version: c.phone.this_pc_carries };
      c.busy = null;
    }
    return;
  }
  // Pairing: the service, then the phone, then the phone pairs (or the offer runs out).
  const p = c.pairing;
  if (p.stage === 'asking_the_service') {
    c.pairing = { ...p, stage: 'handing', line: PHONE_WORDS.handing };
    return;
  }
  if (p.stage === 'handing') {
    c.pairing = { ...p, stage: 'waiting', line: PHONE_WORDS.waiting };
    return;
  }
  if (p.stage === 'waiting') {
    c.busy = null;
    if (c.offerRunsOut > 0) {
      c.offerRunsOut -= 1;
      c.pairing = { ...p, stage: 'failed', line: PHONE_WORDS.ranOut[0], advice: PHONE_WORDS.ranOut[1] };
      return;
    }
    const id = family.pairPhone(c.pairingChild ?? '', (c.phone ?? c.plugs).name);
    const name = family.nameOf(id);
    c.pairing = { stage: 'paired', device: { device_id: id, name }, ways: ['cable'], line: PHONE_WORDS.overCable, advice: null };
    c.sincePaired = 0;
    return;
  }
  if (p.stage === 'paired') {
    c.sincePaired += 1;
    if (p.device) family.setUpOneMore(p.device.device_id);
    if (c.homeAfter !== null && c.sincePaired >= c.homeAfter && !p.ways.includes('home_network')) {
      c.pairing = { ...p, ways: [...p.ways, 'home_network'], line: PHONE_WORDS.overWifi };
    }
  }
  // Switching USB debugging off.
  if (c.debugging.stage === 'switching_off') {
    c.busy = null;
    if (c.debuggingFails) c.debugging = { stage: 'still_on', line: PHONE_WORDS.stillOn[0], advice: PHONE_WORDS.stillOn[1] };
    else {
      c.debugging = { stage: 'off', line: PHONE_WORDS.isOff, advice: null };
      c.case = 'off';
    }
  }
}

/** Answers one of the five requests, as the console's desk does. */
export function answerPhone(c: MockCable, name: string, args: Record<string, unknown>, family: FamilySide): PhoneAnswer {
  const err = (code: string, words: string): PhoneAnswer => ({ ok: false, code, words });
  const phoneThere = c.case === 'ok' && c.phone !== null;
  switch (name) {
    case 'phone.look':
      if (!c.frozen) tick(c, family);
      return { ok: true, value: lookOf(c) };
    case 'phone.ask':
      if (c.case === 'ok') return err('already_connected', PHONE_WORDS.alreadyConnected);
      if (c.case === 'allow') return err('busy', PHONE_WORDS.busy);
      if (c.case !== 'unknown' && !(c.case === 'trouble' && c.trouble === PHONE_WORDS.notAllowedInTime)) return err('no_phone', PHONE_WORDS.noPhone);
      c.case = 'allow';
      c.trouble = null;
      c.askedSecs = 0;
      c.busy = 'asking';
      return { ok: true, value: { started: true } };
    case 'phone.install':
      if (c.phone && !c.phone.recent_enough) return err('too_old', `This phone has Android ${c.phone.android}. LiMo needs Android 8.0 or later.`);
      if (!phoneThere) return err('no_phone', PHONE_WORDS.noPhone);
      if (c.busy) return err('busy', PHONE_WORDS.busy);
      if (c.phone!.limo.on_it && c.phone!.limo.version === c.phone!.this_pc_carries) {
        c.install = { stage: 'not_needed', sent: null, line: `LiMo ${c.phone!.this_pc_carries} is on the phone already.`, advice: null };
        return { ok: true, value: { started: false } };
      }
      c.install = { stage: 'sending', sent: 0, line: PHONE_WORDS.sending, advice: null };
      c.busy = 'installing';
      return { ok: true, value: { started: true } };
    case 'phone.pair': {
      const keys = Object.keys(args);
      if (keys.length !== 1 || typeof args.child !== 'string') return err('unknown_request', 'phone.pair takes the child, as {"child": "<its id>"}, and nothing else');
      if (c.pairing.stage === 'paired') return { ok: true, value: { started: false } };
      if (!phoneThere && c.pairing.stage === 'not_asked') return err('no_phone', PHONE_WORDS.noPhone);
      if (c.phone && !c.phone.limo.on_it) return err('not_yet', PHONE_WORDS.notYet);
      if (c.busy) return err('busy', PHONE_WORDS.busy);
      c.pairingChild = args.child;
      c.pairing = { stage: 'asking_the_service', device: null, ways: [], line: PHONE_WORDS.asking, advice: null };
      c.busy = 'pairing';
      return { ok: true, value: { started: true } };
    }
    case 'phone.debugging_off':
      if (!phoneThere && c.debugging.stage !== 'off') return err('no_phone', PHONE_WORDS.noPhone);
      if (c.debugging.stage === 'off') return { ok: true, value: { started: false } };
      if (c.busy) return err('busy', PHONE_WORDS.busy);
      c.debugging = { stage: 'switching_off', line: PHONE_WORDS.switchingOff, advice: null };
      c.busy = 'switching_off';
      return { ok: true, value: { started: true } };
    case 'phone.done':
      // It lets go of the phone; a look after this starts afresh.
      Object.assign(c, freshCable({ frozen: c.frozen, plugs: c.plugs, plan: c.plan, case: c.case === 'trouble' ? 'none' : c.case }));
      return { ok: true, value: { done: true } };
    default:
      return err('unknown_request', 'that is not the name of a request');
  }
}
