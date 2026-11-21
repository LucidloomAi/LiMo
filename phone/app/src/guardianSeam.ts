// The guardian's part of the seam: what the guardian's screens ask, as plain
// data. In the console on the PC, src/host.ts sends each request to the
// console's Rust side, which passes it through the door to LiMo's service
// (console/README.md); on a guardian's phone it will go over the phone's
// own link, later. The tests pass a host of their own.
//
// The door answers in its own words (console/src/protocol.rs, Answer); the
// reader below checks each answer and turns it into one outcome, so a wrong
// shape fails in one place with a message.

import { SeamError } from './childSeam';

/** What the door can answer a guardian's screen, as the screen needs it. */
export type DoorOutcome =
  /** The door answered "about": whether a family exists on this PC. */
  | { readonly kind: 'about'; readonly family: boolean }
  /** The password was right. */
  | { readonly kind: 'signed-in' }
  /** The password was not right; this many more wrong tries before a wait. */
  | { readonly kind: 'wrong'; readonly triesBeforeWait: number }
  /** Too many wrong tries: no password is looked at until then (seconds since 1970). */
  | { readonly kind: 'wait'; readonly until: number }
  /** The console runs in a child's Windows account: the settings never open there. */
  | { readonly kind: 'child-account'; readonly child: string }
  /** The door could not tell whose account the console runs in. */
  | { readonly kind: 'unknown-account' }
  /** The door could not be asked, in the console's own words for a guardian. */
  | { readonly kind: 'problem'; readonly code: string; readonly words: string };

/** What the guardian's screens ask. Every call answers with an outcome and never throws for the door's sake. */
export interface GuardianHost {
  /** What answers, asked first: a child's account is told at once. */
  about(): Promise<DoorOutcome>;
  /** Signs in with the family's password. */
  signIn(password: string): Promise<DoorOutcome>;
}

type Obj = Record<string, unknown>;

function obj(x: unknown, what: string): Obj {
  if (typeof x !== 'object' || x === null || Array.isArray(x)) throw new SeamError(`${what} is not an object`);
  return x as Obj;
}

function count(o: Obj, key: string, what: string): number {
  const v = o[key];
  if (typeof v !== 'number' || !Number.isInteger(v) || v < 0) throw new SeamError(`${what}.${key} is not a whole number`);
  return v;
}

/** Reads one answer of the door, as console/src/protocol.rs writes it. */
export function readDoorAnswer(x: unknown): DoorOutcome {
  const a = obj(x, 'the answer');
  switch (a.answer) {
    case 'about':
      if (typeof a.family !== 'boolean') throw new SeamError('the answer.family is not true or false');
      return { kind: 'about', family: a.family };
    case 'signed_in':
      return { kind: 'signed-in' };
    case 'wrong_password':
      return { kind: 'wrong', triesBeforeWait: count(a, 'tries_before_wait', 'the answer') };
    case 'must_wait':
      return { kind: 'wait', until: count(a, 'until', 'the answer') };
    case 'refused': {
      const why = obj(a.why, 'the answer.why');
      if (why.refusal === 'child_account') {
        if (typeof why.child !== 'string' || why.child === '') throw new SeamError('the answer.why.child is not a name');
        return { kind: 'child-account', child: why.child };
      }
      if (why.refusal === 'unknown_account') return { kind: 'unknown-account' };
      throw new SeamError('the answer.why.refusal is not one the screens know');
    }
    default:
      throw new SeamError('the answer is not one the screens know');
  }
}

/** Reads the console's word that the door could not be asked: `{ code, words }`. */
export function readDoorProblem(x: unknown): DoorOutcome | null {
  if (typeof x !== 'object' || x === null) return null;
  const p = x as Obj;
  if (typeof p.code !== 'string' || typeof p.words !== 'string' || p.words === '') return null;
  return { kind: 'problem', code: p.code, words: p.words };
}
