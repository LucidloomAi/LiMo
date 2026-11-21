import { registerRootComponent } from 'expo';

import { ConsoleApp } from './src/console/ConsoleApp';

// The guardian console's page, for the web only: `npm run build:console`
// bundles from here into dist/console, which the console's Tauri window on
// the PC embeds (scripts/build-console.mjs). Android's bundle starts at
// index.ts and never reaches this file.
registerRootComponent(ConsoleApp);
