// How a LiMo icon is painted.
//
// A drawing (shapes.js) says what a thing is, as a short list of simple
// shapes with a ROLE each. This file says how each role is painted, from ONE
// colour of the Paper and Ink palette. So every icon of one colour matches,
// and a new colour or a new look needs no new drawing.
//
// The look is "light clay": soft rounded forms in pale tones, lit from the
// top left, with a soft shadow under them and no outlines. Each icon has a
// lighter clay (its body) and a darker clay (its smaller parts). Marks on
// the lighter clay are dark, marks on the darker clay are pale.
//
// No SVG filters are used (they are not dependable on phones): the shadow is
// two faint copies of the shape set a little lower.
//
// This file runs in a browser (sheet.html) and in Node (export.js).

(function (root, factory) {
  if (typeof module === 'object' && module.exports) module.exports = factory();
  else root.LIMO_CLAY = factory();
})(typeof self !== 'undefined' ? self : this, function () {
  // The Paper and Ink palette. An icon takes exactly one of these.
  var PALETTE = {
    honey: '#e89c1c', // time, and the things that are LiMo's own
    teal: '#0e7c66', // allowed, at home, connected, done
    coral: '#cf4326', // locked, blocked, stop, danger
    ink: '#1b2638', // plain things: devices, people, settings
  };
  var CARD = '#fffdf6';
  var INK = '#1b2638';

  function clamp(n) { return Math.max(0, Math.min(255, Math.round(n))); }
  function parse(hex) {
    var h = String(hex).replace('#', '');
    return [0, 2, 4].map(function (i) { return parseInt(h.slice(i, i + 2), 16) || 0; });
  }
  function toHex(rgb) {
    return '#' + rgb.map(function (v) { return clamp(v).toString(16).padStart(2, '0'); }).join('');
  }
  function mix(hex, target, amount) {
    var a = parse(hex), b = parse(target);
    return toHex(a.map(function (v, i) { return v + (b[i] - v) * amount; }));
  }

  // The looks. "light" is LiMo's. The others exist only so the review sheet
  // can show the same drawings another way.
  var LOOKS = {
    light: function (c, onDark) {
      // A plain thing is paper and ink themselves: a cream body, ink parts.
      // A wash of ink would only be grey.
      if (c === INK) {
        return {
          bodyA: '#fffdf6', bodyB: '#e2d7bc',
          // On an ink surface the ink parts would vanish, so they go paler.
          topA: onDark ? '#c3cbdd' : '#6f7c9a', topB: onDark ? '#7c88a6' : '#263250',
          panel: '#fffdf6', panelOpacity: 1,
          onBody: '#27324e', onTop: onDark ? '#1b2638' : '#f6f1e4', free: onDark ? '#efe7d2' : '#27324e',
          shadow: onDark ? '#0b101c' : '#5f5538', shadowA: onDark ? 0.3 : 0.15, shadowB: onDark ? 0.36 : 0.2,
          shine: 0.85, topShine: 0.38,
        };
      }
      // Everything else is a pale wash of its colour with stronger parts.
      return {
        bodyA: mix(c, CARD, 0.84), bodyB: mix(c, CARD, 0.48),
        topA: mix(c, CARD, 0.42), topB: mix(c, CARD, 0.06),
        panel: CARD, panelOpacity: 0.96,
        onBody: mix(c, INK, 0.45), onTop: CARD, free: mix(c, INK, 0.45),
        shadow: mix(c, INK, 0.5), shadowA: 0.13, shadowB: 0.17, shine: 0.7, topShine: 0.5,
      };
    },
    // Full colour clay with white marks, the look of the TinyTales menus.
    full: function (c) {
      return {
        bodyA: mix(c, '#ffffff', 0.5), bodyB: c,
        topA: c, topB: mix(c, '#000000', 0.36),
        panel: '#ffffff', panelOpacity: 0.9,
        onBody: '#ffffff', onTop: '#ffffff', free: mix(c, '#000000', 0.36),
        shadow: mix(c, '#000000', 0.36), shadowA: 0.12, shadowB: 0.16, shine: 0.62, topShine: 0.5,
      };
    },
    // Ink clay with paper marks: how an icon that is switched off looks in
    // TutorBee. Shown for comparison only.
    dark: function () {
      return {
        bodyA: '#5b6885', bodyB: '#27324e',
        topA: '#3b4766', topB: '#121a2c',
        panel: '#efe7d2', panelOpacity: 0.95,
        onBody: '#efe7d2', onTop: '#efe7d2', free: '#27324e',
        shadow: '#121a2c', shadowA: 0.14, shadowB: 0.2, shine: 0.42, topShine: 0.3,
      };
    },
  };

  function attrs(a) {
    return Object.keys(a).map(function (k) { return k + '="' + a[k] + '"'; }).join(' ');
  }
  function el(tag, a, extra) {
    return '<' + tag + ' ' + attrs(a) + (extra ? ' ' + extra : '') + '/>';
  }

  var seq = 0;

  // One icon as an SVG string.
  //   shapes  the drawing: a list of [role, element, attributes]
  //   opts    { size, clay: a PALETTE name or a hex colour, look, title,
  //             on: 'dark' when the icon sits on an ink surface,
  //             id: a name for its gradients, unique on the page }
  function svg(shapes, opts) {
    opts = opts || {};
    var size = opts.size || 48;
    var colour = PALETTE[opts.clay] || opts.clay || PALETTE.ink;
    var t = (LOOKS[opts.look] || LOOKS.light)(colour, opts.on === 'dark');
    // Ids must differ between icons on one page, or the last one's colours
    // would paint them all.
    var id = opts.id || 'lc' + (seq += 1);
    var round = 'stroke-linecap="round" stroke-linejoin="round"';
    var light = 'url(#' + id + 'a)', dark = 'url(#' + id + 'b)';

    // A form: its shadow (two faint copies set lower), its clay, its light.
    function form(tag, a, fill, shine) {
      return (
        '<g transform="translate(0 2.8)" opacity="' + t.shadowA + '">' + el(tag, a, 'fill="' + t.shadow + '"') + '</g>' +
        '<g transform="translate(0 1.4)" opacity="' + t.shadowB + '">' + el(tag, a, 'fill="' + t.shadow + '"') + '</g>' +
        el(tag, a, 'fill="' + fill + '"') +
        el(tag, a, 'fill="url(#' + id + shine + ')"')
      );
    }
    function stroke(tag, a, paint, width) {
      return el(tag, a, 'fill="none" stroke="' + paint + '" stroke-width="' + width + '" ' + round);
    }

    var out = shapes.map(function (shape) {
      var role = shape[0], tag = shape[1], a = shape[2];
      var w = a['stroke-width'];
      var plain = {};
      Object.keys(a).forEach(function (k) { if (k !== 'stroke-width') plain[k] = a[k]; });
      switch (role) {
        case 'body': return form(tag, plain, light, 's');
        case 'back':
        case 'top': return form(tag, plain, dark, 't');
        case 'arm': return (
          '<g transform="translate(0 2.4)" opacity="' + t.shadowA + '">' + stroke(tag, plain, t.shadow, w || 5) + '</g>' +
          '<g transform="translate(0 1.2)" opacity="' + t.shadowB + '">' + stroke(tag, plain, t.shadow, w || 5) + '</g>' +
          stroke(tag, plain, dark, w || 5)
        );
        // A display: set into the body, so it has a gloss and no shadow.
        case 'screen': return el(tag, plain, 'fill="' + dark + '"') + el(tag, plain, 'fill="url(#' + id + 'g)"');
        case 'panel': return el(tag, plain, 'fill="' + t.panel + '" opacity="' + t.panelOpacity + '"');
        case 'dot': return el(tag, plain, 'fill="' + t.onBody + '"');
        case 'line': return stroke(tag, plain, t.onBody, w || 3.4);
        case 'tdot': return el(tag, plain, 'fill="' + t.onTop + '"');
        case 'tline': return stroke(tag, plain, t.onTop, w || 3.2);
        case 'mark': return el(tag, plain, 'fill="' + t.free + '"');
        case 'ring': return stroke(tag, plain, t.free, w || 3.2);
        case 'gap': return stroke(tag, plain, t.bodyA, w || 8);
        default: return '';
      }
    }).join('');

    return (
      '<svg xmlns="http://www.w3.org/2000/svg" width="' + size + '" height="' + size + '" viewBox="0 0 48 48"' +
      (opts.title ? ' role="img" aria-label="' + opts.title + '"' : ' aria-hidden="true"') + '>' +
      '<defs>' +
      // The lighter clay: each form is lit by itself, from its own top left.
      '<linearGradient id="' + id + 'a" x1="0" y1="0" x2="1" y2="1">' +
        '<stop offset="0" stop-color="' + t.bodyA + '"/><stop offset="1" stop-color="' + t.bodyB + '"/></linearGradient>' +
      // The darker clay: one gradient across the whole icon, so its separate
      // parts shade as one.
      '<linearGradient id="' + id + 'b" gradientUnits="userSpaceOnUse" x1="6" y1="6" x2="42" y2="42">' +
        '<stop offset="0" stop-color="' + t.topA + '"/><stop offset="1" stop-color="' + t.topB + '"/></linearGradient>' +
      '<radialGradient id="' + id + 's" cx="0.3" cy="0.22" r="0.62">' +
        '<stop offset="0" stop-color="#ffffff" stop-opacity="' + t.shine + '"/>' +
        '<stop offset="1" stop-color="#ffffff" stop-opacity="0"/></radialGradient>' +
      '<radialGradient id="' + id + 't" cx="0.3" cy="0.22" r="0.62">' +
        '<stop offset="0" stop-color="#ffffff" stop-opacity="' + t.topShine + '"/>' +
        '<stop offset="1" stop-color="#ffffff" stop-opacity="0"/></radialGradient>' +
      // The gloss of a display: light falling across its top left corner.
      '<linearGradient id="' + id + 'g" x1="0" y1="0" x2="1" y2="1">' +
        '<stop offset="0" stop-color="#ffffff" stop-opacity="0.34"/>' +
        '<stop offset="0.45" stop-color="#ffffff" stop-opacity="0.06"/>' +
        '<stop offset="1" stop-color="#ffffff" stop-opacity="0"/></linearGradient>' +
      '</defs>' + out + '</svg>'
    );
  }

  return { PALETTE: PALETTE, LOOKS: Object.keys(LOOKS), svg: svg, mix: mix };
});
