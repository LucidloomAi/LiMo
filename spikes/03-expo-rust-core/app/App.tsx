import { StatusBar } from 'expo-status-bar';
import { useState } from 'react';
import {
  Pressable,
  StatusBar as NativeStatusBar,
  ScrollView,
  StyleSheet,
  Text,
  TextInput,
  View,
} from 'react-native';

import { coreVersion, decideLease, errorName, errorText, logLine, type LeaseDecision } from './modules/limo-core';

// How many calls the startup check makes to measure an average call.
const TIMING_CALLS = 1000;

type StartupReport = {
  version: string;
  lines: string[];
};

type AskResult =
  | { kind: 'decision'; decision: LeaseDecision; jsMs: number }
  | { kind: 'error'; name: string; text: string; jsMs: number };

// Kept for the life of the process, so the check and its log lines happen
// once per app start, even if the screen is drawn again.
let startupReport: StartupReport | null = null;

function now(): number {
  return performance.now();
}

function describeDuration(ms: number): string {
  if (ms < 1) return `${(ms * 1000).toFixed(1)} microseconds`;
  return `${ms.toFixed(3)} ms`;
}

// Calls the core by itself on start and writes the result to logcat with the
// tag LiMoSpike03, so the whole chain can be checked over USB without
// tapping anything.
function runStartupCheck(): StartupReport {
  if (startupReport) return startupReport;
  const lines: string[] = [];

  // The first call also loads JNA and the Rust library.
  let version = 'unavailable';
  let versionError = '';
  const firstStart = now();
  try {
    version = coreVersion();
  } catch (e) {
    versionError = `${errorName(e)}: ${errorText(e)}`;
  }
  const firstCallMs = now() - firstStart;

  try {
    const d = decideLease(3600, 3300, 600);
    lines.push(
      `core_version=${version} decide_lease(3600,3300,600) granted=${d.grantedSecs} remaining=${d.remainingSecs} locked=${d.locked}`,
    );
  } catch (e) {
    lines.push(`core_version=${version} decide_lease(3600,3300,600) error=${errorName(e)}`);
  }

  try {
    const d = decideLease(3600, 0, 0);
    lines.push(`decide_lease(3600,0,0) no_error granted=${d.grantedSecs} remaining=${d.remainingSecs} locked=${d.locked}`);
  } catch (e) {
    lines.push(`decide_lease(3600,0,0) error=${errorName(e)}`);
  }

  try {
    let kotlinNanos = 0;
    const loopStart = now();
    for (let i = 0; i < TIMING_CALLS; i++) {
      kotlinNanos += decideLease(3600, i, 600).kotlinToRustNanos;
    }
    const avgUs = ((now() - loopStart) * 1000) / TIMING_CALLS;
    const kotlinAvgUs = kotlinNanos / 1000 / TIMING_CALLS;
    lines.push(
      `timing first_call_ms=${firstCallMs.toFixed(3)} calls=${TIMING_CALLS} js_avg_us=${avgUs.toFixed(2)} kotlin_to_rust_avg_us=${kotlinAvgUs.toFixed(2)}`,
    );
  } catch (e) {
    lines.push(`timing error=${errorName(e)}`);
  }

  if (versionError) lines.push(`core_version_failed ${versionError}`);

  for (const line of lines) {
    console.log(`LiMoSpike03 ${line}`);
    try {
      logLine(line);
    } catch {
      // Nothing more to do: the module itself is missing.
    }
  }
  startupReport = { version, lines };
  return startupReport;
}

function parseField(text: string): number {
  const trimmed = text.trim();
  return trimmed === '' ? Number.NaN : Number(trimmed);
}

export default function App() {
  const [report] = useState<StartupReport>(runStartupCheck);
  const [budget, setBudget] = useState('3600');
  const [used, setUsed] = useState('3300');
  const [lease, setLease] = useState('600');
  const [result, setResult] = useState<AskResult | null>(null);

  function ask() {
    const args = [parseField(budget), parseField(used), parseField(lease)] as const;
    const started = now();
    try {
      const decision = decideLease(...args);
      setResult({ kind: 'decision', decision, jsMs: now() - started });
    } catch (e) {
      setResult({ kind: 'error', name: errorName(e), text: errorText(e), jsMs: now() - started });
    }
  }

  return (
    <ScrollView
      style={styles.screen}
      contentContainerStyle={styles.content}
      keyboardShouldPersistTaps="handled"
    >
      <Text style={styles.title}>LiMo spike 03</Text>
      <Text style={styles.label}>Rust core version</Text>
      <Text style={styles.version}>{report.version}</Text>

      <Field label="Daily budget, seconds" value={budget} onChange={setBudget} />
      <Field label="Used today, seconds" value={used} onChange={setUsed} />
      <Field label="Lease asked for, seconds" value={lease} onChange={setLease} />

      <Pressable
        accessibilityRole="button"
        onPress={ask}
        style={({ pressed }) => [styles.button, pressed && styles.buttonPressed]}
      >
        <Text style={styles.buttonText}>Ask the Rust core</Text>
      </Pressable>

      {result && <ResultBox result={result} />}

      <Text style={styles.label}>Startup check, also written to logcat (tag LiMoSpike03)</Text>
      <View style={styles.box}>
        {report.lines.map((line) => (
          <Text key={line} style={styles.mono}>
            {line}
          </Text>
        ))}
      </View>
      <StatusBar style="dark" />
    </ScrollView>
  );
}

function Field(props: { label: string; value: string; onChange: (text: string) => void }) {
  return (
    <View style={styles.field}>
      <Text style={styles.label}>{props.label}</Text>
      <TextInput
        style={styles.input}
        value={props.value}
        onChangeText={props.onChange}
        keyboardType="number-pad"
        selectTextOnFocus
      />
    </View>
  );
}

function ResultBox({ result }: { result: AskResult }) {
  if (result.kind === 'error') {
    return (
      <View style={[styles.box, styles.errorBox]}>
        <Text style={styles.resultLine}>Error: {result.name}</Text>
        <Text style={styles.detail}>{result.text}</Text>
        <Text style={styles.detail}>Call took {describeDuration(result.jsMs)}, JavaScript to Rust and back.</Text>
      </View>
    );
  }
  const d = result.decision;
  return (
    <View style={styles.box}>
      <Text style={styles.resultLine}>Granted: {d.grantedSecs} s</Text>
      <Text style={styles.resultLine}>Remaining: {d.remainingSecs} s</Text>
      <Text style={styles.resultLine}>Locked: {d.locked ? 'yes' : 'no'}</Text>
      <Text style={styles.detail}>Call took {describeDuration(result.jsMs)}, JavaScript to Rust and back.</Text>
      <Text style={styles.detail}>
        Of that, {describeDuration(d.kotlinToRustNanos / 1e6)} from Kotlin into Rust and back.
      </Text>
    </View>
  );
}

const styles = StyleSheet.create({
  screen: { flex: 1, backgroundColor: '#ffffff' },
  content: { padding: 20, paddingTop: (NativeStatusBar.currentHeight ?? 24) + 20, paddingBottom: 48 },
  title: { fontSize: 26, fontWeight: '700', color: '#111111', marginBottom: 16 },
  label: { fontSize: 15, color: '#444444', marginTop: 12, marginBottom: 4 },
  version: { fontSize: 22, fontWeight: '600', color: '#111111' },
  field: { marginTop: 4 },
  input: {
    borderWidth: 1,
    borderColor: '#888888',
    borderRadius: 6,
    paddingHorizontal: 12,
    paddingVertical: 10,
    fontSize: 20,
    color: '#111111',
  },
  button: {
    marginTop: 20,
    backgroundColor: '#1f4e79',
    borderRadius: 6,
    paddingVertical: 14,
    alignItems: 'center',
  },
  buttonPressed: { opacity: 0.7 },
  buttonText: { color: '#ffffff', fontSize: 18, fontWeight: '600' },
  box: { marginTop: 16, padding: 12, borderRadius: 6, backgroundColor: '#f1f3f5' },
  errorBox: { backgroundColor: '#fde8e8' },
  resultLine: { fontSize: 20, color: '#111111', marginBottom: 2 },
  detail: { fontSize: 14, color: '#333333', marginTop: 6 },
  mono: { fontFamily: 'monospace', fontSize: 12, color: '#111111', marginBottom: 6 },
});
