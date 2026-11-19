// Paper and Ink for React Native. The values come from design/tokens.json,
// through src/generated/tokens.ts (scripts/make-design.mjs): change them
// there, never here. The rules for using them are in design/README.md.

import { tokens } from '../generated/tokens';

export { bodyFamily, display, monoFamily } from './fonts';
export { tokens };

/** The colours. Colour means something: see tokens.meaning. */
export const color = tokens.color;

export const radius = tokens.radius;

/** Borders are this wide; dividers inside a card are dashed. */
export const borderWidth = tokens.border.width;

/** Spacing steps: space(4) is 16. */
export function space(steps: number): number {
  return steps * tokens.space;
}

/**
 * The hard shadow of a card or a button: no blur, set down and to the right.
 * React Native has no hard shadow, so it is drawn as a second shape behind:
 * see the card in src/ui/StartScreen.tsx.
 */
export const hardShadow = tokens.shadow;
