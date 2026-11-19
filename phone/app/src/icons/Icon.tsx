import type { ComponentType, ReactElement } from 'react';
import Svg, { Circle, Defs, G, LinearGradient, Path, RadialGradient, Rect, Stop } from 'react-native-svg';

import { ICONS, type IconName } from '../generated/icons';
import { tokens } from '../generated/tokens';
import type { IconNode, IconTag } from './types';

export type { IconName };

const ELEMENTS: Record<Exclude<IconTag, 'svg'>, ComponentType<Record<string, unknown>>> = {
  defs: Defs as ComponentType<Record<string, unknown>>,
  linearGradient: LinearGradient as ComponentType<Record<string, unknown>>,
  radialGradient: RadialGradient as ComponentType<Record<string, unknown>>,
  stop: Stop as ComponentType<Record<string, unknown>>,
  g: G as ComponentType<Record<string, unknown>>,
  path: Path as ComponentType<Record<string, unknown>>,
  rect: Rect as ComponentType<Record<string, unknown>>,
  circle: Circle as ComponentType<Record<string, unknown>>,
};

function draw(node: IconNode, key: number): ReactElement {
  const [tag, props, children] = node;
  if (tag === 'svg') throw new Error('an icon has an <svg> inside it');
  const Element = ELEMENTS[tag];
  return (
    <Element key={key} {...props}>
      {children?.map(draw)}
    </Element>
  );
}

export type IconProps = {
  name: IconName;
  /**
   * The size on screen. The design's sizes: tokens.icon.row (24) in a row or
   * a tab, tile (32), heading (48) beside a heading, emptyScreen (96).
   */
  size?: number;
  /** "ink" when the icon sits on an ink surface, so a plain icon's ink parts stay visible. */
  on?: 'paper' | 'ink';
  /** Words for a screen reader. Without them the icon is decoration and is skipped. */
  label?: string;
};

/** One of the design's icons (design/icons), painted as the design paints it. */
export function Icon({ name, size = tokens.icon.row, on = 'paper', label }: IconProps) {
  const entry = ICONS[name];
  const tree = on === 'ink' && entry.ink ? entry.ink : entry.paper;
  return (
    <Svg
      width={size}
      height={size}
      viewBox={`0 0 ${tokens.icon.grid} ${tokens.icon.grid}`}
      accessible={Boolean(label)}
      accessibilityLabel={label}
      accessibilityRole={label ? 'image' : undefined}
      importantForAccessibility={label ? 'yes' : 'no-hide-descendants'}
    >
      {tree[2]?.map(draw)}
    </Svg>
  );
}
