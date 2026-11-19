import { Gallery } from './src/preview/Gallery';

// On the web, the app is the preview of the child form's screens: a
// development tool, opened from disk after `npm run preview`. Android's bundle
// takes App.tsx instead and never reaches this file or anything it imports
// (tests/release.test.ts, npm run check:release-js).
export default function App() {
  return <Gallery />;
}
