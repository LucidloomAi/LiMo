import { Link, useFocusEffect } from 'expo-router';
import { useCallback, useState } from 'react';
import { Pressable, ScrollView, StyleSheet, Text, View } from 'react-native';

import { host } from '../src/host';
import type { FamilyView } from '../src/types';

export default function TodayScreen() {
  const [view, setView] = useState<FamilyView | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);

  const load = useCallback(() => {
    host
      .request<FamilyView>('family_view')
      .then((v) => {
        setView(v);
        setError(null);
      })
      .catch((e: unknown) => setError(String(e)));
  }, []);

  // Read again whenever this screen comes back into view (after Child).
  useFocusEffect(load);

  const give = (childId: string) => {
    setBusy(childId);
    host
      .request<FamilyView>('give_minutes', { childId, minutes: 15 })
      .then((v) => {
        setView(v);
        setError(null);
      })
      .catch((e: unknown) => setError(String(e)))
      .finally(() => setBusy(null));
  };

  return (
    <ScrollView contentContainerStyle={styles.page}>
      <Text testID="where">
        Answers from: {view ? view.answeredBy : '...'} (host {host.where()})
      </Text>
      {error ? <Text testID="error">Error: {error}</Text> : null}
      {view?.children.map((child, i) => (
        <View key={child.id} style={styles.row}>
          <Link href={{ pathname: '/child/[id]', params: { id: child.id } }} testID={`child-link-${i}`}>
            <Text testID={`child-name-${i}`} style={styles.name}>
              {child.name}
            </Text>
          </Link>
          <Text testID={`time-used-${i}`}>Used {child.usedMinutes} min</Text>
          <Text testID={`time-left-${i}`}>Left {child.leftMinutes} min</Text>
          {child.locked ? <Text testID={`locked-${i}`}>Locked</Text> : null}
          <Pressable
            testID={`give-15-${i}`}
            accessibilityRole="button"
            disabled={busy === child.id}
            onPress={() => give(child.id)}
            style={styles.button}
          >
            <Text>Give 15 minutes</Text>
          </Pressable>
        </View>
      ))}
    </ScrollView>
  );
}

const styles = StyleSheet.create({
  page: { padding: 16, gap: 16 },
  row: { gap: 4, paddingVertical: 8, borderBottomWidth: 1, borderBottomColor: '#ccc' },
  name: { fontSize: 18, fontWeight: 'bold', textDecorationLine: 'underline' },
  button: { alignSelf: 'flex-start', padding: 8, borderWidth: 1, borderColor: '#888', borderRadius: 4 },
});
