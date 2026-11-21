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

import type { AgeBand, PhonePermission, Weekday } from '../guardianSeam';
import { andList } from './time';

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
  needsGuardian: 'Needs a guardian',
  seeWhatToDo: 'See what to do',
  /** Under the first thing switched off, when there are more: how many more. */
  more: (count: number) => `And ${count} more`,
};

/** The mark under a device that needs a guardian, by what is switched off on it: short, in coral capitals. */
export const NEED_MARK: Readonly<Record<PhonePermission, string>> = {
  notifications: 'Notifications are off',
  phone: 'Phone access is off',
  usage_access: 'Usage access is off',
  accessibility: 'Accessibility is off',
  battery: 'Battery is off',
  protection: 'Protection is off',
};

/** The tabs and the screens not drawn yet. */
export const coming = 'This part of LiMo’s settings is still being drawn, and comes in a later version.';

/** How many, in words, up to ten; past ten the number itself. */
const COUNT = ['No', 'One', 'Two', 'Three', 'Four', 'Five', 'Six', 'Seven', 'Eight', 'Nine', 'Ten'];
export const countWords = (n: number) => COUNT[n] ?? String(n);

// ---------------------------------------------------------------------------
// The second sheet

export const sheet = {
  back: 'Back',
  change: 'Change',
};

export const rules = {
  timeADay: 'Time a day',
  bedtime: 'Bedtime',
  away: 'Away from home',
  /** How much of the day a phone away from home may use, by the share. */
  awayLine: (name: string, percent: number) => {
    const share =
      percent === 0
        ? 'none of the day'
        : percent === 25
          ? 'up to a quarter of the day'
          : percent === 50
            ? 'up to half of the day'
            : percent === 100
              ? 'all of the day'
              : `up to ${percent} percent of the day`;
    return `When ${possessive(name)} phone cannot reach this PC, it may use ${share}.`;
  },
  awayChoices: [
    [0, 'None'],
    [25, 'A quarter'],
    [50, 'Half'],
    [100, 'All'],
  ] as const,
  blocked: 'Blocked',
  allowed: 'Always allowed',
  allowedLine: (name: string) => `They never use up ${possessive(name)} time, and stay open when ${possessive(name)} devices are locked.`,
  neverBlocked: 'The phone call app and emergency calls are never blocked.',
  /** The phone call app, in the list of always allowed apps on a child with a phone. */
  phoneApp: 'Phone',
  neverBlockedMark: 'Never blocked',
  offer: (name: string) => `Add from what ${name} used today`,
  addApp: 'Add an app',
  /** Add an app: a device's own list. */
  pcKnown: (name: string) => `Only programs seen running in ${possessive(name)} accounts since LiMo last started are known.`,
  noList: (device: string) => `${device} has not sent its list of apps yet.`,
  find: 'Find an app',
  addThis: (app: string) => `Add ${app}`,
  allListed: 'Every app on this list is in the rules already.',
  cut: (shown: number, total: number) => `Showing ${shown} of ${total}.`,
  /** Apps in the rules that no device's list has. */
  onNoDevice: 'On no device’s list yet',
  takeOff: 'Take it off',
  use: 'Today’s use',
  onThisPc: 'On this PC',
  onDevice: (device: string) => `On ${device}`,
  more: (name: string) => `More for ${name}`,
  dayStarts: 'A new day starts',
  atMidnight: 'At midnight',
  at: (time: string) => `At ${time}`,
  nameAndAge: 'Name and age',
  remove: (name: string) => `Remove ${name}`,
  asksAgain: 'Asks for the password again.',
  undo: 'Undo changes',
  save: 'Save',
  saveFirst: 'Save or undo these changes first.',
  notSaved: (count: number) => `Not saved. ${countWords(count)} ${count === 1 ? 'thing' : 'things'} to fix.`,
  saved: (name: string) => `Saved. ${possessive(name)} devices have the new rules within a minute.`,
};

export const devices = {
  rename: 'Rename',
  itsName: 'Its name',
  saveName: 'Save the name',
  child: 'Child',
  paired: 'Paired',
  lastHeard: 'Last heard',
  justNow: 'Just now',
  now: 'Now',
  inUse: 'In use',
  notInUse: 'Not in use',
  needs: 'What LiMo needs on it',
  six: {
    notifications: 'Notifications',
    phone: 'Phone',
    usage_access: 'Usage access',
    accessibility: 'Accessibility',
    battery: 'Battery',
    protection: 'Protection',
  } as const,
  on: 'On',
  notOn: 'Not on',
  /** A step no note has said yet, as Add a phone draws it. */
  notYet: 'Not yet',
  protection: 'Protection',
  protectionLevel: { off: 'Off', base: 'On', strict: 'On, strict' } as const,
  phone: 'Phone',
  limoOnIt: 'LiMo on it',
  /** Android's release from its API level, as a phone gives it: 33 is "Android 13". */
  android: (api: number) => {
    const release: Readonly<Record<number, string>> = { 26: '8.0', 27: '8.1', 28: '9', 29: '10', 30: '11', 31: '12', 32: '12L', 33: '13', 34: '14', 35: '15', 36: '16' };
    return release[api] ? `Android ${release[api]}` : `Android, API level ${api}`;
  },
  usbTitle: 'USB debugging is still on.',
  usbText: 'While it is on, any PC with a cable could stop LiMo on this phone.',
  clockAhead: (span: string) => `Its clock is ${span} ahead of this PC’s.`,
  clockBehind: (span: string) => `Its clock is ${span} behind this PC’s.`,
  clockLine: 'LiMo keeps its own count, so no time is gained.',
  removePhone: 'Remove this phone',
  removeLine: 'Asks for the password again. The phone is told at its next contact, lets go of its limits, and says LiMo can be removed.',
  addPhone: 'Add a phone',
  thisPc: 'This PC',
  accounts: 'Windows accounts',
  guardians: 'A guardian’s, no limit',
  notSaid: 'Not said yet',
  /** The choices for whose an account is, under its row once Change is tapped. */
  guardiansChoice: 'A guardian’s',
  notSaidChoice: 'Not said',
  pause: 'Pause the count',
  pauseLine: 'After this long with no keyboard or mouse input, unless something plays: in full screen, with sound, or keeping the screen awake.',
  unlinked: 'An account LiMo was not told about',
  locked: 'Locked',
  free: 'Used freely',
  lockedLine: 'It stays locked until a guardian says whose it is. An administrator’s account is never locked.',
  freeLine: 'It is used freely, and a guardian is told once a day. An administrator’s account is never locked.',
};

export const code = {
  heading: 'A one time code',
  line: 'Time for a child whose device is out of reach or locked, read out over a call.',
  for: 'For',
  on: 'On',
  howMuch: 'How much',
  make: 'Make a code',
  later: 'A guardian who goes out can note a code now and read it out later today.',
  made: 'The code',
  what: (device: string, span: string) => `${device} · ${span}`,
  read: (name: string, device: string) => `Read it to ${name}. It works once, today only, on ${device}, when its time is used up.`,
  thisPcInSentence: 'this PC',
  /** How many more codes the device can be given today, when the door says. */
  left: (n: number, onPc: boolean) => (n === 0 ? `You can make no more for this ${onPc ? 'PC' : 'phone'} today.` : `You can make ${n} more for this ${onPc ? 'PC' : 'phone'} today.`),
  another: 'Make another',
  done: 'Done',
  howMuchChoices: [
    [15, '15 min'],
    [30, '30 min'],
    [60, '1 hour'],
  ] as const,
};

export const more = {
  rows: [
    { key: 'code', icon: 'code', title: 'A one time code', text: 'Time for a child out of reach, read out over a call.' },
    { key: 'secrets', icon: 'key', title: 'Password and PIN', text: 'Change either one.' },
    { key: 'about', icon: 'info', title: 'About LiMo', text: 'Its version, its licences, and removing it.' },
  ] as const,
  back: 'More',
};

export const secrets = {
  password: 'The guardian password',
  now: 'The one you use now',
  newOne: 'A new one',
  again: 'The same again',
  changePassword: 'Change the password',
  pin: 'The guardian PIN',
  choose: 'Choose one your child does not know.',
  newPin: 'A new PIN',
  changePin: 'Change the PIN',
  passwordChanged: 'The password is changed.',
  pinChanged: 'The PIN is changed.',
};

export const about = {
  heading: 'About LiMo',
  forWindows: 'For Windows',
  carries: 'This PC carries LiMo for phones, and puts it on a phone over the cable.',
  licences: 'Licences',
  limo: 'LiMo',
  licence: 'Apache License 2.0',
  others: 'Others',
  notices: 'Third party notices',
  remove: 'Remove',
  removeLimo: 'Remove LiMo from this PC',
  removeLimoLine: 'Asks for the password, then opens Windows’ own uninstaller.',
  deleteFamily: 'Delete the family',
  asksAgain: 'Asks for the password again.',
  read: 'Read',
  readComing: 'Reading the licences here comes in the next version of LiMo’s settings.',
  moreInfo: 'More info on',
  address: 'https://lucidloom.ai',
};

/** Add a phone, A1 to A9: one step a screen, the cable's state at the foot. */
export const addPhone = {
  steps: 9,
  phoneOf: (name: string) => `${possessive(name)} phone`,
  start: 'Start',
  next: 'Next',
  done: 'Done',
  tryAgain: 'Try again',
  a1: {
    heading: 'Add a phone',
    whose: 'Whose phone',
    youNeed: 'You need',
    cable: 'Its USB cable',
    cableText: 'One that moves files, ideally the one that came with it.',
    time: 'About ten minutes',
  },
  a2: {
    heading: 'Switch on USB debugging',
    line: 'Only a person can switch it on, on the phone itself. On most phones:',
    steps: [
      ['Open Settings, then About phone.', 'On a Samsung phone, then Software information.'],
      ['Tap Build number seven times.', 'The phone may ask for its own PIN, then says that developer mode is on.'],
      ['Go back to Settings, open Developer options, and switch on USB debugging.', 'Developer options is near the end of Settings.'],
      ['Plug the phone into this PC.', 'This screen moves on by itself when it sees USB debugging.'],
    ] as const,
    noteLabel: 'On a newer Samsung phone',
    note: 'Also switch off Auto Blocker, in Settings, Security and privacy. It blocks installs over the cable.',
  },
  a3: {
    heading: 'Allow this PC on the phone',
    line: 'Unlock the phone. It asks: Allow USB debugging? Tick Always allow from this computer, then tap Allow.',
    key: 'The phone shows this PC’s key',
    same: 'It should read the same. If it does not, tap Cancel on the phone. The phone asks once; after that it knows this PC.',
  },
  a4: {
    heading: (name: string) => `Put LiMo on ${possessive(name)} phone`,
    putting: (name: string) => `Putting LiMo on ${possessive(name)} phone`,
    asksNothing: 'Installing asks nothing on the phone.',
  },
  a5: {
    heading: (name: string) => `This phone is now ${possessive(name)}.`,
    pairing: (name: string) => `Pairing ${possessive(name)} phone`,
    paired: 'Paired',
    opened: (name: string) => `LiMo opened on the phone by itself, with ${possessive(name)} name on it.`,
    itsName: 'Its name',
  },
  a6: {
    heading: 'Now set up LiMo on the phone',
    line: 'It asks for six things. Each one ticks here when the phone says it is on.',
    byItself: 'This screen moves on by itself when the phone says it is set up.',
  },
  a7: {
    heading: 'At home',
    line: (name: string) => `Checking that ${possessive(name)} phone can reach this PC on the home Wi-Fi.`,
    reached: 'Reached over the home Wi-Fi',
    notYet: 'Not reached over the home Wi-Fi yet. Next goes on without it.',
  },
  a8: {
    heading: 'Switch USB debugging off',
    line: (name: string) => `While it is on, any PC with a cable could stop LiMo on ${possessive(name)} phone. LiMo can switch it off now, over the cable.`,
    leave: 'Leave it on',
    then: 'Then Devices says that it is still on.',
    byHandLabel: 'By hand',
    byHand: 'On the phone: Settings, Developer options, then switch off USB debugging.',
    button: 'Switch it off for me',
  },
  a9: {
    heading: (name: string) => `${possessive(name)} phone is added.`,
    ticks: {
      installed: 'LiMo is on the phone',
      paired: 'Paired with this PC',
      setUp: 'Set up on the phone, all six on',
      setUpNot: 'Not set up on the phone yet',
      home: 'Reached over the home Wi-Fi',
      homeNot: 'Not reached over the home Wi-Fi yet',
      off: 'USB debugging is off',
      on: 'USB debugging is still on',
    },
    unplug: 'You can unplug the phone.',
  },
};

/** Asking for the password again before something that cannot be undone: deleting the family is the drawn pattern for all four. */
export const confirm = {
  password: 'Guardian password',
  deleteFamily: {
    heading: 'Delete the family?',
    line: (children: readonly string[]) =>
      children.length
        ? `Everything LiMo keeps on this PC is deleted: ${andList(children)}, their rules, their time and their devices. It cannot be undone.`
        : 'Everything LiMo keeps on this PC is deleted. It cannot be undone.',
    phones: (phones: readonly string[]) =>
      phones.length === 0
        ? ''
        : phones.length > 1
          ? `${andList(phones)} are told at their next contact, let go of their limits, and say LiMo can be removed. `
          : `${phones[0]} is told at its next contact, lets go of its limits, and says LiMo can be removed. `,
    stays: 'LiMo stays on this PC and starts again from the first step.',
    button: 'Delete the family',
  },
  removeChild: {
    heading: (name: string) => `Remove ${name}?`,
    line: (name: string) => `Everything LiMo keeps for ${name} is deleted: the rules, the time and the devices. It cannot be undone.`,
    button: (name: string) => `Remove ${name}`,
  },
  removePhone: {
    heading: (phone: string) => `Remove ${phone}?`,
    line: 'It is told at its next contact, lets go of its limits, and says LiMo can be removed.',
    button: 'Remove this phone',
  },
  removeLimo: {
    heading: 'Remove LiMo from this PC?',
    line: 'The password allows LiMo to be removed for a few minutes. Then remove it in Windows’ own list of apps.',
    button: 'Allow removing LiMo',
    allowed: (time: string) => `LiMo can be removed until ${time}. Remove it in Windows Settings, Apps, Installed apps.`,
  },
  /** After too many wrong tries. */
  wait: (time: string) => `Too many wrong tries. Try again at ${time}.`,
};
