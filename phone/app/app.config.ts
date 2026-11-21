// One app, two forms, picked by LIMO_FORM when the app is built:
//
//   child      on a child's phone: package ai.lucidloom.limo.child, "LiMo",
//              with the child module (modules/limo-child) and its components
//   guardian   on a parent's phone: package ai.lucidloom.limo, "LiMo Guardian",
//              without the child module
//
// LIMO_FORM must be set for every Expo command that reads this file, Gradle's
// bundling step included; scripts/build.mjs sets it, and
// scripts/form-check.gradle stops a build where android/ was made for the
// other form. There is no default, so a form is never picked by accident.

import type { ConfigContext, ExpoConfig } from 'expo/config';

type Form = 'child' | 'guardian';

const FORMS: Record<Form, { name: string; package: string; slug: string; launcherIcon: string }> = {
  child: { name: 'LiMo', package: 'ai.lucidloom.limo.child', slug: 'limo-child', launcherIcon: 'hourglass' },
  guardian: { name: 'LiMo Guardian', package: 'ai.lucidloom.limo', slug: 'limo-guardian', launcherIcon: 'guardian' },
};

// Permissions no form of LiMo holds. Each is removed from the built app even
// when React Native's template or a library asks for it (the manifest gets
// tools:node="remove"). A later piece that truly needs one takes it off this
// list and says why where it adds it.
const NEVER = [
  // The network beyond what the child form's link needs: no Wi-Fi state, no
  // multicast of LiMo's own (Android's NsdManager does the discovery), no
  // nearby devices.
  'android.permission.ACCESS_WIFI_STATE',
  'android.permission.CHANGE_WIFI_MULTICAST_STATE',
  'android.permission.NEARBY_WIFI_DEVICES',
  // Drawing over other apps: the child's covers are accessibility overlays.
  'android.permission.SYSTEM_ALERT_WINDOW',
  // Asked for by React Native's template, used by nothing in LiMo.
  'android.permission.VIBRATE',
  'android.permission.READ_EXTERNAL_STORAGE',
  'android.permission.WRITE_EXTERNAL_STORAGE',
  'android.permission.MANAGE_EXTERNAL_STORAGE',
  'android.permission.READ_MEDIA_IMAGES',
  'android.permission.READ_MEDIA_VIDEO',
  'android.permission.READ_MEDIA_AUDIO',
  'android.permission.READ_MEDIA_VISUAL_USER_SELECTED',
  // What LiMo never looks at.
  'android.permission.CAMERA',
  'android.permission.RECORD_AUDIO',
  'android.permission.ACCESS_FINE_LOCATION',
  'android.permission.ACCESS_COARSE_LOCATION',
  'android.permission.ACCESS_BACKGROUND_LOCATION',
  'android.permission.READ_CONTACTS',
  'android.permission.WRITE_CONTACTS',
  'android.permission.GET_ACCOUNTS',
  'android.permission.READ_PHONE_NUMBERS',
  'android.permission.READ_CALL_LOG',
  'android.permission.READ_SMS',
  'android.permission.QUERY_ALL_PACKAGES',
  'android.permission.REQUEST_INSTALL_PACKAGES',
  'com.google.android.gms.permission.AD_ID',
];

// What only the child form may hold: the guardian form refuses them too, so
// nothing of the child's can reach a guardian's phone by accident.
const CHILD_ONLY = [
  'android.permission.PACKAGE_USAGE_STATS',
  'android.permission.FOREGROUND_SERVICE',
  'android.permission.FOREGROUND_SERVICE_SPECIAL_USE',
  'android.permission.RECEIVE_BOOT_COMPLETED',
  // The Phone permission (the owner's decision of 20 Nov 2026): the child
  // form asks for it only to know that a call is ringing or on, so a lock
  // never hides a call (see the child module's manifest). The guardian form
  // never holds it.
  'android.permission.READ_PHONE_STATE',
  // The child form's link to the family's PC, and to nothing else (the
  // child module's link/README.md): sockets to the cable's tunnel and the
  // home network, and which network is Wi-Fi. The guardian form opens no
  // socket at all until it has a link of its own.
  'android.permission.INTERNET',
  'android.permission.ACCESS_NETWORK_STATE',
];

function readForm(): Form {
  const value = process.env.LIMO_FORM;
  if (value === 'child' || value === 'guardian') return value;
  throw new Error(
    `LIMO_FORM is ${value === undefined ? 'not set' : `"${value}"`}: set it to child or guardian ` +
      '(npm run build:child, npm run build:guardian).',
  );
}

export default ({ config }: ConfigContext): ExpoConfig => {
  const form = readForm();
  const f = FORMS[form];
  return {
    ...config,
    name: f.name,
    slug: f.slug,
    version: '0.1.0',
    // No scheme: nothing outside the app can open a screen inside it.
    android: {
      package: f.package,
      versionCode: 1,
      allowBackup: false,
      predictiveBackGestureEnabled: false,
      permissions: [],
      blockedPermissions: form === 'guardian' ? [...NEVER, ...CHILD_ONLY] : NEVER,
    },
    // expo-updates is not installed; this keeps it off if it ever were.
    updates: { enabled: false, checkAutomatically: 'NEVER' },
    extra: { limoForm: form },
    plugins: [
      ['./plugins/with-limo-android', { launcherIcon: f.launcherIcon }],
      ['./plugins/with-limo-form', { form }],
      './plugins/with-limo-fonts',
    ],
  };
};
