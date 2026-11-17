// Expo config plugin: during `expo prebuild`, makes three changes to the
// generated android/ folder, so it never needs editing by hand.
//
// In android/gradle.properties:
//
// reactNativeArchitectures=arm64-v8a
//   React Native then builds and packages only that ABI, and the Rust core is
//   built for the same ABI (the module's Gradle task reads the same property).
//   To add ABIs, change the value here and add the matching Rust targets with
//   rustup.
//
// android.builder.sdkDownload=false
//   The Android Gradle Plugin never installs SDK packages (an NDK, CMake,
//   build tools) by itself. A missing package fails the build with its name
//   instead.
//
// In android/build.gradle, one line that applies scripts/newer-ninja.gradle,
// which makes the C++ builds use a ninja that copes with long paths on
// Windows. See that file for why.

const { withGradleProperties, withProjectBuildGradle } = require('expo/config-plugins');

const PROPERTIES = {
  reactNativeArchitectures: 'arm64-v8a',
  'android.builder.sdkDownload': 'false',
};

const APPLY_NINJA = 'apply from: new File(rootDir, "../scripts/newer-ninja.gradle")';

function withProperties(config) {
  return withGradleProperties(config, (cfg) => {
    const keys = Object.keys(PROPERTIES);
    cfg.modResults = cfg.modResults.filter((item) => !(item.type === 'property' && keys.includes(item.key)));
    for (const key of keys) {
      cfg.modResults.push({ type: 'property', key, value: PROPERTIES[key] });
    }
    return cfg;
  });
}

function withNewerNinja(config) {
  return withProjectBuildGradle(config, (cfg) => {
    if (cfg.modResults.language !== 'groovy') {
      throw new Error('with-spike-gradle-properties: expected a Groovy android/build.gradle');
    }
    if (!cfg.modResults.contents.includes(APPLY_NINJA)) {
      cfg.modResults.contents = `${cfg.modResults.contents.trimEnd()}\n\n// LiMo spike 03, see app/scripts/newer-ninja.gradle\n${APPLY_NINJA}\n`;
    }
    return cfg;
  });
}

module.exports = function withSpikeGradleProperties(config) {
  return withNewerNinja(withProperties(config));
};
