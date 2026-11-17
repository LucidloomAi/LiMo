import { requireNativeModule } from 'expo';

/** What the Rust core decided, plus how long Kotlin spent inside the Rust call. */
export type LeaseDecision = {
  grantedSecs: number;
  remainingSecs: number;
  locked: boolean;
  /** Time measured in Kotlin around the Rust call only, in nanoseconds. */
  kotlinToRustNanos: number;
};

type LimoCoreNative = {
  coreVersion(): string;
  decideLease(dailyBudgetSecs: number, usedSecs: number, leaseSecs: number): LeaseDecision;
  log(line: string): void;
};

const native = requireNativeModule<LimoCoreNative>('LimoCore');

/** The Rust crate's version, for example "0.1.0". */
export function coreVersion(): string {
  return native.coreVersion();
}

/**
 * Asks the Rust core for a lease. Throws an Error whose `code` is the Rust
 * error's name ("ZeroLease", "UsedMoreThanADay"), or "InvalidInput" or
 * "CoreUnavailable" from the Kotlin module.
 */
export function decideLease(dailyBudgetSecs: number, usedSecs: number, leaseSecs: number): LeaseDecision {
  return native.decideLease(dailyBudgetSecs, usedSecs, leaseSecs);
}

/** Writes one line to logcat with the tag LiMoSpike03. */
export function logLine(line: string): void {
  native.log(line);
}

/** The error name carried by an error from this module, or "Unknown". */
export function errorName(e: unknown): string {
  if (e && typeof e === 'object' && 'code' in e && typeof (e as { code: unknown }).code === 'string') {
    return (e as { code: string }).code;
  }
  return 'Unknown';
}

/** The last line of a native error's message: the Rust or Kotlin text, without Expo's wrapper text. */
export function errorText(e: unknown): string {
  const message = e instanceof Error ? e.message : String(e);
  const lines = message.split('\n').map((s) => s.trim()).filter(Boolean);
  const last = lines[lines.length - 1] ?? '';
  return last.replace(/^→ Caused by: /, '');
}
