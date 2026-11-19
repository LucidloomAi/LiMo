import Constants from 'expo-constants';

/** LiMo's two forms: on a child's phone, and on a guardian's phone. */
export type Form = 'child' | 'guardian';

/**
 * The form this app was built as. app.config.ts puts it in the app's
 * embedded config (extra.limoForm) from LIMO_FORM at build time.
 */
export function appForm(): Form {
  const form: unknown = Constants.expoConfig?.extra?.limoForm;
  if (form === 'child' || form === 'guardian') return form;
  throw new Error(`This build has no form (extra.limoForm is ${String(form)}). Build it with npm run build:child or build:guardian.`);
}
