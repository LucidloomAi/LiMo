// The design's icons, painted the design's way. Used by make-design.mjs and by
// the launcher icon in plugins/with-limo-android.js.
//
// Every icon is painted by design/icons/clay.js from its drawing in
// design/icons/shapes.js, so the app never paints an icon any other way.

import path from 'node:path';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';

const require = createRequire(import.meta.url);
const repoDir = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..', '..', '..');
export const iconsDir = path.join(repoDir, 'design', 'icons');

const CLAY = require(path.join(iconsDir, 'clay.js'));
const SHAPES = require(path.join(iconsDir, 'shapes.js'));

/**
 * Every icon: { clay, use, paper, ink }, where paper is the icon on paper or
 * a card and ink the icon on an ink surface, both as SVG text. ink is null
 * when it is the same as paper, which is so for every icon but the plain
 * (ink coloured) ones.
 */
export function paintIcons() {
  const out = {};
  for (const [name, icon] of Object.entries(SHAPES)) {
    const paper = CLAY.svg(icon.shapes, { size: 48, clay: icon.clay, id: `limo-${name}` });
    const ink = CLAY.svg(icon.shapes, { size: 48, clay: icon.clay, id: `limo-${name}-ink`, on: 'dark' });
    const same = ink.replaceAll(`limo-${name}-ink`, `limo-${name}`) === paper;
    out[name] = { clay: icon.clay, use: icon.use, paper, ink: same ? null : ink };
  }
  return out;
}

/**
 * The SVG text clay.js writes, as a tree: [tag, attributes, children].
 * Only for that text: elements and attributes, no text content, no comments.
 */
export function parseSvg(text, fail) {
  const root = ['root', {}, []];
  const stack = [root];
  const tag = /<(\/?)([a-zA-Z]+)((?:\s+[a-zA-Z][a-zA-Z0-9:-]*="[^"]*")*)\s*(\/?)>/g;
  let at = 0;
  for (let m = tag.exec(text); m; m = tag.exec(text)) {
    if (text.slice(at, m.index).trim()) fail('text between elements');
    at = tag.lastIndex;
    const [, closing, element, rawAttrs, selfClosing] = m;
    if (closing) {
      const open = stack.pop();
      if (stack.length === 0 || open[0] !== element) fail(`</${element}> does not close <${open?.[0]}>`);
      continue;
    }
    const attrs = {};
    for (const a of rawAttrs.matchAll(/([a-zA-Z][a-zA-Z0-9:-]*)="([^"]*)"/g)) attrs[a[1]] = a[2];
    const node = [element, attrs, []];
    stack[stack.length - 1][2].push(node);
    if (!selfClosing) stack.push(node);
  }
  if (stack.length !== 1) fail('an element is not closed');
  if (text.slice(at).trim()) fail('text after the last element');
  if (root[2].length !== 1) fail('more than one root element');
  return root[2][0];
}
