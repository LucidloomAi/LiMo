import { StatusBar } from 'react-native';
import { SafeAreaProvider } from 'react-native-safe-area-context';

import ChildStart from './app/child';
import GuardianStart from './app/guardian';
import { appForm } from './src/form';

// The root: the form's own start screen. The screens live in app/<form>/ so
// that, if the app later takes up Expo Router (see the README), they are
// already where its routes would be.
export default function App() {
  const form = appForm();
  return (
    <SafeAreaProvider>
      <StatusBar barStyle="dark-content" translucent backgroundColor="transparent" />
      {form === 'child' ? <ChildStart /> : <GuardianStart />}
    </SafeAreaProvider>
  );
}
