// The guardian screens' own words. The sign in's are the guardian console's
// first screen as it is described for its drawing; the drawing is still to
// be made and approved, and these words are kept with it.
//
// What the door says (a wrong password and the tries left, a wait, a
// refusal for a child's account) is the service's own sentence, shown as it
// comes, never made here.

export const signIn = {
  heading: 'LiMo’s settings are for a guardian.',
  text: 'Type the family’s guardian password to open them.',
  field: 'Guardian password',
  button: 'Open the settings',
  checking: 'Checking the password',
  empty: 'Type the password first.',
  forgotten:
    'A forgotten password cannot be found again, because LiMo keeps it nowhere but this PC. An administrator of this PC can set a new one.',
};

export const notForThisAccount = {
  stamp: 'Not for this account',
  heading: 'LiMo’s settings are for a guardian.',
};

export const signedIn = {
  stamp: 'Signed in',
  heading: 'The settings are open.',
  text: 'The rest of the guardian screens come in a later build. This one shows the way in.',
};
