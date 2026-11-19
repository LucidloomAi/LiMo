// The theme's three fonts, by the names React Native knows them.
//
// plugins/with-limo-fonts.js puts them into the Android app from
// ../../screens/fonts (the variable fonts, unchanged, with their licences)
// as one family for body text, one for mono, and one family of Fraunces for
// each optical size in fonts.json. A browser sets Fraunces' optical size to
// the font size; display() picks the nearest one the app carries.

import { Platform } from 'react-native';

import { tokens } from '../generated/tokens';
import FONTS from './fonts.json';

/** Body text: Instrument Sans. Weights 400, 500, 600, 700. */
export const bodyFamily: string = tokens.font.body;

/** Labels, times and other data: Spline Sans Mono. Weights 400, 500, 600, 700. */
export const monoFamily: string = tokens.font.mono;

/** Headings and large numbers: the Fraunces family drawn for a font size near `size`. */
export function display(size: number): string {
  let best = FONTS.displaySizes[0];
  for (const candidate of FONTS.displaySizes) {
    if (Math.abs(Math.log(candidate / size)) < Math.abs(Math.log(best / size))) best = candidate;
  }
  const family = `${tokens.font.display} ${best}`;
  // In a browser (the preview of the screens) the name goes into CSS as it
  // is, where a family name with a number in it must be quoted.
  return Platform.OS === 'web' ? `"${family}"` : family;
}
