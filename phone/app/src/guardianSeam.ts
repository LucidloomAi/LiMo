// The guardian's part of the seam: what the guardian's screens ask, as plain
// data. In the console on the PC, src/host.ts sends each request to the
// console's Rust side, which passes it through the door to LiMo's service
// (../../console/README.md, and the service's side in
// ../../windows/README.md, "The console's door"); on a guardian's phone it
// will go over the phone's own link, later. The tests pass a host of their
// own.
//
// The door answers in its own words: a wrong password, a wait, a refusal
// all come with the service's sentence for the screen, and the screens show
// it as it is. The reader below checks each answer and turns it into one
// outcome, so a wrong shape fails in one place with a message.

import { SeamError } from './childSeam';

/** What became of a request, as the guardian's screens need it. */
export type DoorOutcome =
  /** Done: the family's file's own answer, in the hub's shapes. */
  | { readonly kind: 'done'; readonly value: unknown }
  /** The password was not right; this many more wrong tries before a wait. */
  | { readonly kind: 'wrong'; readonly triesBeforeWait: number; readonly words: string }
  /** Too many wrong tries: nothing is checked until then (seconds since 1970). */
  | { readonly kind: 'wait'; readonly until: number; readonly words: string }
  /** The door does not open for this Windows account (a child's, or one Windows would not say). */
  | { readonly kind: 'refused'; readonly words: string }
  /** The door did not do it, and said why: signed out, busy, no family yet, and the rest. */
  | { readonly kind: 'not-done'; readonly reason: string; readonly words: string }
  /** The door could not be asked: the console's own words (not running, lost, not LiMo's service). */
  | { readonly kind: 'problem'; readonly code: string; readonly words: string };

/** What the guardian's screens ask. Every call answers with an outcome and never throws for the door's sake. */
export interface GuardianHost {
  /** What answers, asked first: a child's account is refused at once. */
  about(): Promise<DoorOutcome>;
  /** Signs in with the family's password. */
  signIn(password: string): Promise<DoorOutcome>;
}

type Obj = Record<string, unknown>;

function obj(x: unknown, what: string): Obj {
  if (typeof x !== 'object' || x === null || Array.isArray(x)) throw new SeamError(`${what} is not an object`);
  return x as Obj;
}

function words(o: Obj, key: string, what: string): string {
  const v = o[key];
  if (typeof v !== 'string' || v === '') throw new SeamError(`${what}.${key} is not a sentence`);
  return v;
}

function count(o: Obj, key: string, what: string): number {
  const v = o[key];
  if (typeof v !== 'number' || !Number.isInteger(v) || v < 0) throw new SeamError(`${what}.${key} is not a whole number`);
  return v;
}

/** Reads one reply of the door as the console hands it on. */
export function readDoorAnswer(x: unknown): DoorOutcome {
  const a = obj(x, 'the reply');
  switch (a.outcome) {
    case 'done':
      if (!('value' in a)) throw new SeamError('the reply has no value');
      return { kind: 'done', value: a.value };
    case 'refused':
      return { kind: 'refused', words: words(a, 'message', 'the reply') };
    case 'problem': {
      const p = obj(a.problem, 'the reply.problem');
      if (typeof p.kind !== 'string' || p.kind === '') throw new SeamError('the reply.problem.kind is not a word');
      const said = words(p, 'message', 'the reply.problem');
      if (p.kind === 'wrong_password') return { kind: 'wrong', triesBeforeWait: count(p, 'tries_before_wait', 'the reply.problem'), words: said };
      if (p.kind === 'must_wait') return { kind: 'wait', until: count(p, 'wait_until', 'the reply.problem'), words: said };
      return { kind: 'not-done', reason: p.kind, words: said };
    }
    default:
      throw new SeamError('the reply is not one the screens know');
  }
}

/** Reads the console's word that the door could not be asked: `{ code, words }`. */
export function readDoorProblem(x: unknown): DoorOutcome | null {
  if (typeof x !== 'object' || x === null) return null;
  const p = x as Obj;
  if (typeof p.code !== 'string' || typeof p.words !== 'string' || p.words === '') return null;
  return { kind: 'problem', code: p.code, words: p.words };
}
