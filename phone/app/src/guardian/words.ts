// The guardian screens' own words, word for word as the approved drawings
// have them (design/screens/console.js): sign in, the first start and Today.
//
// Not here, and never made here: what the door says (a wrong password and
// the tries left, a refusal for a child's account, a problem with the
// service), which the screens show as it comes; and every span of time and
// time of day, which src/guardian/time.ts writes from the service's numbers.
//
// The words name the child and never say "her" or "him": the screens know a
// name, not how a child is spoken of.

import type { AgeBand, Weekday } from '../guardianSeam';

/** A name and a typographic apostrophe s: the child’s, by name. */
export const possessive = (name: string) => `${name}’s`;

export const signIn = {
  heading: 'LiMo’s settings are for a guardian.',
  field: 'Guardian password',
  button: 'Sign in',
  empty: 'Type the password first.',
  forgottenLabel: 'Forgotten it?',
  forgotten:
    'A forgotten password cannot be found again, because LiMo keeps it nowhere but this PC. An administrator of this PC can set a new one.',
  /** A wait after too many wrong tries, at a time on the PC's clock. */
  wait: (time: string) => `Too many wrong tries. Try again at ${time}.`,
  /** Back at sign in after the service ended the session. */
  signedOut: 'LiMo’s settings closed themselves after 15 minutes with nothing done. Type the guardian password to open them again.',
};

export const steps = {
  back: 'Back',
  count: (at: number, of: number) => `Step ${at} of ${of}`,
  dots: (at: number, of: number) => `${at} of ${of}`,
  go: 'Continue',
};

export const welcome = {
  label: 'Welcome',
  heading: 'What LiMo does',
  rows: [
    { icon: 'hourglass', title: 'Counts time', text: 'On this PC and on phones, while a child uses them.' },
    { icon: 'lock', title: 'Locks', text: 'When the day’s time is used up, and at bedtime.' },
    { icon: 'block', title: 'Blocks', text: 'The programs and apps you choose.' },
    { icon: 'eyeoff', title: 'Never looks inside', text: 'No screenshots, nothing typed, and nothing leaves your home.' },
  ] as const,
  /** Said only in the console on the PC, where Windows draws the window. */
  window: 'This window is drawn by a part of Windows that contacts Microsoft when it starts. LiMo itself sends nothing anywhere.',
  tick: 'I am a guardian of this family, and I agree.',
};

const PASSWORD_LINE = 'You type it to open LiMo’s settings on this PC.';
const PIN_LINE = 'You type it on a child’s locked PC or phone to give time or open it.';

export const password = {
  heading: 'The guardian password',
  line: PASSWORD_LINE,
  field: 'Password',
  hint: '8 or more characters',
  again: 'The same again',
  notSame: 'The two are not the same. Type the password again.',
};

export const pin = {
  heading: 'The guardian PIN',
  passwordTitle: 'The password',
  passwordLine: PASSWORD_LINE,
  pinTitle: 'The PIN',
  pinLine: PIN_LINE,
  choose: 'Choose one your child does not know.',
  field: 'PIN',
  hint: '6 to 8 digits',
  again: 'The same again',
  notSame: 'The two are not the same. Type the PIN again.',
};

export const AGE_WORDS: Readonly<Record<AgeBand, string>> = {
  under_six: 'Under 6',
  six_to_nine: '6 to 9',
  ten_to_twelve: '10 to 12',
  thirteen_to_seventeen: '13 to 17',
};

export const child = {
  heading: 'Your child',
  name: 'Name',
  age: 'Age',
  later: 'You can add another child later.',
};

export const DAY_WORDS: Readonly<Record<Weekday, string>> = {
  monday: 'Monday',
  tuesday: 'Tuesday',
  wednesday: 'Wednesday',
  thursday: 'Thursday',
  friday: 'Friday',
  saturday: 'Saturday',
  sunday: 'Sunday',
};

export const timeADay = {
  heading: 'Time a day',
  forAll: (name: string) => `For all of ${possessive(name)} devices together.`,
  every: 'Every day',
  each: 'Each day on its own',
  less: 'Less',
  more: 'More',
  noLimit: 'No limit',
};

export const bedtime = {
  heading: 'Bedtime',
  line: (name: string) => `${possessive(name)} devices lock for these hours. Only a guardian’s PIN opens them.`,
  when: 'When',
  every: 'Every day',
  school: 'School nights only',
  none: 'No bedtime',
  from: 'From',
  to: 'To',
};

export const accounts = {
  heading: 'Accounts on this PC',
  line: 'Say whose each account is. A guardian’s account has no limit.',
  admin: 'Administrator',
  standard: 'Standard',
  signedInNow: 'signed in now',
  guardians: 'A guardian’s',
  later: 'Later',
  laterLine: 'An account left for later stays locked until a guardian says whose it is. An administrator’s account is never locked.',
  warnBold: (child: string) => `${possessive(child)} account is an administrator.`,
  warnText: 'LiMo cannot hold against an administrator. Make it a standard account in Windows Settings, Accounts, Other users.',
  warnSteps: (account: string) => [`There, choose ${account}, then Change account type.`, 'Pick Standard User, then OK.'] as const,
  changesNone: 'LiMo changes no account itself.',
};

export const done = {
  heading: 'The family is set up.',
  passwordAndPin: 'Guardian password and PIN',
  child: (name: string, band: string) => `${name}, ${band}`,
  aDay: (figure: string) => `${figure} a day`,
  bedtime: (from: string, to: string) => `Bedtime from ${from} to ${to}`,
  schoolNights: ' on school nights',
  noBedtime: 'No bedtime',
  account: (name: string) => `${possessive(name)} account on this PC`,
  phone: (name: string) => `${possessive(name)} phone`,
  phoneLine: 'Connect it to this PC with a USB cable.',
  later: 'Later',
  addNow: (name: string) => `Add ${possessive(name)} phone now`,
};

export const tabs = { today: 'Today', rules: 'Rules', devices: 'Devices', more: 'More' };

export const today = {
  leftToday: 'Left today',
  locked: 'Locked',
  nothingWaiting: 'Nothing is waiting for you.',
  whenAsks: (name: string) => `When ${name} asks for more time, the request shows here.`,
  waitingForYou: 'Waiting for you',
  asks: (name: string, span: string) => `${name} asks for ${span}`,
  /** Where and when a request was made: "On this PC · 6:04 pm". */
  onDevice: (device: string, time: string) => `On ${device} · ${time}`,
  thisPcInSentence: 'this PC',
  quote: (reason: string) => `“${reason}”`,
  give: (span: string) => `Give ${span}`,
  no: 'No',
  plus: (minutes: number) => `+${minutes} min`,
  lockNow: 'Lock now',
  openUntil: 'Open until',
  lift: 'Lift',
  thisPc: 'This PC',
  inUse: 'In use',
  notInUse: 'Not in use',
  lastHeard: (ago: string) => `Last heard ${ago}`,
  notHeard: 'Not heard yet',
  notAdded: (name: string) => `${possessive(name)} phone is not added yet.`,
  addPhone: (name: string) => `Add ${possessive(name)} phone`,
};

/** The tabs and the screens not drawn yet. */
export const coming = 'This part of LiMo’s settings is still being drawn, and comes in a later version.';
