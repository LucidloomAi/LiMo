# LiMo design: Paper and Ink

Every LiMo screen follows one theme, called Paper and Ink. It is the theme
of the LucidLoom AI products, so LiMo looks like one of the family. This
folder holds the theme's values and LiMo's icons.

It covers every surface a family sees: the phone app, the guardian console
on the PC, the time left display and the lock screen.

```
tokens.json          the colours, type, corners, borders and shadows
icons/
  shapes.js          the drawings, as plain data
  clay.js            how a drawing is painted
  sheet.html         the review sheet: open it in a browser
  export.js          writes svg/ and ICONS.md
  svg/               every icon as its own file
  ICONS.md           every icon, with its colour and what it is for
  preview.png        a picture of the review sheet
```

![The LiMo icon set on its review sheet](icons/preview.png)

## The theme

A page is warm paper with a faint dot grid. Text and lines are ink. A card
is a lighter paper with a thin ink border and a hard shadow set down and to
the right, as if it were cut out and laid on the page. Dividers inside a
card are dashed. A state is shown as a small stamp: mono capitals in an
outlined box.

| Value | Colour | Used for |
| --- | --- | --- |
| paper | `#f6f1e4` | the page |
| paper2 | `#efe7d2` | a quieter band, such as a tab bar |
| card | `#fffdf6` | cards and buttons |
| ink | `#1b2638` | text, borders, shadows |
| inkSoft | `#46506a` | secondary text |
| marigold | `#e89c1c` | time, and the things that are LiMo's own |
| marigoldDeep | `#b97a08` | marigold for text and large numbers |
| teal | `#0e7c66` | allowed, at home, connected, done |
| coral | `#cf4326` | locked, blocked, stop, danger |

Type: Fraunces for headings and large numbers, Instrument Sans for reading
text, Spline Sans Mono for labels, times and other data. All three are
published under the SIL Open Font License 1.1. Check the licence file that
comes with each font before bundling it.

Shape: borders are 1.5 wide, corners are 7, 10 or 14, and the card shadow
is 4 across and 4 down with no blur. Spacing steps by 4.

## Rules

1. **Colour means something.** Marigold is time. Teal is allowed, at home
   or done. Coral is locked, blocked or dangerous. Everything else is ink
   on paper. Colour is never decoration.
2. **Plain facts and controls stay plain.** A choice, a setting or a number
   is an ink outline with mono type, not an illustration.
3. **Nothing changes size when it is picked.** A selected chip, a message
   that appears and a label that turns bold all keep the room they had.
4. **A tap shows a result.** After any tap, the settled screen must look
   different to someone who was not told what to expect.
5. **No emoji and no icon fonts.** Icons come from this set. Small marks
   beside text, such as arrows and ticks, are plain ink strokes.
6. **Every screen is checked at phone size and at desktop size,** for
   height as well as width, before it is called done.

## The icons

The icons are light clay: soft rounded forms in pale tones, lit from the
top left, with a soft shadow and no outlines. Each icon has a lighter clay
for its body and a darker clay for its smaller parts. Marks on the lighter
clay are dark.

An icon takes one colour, by the rule above. A plain thing, such as a
device or a person, is paper and ink themselves: a cream body with ink
parts.

**To look at them,** open `icons/sheet.html` in a browser. Add `?close=1`
to the address for a close up of every drawing.

**To use one,** take its file from `icons/svg/`, or call `svg()` in
`clay.js` with the drawing from `shapes.js`. Sizes: 24 in a row or a tab,
32 on a tile, 48 beside a heading, 96 on an empty screen. On an ink
surface pass `on: 'dark'`, so the ink parts of a plain icon stay visible.

**To add one,** add a drawing to `shapes.js` and run `node export.js` in
the `icons` folder. The rules for a drawing are at the top of `shapes.js`:
a handful of chunky shapes, about 38 of the 48 units filled both ways so
every icon looks one size, every corner rounded, one colour. Then look at
it on the sheet at 24 before using it.

The drawings are data, not pictures, so the phone app and the console can
paint them with their own drawing code from the same file.
