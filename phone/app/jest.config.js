// The child form's screen tests: React Native under jest, with Expo's preset
// for Android, against the mock host (src/mock). Nothing here runs on a phone.
//
//   npm test

/** @type {import('jest').Config} */
module.exports = {
  preset: 'jest-expo/android',
  testMatch: ['<rootDir>/tests/**/*.test.ts?(x)'],
  // The Android preset expects a babel.config.js, which this app does not
  // have (Metro uses Expo's own preset without one). The same preset is
  // named here for jest only, so the app's build is left as it is.
  transform: {
    '\\.[jt]sx?$': [
      'babel-jest',
      {
        presets: [require.resolve('expo/internal/babel-preset')],
        caller: { name: 'metro', bundler: 'jest', platform: 'android' },
      },
    ],
  },
  // android/ is made by expo prebuild and holds copies of node packages.
  modulePathIgnorePatterns: ['<rootDir>/android/', '<rootDir>/dist/', '<rootDir>/out/'],
};
