// Turns one painted icon (the SVG that design/icons/clay.js writes) into an
// Android vector drawable, for the child form's own Kotlin views and for the
// launcher icon. Used by make-design.mjs.
//
// It handles exactly what clay.js writes, and refuses anything else with a
// message, rather than drawing it wrongly:
//   paths (M L H V C Q A Z, absolute or relative), rects with rounded
//   corners, circles; fills and round stroked lines; linear and radial
//   gradients, in the shape's own box (the SVG default) or in the icon's
//   space; a group that sets a shape down (translate) and fades it
//   (opacity); a shape turned about a point (rotate).
//
// A vector drawable has no gradient "in the shape's own box", and its radial
// gradients are always round. So a shape filled that way is drawn inside a
// group that stretches a 1 by 1 square onto the shape's box, with the path
// written in that square's units. The gradient is then exact, round or not.

// ---------------------------------------------------------------------------
// Paths

const ARGS = { M: 2, L: 2, H: 1, V: 1, C: 6, S: 4, Q: 4, T: 2, A: 7, Z: 0 };

/** A path as absolute commands: M, L, C, Q, A and Z only. */
export function parsePath(d, fail) {
  const tokens = d.match(/[a-zA-Z]|-?(?:\d+\.?\d*|\.\d+)(?:e-?\d+)?/g) ?? [];
  const out = [];
  let i = 0;
  let x = 0;
  let y = 0;
  let startX = 0;
  let startY = 0;
  let cmd = null;
  const num = () => {
    const t = tokens[i++];
    if (t === undefined || /[a-zA-Z]/.test(t)) fail(`path "${d}": a number is missing`);
    return Number(t);
  };
  while (i < tokens.length) {
    if (/[a-zA-Z]/.test(tokens[i])) cmd = tokens[i++];
    else if (!cmd) fail(`path "${d}" does not start with a command`);
    const upper = cmd.toUpperCase();
    if (!(upper in ARGS) || upper === 'S' || upper === 'T') fail(`path "${d}": command ${cmd} is not handled`);
    const rel = cmd !== upper;
    if (upper === 'Z') {
      out.push(['Z']);
      x = startX;
      y = startY;
      cmd = null;
      continue;
    }
    if (upper === 'M') {
      x = (rel ? x : 0) + num();
      y = (rel ? y : 0) + num();
      startX = x;
      startY = y;
      out.push(['M', x, y]);
      cmd = rel ? 'l' : 'L'; // further pairs are lines
    } else if (upper === 'L') {
      x = (rel ? x : 0) + num();
      y = (rel ? y : 0) + num();
      out.push(['L', x, y]);
    } else if (upper === 'H') {
      x = (rel ? x : 0) + num();
      out.push(['L', x, y]);
    } else if (upper === 'V') {
      y = (rel ? y : 0) + num();
      out.push(['L', x, y]);
    } else if (upper === 'C') {
      const p = [num(), num(), num(), num(), num(), num()];
      if (rel) for (let k = 0; k < 6; k++) p[k] += k % 2 ? y : x;
      out.push(['C', ...p]);
      x = p[4];
      y = p[5];
    } else if (upper === 'Q') {
      const p = [num(), num(), num(), num()];
      if (rel) for (let k = 0; k < 4; k++) p[k] += k % 2 ? y : x;
      out.push(['Q', ...p]);
      x = p[2];
      y = p[3];
    } else if (upper === 'A') {
      const rx = num();
      const ry = num();
      const rot = num();
      const large = num();
      const sweep = num();
      const ex = (rel ? x : 0) + num();
      const ey = (rel ? y : 0) + num();
      if (rot !== 0) fail(`path "${d}": a turned arc is not handled`);
      out.push(['A', rx, ry, 0, large, sweep, ex, ey]);
      x = ex;
      y = ey;
    }
  }
  return out;
}

function rectPath(a) {
  const x = Number(a.x ?? 0);
  const y = Number(a.y ?? 0);
  const w = Number(a.width);
  const h = Number(a.height);
  const r = Math.min(Number(a.rx ?? a.ry ?? 0), w / 2, h / 2);
  if (!r) return [['M', x, y], ['L', x + w, y], ['L', x + w, y + h], ['L', x, y + h], ['Z']];
  return [
    ['M', x + r, y],
    ['L', x + w - r, y],
    ['A', r, r, 0, 0, 1, x + w, y + r],
    ['L', x + w, y + h - r],
    ['A', r, r, 0, 0, 1, x + w - r, y + h],
    ['L', x + r, y + h],
    ['A', r, r, 0, 0, 1, x, y + h - r],
    ['L', x, y + r],
    ['A', r, r, 0, 0, 1, x + r, y],
    ['Z'],
  ];
}

function circlePath(a) {
  const cx = Number(a.cx);
  const cy = Number(a.cy);
  const r = Number(a.r);
  return [['M', cx - r, cy], ['A', r, r, 0, 1, 0, cx + r, cy], ['A', r, r, 0, 1, 0, cx - r, cy], ['Z']];
}

// Points along an arc, from SVG's endpoint form (F.6.5 of the SVG 1.1 spec).
function arcPoints(x1, y1, rx, ry, large, sweep, x2, y2) {
  const dx = (x1 - x2) / 2;
  const dy = (y1 - y2) / 2;
  let lambda = (dx * dx) / (rx * rx) + (dy * dy) / (ry * ry);
  if (lambda > 1) {
    rx *= Math.sqrt(lambda);
    ry *= Math.sqrt(lambda);
    lambda = 1;
  }
  const sign = large === sweep ? -1 : 1;
  const num = rx * rx * ry * ry - rx * rx * dy * dy - ry * ry * dx * dx;
  const k = sign * Math.sqrt(Math.max(0, num / (rx * rx * dy * dy + ry * ry * dx * dx)));
  const cxp = (k * rx * dy) / ry;
  const cyp = (-k * ry * dx) / rx;
  const cx = cxp + (x1 + x2) / 2;
  const cy = cyp + (y1 + y2) / 2;
  const angle = (ux, uy, vx, vy) => Math.atan2(ux * vy - uy * vx, ux * vx + uy * vy);
  const t1 = angle(1, 0, (dx - cxp) / rx, (dy - cyp) / ry);
  let dt = angle((dx - cxp) / rx, (dy - cyp) / ry, (-dx - cxp) / rx, (-dy - cyp) / ry);
  if (!sweep && dt > 0) dt -= 2 * Math.PI;
  if (sweep && dt < 0) dt += 2 * Math.PI;
  const pts = [];
  for (let s = 0; s <= 64; s++) {
    const t = t1 + (dt * s) / 64;
    pts.push([cx + rx * Math.cos(t), cy + ry * Math.sin(t)]);
  }
  return pts;
}

/** The box a path covers: [x, y, width, height]. Curves are followed closely. */
export function pathBox(cmds) {
  let minX = Infinity;
  let minY = Infinity;
  let maxX = -Infinity;
  let maxY = -Infinity;
  const add = (px, py) => {
    minX = Math.min(minX, px);
    minY = Math.min(minY, py);
    maxX = Math.max(maxX, px);
    maxY = Math.max(maxY, py);
  };
  let x = 0;
  let y = 0;
  let sx = 0;
  let sy = 0;
  for (const c of cmds) {
    switch (c[0]) {
      case 'M':
        x = sx = c[1];
        y = sy = c[2];
        add(x, y);
        break;
      case 'L':
        x = c[1];
        y = c[2];
        add(x, y);
        break;
      case 'C':
        for (let s = 1; s <= 64; s++) {
          const t = s / 64;
          const u = 1 - t;
          add(
            u * u * u * x + 3 * u * u * t * c[1] + 3 * u * t * t * c[3] + t * t * t * c[5],
            u * u * u * y + 3 * u * u * t * c[2] + 3 * u * t * t * c[4] + t * t * t * c[6],
          );
        }
        x = c[5];
        y = c[6];
        break;
      case 'Q':
        for (let s = 1; s <= 64; s++) {
          const t = s / 64;
          const u = 1 - t;
          add(u * u * x + 2 * u * t * c[1] + t * t * c[3], u * u * y + 2 * u * t * c[2] + t * t * c[4]);
        }
        x = c[3];
        y = c[4];
        break;
      case 'A':
        for (const [px, py] of arcPoints(x, y, c[1], c[2], c[4], c[5], c[6], c[7])) add(px, py);
        x = c[6];
        y = c[7];
        break;
      case 'Z':
        x = sx;
        y = sy;
        break;
    }
  }
  return [minX, minY, maxX - minX, maxY - minY];
}

const fmt = (n) => {
  const s = Number(n.toFixed(4)).toString();
  return s === '-0' ? '0' : s;
};

/** Path data, optionally moved into the unit square of box [x, y, w, h]. */
function pathData(cmds, box) {
  const [bx, by, bw, bh] = box ?? [0, 0, 1, 1];
  const px = (v) => fmt((v - bx) / bw);
  const py = (v) => fmt((v - by) / bh);
  return cmds
    .map((c) => {
      switch (c[0]) {
        case 'M':
        case 'L':
          return `${c[0]}${px(c[1])},${py(c[2])}`;
        case 'C':
          return `C${px(c[1])},${py(c[2])} ${px(c[3])},${py(c[4])} ${px(c[5])},${py(c[6])}`;
        case 'Q':
          return `Q${px(c[1])},${py(c[2])} ${px(c[3])},${py(c[4])}`;
        case 'A':
          return `A${fmt(c[1] / bw)},${fmt(c[2] / bh)} 0 ${c[4]},${c[5]} ${px(c[6])},${py(c[7])}`;
        default:
          return 'Z';
      }
    })
    .join(' ');
}

// ---------------------------------------------------------------------------
// Colours and gradients

function argb(hex, opacity, fail) {
  if (!/^#[0-9a-f]{6}$/i.test(hex)) fail(`colour ${hex} is not #rrggbb`);
  const a = Math.round(Math.min(1, Math.max(0, opacity)) * 255);
  return `#${a.toString(16).padStart(2, '0')}${hex.slice(1).toLowerCase()}`;
}

function gradientXml(grad, alpha, indent, fail) {
  const [tag, a, stops] = grad;
  const items = (stops ?? []).map(([stopTag, s]) => {
    if (stopTag !== 'stop') fail(`<${stopTag}> inside a gradient`);
    const color = argb(s['stop-color'], Number(s['stop-opacity'] ?? 1) * alpha, fail);
    return `${indent}    <item android:offset="${fmt(Number(s.offset))}" android:color="${color}" />`;
  });
  let head;
  if (tag === 'linearGradient') {
    head = `android:type="linear" android:startX="${fmt(Number(a.x1 ?? 0))}" android:startY="${fmt(Number(a.y1 ?? 0))}" android:endX="${fmt(Number(a.x2 ?? 1))}" android:endY="${fmt(Number(a.y2 ?? 0))}"`;
  } else if (tag === 'radialGradient') {
    if (a.fx !== undefined || a.fy !== undefined) fail('a radial gradient with its own focus is not handled');
    head = `android:type="radial" android:centerX="${fmt(Number(a.cx ?? 0.5))}" android:centerY="${fmt(Number(a.cy ?? 0.5))}" android:gradientRadius="${fmt(Number(a.r ?? 0.5))}"`;
  } else {
    fail(`<${tag}> is not a gradient`);
  }
  return [`${indent}<gradient ${head}>`, ...items, `${indent}</gradient>`].join('\n');
}

// ---------------------------------------------------------------------------
// The icon

/**
 * One icon as a vector drawable.
 *   svg      the tree of the painted icon: [tag, attributes, children]
 *   opts     { sizeDp: the drawable's size (48 by default),
 *              gridDp: how big the icon's 48 unit grid is drawn inside it
 *                      (sizeDp by default; less leaves a margin, as a
 *                      launcher icon's foreground needs),
 *              fail:   called with a message for anything not handled }
 */
export function toVectorDrawable(svg, opts) {
  const fail = opts.fail;
  const sizeDp = opts.sizeDp ?? 48;
  const gridDp = opts.gridDp ?? sizeDp;
  const viewport = (48 * sizeDp) / gridDp;
  const offset = (viewport - 48) / 2;

  const [rootTag, , rootChildren] = svg;
  if (rootTag !== 'svg') fail('the icon is not an <svg>');
  const gradients = {};
  const shapes = [];
  for (const child of rootChildren ?? []) {
    if (child[0] === 'defs') {
      for (const g of child[2] ?? []) gradients[g[1].id] = g;
    } else {
      shapes.push(child);
    }
  }

  const lines = [];
  const gradientOf = (value) => {
    const m = /^url\(#([^)]+)\)$/.exec(value);
    if (!m) return null;
    const grad = gradients[m[1]];
    if (!grad) fail(`gradient ${m[1]} is not defined`);
    return grad;
  };
  const inShapeBox = (grad) => Boolean(grad) && grad[1].gradientUnits !== 'userSpaceOnUse';

  function shape(node, alpha, indent) {
    const [tag, a] = node;
    let cmds;
    if (tag === 'path') cmds = parsePath(a.d, fail);
    else if (tag === 'rect') cmds = rectPath(a);
    else if (tag === 'circle') cmds = circlePath(a);
    else fail(`<${tag}> is not a shape`);
    alpha *= Number(a.opacity ?? 1);

    let inner = indent;
    const closeGroups = [];
    if (a.transform) {
      const r = /^rotate\((-?[\d.]+)[ ,]+(-?[\d.]+)[ ,]+(-?[\d.]+)\)$/.exec(a.transform);
      if (!r) fail(`transform "${a.transform}" is not handled`);
      lines.push(`${inner}<group android:rotation="${fmt(Number(r[1]))}" android:pivotX="${fmt(Number(r[2]))}" android:pivotY="${fmt(Number(r[3]))}">`);
      closeGroups.push(`${inner}</group>`);
      inner += '    ';
    }

    const fill = a.fill && a.fill !== 'none' ? a.fill : null;
    const stroke = a.stroke && a.stroke !== 'none' ? a.stroke : null;
    if (fill && stroke) fail('a shape with both a fill and a stroke is not handled');
    if (!fill && !stroke) fail('a shape with neither a fill nor a stroke');
    const value = fill ?? stroke;
    const attr = fill ? 'fillColor' : 'strokeColor';
    const grad = gradientOf(value);

    // A gradient in the shape's own box: draw the shape in a 1 by 1 square
    // stretched onto that box. Only fills: a stroke would be stretched too.
    let box = null;
    if (inShapeBox(grad)) {
      if (stroke) fail(`a stroke with gradient ${grad[1].id} in the shape's box is not handled`);
      box = pathBox(cmds);
      lines.push(
        `${inner}<group android:translateX="${fmt(box[0])}" android:translateY="${fmt(box[1])}" android:scaleX="${fmt(box[2])}" android:scaleY="${fmt(box[3])}">`,
      );
      closeGroups.push(`${inner}</group>`);
      inner += '    ';
    }

    const attrs = [`android:pathData="${pathData(cmds, box)}"`];
    if (!grad) attrs.push(`android:${attr}="${argb(value, alpha, fail)}"`);
    if (stroke) {
      attrs.push(`android:strokeWidth="${fmt(Number(a['stroke-width'] ?? 1))}"`);
      if (a['stroke-linecap']) attrs.push(`android:strokeLineCap="${a['stroke-linecap']}"`);
      if (a['stroke-linejoin']) attrs.push(`android:strokeLineJoin="${a['stroke-linejoin']}"`);
    }
    if (grad) {
      lines.push(
        `${inner}<path ${attrs.join(' ')}>`,
        `${inner}    <aapt:attr name="android:${attr}">`,
        gradientXml(grad, alpha, `${inner}        `, fail),
        `${inner}    </aapt:attr>`,
        `${inner}</path>`,
      );
    } else {
      lines.push(`${inner}<path ${attrs.join(' ')} />`);
    }
    while (closeGroups.length) lines.push(closeGroups.pop());
  }

  function node(n, alpha, indent) {
    const [tag, a, children] = n;
    if (tag !== 'g') return shape(n, alpha, indent);
    const t = /^translate\((-?[\d.]+)[ ,]+(-?[\d.]+)\)$/.exec(a.transform ?? 'translate(0 0)');
    if (!t) fail(`group transform "${a.transform}" is not handled`);
    lines.push(`${indent}<group android:translateX="${fmt(Number(t[1]))}" android:translateY="${fmt(Number(t[2]))}">`);
    for (const c of children ?? []) node(c, alpha * Number(a.opacity ?? 1), `${indent}    `);
    lines.push(`${indent}</group>`);
  }

  lines.push(
    '<vector xmlns:android="http://schemas.android.com/apk/res/android"',
    '    xmlns:aapt="http://schemas.android.com/aapt"',
    `    android:width="${fmt(sizeDp)}dp"`,
    `    android:height="${fmt(sizeDp)}dp"`,
    `    android:viewportWidth="${fmt(viewport)}"`,
    `    android:viewportHeight="${fmt(viewport)}">`,
    `    <group android:translateX="${fmt(offset)}" android:translateY="${fmt(offset)}">`,
  );
  for (const s of shapes) node(s, 1, '        ');
  lines.push('    </group>', '</vector>', '');
  return lines.join('\n');
}
