/** The SVG elements a LiMo icon is made of (what design/icons/clay.js writes). */
export type IconTag = 'svg' | 'defs' | 'linearGradient' | 'radialGradient' | 'stop' | 'g' | 'path' | 'rect' | 'circle';

/**
 * One element of a painted icon: its tag, its props as react-native-svg
 * names them, and its children. scripts/make-design.mjs writes these.
 */
export type IconNode = readonly [tag: IconTag, props: Readonly<Record<string, string | number>>, children?: readonly IconNode[]];
