// Expo config plugin: what differs between LiMo's two forms on Android,
// beyond what app.config.ts sets (package, name, permissions), during
// `expo prebuild`.
//
// The guardian form leaves out the child module (modules/limo-child) and so
// every component in its manifest: no accessibility service, no device
// admin, no usage access, no foreground service, no receivers. It is left
// out of autolinking in android/settings.gradle, so Gradle never sees it.
//
// Both forms write the form into android/gradle.properties as limo.form.
// scripts/form-check.gradle compares it with LIMO_FORM on every build,
// because Expo reads app.config.ts again while Gradle bundles the
// JavaScript: android/ made for one form and a bundle made for the other
// must never meet.

const { withGradleProperties, withSettingsGradle } = require('expo/config-plugins');

const FORMS = ['child', 'guardian'];
const EXCLUDE_CHILD = 'expoAutolinking.exclude = ["limo-child"]';

function withFormProperty(config, form) {
  return withGradleProperties(config, (cfg) => {
    cfg.modResults = cfg.modResults.filter((item) => !(item.type === 'property' && item.key === 'limo.form'));
    cfg.modResults.push({ type: 'property', key: 'limo.form', value: form });
    return cfg;
  });
}

function withoutChildModule(config) {
  return withSettingsGradle(config, (cfg) => {
    const anchor = 'expoAutolinking.useExpoModules()';
    const text = cfg.modResults.contents;
    if (!text.includes(anchor)) throw new Error(`with-limo-form: android/settings.gradle has no ${anchor}`);
    if (!text.includes(EXCLUDE_CHILD)) {
      cfg.modResults.contents = text.replace(
        anchor,
        `// LiMo guardian form: the child module stays out (plugins/with-limo-form.js)\n${EXCLUDE_CHILD}\n${anchor}`,
      );
    }
    return cfg;
  });
}

module.exports = function withLimoForm(config, { form } = {}) {
  if (!FORMS.includes(form)) throw new Error(`with-limo-form: unknown form ${form}`);
  config = withFormProperty(config, form);
  if (form === 'guardian') config = withoutChildModule(config);
  return config;
};
