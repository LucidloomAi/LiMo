// The guardian screens' own words. The sign in's are the guardian console's
// first screen as it is described for its drawing; the drawing is still to
// be made and approved, and these words are kept with it.
//
// The words name the child and never say "her" or "him".

import { possessive } from '../child/words';

export const signIn = {
  heading: 'LiMo’s settings are for a guardian.',
  text: 'Type the family’s guardian password to open them.',
  field: 'Guardian password',
  button: 'Open the settings',
  checking: 'Checking the password',
  wrong: (triesBeforeWait: number) =>
    triesBeforeWait === 0
      ? 'That password is not right. The next wrong try starts a wait.'
      : `That password is not right. ${triesBeforeWait} more ${triesBeforeWait === 1 ? 'try' : 'tries'} before a wait.`,
  wait: (time: string) => `Too many wrong tries. Try again at ${time}.`,
  empty: 'Type the password first.',
  forgotten:
    'A forgotten password cannot be found again, because LiMo keeps it nowhere but this PC. An administrator of this PC can set a new one.',
};

export const notForThisAccount = {
  stamp: 'Not for this account',
  heading: 'LiMo’s settings are for a guardian.',
  child: (child: string) => `LiMo’s settings cannot be opened from ${possessive(child)} account. A guardian opens them from their own.`,
  unknown: 'LiMo cannot tell whose Windows account this is, so its settings do not open here.',
};

export const signedIn = {
  stamp: 'Signed in',
  heading: 'The settings are open.',
  text: 'The rest of the guardian screens come in a later build. This one shows the way in.',
};

/** A moment of the day on this PC, such as "18:42" or "6:42 pm", as the PC writes times. */
export function timeOfDay(unixSeconds: number): string {
  return new Date(unixSeconds * 1000).toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' });
}
