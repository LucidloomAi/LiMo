// Expo config plugin: puts the theme's three fonts into the Android app, each
// beside its licence, during `expo prebuild`.
//
// The fonts are the variable fonts in ../../screens/fonts, unchanged, the
// same files the PC's screens and the design's pages use. React Native on
// Android cannot set a variable font's axes from a style, so each weight
// (and for Fraunces each optical size) is a <font> entry in an Android font
// family that points at the one file with its axes set
// (app:fontVariationSettings). React Native then picks the entry by
// fontWeight. The families and their names:
//
//   "Instrument Sans"       body text, wght 400 500 600 700
//   "Spline Sans Mono"      labels, times, data, wght 400 500 600 700
//   "Fraunces 18" ... "72"  headings and large numbers: one family for each
//                           optical size in src/theme/fonts.json, wght as
//                           above. A browser sets opsz to the font size;
//                           src/theme/fonts.ts picks the nearest family.
//
// Fraunces keeps its own defaults for its other axes (SOFT 0, WONK 1), as a
// browser does. On Android 8.0 and 8.1 React Native can only ask for normal
// or bold, so weights below 600 draw as 400 there and the rest as 700.
//
// Written into android/app/src/main/:
//   res/font/limo_*.ttf and limo_*.xml       the files and the families
//   assets/licences/<font>/OFL.txt           each font's licence
//   MainApplication.kt                       one line per family, registering it

const fs = require('fs');
const path = require('path');
const { AndroidConfig, CodeGenerator, withDangerousMod, withMainApplication } = require('expo/config-plugins');

const FONTS = require('../src/theme/fonts.json');
const TOKENS = require('../../../design/tokens.json');

const FONTS_DIR = path.resolve(__dirname, '..', '..', '..', 'screens', 'fonts');

// [family name, resource name of the family, key in FONTS.files, axes for a weight]
function families() {
  const list = [
    [TOKENS.font.body, 'limo_body', 'body', (w) => `'wght' ${w}`],
    [TOKENS.font.mono, 'limo_mono', 'mono', (w) => `'wght' ${w}`],
  ];
  for (const size of FONTS.displaySizes) {
    list.push([`${TOKENS.font.display} ${size}`, `limo_display_${size}`, 'display', (w) => `'wght' ${w}, 'opsz' ${size}`]);
  }
  return list;
}

const fileResource = (key) => `limo_font_${key}`;

function familyXml(fileKey, axes) {
  const fonts = FONTS.weights.map(
    (w) =>
      `    <font app:font="@font/${fileResource(fileKey)}" app:fontStyle="normal" app:fontWeight="${w}" app:fontVariationSettings="${axes(w)}" />`,
  );
  return [
    '<?xml version="1.0" encoding="utf-8"?>',
    '<!-- Written by plugins/with-limo-fonts.js during expo prebuild. -->',
    '<font-family xmlns:app="http://schemas.android.com/apk/res-auto">',
    ...fonts,
    '</font-family>',
    '',
  ].join('\n');
}

function withFontFiles(config) {
  return withDangerousMod(config, [
    'android',
    async (cfg) => {
      const main = path.join(cfg.modRequest.platformProjectRoot, 'app', 'src', 'main');
      const fontDir = path.join(main, 'res', 'font');
      fs.mkdirSync(fontDir, { recursive: true });
      for (const [key, file] of Object.entries(FONTS.files)) {
        const source = path.join(FONTS_DIR, file);
        if (!fs.existsSync(source)) throw new Error(`with-limo-fonts: ${source} is missing`);
        fs.copyFileSync(source, path.join(fontDir, `${fileResource(key)}.ttf`));
        const folder = path.dirname(file);
        const licence = path.join(FONTS_DIR, folder, 'OFL.txt');
        if (!fs.existsSync(licence)) throw new Error(`with-limo-fonts: ${licence} is missing`);
        const target = path.join(main, 'assets', 'licences', folder);
        fs.mkdirSync(target, { recursive: true });
        fs.copyFileSync(licence, path.join(target, 'OFL.txt'));
      }
      for (const [, resource, fileKey, axes] of families()) {
        fs.writeFileSync(path.join(fontDir, `${resource}.xml`), familyXml(fileKey, axes));
      }
      return cfg;
    },
  ]);
}

function withFontRegistration(config) {
  return withMainApplication(config, (cfg) => {
    if (cfg.modResults.language !== 'kt') throw new Error('with-limo-fonts: expected MainApplication.kt');
    cfg.modResults.contents = AndroidConfig.CodeMod.addImports(
      cfg.modResults.contents,
      ['com.facebook.react.common.assets.ReactFontManager'],
      false,
    );
    const calls = families().map(
      ([family, resource]) => `    ReactFontManager.getInstance().addCustomFont(this, ${JSON.stringify(family)}, R.font.${resource})`,
    );
    cfg.modResults.contents = CodeGenerator.mergeContents({
      src: cfg.modResults.contents,
      comment: '    //',
      tag: 'limo-fonts',
      offset: 1,
      anchor: /super\.onCreate\(\)/,
      newSrc: calls.join('\n'),
    }).contents;
    return cfg;
  });
}

module.exports = function withLimoFonts(config) {
  return withFontRegistration(withFontFiles(config));
};
