// Writes every icon as its own SVG file, and the list of icons.
//
//   node export.js
//
// Output, next to this file:
//   svg/<name>.svg   each icon in LiMo's look, 48 by 48
//   ICONS.md         every icon with its colour and what it is for
//
// It also checks the drawings, and stops with a message if one breaks a
// rule that a machine can check.

const fs = require('fs');
const path = require('path');
const CLAY = require('./clay.js');
const SHAPES = require('./shapes.js');
const TOKENS = require('../tokens.json');

const problems = [];

// The icon colours must be the theme's colours.
const same = { honey: 'marigold', teal: 'teal', coral: 'coral', ink: 'ink' };
for (const [clay, token] of Object.entries(same)) {
  if (CLAY.PALETTE[clay] !== TOKENS.color[token]) {
    problems.push(`clay.js ${clay} is ${CLAY.PALETTE[clay]} but tokens.json ${token} is ${TOKENS.color[token]}`);
  }
}

const ROLES = ['back', 'body', 'top', 'arm', 'screen', 'panel', 'dot', 'line', 'tdot', 'tline', 'mark', 'ring', 'gap'];
const names = Object.keys(SHAPES);
for (const name of names) {
  const icon = SHAPES[name];
  if (!/^[a-z]+$/.test(name)) problems.push(`${name}: a name is lower case letters only`);
  if (!CLAY.PALETTE[icon.clay]) problems.push(`${name}: unknown colour ${icon.clay}`);
  if (!icon.use) problems.push(`${name}: say what it is for`);
  if (!Array.isArray(icon.shapes) || !icon.shapes.length) problems.push(`${name}: no shapes`);
  for (const [role, tag] of icon.shapes || []) {
    if (!ROLES.includes(role)) problems.push(`${name}: unknown role ${role}`);
    if (!['rect', 'circle', 'path'].includes(tag)) problems.push(`${name}: unknown element ${tag}`);
  }
  if ((icon.shapes || []).length > 9) problems.push(`${name}: ${icon.shapes.length} shapes, keep it to a handful`);
  const out = CLAY.svg(icon.shapes, { clay: icon.clay, id: 'limo-' + name });
  if (/NaN|undefined/.test(out)) problems.push(`${name}: a number is missing in its drawing`);
}

if (problems.length) {
  console.log(problems.map((p) => 'PROBLEM ' + p).join('\n'));
  console.log(`${problems.length} problem(s); nothing was written.`);
  process.exit(1);
}

const dir = path.join(__dirname, 'svg');
fs.mkdirSync(dir, { recursive: true });
for (const old of fs.readdirSync(dir)) {
  if (old.endsWith('.svg') && !names.includes(old.slice(0, -4))) fs.unlinkSync(path.join(dir, old));
}
for (const name of names) {
  const icon = SHAPES[name];
  const out = CLAY.svg(icon.shapes, { size: 48, clay: icon.clay, id: 'limo-' + name });
  fs.writeFileSync(path.join(dir, name + '.svg'), out + '\n');
}

const rows = names.map((name) => `| \`${name}\` | <img src="svg/${name}.svg" width="32" height="32" alt=""> | ${SHAPES[name].clay} | ${SHAPES[name].use} |`);
fs.writeFileSync(
  path.join(__dirname, 'ICONS.md'),
  [
    '# LiMo icons',
    '',
    'Written by `node export.js`. Do not edit by hand: change `shapes.js` and run it again.',
    '',
    `${names.length} icons.`,
    '',
    '| Name | Icon | Colour | What it is for |',
    '| --- | --- | --- | --- |',
    ...rows,
    '',
  ].join('\n'),
);

console.log(`OK: ${names.length} icons written to svg/ and listed in ICONS.md.`);
