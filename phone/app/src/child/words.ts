// The child form's own words: setup, the child's introduction, the labels of
// Today and How it works, and the banner's labels. They are the drawings'
// words (design/screens/phone-setup.js and phone.js), word for word.
//
// Words about time and rules are not here, and neither is the sentence the
// banner says about a step that is switched off: they come from the phone's
// engine, through the host (TodayView, RulesView).
//
// The words name the child and never say "her" or "him": the app knows a
// name, not how a child is spoken of.

import type { StepId } from '../host';
import type { IconName } from '../icons/Icon';
import type { Rich } from '../ui/paper/parts';

/** A name and a typographic apostrophe s: the child’s, by name. */
export const possessive = (name: string) => `${name}’s`;

export const notSetUp = {
  heading: 'This phone is not set up yet.',
  text: ['Connect it to the family PC with a USB cable, then choose ', { strong: 'Add a phone' }, ' in LiMo on the PC.'] as Rich,
};

export const welcome = {
  eyebrow: 'For a guardian',
  heading: (name: string) => `This phone is now ${possessive(name)}.`,
  text: 'Six short steps finish the setup. Each one asks Android for one thing, and says why.',
  button: 'Start',
};

export const agree = {
  label: 'Before you start',
  heading: 'What LiMo does here',
  rows: [
    { icon: 'hourglass', title: 'Counts time', text: 'How long each app is open, while the screen is on.' },
    { icon: 'lock', title: 'Covers and locks', text: 'A blocked app is covered. The phone locks when today’s time is used up, or at bedtime.' },
    { icon: 'eyeoff', title: 'Never reads', text: 'Messages, web addresses or anything typed. It knows which app is open, never what is in it.' },
  ] satisfies readonly { icon: IconName; title: string; text: string }[],
  line: 'Nothing leaves your home. This phone talks only to your family PC.',
  tick: (name: string) => `I am ${possessive(name)} guardian, and I agree.`,
  button: 'Continue',
};

export type StepWords = {
  icon: IconName;
  title: string;
  why: (name: string) => string;
  what: Rich;
  button: string;
  /** A quiet outlined note under what happens; `only: 'samsung'` shows it only on a Samsung phone. */
  note?: { label: string; text: string; only?: 'samsung' };
};

const allow: Rich = ['Android asks once. Choose ', { strong: 'Allow' }, '.'];

export const steps: Readonly<Record<StepId, StepWords>> = {
  notifications: {
    icon: 'bell',
    title: 'Notifications',
    why: (name) => `So ${name} sees the time left, and a warning before the phone locks.`,
    what: allow,
    button: 'Allow notifications',
  },
  phone: {
    // A handset, so the Phone step differs from S1's picture of the device (`phone`).
    icon: 'call',
    title: 'Phone',
    why: () => 'So a lock never hides a call. LiMo is told that a call is ringing or on, never who is calling.',
    what: allow,
    button: 'Allow',
    note: {
      label: 'What Android asks',
      text: 'Android’s words are “make and manage phone calls”. LiMo never makes a call. It only learns that one is ringing or on.',
    },
  },
  usage: {
    icon: 'chart',
    title: 'Usage access',
    why: () => 'So LiMo can count the time in each app. It gets app names and minutes from Android, nothing else.',
    what: ['Android opens its own settings. Find LiMo in the list and switch it on.'],
    button: 'Open Android’s settings',
  },
  accessibility: {
    icon: 'shield',
    title: 'Accessibility',
    why: () =>
      'So LiMo can see which app is in front, and cover it when it is blocked or the time is used up. It is told the app’s name only, never what is on the screen.',
    what: ['Android opens its own settings and shows a warning of its own. Find LiMo under Installed apps and switch it on.'],
    button: 'Open Android’s settings',
  },
  battery: {
    icon: 'battery',
    title: 'Battery',
    why: () => 'So Android does not put LiMo to sleep to save battery.',
    what: allow,
    button: 'Allow',
    note: {
      label: 'On a Samsung phone',
      text: 'Also open Settings, then Battery (on some phones Battery and device care, then Battery), then Background usage limits, then Never sleeping apps, and add LiMo there.',
      only: 'samsung',
    },
  },
  protection: {
    icon: 'key',
    title: 'Protection',
    why: () => 'So a guardian’s PIN is needed to switch this off, and LiMo cannot be removed while it is on.',
    what: ['Android asks you to make LiMo a device admin app. Choose ', { strong: 'Activate' }, '.'],
    button: 'Turn on protection',
  },
};

export const step = {
  why: 'Why',
  what: 'What happens',
  count: (at: number, of: number) => `Step ${at} of ${of}`,
  on: 'On',
  notOnYet: 'Not on yet',
  onLine: 'Android says it is on.',
  /** Under the stamp when Android's own screen could not be opened. */
  notOpened: 'Android did not open it. Try again.',
  /** Under the stamp once Android's screen was opened twice and the step is still off. */
  stuck: 'Android has not turned this on. You can go on and come back to it from Today.',
  goOn: 'Go on without it',
  next: 'Next',
};

/** How many steps, in words, as the drawings say it ("Two"): there are six, and past six the number itself. */
const COUNT = ['No', 'One', 'Two', 'Three', 'Four', 'Five', 'Six'];
export const countInWords = (count: number) => COUNT[count] ?? String(count);

/**
 * A sentence starts with a capital, also when it starts with the child's
 * name and the phone keeps no name: then the name is "your child".
 */
const sentence = (text: string) => text.charAt(0).toUpperCase() + text.slice(1);

export const done = {
  heading: 'This phone is set up.',
  /** At the end of the row of a step Android does not say is on. */
  notOn: 'Not on',
  /** Under the rows when steps are off, in setup order: which, or how many, and where a guardian can turn them on. */
  offLine: (off: readonly StepId[], name: string) => {
    const one = off.length === 1;
    const what = one ? `${steps[off[0]].title} is not on yet.` : `${countInWords(off.length)} steps are not on yet.`;
    return `${what} ${sentence(possessive(name))} Today screen shows a guardian how to turn ${one ? 'it' : 'them'} on.`;
  },
  after: (name: string) => `Hand the phone to ${name}. The next three screens are ${possessive(name)}.`,
  button: (name: string) => `Show ${name} how it works`,
};

export type IntroWords = { icon: IconName; eyebrow?: (name: string) => string; title: string; text: Rich; button: string };

export const intro: readonly IntroWords[] = [
  {
    icon: 'hourglass',
    eyebrow: (name) => `For ${name}`,
    title: 'What LiMo counts',
    text: ['The time your screen is on with an app open. Calls, the home screen and LiMo itself do not count.'],
    button: 'Next',
  },
  {
    icon: 'eyeoff',
    title: 'What LiMo never looks at',
    text: ['Your messages, what you type, and the pages you visit. It only knows which app is open, and for how long.'],
    button: 'Next',
  },
  {
    icon: 'hand',
    title: 'How to ask for more time',
    text: [
      'When the time is used up, tap ',
      { strong: 'Ask for more time' },
      '. A guardian answers. A guardian who is away can read you a code to type.',
    ],
    button: 'Got it',
  },
];

export const introCount = (at: number, of: number) => `${at} of ${of}`;

export const tabs = { today: 'Today', how: 'How it works' };

export const today = {
  left: 'Left today',
  quiet: 'Looking at this screen does not use your time.',
  ask: 'Ask for more time',
  apps: 'Today on this phone',
  needsGuardian: 'Needs a guardian',
  /** Under the engine's sentence when more than one step is off: how many more there are. */
  more: (count: number) => `And ${count} more`,
  showGuardian: 'Show a guardian what to do',
};

export const how = {
  rules: 'Your rules',
  timeADay: 'Time a day',
  screensOff: 'Screens off',
  alwaysOpen: 'Always open',
  notAllowed: 'Not allowed',
  does: 'What LiMo does',
  about: 'About',
  name: (version: string) => `LiMo ${version}`,
  /** Then the maker's address, as plain text: the design's own (design/tokens.json, maker.site). */
  more: 'More info on ',
  site: 'https://lucidloom.ai',
};
