// Turns the design into what the app can use. The design stays the one source:
// nothing written here is committed, and every build and type check runs this
// first (it also runs after `npm install`).
//
//   node scripts/make-design.mjs
//
// Reads, from the repository:
//   design/tokens.json         the Paper and Ink values
//   design/icons/shapes.js     the icon drawings, as data
//   design/icons/clay.js       how a drawing is painted
//   design/icons/svg/*.svg     the exported icons, to check they are up to date
//   docs/images/lucidloom-logo.png
//
// Writes, into src/generated/ (git ignored):
//   tokens.ts                  the tokens as a typed constant
//   icons.ts                   every icon as a small tree that src/icons/Icon.tsx
//                              draws with react-native-svg
//   lucidloom-logo.png         the maker's logo, for the start screens
//
// and into modules/limo-child/android/src/generated/res/ (git ignored):
//   values/limo_tokens.xml     the colours and sizes as Android resources, for
//                              the child form's own Kotlin views
//   drawable/limo_icon_*.xml   every icon as a vector drawable for those views
//                              (scripts/vector-drawable.mjs); _ink for the
//                              icons that change on an ink surface
//
// A file is only rewritten when its content changed, so Metro and Gradle see
// no change after a run that changed nothing.
//
// It stops with a message, writing nothing, when the design breaks something
// a machine can check: a token the app needs is missing, or design/icons/svg
// is older than shapes.js (run `node export.js` in design/icons).

import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { iconsDir, paintIcons, parseSvg } from './icons.mjs';
import { toVectorDrawable } from './vector-drawable.mjs';

const appDir = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const repoDir = path.resolve(appDir, '..', '..');
const designDir = path.join(repoDir, 'design');
const outDir = path.join(appDir, 'src', 'generated');
const resOutDir = path.join(appDir, 'modules', 'limo-child', 'android', 'src', 'generated', 'res');

const problems = [];

// ---------------------------------------------------------------------------
// Tokens

const tokens = JSON.parse(fs.readFileSync(path.join(designDir, 'tokens.json'), 'utf8'));

const NEEDED = {
  color: ['paper', 'paper2', 'card', 'ink', 'inkSoft', 'marigold', 'marigoldDeep', 'teal', 'coral', 'border', 'shadowInk'],
  font: ['display', 'body', 'mono'],
  radius: ['sm', 'md', 'lg', 'pill'],
  border: ['width', 'divider'],
  shadow: ['card', 'button'],
  icon: ['grid', 'row', 'tile', 'heading', 'emptyScreen'],
  maker: ['name', 'words', 'logoSize'],
};
for (const [group, keys] of Object.entries(NEEDED)) {
  for (const key of keys) {
    if (tokens[group]?.[key] === undefined) problems.push(`tokens.json has no ${group}.${key}`);
  }
}
if (typeof tokens.space !== 'number') problems.push('tokens.json space is not a number');
for (const [key, value] of Object.entries(tokens.color ?? {})) {
  if (!/^#[0-9a-f]{6}$/i.test(value) && !/^rgba\(\d+,\d+,\d+,(0|1|0?\.\d+)\)$/.test(value)) {
    problems.push(`tokens.json color.${key} is ${value}: the app reads #rrggbb or rgba(r,g,b,a) only`);
  }
}

// A CSS shadow such as "4px 4px 0 rgba(27,38,56,0.85)", as numbers React
// Native can use. The theme's card shadow is hard (no blur): the app draws it
// as a second shape set down and to the right.
function parseShadow(name, css) {
  const m = /^(-?[\d.]+)(?:px)? (-?[\d.]+)(?:px)? (-?[\d.]+)(?:px)? (rgba\([^)]*\)|#[0-9a-f]{6})$/i.exec(String(css).trim());
  if (!m) {
    problems.push(`tokens.json ${name} is "${css}": expected "X Y BLUR COLOUR"`);
    return null;
  }
  return { x: Number(m[1]), y: Number(m[2]), blur: Number(m[3]), color: m[4] };
}

const shadows = {};
for (const [key, css] of Object.entries(tokens.shadow ?? {})) shadows[key] = parseShadow(`shadow.${key}`, css);

const tokensOut = {
  name: tokens.name,
  color: tokens.color,
  meaning: tokens.meaning,
  font: tokens.font,
  radius: tokens.radius,
  border: tokens.border,
  shadow: shadows,
  space: tokens.space,
  icon: tokens.icon,
  maker: { name: tokens.maker?.name, words: tokens.maker?.words, logoSize: tokens.maker?.logoSize },
  tabBar: tokens.tabBar,
};

// ---------------------------------------------------------------------------
// Icons

// Every icon is painted by clay.js (scripts/icons.mjs). A broken drawing
// stops this script; it never draws an icon some other way.
function attempt(name, work) {
  try {
    return work((message) => {
      throw new Error(`${name}: ${message}`);
    });
  } catch (e) {
    problems.push(e.message);
    return null;
  }
}

// The names react-native-svg uses, and the values it wants as numbers.
const PROP_NAMES = {
  'stop-color': 'stopColor',
  'stop-opacity': 'stopOpacity',
  'stroke-width': 'strokeWidth',
  'stroke-linecap': 'strokeLinecap',
  'stroke-linejoin': 'strokeLinejoin',
};
const NUMBERS = new Set(['x', 'y', 'width', 'height', 'rx', 'cx', 'cy', 'r', 'x1', 'y1', 'x2', 'y2', 'offset', 'opacity', 'stopOpacity', 'strokeWidth']);
const DROPPED = new Set(['xmlns', 'width', 'height', 'viewBox', 'aria-hidden', 'role', 'aria-label']);
const ELEMENTS = new Set(['svg', 'defs', 'linearGradient', 'radialGradient', 'stop', 'g', 'path', 'rect', 'circle']);

function toTree(node, name, isRoot) {
  const [element, attrs, children] = node;
  if (!ELEMENTS.has(element)) problems.push(`${name}: <${element}> is not drawn by the app`);
  const props = {};
  for (const [key, value] of Object.entries(attrs)) {
    if (isRoot && DROPPED.has(key)) continue;
    const prop = PROP_NAMES[key] ?? key;
    if (/[^a-zA-Z0-9]/.test(prop)) problems.push(`${name}: attribute ${key} is not drawn by the app`);
    if (NUMBERS.has(prop)) {
      const n = Number(value);
      if (!Number.isFinite(n)) problems.push(`${name}: ${key}="${value}" is not a number`);
      props[prop] = n;
    } else {
      props[prop] = value;
    }
  }
  const out = [element, props];
  if (children.length) out.push(children.map((c) => toTree(c, name, false)));
  return out;
}

const icons = {};
const drawables = [];
const svgDir = path.join(iconsDir, 'svg');
const painted = paintIcons();
for (const [name, icon] of Object.entries(painted)) {
  const exported = path.join(svgDir, `${name}.svg`);
  if (!fs.existsSync(exported)) {
    problems.push(`design/icons/svg/${name}.svg is missing: run \`node export.js\` in design/icons`);
  } else if (fs.readFileSync(exported, 'utf8').trim() !== icon.paper) {
    problems.push(`design/icons/svg/${name}.svg differs from shapes.js: run \`node export.js\` in design/icons`);
  }
  attempt(name, (fail) => {
    const paper = parseSvg(icon.paper, fail);
    // On an ink surface only a plain (ink) icon changes: its ink parts go pale.
    const ink = icon.ink ? parseSvg(icon.ink, fail) : null;
    icons[name] = { clay: icon.clay, use: icon.use, paper: toTree(paper, name, true) };
    if (ink) icons[name].ink = toTree(ink, name, true);
    drawables.push([`limo_icon_${name}.xml`, toVectorDrawable(paper, { fail })]);
    if (ink) drawables.push([`limo_icon_${name}_ink.xml`, toVectorDrawable(ink, { fail })]);
  });
}
for (const file of fs.readdirSync(svgDir)) {
  if (file.endsWith('.svg') && !painted[file.slice(0, -4)]) {
    problems.push(`design/icons/svg/${file} has no drawing in shapes.js: run \`node export.js\` in design/icons`);
  }
}

// ---------------------------------------------------------------------------
// Write

if (problems.length) {
  console.error(problems.map((p) => `PROBLEM ${p}`).join('\n'));
  console.error(`${problems.length} problem(s); nothing was written to src/generated.`);
  process.exit(1);
}

const HEADER = '// Written by scripts/make-design.mjs. Do not edit: change the design and run it again.\n';

const tokensTs =
  HEADER +
  '// From design/tokens.json.\n\n' +
  `export const tokens = ${JSON.stringify(tokensOut, null, 2)} as const;\n`;

const iconsTs =
  HEADER +
  '// From design/icons/shapes.js, painted by design/icons/clay.js.\n\n' +
  "import type { IconNode } from '../icons/types';\n\n" +
  'type IconEntry = {\n' +
  '  /** The colour of the icon, by its meaning. */\n' +
  "  readonly clay: 'honey' | 'teal' | 'coral' | 'ink';\n" +
  '  /** What the icon is for. */\n' +
  '  readonly use: string;\n' +
  '  /** The icon on paper or a card. */\n' +
  '  readonly paper: IconNode;\n' +
  '  /** The icon on an ink surface, when it differs. */\n' +
  '  readonly ink?: IconNode;\n' +
  '};\n\n' +
  `export type IconName =\n${Object.keys(icons).map((n) => `  | '${n}'`).join('\n')};\n\n` +
  `export const ICONS: Readonly<Record<IconName, IconEntry>> = ${JSON.stringify(icons)};\n`;

// The colours as Android wants them: #rrggbb, or #aarrggbb for rgba().
function androidColor(value) {
  if (value.startsWith('#')) return value.toLowerCase();
  const [r, g, b, a] = value.slice(5, -1).split(',').map(Number);
  const hex = (n) => Math.round(n).toString(16).padStart(2, '0');
  return `#${hex(a * 255)}${hex(r)}${hex(g)}${hex(b)}`;
}
const snake = (key) => key.replace(/[A-Z]/g, (c) => `_${c.toLowerCase()}`);
const tokensXml = [
  '<?xml version="1.0" encoding="utf-8"?>',
  '<!-- Written by scripts/make-design.mjs from design/tokens.json. Do not edit: change the design and run it again. -->',
  '<resources>',
  ...Object.entries(tokens.color).map(([key, value]) => `    <color name="limo_${snake(key)}">${androidColor(value)}</color>`),
  ...Object.entries(tokens.radius).map(([key, value]) => `    <dimen name="limo_radius_${snake(key)}">${value}dp</dimen>`),
  `    <dimen name="limo_border_width">${tokens.border.width}dp</dimen>`,
  `    <dimen name="limo_space">${tokens.space}dp</dimen>`,
  '</resources>',
  '',
].join('\n');

const outputs = [
  [outDir, [
    ['tokens.ts', tokensTs],
    ['icons.ts', iconsTs],
    ['lucidloom-logo.png', fs.readFileSync(path.join(repoDir, 'docs', 'images', 'lucidloom-logo.png'))],
  ]],
  [path.join(resOutDir, 'values'), [
    ['limo_tokens.xml', tokensXml],
  ]],
  [path.join(resOutDir, 'drawable'), drawables.map(([file, xml]) => [
    file,
    xml.replace('<vector ', `<!-- Written by scripts/make-design.mjs from design/icons. Do not edit. -->
<vector `),
  ])],
];

let written = 0;
for (const [dir, files] of outputs) {
  fs.mkdirSync(dir, { recursive: true });
  for (const [file, content] of files) {
    const target = path.join(dir, file);
    const bytes = Buffer.isBuffer(content) ? content : Buffer.from(content, 'utf8');
    if (fs.existsSync(target) && fs.readFileSync(target).equals(bytes)) continue;
    fs.writeFileSync(target, bytes);
    written += 1;
  }
  // Anything else in a folder this script owns is left over from an older run.
  for (const file of fs.readdirSync(dir)) {
    if (!files.some(([name]) => name === file)) fs.rmSync(path.join(dir, file), { recursive: true });
  }
}

console.log(
  `make-design: ${Object.keys(icons).length} icons, tokens "${tokens.name}"; ` +
    (written ? `${written} file(s) written.` : 'everything was up to date.'),
);
