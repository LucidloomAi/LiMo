/* LiMo on the phone: what the lock, the ways out, the block screen, the
   child's Today screen and a warning show, as functions that return HTML.
   Drawings to agree the look, not the product.

   The words are the PC lock screen's own (pc.js), with "PC" changed to
   "phone" and "programs" to "apps". Icons are the light clay set in
   ../icons/svg.

   A screen is a page that scrolls, and may have one thing pinned to the
   bottom, above the gesture bar or the keyboard: Emergency call on a lock
   or block screen, the main button on a way out. */
(function (root) {
  'use strict';

  function icon(name, size) {
    return '<img src="../icons/svg/' + name + '.svg" width="' + size + '" height="' + size + '" alt="">';
  }

  // Small marks beside text are plain ink strokes, never icons.
  function stroke(w, h, d) {
    return '<svg class="mk" width="' + w + '" height="' + h + '" viewBox="0 0 ' + w + ' ' + h +
      '" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">' + d + '</svg>';
  }
  var MARK = {
    right: stroke(16, 16, '<path d="M6 3l5 5-5 5"/>'),
    left: stroke(16, 16, '<path d="M10 3L5 8l5 5"/>'),
    del: stroke(24, 16, '<path d="M9 2h12.5v12H9L2.5 8z"/><path d="M12.5 5.5l5 5M17.5 5.5l-5 5"/>')
  };

  var CLOCK = '9:42';
  var NOW = '9:42 pm';
  var DATE = 'Wed 18 Nov';
  var CHILD = 'Mira';

  // The reasons drawn here, in the engine's names, with the PC's words.
  var WHY = {
    time_used_up: {
      icon: 'hourglass', title: 'Today’s time is used up.',
      detail: 'You have used all of today’s screen time.',
      until: 'Tomorrow, 7:00 am', ways: ['ask', 'code', 'pin']
    },
    blocked_hours: {
      icon: 'moon', title: 'Screen time is off for now.',
      detail: 'This is a time when the phone is not used.',
      until: 'Tomorrow, 7:00 am', ways: ['pin']
    }
  };

  var WAYS = {
    ask: ['hand', 'Ask for more time', 'A guardian sees your request in LiMo'],
    code: ['code', 'Type a one time code', 'A guardian who is away can read you 8 digits'],
    pin: ['pin', 'Guardian PIN', 'For a guardian who is with you']
  };
  var ASK_MINUTES = [[15, '15 min'], [30, '30 min'], [45, '45 min'], [60, '1 hour']];
  var PIN_ADD = [['add15', '15 min'], ['add30', '30 min'], ['add60', '1 hour']];
  var PIN_OPEN = [['open60', '1 hour'], ['open120', '2 hours'], ['today', 'Today']];
  var STILL_OPEN = ['Phone', 'Messages', 'Clock', 'Maps'];

  function fresh(over) {
    var s = {
      why: 'time_used_up', panel: 'ways',
      request: null,     // { minutes, at, answer: 'waiting' | 'refused' | 'granted', given }
      waitUntil: null,   // '9:47 pm' when wrong tries have set a wait
      askMinutes: 30, reason: '', typed: '', pinAct: 'add30', error: ''
    };
    for (var k in over || {}) { s[k] = over[k]; }
    return s;
  }

  // ---------- The phone ----------

  // A time never breaks between its digits and am or pm.
  function nb(html) {
    return html.replace(/(\d) (am|pm)\b/g, '$1 $2');
  }

  /** A phone 390 by 844 (or 360 by 640 with small), with Android's own bars
      kept clear. `screen` is the page's HTML, or { page, pin } when something
      is pinned to the bottom. With keyboard, the phone's own keyboard is up
      and the pinned thing sits on it. */
  function frame(screen, opt) {
    opt = opt || {};
    var page = typeof screen === 'string' ? screen : screen.page;
    var pin = typeof screen === 'string' ? '' : screen.pin || '';
    var two = typeof screen !== 'string' && screen.pins === 2;
    var cls = 'ph-frame' + (opt.small ? ' small' : '') + (opt.keyboard ? ' kb' : '') + (pin ? ' pinned' : '') + (two ? ' pin2' : '') + (opt.cls ? ' ' + opt.cls : '');
    return nb('<div class="' + cls + '">' +
      '<div class="ph-status">' + CLOCK + '</div>' +
      '<div class="ph-page"' + (opt.scrollTo ? ' data-scroll-to="' + opt.scrollTo + '"' : '') + '>' + page + '</div>' +
      (pin ? '<div class="ph-pin">' + pin + '</div>' : '') +
      (opt.keyboard ? '<div class="ph-keyboard">The phone’s keyboard</div>' : '') +
      (opt.over || '') +
      '<div class="ph-gesture"><i></i></div></div>');
  }

  /** Only the card, as wide as the phone. */
  function crop(inner) {
    return nb('<div class="ph-crop">' + inner + '</div>');
  }

  // ---------- The parts of a lock ----------

  function top(right) {
    return '<div class="ph-top"><div class="ph-mark">' + icon('hourglass', 24) + '<span>LiMo</span></div>' +
      (right === undefined ? '<div class="ph-now"><b>' + NOW + '</b><span>' + DATE + '</span></div>' : right) + '</div>';
  }

  function label(text, hint, cls) {
    return '<div class="ph-label' + (cls ? ' ' + cls : '') + '">' + text + (hint ? '<em>' + hint + '</em>' : '') + '</div>';
  }

  function reason(w) {
    return '<div class="ph-card">' +
      '<div class="ph-why-top">' + icon(w.icon, 64) + '<span class="ph-stamp">Locked</span></div>' +
      label(CHILD + ' · this phone', '', 'ph-eyebrow') +
      '<h1 class="ph-h1">' + w.title + '</h1>' +
      '<p class="ph-detail">' + w.detail + '</p>' +
      '<div class="ph-until">' + label('Opens again') + '<b>' + w.until + '</b></div></div>';
  }

  function requestLine(r) {
    if (!r) { return ''; }
    var st = r.answer === 'granted' ? '<span class="ph-st teal">Given ' + r.given + ' min</span>'
      : r.answer === 'refused' ? '<span class="ph-st coral">Not this time</span>'
        : '<span class="ph-st">Waiting</span>';
    return '<div class="ph-req">' + icon('hand', 24) + '<span>You asked for ' + r.minutes + ' min at ' + r.at + '</span>' + st + '</div>';
  }

  /** What you can do: a row for each way out the rules allow. */
  function ways(s, w) {
    var waiting = s.request && s.request.answer === 'waiting';
    var rows = w.ways.filter(function (k) { return !(k === 'ask' && waiting); }).map(function (k) {
      var off = s.waitUntil && k !== 'ask';
      return '<div class="ph-row' + (off ? ' off' : '') + '">' +
        '<span class="ph-tile">' + icon(WAYS[k][0], 32) + '</span>' +
        '<span><b>' + WAYS[k][1] + '</b><i>' + (off ? 'Wait until ' + s.waitUntil : WAYS[k][2]) + '</i></span>' +
        (off ? '' : MARK.right) + '</div>';
    }).join('');
    return label('What you can do', '', 'ph-sec') + requestLine(s.request) +
      (s.waitUntil ? '<div class="ph-wait">' + icon('alert', 24) + '<span>Too many wrong tries. The next try is at ' + s.waitUntil + '.</span></div>' : '') +
      '<div class="ph-rows">' + rows + '</div>';
  }

  /** The apps that are always allowed, drawn as plain letter tiles. */
  function stillOpen() {
    return label('Still open', '', 'ph-sec') + '<div class="ph-apps">' + STILL_OPEN.map(function (a) {
      return '<div class="ph-app"><span class="ph-letter">' + a.charAt(0) + '</span><span>' + a + '</span></div>';
    }).join('') + '</div>';
  }

  // The maker's mark: said once, small, at the end of a screen that fills
  // the display. Not a link: nothing on a lock opens anything.
  var MAKER = '<div class="ph-maker" aria-label="Made by LucidLoom AI"><span>Made by</span>' +
    '<img src="../../docs/images/lucidloom-logo.png" alt="" width="22" height="22"><b>LucidLoom AI</b></div>';

  // A lock never stands between a child and an emergency call.
  var EMERGENCY = '<div class="ph-btn">Emergency call</div>';
  // A call is never a way around the limits, and never trapped behind a
  // lock: while a call is on, the lock carries a way back to it, in teal,
  // the colour for allowed and connected.
  var BACK_TO_CALL = '<div class="ph-btn teal">Back to the call</div>';
  var NOTHING_CLOSED = 'Nothing was closed. Your apps are still open behind this screen.';

  /** The end of the scrolling part: the plain line, then the maker. */
  function tail(line) {
    return '<div class="ph-tail"><div class="ph-foot">' + line + '</div>' + MAKER + '</div>';
  }

  // ---------- A. and C. The lock ----------

  /** Emergency call is pinned to the bottom; the rest scrolls above it.
      During a call (s.call), Back to the call is pinned directly above it. */
  function lock(s) {
    var w = WHY[s.why] || WHY.time_used_up;
    return {
      page: '<div class="ph-lock">' + top() + reason(w) + ways(s, w) + stillOpen() + tail(NOTHING_CLOSED) + '</div>',
      pin: (s.call ? BACK_TO_CALL : '') + EMERGENCY,
      pins: s.call ? 2 : 1
    };
  }

  // ---------- B. A way out, opened ----------

  function strip(w) {
    return '<div class="ph-strip">' + icon(w.icon, 24) + '<div><b>' + w.title + '</b>' +
      '<span>Opens ' + w.until.charAt(0).toLowerCase() + w.until.slice(1) + '</span></div></div>';
  }
  function head(title) {
    return '<div class="ph-back">' + MARK.left + 'Back</div><h2 class="ph-h2">' + title + '</h2>';
  }
  function chips(list, picked) {
    return '<div class="ph-chips">' + list.map(function (c) {
      return '<span class="ph-chip' + (c[0] === picked ? ' on' : '') + '">' + c[1] + '</span>';
    }).join('') + '</div>';
  }
  function pad() {
    var keys = ['1', '2', '3', '4', '5', '6', '7', '8', '9'].map(function (d) { return '<span class="ph-key">' + d + '</span>'; });
    keys.push('<span class="ph-key none"></span>');
    keys.push('<span class="ph-key">0</span>');
    keys.push('<span class="ph-key" aria-label="Remove the last digit">' + MARK.del + '</span>');
    return '<div class="ph-pad">' + keys.join('') + '</div>';
  }

  // Each way out: what its card holds, and its main button, which is pinned.
  var PANEL = {
    ask: function (s) {
      return [head('Ask for more time') +
        label('How long', '', 'gap') + chips(ASK_MINUTES, s.askMinutes) +
        label('Why', 'you can leave this empty', 'gap') +
        '<div class="ph-text">' + s.reason + '<span class="ph-caret"></span></div>' +
        '<div class="ph-count">' + s.reason.length + ' / 200</div>' +
        '<p class="ph-small">A guardian can say yes, give a different amount, or say no.</p>',
        'Send request'];
    },
    code: function (s) {
      var cells = '';
      for (var i = 0; i < 8; i += 1) {
        cells += '<span class="ph-cell' + (i === 4 ? ' gap' : '') + (i === s.typed.length ? ' at' : '') + '">' + (s.typed.charAt(i) || '') + '</span>';
      }
      return [head('Type a one time code') +
        '<p class="ph-note">A guardian who is away from home can read you a code. It has 8 digits.</p>' +
        '<div class="ph-cells">' + cells + '</div><div class="ph-err">' + s.error + '</div>' +
        pad() +
        '<p class="ph-small">A code works once, only today, and only on this phone.</p>',
        'Use this code'];
    },
    // In a phone's order: the guardian chooses what to give, then types.
    pin: function (s) {
      var dots = '';
      for (var i = 0; i < s.typed.length; i += 1) { dots += '<span class="dot"></span>'; }
      return [head('Guardian PIN') +
        '<p class="ph-note">For a guardian. Type your PIN and choose what to give.</p>' +
        label('Add time', '', 'gap') + chips(PIN_ADD, s.pinAct) +
        label('Or keep it open for', '', 'gap') + chips(PIN_OPEN, s.pinAct) +
        '<div class="ph-pinbox">' + (dots || '<span class="ph">6 to 8 digits</span>') + '</div><div class="ph-err">' + s.error + '</div>' +
        pad(),
        'Unlock'];
    }
  };

  /** The card of a way out: the reason as a strip, then the way out. */
  function wayCard(s) {
    var w = WHY[s.why] || WHY.time_used_up;
    return '<div class="ph-card">' + strip(w) + PANEL[s.panel](s)[0] + '</div>';
  }

  /** A way out takes the screen. The reason strip opens it, so there is no
      LiMo and clock row. Its main button is pinned; Emergency call and the
      end are at the end of the scrolling part. */
  function way(s) {
    return {
      page: '<div class="ph-lock">' + wayCard(s) +
        '<div class="ph-end">' + EMERGENCY + '</div>' + tail(NOTHING_CLOSED) + '</div>',
      pin: '<div class="ph-btn go">' + PANEL[s.panel](s)[1] + '</div>'
    };
  }

  /** The ways out with a request or a wait: what you can do, as it sits under the reason. */
  function waysOnly(s) {
    return ways(s, WHY[s.why] || WHY.time_used_up);
  }

  // ---------- D. The block screen ----------

  function block(app) {
    return {
      page: '<div class="ph-lock">' + top() +
        '<div class="ph-mid"><div class="ph-card">' +
        '<div class="ph-why-top">' + icon('block', 64) + '<span class="ph-stamp">Blocked</span></div>' +
        label(CHILD + ' · this phone', '', 'ph-eyebrow') +
        '<h1 class="ph-h1">' + app + ' is not allowed.</h1>' +
        '<p class="ph-detail">It is blocked by your family’s rules.</p>' +
        '<div class="ph-btn ink">Go to the home screen</div></div></div>' +
        tail('Nothing else was closed.') + '</div>',
      pin: EMERGENCY
    };
  }

  // ---------- E. The child's own screen in the app: Today ----------

  // Looking at LiMo's own screen never uses time, so nothing is counted
  // while a child reads this screen, and it says so.
  var USED = [['Maps', 12], ['Chrome', 9], ['Gallery', 6], ['Clock', 4]];

  function today() {
    return '<div class="ph-lock ph-today">' + top('<span class="ph-who">' + CHILD + '</span>') +
      '<div class="ph-card">' +
        '<b class="ph-big">1 h 12 min</b>' + label('Left today', '', 'ph-big-label') +
        '<p class="ph-quiet">Looking at this screen does not use your time.</p>' +
        '<div class="ph-used"><div class="ph-bar"><i style="width:40%"></i></div>' +
        '<p>48 min used of 2 h</p>' + label('On this phone 31 min · on the PC 17 min') + '</div>' +
      '</div>' +
      '<div class="ph-card ph-hours"><span class="ph-tile lg">' + icon('moon', 32) + '</span>' +
        '<p>Screens are off from 9:00 pm to 7:00 am.</p></div>' +
      '<div class="ph-btn">Ask for more time</div>' +
      label('Today on this phone', '', 'ph-sec') +
      '<div class="ph-card ph-list">' + USED.map(function (u) {
        return '<div class="ph-li"><span class="ph-letter sm">' + u[0].charAt(0) + '</span><span>' + u[0] + '</span><span class="min">' + u[1] + ' min</span></div>';
      }).join('') + '</div>' +
      '</div>';
  }

  var TABS = '<nav class="ph-tabs">' +
    '<span class="ph-tab on">' + icon('sun', 24) + 'Today</span>' +
    '<span class="ph-tab">' + icon('help', 24) + 'How it works</span></nav>';

  // ---------- F. A warning over the child's own app ----------

  function warning() {
    return '<div class="ph-app-behind"><span>The child’s own app</span></div>' +
      '<div class="ph-nt"><div class="ph-nt-top"><span class="ph-tile lg">' + icon('bell', 32) + '</span>' +
      '<div><h3>5 minutes of screen time left today.</h3><p>Save what you are doing.</p></div></div>' +
      '<div class="ph-nt-foot">' + label('Locks about 9:47 pm') + '<div class="ph-btn">OK</div></div></div>';
  }

  // ---------- G. Android's own notification: only the words ----------

  function shade() {
    function ntf(title, text) {
      return '<div class="ph-ntf"><span class="ic"></span><div><small>LiMo</small><b>' + title + '</b><span>' + text + '</span></div></div>';
    }
    return '<div class="ph-shade">' + label('In the notification shade') +
      ntf('1 h 12 min left today', 'Counting') +
      ntf('5 minutes of screen time left today', 'Locks about 9:47 pm. Save what you are doing.') + '</div>';
  }

  root.LIMO_PHONE = {
    WHY: WHY, fresh: fresh, frame: frame, crop: crop,
    lock: lock, way: way, wayCard: wayCard, waysOnly: waysOnly,
    block: block, today: today, TABS: TABS, warning: warning, shade: shade
  };
}(typeof window !== 'undefined' ? window : this));
