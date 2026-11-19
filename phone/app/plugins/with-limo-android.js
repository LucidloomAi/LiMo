// Expo config plugin: LiMo's Android settings for both forms, so that the
// android/ folder `expo prebuild` makes never needs editing by hand.
//
// In android/gradle.properties:
//   reactNativeArchitectures=arm64-v8a     the one ABI, as in proof three; adding
//                                          one is a value here (and a Rust target)
//   android.builder.sdkDownload=false      the build never installs SDK packages;
//                                          a missing one fails with its name
//   android.minSdkVersion=26               oldest Android: 8.0
//   org.gradle.jvmargs                     room for the Kotlin compiler (proof
//                                          three's daemon ran out of metaspace)
//   EX_DEV_CLIENT_NETWORK_INSPECTOR=false  off, though no dev client is linked
//
// In android/build.gradle: applies scripts/newer-ninja.gradle (long paths on
// Windows, from proof three), scripts/no-build-paths.gradle (no path of the
// build PC inside the C++ libraries) and scripts/form-check.gradle.
//
// In the manifest: no <queries> for web links (LiMo opens none), and
// data extraction rules that keep everything out of cloud backups and
// device to device transfers. allowBackup=false (app.config.ts) already stops
// cloud backups; from Android 12 a transfer to a new phone would still copy
// the app's files without these rules.
//
// The look before the first screen draws: a light theme on paper (never the
// dark one, the screens are always paper), dark status and navigation bar
// icons, and no splash picture. The launcher icon is an adaptive icon: the
// design's icon (option launcherIcon, a name in design/icons/shapes.js) on
// paper, drawn as a vector by scripts/vector-drawable.mjs. Expo's placeholder
// pictures are removed.
//
// The debug manifests lose SYSTEM_ALERT_WINDOW ("draw over other apps"),
// which React Native's template asks for in debug builds. No form of LiMo
// asks for it.

const fs = require('fs');
const path = require('path');
const { pathToFileURL } = require('url');
const {
  withAndroidColors,
  withAndroidManifest,
  withAndroidStyles,
  withDangerousMod,
  withGradleProperties,
  withProjectBuildGradle,
} = require('expo/config-plugins');

const TOKENS = require('../../../design/tokens.json');

const PROPERTIES = {
  reactNativeArchitectures: 'arm64-v8a',
  'android.builder.sdkDownload': 'false',
  'android.minSdkVersion': '26',
  'org.gradle.jvmargs': '-Xmx4096m -XX:MaxMetaspaceSize=1024m',
  EX_DEV_CLIENT_NETWORK_INSPECTOR: 'false',
};

const APPLY = [
  'apply from: new File(rootDir, "../scripts/newer-ninja.gradle")',
  'apply from: new File(rootDir, "../scripts/no-build-paths.gradle")',
  'apply from: new File(rootDir, "../scripts/form-check.gradle")',
];

function withProperties(config) {
  return withGradleProperties(config, (cfg) => {
    const keys = Object.keys(PROPERTIES);
    cfg.modResults = cfg.modResults.filter((item) => !(item.type === 'property' && keys.includes(item.key)));
    for (const key of keys) cfg.modResults.push({ type: 'property', key, value: PROPERTIES[key] });
    return cfg;
  });
}

// The lines go before React Native's root plugin, which makes Gradle evaluate
// :app at once (evaluationDependsOn). Applied after it, as proof three did,
// the ninja reaches every library but not the app's own C++ build
// (libappmodules.so and the codegen of react-native-svg and
// react-native-safe-area-context), whose paths are the longest.
const ANCHOR = 'apply plugin: "expo-root-project"';

function withBuildScripts(config) {
  return withProjectBuildGradle(config, (cfg) => {
    if (cfg.modResults.language !== 'groovy') throw new Error('with-limo-android: expected a Groovy android/build.gradle');
    const text = cfg.modResults.contents;
    if (!text.includes(ANCHOR)) throw new Error(`with-limo-android: android/build.gradle has no ${ANCHOR}`);
    const missing = APPLY.filter((line) => !text.includes(line));
    if (missing.length) {
      cfg.modResults.contents = text.replace(ANCHOR, `// LiMo, see plugins/with-limo-android.js\n${missing.join('\n')}\n\n${ANCHOR}`);
    }
    return cfg;
  });
}

function withManifestRules(config) {
  return withAndroidManifest(config, (cfg) => {
    const manifest = cfg.modResults.manifest;
    delete manifest.queries;
    const app = manifest.application?.[0];
    if (!app) throw new Error('with-limo-android: the manifest has no <application>');
    app.$['android:dataExtractionRules'] = '@xml/limo_data_extraction';
    return cfg;
  });
}

function withColors(config) {
  return withAndroidColors(config, (cfg) => {
    const set = (name, value) => {
      const list = cfg.modResults.resources.color ?? (cfg.modResults.resources.color = []);
      const found = list.find((c) => c.$.name === name);
      if (found) found._ = value;
      else list.push({ $: { name }, _: value });
    };
    set('limo_paper', TOKENS.color.paper);
    set('limo_ink', TOKENS.color.ink);
    set('limo_launcher_background', TOKENS.color.paper);
    set('colorPrimary', TOKENS.color.ink);
    set('splashscreen_background', TOKENS.color.paper);
    return cfg;
  });
}

function withTheme(config) {
  return withAndroidStyles(config, (cfg) => {
    const styles = cfg.modResults.resources.style ?? [];
    const style = (name) => {
      const found = styles.find((s) => s.$.name === name);
      if (!found) throw new Error(`with-limo-android: styles.xml has no ${name}`);
      found.item = found.item ?? [];
      return found;
    };
    const setItem = (s, name, value) => {
      const found = s.item.find((i) => i.$.name === name);
      if (found) found._ = value;
      else s.item.push({ $: { name }, _: value });
    };
    const app = style('AppTheme');
    app.$.parent = 'Theme.AppCompat.Light.NoActionBar';
    setItem(app, 'android:windowBackground', '@color/limo_paper');
    setItem(app, 'android:statusBarColor', '@android:color/transparent');
    setItem(app, 'android:navigationBarColor', '@android:color/transparent');
    setItem(app, 'android:windowLightStatusBar', 'true');
    setItem(app, 'android:windowLightNavigationBar', 'true');
    const splash = style('Theme.App.SplashScreen');
    setItem(splash, 'android:windowBackground', '@color/limo_paper');
    return cfg;
  });
}

const DATA_EXTRACTION = `<?xml version="1.0" encoding="utf-8"?>
<!--
Written by plugins/with-limo-android.js. Nothing of LiMo's leaves the phone in
a backup or in a transfer to a new phone: its files hold the pairing with the
family's master PC, which must not turn up on another phone.
-->
<data-extraction-rules>
    <cloud-backup>
${['root', 'file', 'database', 'sharedpref', 'external'].map((d) => `        <exclude domain="${d}" path="." />`).join('\n')}
    </cloud-backup>
    <device-transfer>
${['root', 'file', 'database', 'sharedpref', 'external'].map((d) => `        <exclude domain="${d}" path="." />`).join('\n')}
    </device-transfer>
</data-extraction-rules>
`;

const ADAPTIVE_ICON = `<?xml version="1.0" encoding="utf-8"?>
<!-- Written by plugins/with-limo-android.js. -->
<adaptive-icon xmlns:android="http://schemas.android.com/apk/res/android">
    <background android:drawable="@color/limo_launcher_background" />
    <foreground android:drawable="@drawable/limo_launcher_foreground" />
</adaptive-icon>
`;

function withFiles(config, launcherIcon) {
  return withDangerousMod(config, [
    'android',
    async (cfg) => {
      const main = path.join(cfg.modRequest.platformProjectRoot, 'app', 'src', 'main');
      const res = path.join(main, 'res');

      fs.mkdirSync(path.join(res, 'xml'), { recursive: true });
      fs.writeFileSync(path.join(res, 'xml', 'limo_data_extraction.xml'), DATA_EXTRACTION);

      // Expo's placeholder launcher and splash pictures.
      for (const dir of fs.readdirSync(res)) {
        if (/^mipmap-.*dpi$/.test(dir)) fs.rmSync(path.join(res, dir), { recursive: true });
        if (/^drawable-.*dpi$/.test(dir)) {
          const logo = path.join(res, dir, 'splashscreen_logo.png');
          if (fs.existsSync(logo)) fs.rmSync(logo);
          if (fs.readdirSync(path.join(res, dir)).length === 0) fs.rmdirSync(path.join(res, dir));
        }
      }
      const layered = path.join(res, 'drawable', 'ic_launcher_background.xml');
      if (fs.existsSync(layered)) fs.rmSync(layered);

      // The launcher icon, from the design.
      const scripts = path.resolve(__dirname, '..', 'scripts');
      const { paintIcons, parseSvg } = await import(pathToFileURL(path.join(scripts, 'icons.mjs')).href);
      const { toVectorDrawable } = await import(pathToFileURL(path.join(scripts, 'vector-drawable.mjs')).href);
      const icon = paintIcons()[launcherIcon];
      if (!icon) throw new Error(`with-limo-android: no icon named ${launcherIcon} in design/icons/shapes.js`);
      const fail = (message) => {
        throw new Error(`with-limo-android: launcher icon ${launcherIcon}: ${message}`);
      };
      // An adaptive icon's foreground is 108 by 108 and shows the middle 72;
      // the icon's 48 unit grid is drawn 60 across, so its drawing sits well
      // inside the circle every launcher keeps.
      const foreground = toVectorDrawable(parseSvg(icon.paper, fail), { sizeDp: 108, gridDp: 60, fail });
      fs.mkdirSync(path.join(res, 'drawable'), { recursive: true });
      fs.writeFileSync(
        path.join(res, 'drawable', 'limo_launcher_foreground.xml'),
        foreground.replace('<vector ', `<!-- Written by plugins/with-limo-android.js: design/icons ${launcherIcon}. -->\n<vector `),
      );
      fs.mkdirSync(path.join(res, 'mipmap-anydpi-v26'), { recursive: true });
      fs.writeFileSync(path.join(res, 'mipmap-anydpi-v26', 'ic_launcher.xml'), ADAPTIVE_ICON);
      fs.writeFileSync(path.join(res, 'mipmap-anydpi-v26', 'ic_launcher_round.xml'), ADAPTIVE_ICON);

      // No "draw over other apps", not even in a debug build.
      for (const variant of ['debug', 'debugOptimized']) {
        const file = path.join(cfg.modRequest.platformProjectRoot, 'app', 'src', variant, 'AndroidManifest.xml');
        if (!fs.existsSync(file)) continue;
        const text = fs.readFileSync(file, 'utf8').replace(/\s*<uses-permission android:name="android\.permission\.SYSTEM_ALERT_WINDOW"\s*\/>/g, '');
        if (text.includes('SYSTEM_ALERT_WINDOW')) throw new Error(`with-limo-android: could not remove SYSTEM_ALERT_WINDOW from ${variant}`);
        fs.writeFileSync(file, text);
      }
      return cfg;
    },
  ]);
}

module.exports = function withLimoAndroid(config, { launcherIcon } = {}) {
  if (!launcherIcon) throw new Error('with-limo-android: say which design icon is the launcher icon (launcherIcon)');
  config = withProperties(config);
  config = withBuildScripts(config);
  config = withManifestRules(config);
  config = withColors(config);
  config = withTheme(config);
  config = withFiles(config, launcherIcon);
  return config;
};
