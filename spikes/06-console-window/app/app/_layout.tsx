import { Stack } from 'expo-router';
import { useEffect } from 'react';

import { startSelfCheckIfAsked } from '../src/selfCheck';

export default function RootLayout() {
  useEffect(() => {
    // Does nothing unless the Rust side was started with --self-check.
    startSelfCheckIfAsked();
  }, []);

  return (
    <Stack>
      <Stack.Screen name="index" options={{ title: 'Today' }} />
      <Stack.Screen name="child/[id]" options={{ title: 'Child' }} />
    </Stack>
  );
}
