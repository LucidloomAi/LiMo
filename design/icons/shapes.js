// The drawings of LiMo's icons, on a 48 by 48 grid.
//
// A drawing is a short list of simple shapes. Each shape has a ROLE, and the
// theme (clay.js) decides how a role is painted. A drawing says what the
// thing is. Its colour says what it means (see `clay` on each icon).
//
// Roles, back to front:
//   back    a part behind the body, in the darker clay
//   body    the main form, in the lighter clay
//   top     a part on the body, in the darker clay
//   arm     a thick bar in the darker clay (a shackle, a handle, an arc)
//   screen  a glossy inset in the darker clay (a display)
//   panel   a pale area on the body (a clock face)
//   dot     small marks on the lighter clay
//   line    strokes on the lighter clay
//   tdot    small marks on the darker clay
//   tline   strokes on the darker clay
//   mark    small marks on a panel, or standing free
//   ring    strokes on a panel, or standing free
//   gap     a stroke in the body's own colour, to part two things
//
// Rules for a new drawing:
//   A handful of chunky shapes. Small details blur at 24 pixels.
//   Fill about 38 of the 48 units both ways, so every icon looks one size.
//   Round every corner.
//   One colour: honey for time and LiMo's own things, teal for allowed and
//   at home, coral for locked and stop, ink for plain things.
//
// icon = { clay, use, shapes }, shape = [role, element, attributes]

(function (root, factory) {
  if (typeof module === 'object' && module.exports) module.exports = factory();
  else root.LIMO_SHAPES = factory();
})(typeof self !== 'undefined' ? self : this, function () {
  function n(v) { return Number(v.toFixed(2)); }

  // A rounded rectangle as one closed piece of a path, drawn clockwise.
  function rr(x, y, w, h, r) {
    return 'M' + n(x + r) + ' ' + n(y) + ' H' + n(x + w - r) + ' a' + r + ' ' + r + ' 0 0 1 ' + r + ' ' + r +
      ' V' + n(y + h - r) + ' a' + r + ' ' + r + ' 0 0 1 ' + (-r) + ' ' + r +
      ' H' + n(x + r) + ' a' + r + ' ' + r + ' 0 0 1 ' + (-r) + ' ' + (-r) +
      ' V' + n(y + r) + ' a' + r + ' ' + r + ' 0 0 1 ' + r + ' ' + (-r) + ' Z';
  }

  // A polygon with soft corners. `r` is how far from each corner the curve
  // starts: one number, or a function of the corner's index.
  function roundPoly(pts, r) {
    var d = '';
    var count = pts.length;
    for (var i = 0; i < count; i += 1) {
      var p = pts[i], a = pts[(i + count - 1) % count], b = pts[(i + 1) % count];
      var ra = typeof r === 'function' ? r(i) : r;
      var la = Math.hypot(a[0] - p[0], a[1] - p[1]), lb = Math.hypot(b[0] - p[0], b[1] - p[1]);
      var ka = Math.min(ra, la / 2) / la, kb = Math.min(ra, lb / 2) / lb;
      var s = [p[0] + (a[0] - p[0]) * ka, p[1] + (a[1] - p[1]) * ka];
      var e = [p[0] + (b[0] - p[0]) * kb, p[1] + (b[1] - p[1]) * kb];
      d += (i ? ' L' : 'M') + n(s[0]) + ' ' + n(s[1]) + ' Q' + n(p[0]) + ' ' + n(p[1]) + ' ' + n(e[0]) + ' ' + n(e[1]);
    }
    return d + ' Z';
  }

  // A bar with round ends between two points, as one closed clockwise piece.
  function capsule(x1, y1, x2, y2, r) {
    var len = Math.hypot(x2 - x1, y2 - y1);
    var dx = (x2 - x1) / len, dy = (y2 - y1) / len;
    var base = Math.atan2(dy, dx);
    var pts = [];
    var i;
    for (i = 0; i <= 10; i += 1) {
      var t1 = base - Math.PI / 2 + (Math.PI * i) / 10;
      pts.push([x2 + r * Math.cos(t1), y2 + r * Math.sin(t1)]);
    }
    for (i = 0; i <= 10; i += 1) {
      var t2 = base + Math.PI / 2 + (Math.PI * i) / 10;
      pts.push([x1 + r * Math.cos(t2), y1 + r * Math.sin(t2)]);
    }
    var area = 0;
    for (i = 0; i < pts.length; i += 1) {
      var q = pts[(i + 1) % pts.length];
      area += pts[i][0] * q[1] - q[0] * pts[i][1];
    }
    if (area < 0) pts.reverse();
    return 'M' + pts.map(function (p) { return n(p[0]) + ' ' + n(p[1]); }).join(' L') + ' Z';
  }

  function star(cx, cy, outer, inner) {
    var pts = [];
    for (var i = 0; i < 10; i += 1) {
      var a = (Math.PI / 5) * i - Math.PI / 2;
      var r = i % 2 === 0 ? outer : inner;
      pts.push([cx + r * Math.cos(a), cy + r * Math.sin(a)]);
    }
    return roundPoly(pts, function (i) { return i % 2 === 0 ? outer * 0.2 : outer * 0.07; });
  }

  function gear(cx, cy, outer, rootR, teeth) {
    var pts = [];
    var step = (Math.PI * 2) / teeth;
    for (var i = 0; i < teeth; i += 1) {
      var a = step * i - Math.PI / 2;
      [[-0.3, rootR], [-0.17, outer], [0.17, outer], [0.3, rootR]].forEach(function (o) {
        var rad = a + step * o[0];
        pts.push([cx + o[1] * Math.cos(rad), cy + o[1] * Math.sin(rad)]);
      });
    }
    return roundPoly(pts, 1.5);
  }

  function rays(cx, cy, r1, r2, count) {
    var d = '';
    for (var i = 0; i < count; i += 1) {
      var a = ((Math.PI * 2) / count) * i;
      d += 'M' + n(cx + r1 * Math.cos(a)) + ' ' + n(cy + r1 * Math.sin(a)) +
        ' L' + n(cx + r2 * Math.cos(a)) + ' ' + n(cy + r2 * Math.sin(a)) + ' ';
    }
    return d.trim();
  }

  // Head and shoulders. The shoulders are a soft mound that ends flat.
  function shoulders(cx, top, halfWidth, bottom) {
    var k = n(halfWidth * 0.44), r = n(Math.min(2.6, halfWidth * 0.16));
    return 'M' + n(cx - halfWidth) + ' ' + n(bottom - r) +
      ' C' + n(cx - halfWidth) + ' ' + n(top + (bottom - top) * 0.36) + ' ' + n(cx - k) + ' ' + n(top) + ' ' + n(cx) + ' ' + n(top) +
      ' C' + n(cx + k) + ' ' + n(top) + ' ' + n(cx + halfWidth) + ' ' + n(top + (bottom - top) * 0.36) + ' ' + n(cx + halfWidth) + ' ' + n(bottom - r) +
      ' a' + r + ' ' + r + ' 0 0 1 ' + (-r) + ' ' + r + ' H' + n(cx - halfWidth + r) +
      ' a' + r + ' ' + r + ' 0 0 1 ' + (-r) + ' ' + (-r) + ' Z';
  }

  var DISC = ['body', 'circle', { cx: 24, cy: 24, r: 19.5 }];

  return {
    // ---------- Places and people ----------
    home: {
      clay: 'teal', use: 'At home, the home network',
      shapes: [
        ['body', 'rect', { x: 10, y: 19, width: 28, height: 24, rx: 6 }],
        ['top', 'path', { d: 'M22 5.9 a3.1 3.1 0 0 1 4 0 L43 20.3 a2.3 2.3 0 0 1 -1.5 4.1 H6.5 A2.3 2.3 0 0 1 5 20.3 Z' }],
        ['dot', 'path', { d: 'M19.5 43 V33.5 a4.5 4.5 0 0 1 9 0 V43 Z' }],
      ],
    },
    family: {
      clay: 'ink', use: 'The family',
      shapes: [
        ['top', 'path', { d: shoulders(17, 29, 13, 43.5) }],
        ['body', 'circle', { cx: 17, cy: 16.5, r: 10.2 }],
        ['top', 'path', { d: shoulders(33.5, 34.4, 9.5, 43.5) }],
        ['body', 'circle', { cx: 33.5, cy: 25.6, r: 7.4 }],
      ],
    },
    child: {
      clay: 'ink', use: 'A child',
      shapes: [
        ['top', 'path', { d: shoulders(24, 29.2, 16.5, 43.5) }],
        ['body', 'circle', { cx: 24, cy: 17.5, r: 12.3 }],
        ['dot', 'circle', { cx: 19.6, cy: 17, r: 1.8 }],
        ['dot', 'circle', { cx: 28.4, cy: 17, r: 1.8 }],
        ['line', 'path', { d: 'M20 22 C21.8 24.1 26.2 24.1 28 22', 'stroke-width': 2.4 }],
      ],
    },
    guardian: {
      clay: 'ink', use: 'A guardian',
      shapes: [
        ['top', 'path', { d: shoulders(21, 28.6, 16.5, 43.5) }],
        ['body', 'circle', { cx: 21, cy: 16.2, r: 11.2 }],
        ['body', 'path', { d: 'M35 23.6 C37.5 25.5 40.3 26.4 43.2 26.5 a1.2 1.2 0 0 1 1.2 1.2 V32.6 C44.4 38 40.8 42 35 44.2 C29.2 42 25.6 38 25.6 32.6 V27.7 a1.2 1.2 0 0 1 1.2 -1.2 C29.7 26.4 32.5 25.5 35 23.6 Z' }],
        ['line', 'path', { d: 'M30.9 33.4 l2.8 2.8 l5.2 -5.7', 'stroke-width': 2.8 }],
      ],
    },
    pc: {
      clay: 'ink', use: 'The master PC',
      shapes: [
        ['back', 'rect', { x: 20.5, y: 30, width: 7, height: 10, rx: 2 }],
        ['top', 'rect', { x: 13, y: 37.2, width: 22, height: 6.3, rx: 3.15 }],
        ['body', 'rect', { x: 4.5, y: 6, width: 39, height: 27.5, rx: 7 }],
        ['screen', 'rect', { x: 8.7, y: 10.2, width: 30.6, height: 19.1, rx: 3.6 }],
      ],
    },
    phone: {
      clay: 'ink', use: 'A phone',
      shapes: [
        ['body', 'rect', { x: 12.5, y: 4, width: 23, height: 40, rx: 7 }],
        ['screen', 'rect', { x: 16, y: 8.2, width: 16, height: 26.4, rx: 3.4 }],
        ['dot', 'rect', { x: 20.2, y: 37.5, width: 7.6, height: 2.6, rx: 1.3 }],
      ],
    },
    // A handset, drawn upright (the ear at the top, the mouth at the bottom,
    // both facing right, the back bowed) and turned as handsets are drawn.
    // Not a phone: the phone permission is about calls, so it has its own
    // drawing. The pads are the faces of the ear and mouth pieces.
    call: {
      clay: 'ink', use: 'A call, ringing or on',
      shapes: [
        ['body', 'path', { d: 'M19.5 5.3 H29.8 a5.2 5.2 0 0 1 5.2 5.2 V13.6 a5.2 5.2 0 0 1 -5.2 5.2 a5.6 5.6 0 0 0 0 11.2 a5.2 5.2 0 0 1 5.2 5.2 V38.3 a5.2 5.2 0 0 1 -5.2 5.2 H19.5 C14.5 43.5 11.2 38.8 11.2 33.2 V15.6 C11.2 10 14.5 5.3 19.5 5.3 Z', transform: 'rotate(-45 24 24)' }],
        ['top', 'path', { d: 'M29 5.3 H29.8 a5.2 5.2 0 0 1 5.2 5.2 V13.6 a5.2 5.2 0 0 1 -5.2 5.2 H29 a2 2 0 0 1 -2 -2 V7.3 a2 2 0 0 1 2 -2 Z', transform: 'rotate(-45 24 24)' }],
        ['top', 'path', { d: 'M29 30 H29.8 a5.2 5.2 0 0 1 5.2 5.2 V38.3 a5.2 5.2 0 0 1 -5.2 5.2 H29 a2 2 0 0 1 -2 -2 V32 a2 2 0 0 1 2 -2 Z', transform: 'rotate(-45 24 24)' }],
      ],
    },
    devices: {
      clay: 'ink', use: 'All of a child’s devices',
      shapes: [
        ['back', 'rect', { x: 15, y: 27, width: 6, height: 10, rx: 2 }],
        ['top', 'rect', { x: 8.5, y: 34.6, width: 19, height: 5.6, rx: 2.8 }],
        ['body', 'rect', { x: 3.5, y: 6, width: 33, height: 24, rx: 6 }],
        ['screen', 'rect', { x: 7.2, y: 9.7, width: 25.6, height: 16.6, rx: 3 }],
        ['body', 'rect', { x: 26.5, y: 16.5, width: 18, height: 28, rx: 5.4 }],
        ['screen', 'rect', { x: 29.4, y: 19.8, width: 12.2, height: 18.4, rx: 2.5 }],
      ],
    },
    apps: {
      clay: 'ink', use: 'Apps',
      shapes: [
        ['body', 'rect', { x: 5.5, y: 5.5, width: 16.5, height: 16.5, rx: 5.2 }],
        ['body', 'rect', { x: 26, y: 5.5, width: 16.5, height: 16.5, rx: 5.2 }],
        ['body', 'rect', { x: 5.5, y: 26, width: 16.5, height: 16.5, rx: 5.2 }],
        ['top', 'rect', { x: 26, y: 26, width: 16.5, height: 16.5, rx: 5.2 }],
      ],
    },
    account: {
      clay: 'ink', use: 'A Windows account',
      shapes: [
        ['body', 'rect', { x: 4.5, y: 9, width: 39, height: 30, rx: 7.5 }],
        ['top', 'circle', { cx: 15.6, cy: 19.6, r: 4.7 }],
        ['top', 'path', { d: shoulders(15.6, 26, 7, 33.6) }],
        ['line', 'path', { d: 'M27.5 18.5 H37 M27.5 24.5 H37 M27.5 30.5 H33', 'stroke-width': 3 }],
      ],
    },

    // ---------- Time ----------
    clock: {
      clay: 'honey', use: 'Time used',
      shapes: [
        DISC,
        ['panel', 'circle', { cx: 24, cy: 24, r: 14 }],
        ['ring', 'path', { d: 'M24 15.5 V24 L30 27.6', 'stroke-width': 3.4 }],
      ],
    },
    hourglass: {
      clay: 'honey', use: 'Time left',
      shapes: [
        ['body', 'path', { d: 'M13.5 9 H34.5 V15.5 C34.5 20.6 28.4 22.4 28.4 24 C28.4 25.6 34.5 27.4 34.5 32.5 V39 H13.5 V32.5 C13.5 27.4 19.6 25.6 19.6 24 C19.6 22.4 13.5 20.6 13.5 15.5 Z' }],
        ['dot', 'path', { d: 'M18.2 14.5 H29.8 C29.2 18 26.4 19.8 24 19.8 C21.6 19.8 18.8 18 18.2 14.5 Z' }],
        ['dot', 'path', { d: 'M17.4 37 C17.8 32.6 20.8 30.4 24 30.4 C27.2 30.4 30.2 32.6 30.6 37 Z' }],
        ['top', 'rect', { x: 9, y: 4.5, width: 30, height: 7.4, rx: 3.7 }],
        ['top', 'rect', { x: 9, y: 36.1, width: 30, height: 7.4, rx: 3.7 }],
      ],
    },
    calendar: {
      clay: 'honey', use: 'The budget for each day of the week',
      shapes: [
        ['body', 'rect', { x: 6, y: 9, width: 36, height: 34, rx: 8 }],
        ['top', 'path', { d: 'M14 9 H34 a8 8 0 0 1 8 8 V20.5 H6 V17 a8 8 0 0 1 8 -8 Z' }],
        ['ring', 'path', { d: 'M15.5 5.6 V12.4 M32.5 5.6 V12.4', 'stroke-width': 3.8 }],
        ['dot', 'rect', { x: 12, y: 25.6, width: 6.4, height: 5.2, rx: 1.9 }],
        ['dot', 'rect', { x: 20.8, y: 25.6, width: 6.4, height: 5.2, rx: 1.9 }],
        ['dot', 'rect', { x: 29.6, y: 25.6, width: 6.4, height: 5.2, rx: 1.9 }],
        ['dot', 'rect', { x: 12, y: 33.4, width: 6.4, height: 5.2, rx: 1.9 }],
        ['dot', 'rect', { x: 20.8, y: 33.4, width: 6.4, height: 5.2, rx: 1.9 }],
      ],
    },
    sun: {
      clay: 'honey', use: 'Today',
      shapes: [
        ['arm', 'path', { d: rays(24, 24, 15.6, 18.7, 8), 'stroke-width': 4.4 }],
        ['body', 'circle', { cx: 24, cy: 24, r: 10.8 }],
      ],
    },
    moon: {
      clay: 'honey', use: 'Bedtime, hours that are off limits',
      shapes: [
        ['body', 'path', { d: 'M23.21 6.5 A18.5 18.5 0 1 0 41.04 29.09 A14.4 14.4 0 0 1 23.21 6.5 Z' }],
        ['top', 'path', { d: star(35.2, 15.4, 6.6, 3.1) }],
      ],
    },
    addtime: {
      clay: 'honey', use: 'Give extra time',
      shapes: [
        ['body', 'circle', { cx: 20.5, cy: 21, r: 16.5 }],
        ['panel', 'circle', { cx: 20.5, cy: 21, r: 11.6 }],
        ['ring', 'path', { d: 'M20.5 13.6 V21 L25.4 24', 'stroke-width': 3.2 }],
        ['top', 'circle', { cx: 35, cy: 35, r: 9.6 }],
        ['tline', 'path', { d: 'M35 30.4 V39.6 M30.4 35 H39.6', 'stroke-width': 3.2 }],
      ],
    },
    bell: {
      clay: 'honey', use: 'A warning before the time runs out',
      shapes: [
        ['back', 'circle', { cx: 24, cy: 38.4, r: 5 }],
        ['back', 'rect', { x: 21.2, y: 3.6, width: 5.6, height: 6.5, rx: 2.8 }],
        ['body', 'path', { d: 'M24 7 C16.4 7 12.4 12.8 12.4 20 C12.4 27.4 10.3 30.6 7.5 33.5 C6.3 34.8 7.2 36.8 8.9 36.8 H39.1 C40.8 36.8 41.7 34.8 40.5 33.5 C37.7 30.6 35.6 27.4 35.6 20 C35.6 12.8 31.6 7 24 7 Z' }],
      ],
    },

    // ---------- Locks and ways out ----------
    lock: {
      clay: 'coral', use: 'Locked',
      shapes: [
        ['arm', 'path', { d: 'M16 23 V16 a8 8 0 0 1 16 0 V23', 'stroke-width': 5.4 }],
        ['body', 'rect', { x: 8.5, y: 20, width: 31, height: 23.5, rx: 8 }],
        ['dot', 'circle', { cx: 24, cy: 29.6, r: 3.4 }],
        ['line', 'path', { d: 'M24 30.5 V36', 'stroke-width': 3.4 }],
      ],
    },
    unlock: {
      clay: 'teal', use: 'Unlocked',
      shapes: [
        ['arm', 'path', { d: 'M16 24 V13.6 a7.6 7.6 0 0 1 15.2 0 V14.6', 'stroke-width': 5.4 }],
        ['body', 'rect', { x: 8.5, y: 22, width: 31, height: 21.5, rx: 8 }],
        ['dot', 'circle', { cx: 24, cy: 30.6, r: 3.3 }],
        ['line', 'path', { d: 'M24 31.5 V36.4', 'stroke-width': 3.3 }],
      ],
    },
    key: {
      clay: 'honey', use: 'The guardian password',
      shapes: [
        ['arm', 'path', { d: 'M22.5 23.5 L39.4 40.4 M32.4 33.4 l4.4 -4.4 M37.6 38.6 l3.4 -3.4', 'stroke-width': 5.2 }],
        ['body', 'circle', { cx: 15.6, cy: 16.6, r: 11 }],
        ['dot', 'circle', { cx: 13.2, cy: 14.2, r: 3.7 }],
      ],
    },
    pin: {
      clay: 'honey', use: 'The guardian PIN',
      shapes: [
        ['body', 'rect', { x: 3.5, y: 10, width: 41, height: 28, rx: 10.5 }],
        ['dot', 'circle', { cx: 11.8, cy: 24, r: 3.1 }],
        ['dot', 'circle', { cx: 19.9, cy: 24, r: 3.1 }],
        ['dot', 'circle', { cx: 28.1, cy: 24, r: 3.1 }],
        ['dot', 'circle', { cx: 36.2, cy: 24, r: 3.1 }],
      ],
    },
    code: {
      clay: 'honey', use: 'A one time code',
      shapes: [
        ['body', 'path', { d: 'M10.5 9 H37.5 a6.5 6.5 0 0 1 6.5 6.5 V19.4 a4.6 4.6 0 0 0 0 9.2 V32.5 a6.5 6.5 0 0 1 -6.5 6.5 H10.5 A6.5 6.5 0 0 1 4 32.5 V28.6 a4.6 4.6 0 0 0 0 -9.2 V15.5 A6.5 6.5 0 0 1 10.5 9 Z' }],
        ['line', 'path', { d: 'M21 19.4 L24.8 16.8 V31 M20.6 31 H29', 'stroke-width': 3.3 }],
      ],
    },
    shield: {
      clay: 'teal', use: 'Strict mode',
      shapes: [
        ['body', 'path', { d: 'M24 4.3 C28.6 7.8 33.8 9.4 39.3 9.5 a2 2 0 0 1 2 2 V22 C41.3 32.6 34.2 40 24 43.7 C13.8 40 6.7 32.6 6.7 22 V11.5 a2 2 0 0 1 2 -2 C14.2 9.4 19.4 7.8 24 4.3 Z' }],
        ['line', 'path', { d: 'M16.6 23.6 l5.2 5.2 l9.6 -10.6', 'stroke-width': 4 }],
      ],
    },
    hand: {
      clay: 'honey', use: 'A child asks for more time',
      shapes: [
        ['body', 'path', {
          d: rr(16.4, 8.4, 5.6, 22, 2.8) + ' ' + rr(22.7, 5, 5.6, 25, 2.8) + ' ' + rr(29, 6.8, 5.6, 24, 2.8) + ' ' +
            rr(35.3, 11.6, 5.6, 20, 2.8) + ' ' + rr(16.4, 22.5, 24.5, 18.4, 8) + ' ' + capsule(18.4, 33.6, 10.4, 24.4, 3.2),
        }],
        ['top', 'rect', { x: 18, y: 36.6, width: 21.4, height: 7, rx: 3.3 }],
      ],
    },

    // ---------- Answers and states ----------
    check: {
      clay: 'teal', use: 'Yes, allowed, done',
      shapes: [DISC, ['line', 'path', { d: 'M15 24.6 l6 6 l12 -13', 'stroke-width': 4.4 }]],
    },
    cross: {
      clay: 'coral', use: 'No, close',
      shapes: [DISC, ['line', 'path', { d: 'M17.2 17.2 L30.8 30.8 M30.8 17.2 L17.2 30.8', 'stroke-width': 4.4 }]],
    },
    block: {
      clay: 'coral', use: 'A blocked app',
      shapes: [DISC, ['line', 'path', { d: 'M14.4 24 H33.6', 'stroke-width': 5.4 }]],
    },
    star: {
      clay: 'teal', use: 'An app that is always allowed',
      shapes: [['body', 'path', { d: star(24, 25.4, 21.5, 10.4) }]],
    },
    add: {
      clay: 'ink', use: 'Add a child or a device',
      shapes: [DISC, ['line', 'path', { d: 'M24 15 V33 M15 24 H33', 'stroke-width': 4.4 }]],
    },
    info: {
      clay: 'ink', use: 'More about this',
      shapes: [
        DISC,
        ['dot', 'circle', { cx: 24, cy: 15.4, r: 2.9 }],
        ['line', 'path', { d: 'M24 22.6 V33.4', 'stroke-width': 4.4 }],
      ],
    },
    help: {
      clay: 'ink', use: 'Help',
      shapes: [
        DISC,
        ['line', 'path', { d: 'M18.4 19 a5.7 5.7 0 1 1 8.8 4.8 C25.3 25 24 26.2 24 28.4', 'stroke-width': 4 }],
        ['dot', 'circle', { cx: 24, cy: 34, r: 2.7 }],
      ],
    },
    alert: {
      clay: 'coral', use: 'Something a guardian should look at',
      shapes: [
        ['body', 'path', { d: roundPoly([[24, 2.6], [45.4, 42], [2.6, 42]], 6.6) }],
        ['line', 'path', { d: 'M24 17.8 V28', 'stroke-width': 4.4 }],
        ['dot', 'circle', { cx: 24, cy: 34.6, r: 2.8 }],
      ],
    },

    // ---------- Away, links and set up ----------
    away: {
      clay: 'ink', use: 'A phone that is away from home',
      shapes: [
        ['body', 'rect', { x: 6, y: 21, width: 20, height: 20.5, rx: 5 }],
        ['top', 'path', { d: roundPoly([[16, 7.6], [29.4, 22.8], [2.6, 22.8]], 3.4) }],
        ['dot', 'path', { d: 'M13 41.5 V33.6 a3 3 0 0 1 6 0 V41.5 Z' }],
        ['arm', 'path', { d: 'M31.6 31.6 H41.6 M36.8 26.4 L42 31.6 L36.8 36.8', 'stroke-width': 4.6 }],
      ],
    },
    wifi: {
      clay: 'teal', use: 'In reach on the home Wi-Fi',
      shapes: [
        ['arm', 'path', { d: 'M6.6 19.6 A24.6 24.6 0 0 1 41.4 19.6', 'stroke-width': 4.8 }],
        ['arm', 'path', { d: 'M12.6 26.2 A16.1 16.1 0 0 1 35.4 26.2', 'stroke-width': 4.8 }],
        ['arm', 'path', { d: 'M18.6 32.6 A7.6 7.6 0 0 1 29.4 32.6', 'stroke-width': 4.8 }],
        ['body', 'circle', { cx: 24, cy: 39, r: 3.9 }],
      ],
    },
    usb: {
      clay: 'ink', use: 'Set up over a USB cable',
      shapes: [
        ['arm', 'path', { d: 'M24 35 V40.6', 'stroke-width': 6 }],
        ['top', 'rect', { x: 16.2, y: 4.5, width: 15.6, height: 15, rx: 2.8 }],
        ['tdot', 'rect', { x: 19.6, y: 9, width: 3.2, height: 4, rx: 1 }],
        ['tdot', 'rect', { x: 25.2, y: 9, width: 3.2, height: 4, rx: 1 }],
        ['body', 'rect', { x: 12, y: 16.5, width: 24, height: 20.5, rx: 5.5 }],
      ],
    },
    battery: {
      clay: 'ink', use: 'The battery, and Android putting apps to sleep',
      shapes: [
        ['back', 'rect', { x: 19, y: 4.5, width: 10, height: 6.5, rx: 2.4 }],
        ['body', 'rect', { x: 11.5, y: 8.5, width: 25, height: 35, rx: 6.5 }],
        ['screen', 'rect', { x: 15.5, y: 25.5, width: 17, height: 13.5, rx: 3 }],
      ],
    },
    qr: {
      clay: 'ink', use: 'Pair by scanning a code',
      shapes: [
        ['body', 'rect', { x: 4.5, y: 4.5, width: 39, height: 39, rx: 8.5 }],
        ['dot', 'rect', { x: 10.5, y: 10.5, width: 10, height: 10, rx: 2.8 }],
        ['dot', 'rect', { x: 27.5, y: 10.5, width: 10, height: 10, rx: 2.8 }],
        ['dot', 'rect', { x: 10.5, y: 27.5, width: 10, height: 10, rx: 2.8 }],
        ['dot', 'rect', { x: 27.5, y: 27.5, width: 4.4, height: 4.4, rx: 1.4 }],
        ['dot', 'rect', { x: 33.1, y: 33.1, width: 4.4, height: 4.4, rx: 1.4 }],
      ],
    },
    link: {
      clay: 'teal', use: 'A paired device',
      shapes: [
        ['arm', 'rect', { x: 4.5, y: 17, width: 22.5, height: 14, rx: 7, transform: 'rotate(-45 24 24)', 'stroke-width': 4.8 }],
        ['arm', 'rect', { x: 21, y: 17, width: 22.5, height: 14, rx: 7, transform: 'rotate(-45 24 24)', 'stroke-width': 4.8 }],
      ],
    },
    install: {
      clay: 'teal', use: 'Install on a phone',
      shapes: [
        ['body', 'path', { d: 'M5.5 28.5 a3.2 3.2 0 0 1 6.4 0 V34.6 a2.5 2.5 0 0 0 2.5 2.5 H33.6 a2.5 2.5 0 0 0 2.5 -2.5 V28.5 a3.2 3.2 0 0 1 6.4 0 V37 a6.5 6.5 0 0 1 -6.5 6.5 H12 A6.5 6.5 0 0 1 5.5 37 Z' }],
        ['top', 'path', { d: roundPoly([[20.4, 4.5], [27.6, 4.5], [27.6, 17.6], [34.6, 17.6], [24, 30.6], [13.4, 17.6], [20.4, 17.6]], 2.4) }],
      ],
    },
    sync: {
      clay: 'teal', use: 'Up to date',
      shapes: [
        ['arm', 'path', { d: 'M8.78 18.46 A16.2 16.2 0 0 1 38.03 15.9', 'stroke-width': 4.8 }],
        ['arm', 'path', { d: 'M39.22 29.54 A16.2 16.2 0 0 1 9.97 32.1', 'stroke-width': 4.8 }],
        ['top', 'path', { d: roundPoly([[41.03, 21.1], [42.71, 13.2], [33.35, 18.6]], 1.6) }],
        ['top', 'path', { d: roundPoly([[6.97, 26.9], [5.29, 34.8], [14.65, 29.4]], 1.6) }],
      ],
    },

    // ---------- Reading and settings ----------
    chart: {
      clay: 'ink', use: 'Usage over the week',
      shapes: [
        ['body', 'rect', { x: 5.5, y: 25, width: 10.6, height: 18.5, rx: 3.8 }],
        ['body', 'rect', { x: 18.7, y: 15.5, width: 10.6, height: 28, rx: 3.8 }],
        ['top', 'rect', { x: 31.9, y: 5, width: 10.6, height: 38.5, rx: 3.8 }],
      ],
    },
    list: {
      clay: 'ink', use: 'What happened, in order',
      shapes: [
        ['body', 'rect', { x: 7.5, y: 4.5, width: 33, height: 39, rx: 7.5 }],
        ['dot', 'circle', { cx: 15.2, cy: 14.6, r: 2.3 }],
        ['dot', 'circle', { cx: 15.2, cy: 24, r: 2.3 }],
        ['dot', 'circle', { cx: 15.2, cy: 33.4, r: 2.3 }],
        ['line', 'path', { d: 'M21.2 14.6 H32.8 M21.2 24 H32.8 M21.2 33.4 H28.6', 'stroke-width': 3.1 }],
      ],
    },
    gear: {
      clay: 'ink', use: 'Settings',
      shapes: [
        ['body', 'path', { d: gear(24, 24, 20.4, 15.4, 8) }],
        ['dot', 'circle', { cx: 24, cy: 24, r: 6 }],
      ],
    },
    edit: {
      clay: 'ink', use: 'Change a name or a rule',
      shapes: [
        ['top', 'rect', { x: 18.6, y: 2.4, width: 10.8, height: 9, rx: 3, transform: 'rotate(45 24 24)' }],
        ['body', 'rect', { x: 18.6, y: 8.6, width: 10.8, height: 25.4, rx: 1.6, transform: 'rotate(45 24 24)' }],
        ['top', 'path', { d: 'M18.6 33.4 H29.4 L25.3 43.2 a1.4 1.4 0 0 1 -2.6 0 Z', transform: 'rotate(45 24 24)' }],
      ],
    },
    trash: {
      clay: 'coral', use: 'Delete for good',
      shapes: [
        ['arm', 'path', { d: 'M19.4 11 V8.6 a2.6 2.6 0 0 1 2.6 -2.6 H26 a2.6 2.6 0 0 1 2.6 2.6 V11', 'stroke-width': 3.6 }],
        ['body', 'path', { d: 'M10.6 15 H37.4 L35.4 39.2 a4.7 4.7 0 0 1 -4.7 4.3 H17.3 a4.7 4.7 0 0 1 -4.7 -4.3 Z' }],
        ['top', 'rect', { x: 6.5, y: 10, width: 35, height: 7.4, rx: 3.7 }],
        ['line', 'path', { d: 'M19.8 23.6 V36.4 M28.2 23.6 V36.4', 'stroke-width': 3.1 }],
      ],
    },
    eyeoff: {
      clay: 'ink', use: 'What LiMo never looks at',
      shapes: [
        ['body', 'path', { d: 'M3.6 24 C8.6 15.6 15.6 11.4 24 11.4 C32.4 11.4 39.4 15.6 44.4 24 C39.4 32.4 32.4 36.6 24 36.6 C15.6 36.6 8.6 32.4 3.6 24 Z' }],
        ['top', 'circle', { cx: 24, cy: 24, r: 7.8 }],
        ['gap', 'path', { d: 'M11.8 38.2 L36.2 9.8', 'stroke-width': 7.8 }],
        ['ring', 'path', { d: 'M11.8 38.2 L36.2 9.8', 'stroke-width': 3.8 }],
      ],
    },
  };
});
