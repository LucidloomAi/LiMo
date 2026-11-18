import { router, useLocalSearchParams } from 'expo-router';
import { useCallback, useEffect, useState } from 'react';
import { Pressable, ScrollView, StyleSheet, Text } from 'react-native';

import { host } from '../../src/host';
import type { FamilyView } from '../../src/types';

export default function ChildScreen() {
  const { id } = useLocalSearchParams<{ id: string }>();
  const [view, setView] = useState<FamilyView | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const show = useCallback((v: FamilyView) => {
    setView(v);
    setError(null);
  }, []);

  useEffect(() => {
    host
      .request<FamilyView>('family_view')
      .then(show)
      .catch((e: unknown) => setError(String(e)));
  }, [show]);

  const lock = () => {
    setBusy(true);
    host
      .request<FamilyView>('lock_child', { childId: id })
      .then(show)
      .catch((e: unknown) => setError(String(e)))
      .finally(() => setBusy(false));
  };

  const back = () => {
    if (router.canGoBack()) {
      router.back();
    } else {
      router.replace('/');
    }
  };

  const child = view?.children.find((c) => c.id === id);

  return (
    <ScrollView contentContainerStyle={styles.page}>
      {error ? <Text testID="error">Error: {error}</Text> : null}
      {view && !child ? <Text testID="not-found">No child with id {id}</Text> : null}
      {child ? (
        <>
          <Text testID="child-heading" style={styles.heading}>
            {child.name}
          </Text>
          <Text testID="child-time-left">Left {child.leftMinutes} min</Text>
          <Text testID="child-status">{child.locked ? 'Locked' : 'Unlocked'}</Text>
          <Text style={styles.sub}>Devices</Text>
          {child.devices.map((d, i) => (
            <Text key={d.id} testID={`device-${i}`}>
              {d.name} ({d.kind}){d.locked ? ', locked' : ''}
            </Text>
          ))}
          <Pressable
            testID="lock-now"
            accessibilityRole="button"
            disabled={busy}
            onPress={lock}
            style={styles.button}
          >
            <Text>Lock now</Text>
          </Pressable>
        </>
      ) : null}
      <Pressable testID="back" accessibilityRole="button" onPress={back} style={styles.button}>
        <Text>Back</Text>
      </Pressable>
    </ScrollView>
  );
}

const styles = StyleSheet.create({
  page: { padding: 16, gap: 8 },
  heading: { fontSize: 22, fontWeight: 'bold' },
  sub: { marginTop: 8, fontWeight: 'bold' },
  button: { alignSelf: 'flex-start', padding: 8, borderWidth: 1, borderColor: '#888', borderRadius: 4, marginTop: 8 },
});
