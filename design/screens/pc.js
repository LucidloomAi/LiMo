/* LiMo on the PC: what the lock screen, the time left display and its
   notices show, as functions that return HTML. A design sample: the real
   ones are drawn by LiMo's helper in the child's session, from the orders
   the agent's engine gives (agent/src/orders.rs).

   Needs ../icons/clay.js and ../icons/shapes.js loaded first. */
(function (root) {
  'use strict';

  var CLAY = root.LIMO_CLAY, SHAPES = root.LIMO_SHAPES;

  function icon(name, size) {
    var d = SHAPES[name];
    return CLAY.svg(d.shapes, { size: size, clay: d.clay, look: 'light' });
  }
  function esc(s) {
    return String(s).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');
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

  // Every reason a session can be locked for, in the engine's names. The
  // title is the short line; the detail is the engine's own message, or
  // close to it. `ways` is what this sample offers for the reason: on the
  // PC the engine says which ways are open.
  var WHY = {
    time_used_up: {
      icon: 'hourglass', title: 'Today’s time is used up.',
      detail: 'You have used all of today’s screen time.',
      until: 'Tomorrow, 7:00 am', ways: ['ask', 'code', 'pin']
    },
    blocked_hours: {
      icon: 'moon', title: 'Screen time is off for now.',
      detail: 'This is a time when the PC is not used.',
      until: 'Tomorrow, 7:00 am', ways: ['pin']
    },
    no_time_today: {
      icon: 'calendar', title: 'No screen time today.',
      detail: 'Today has no screen time in your family’s rules.',
      until: 'Tomorrow, 7:00 am', ways: ['ask', 'code', 'pin']
    },
    guardian_lock: {
      icon: 'lock', title: 'Locked by a guardian.',
      detail: 'A guardian has locked the PC for now.',
      untilPlain: 'When a guardian unlocks it', ways: ['pin']
    },
    held_by_another_device: {
      icon: 'away', title: 'The rest is on another device.',
      detail: 'The rest of today’s time is on a device that is out of the house. It comes here when that device is back home.',
      untilPlain: 'When that device is back home', ways: ['ask', 'code', 'pin']
    },
    waiting_for_time: {
      icon: 'sync', stamp: 'One moment', stampKind: 'ink', title: 'Getting more time.',
      detail: 'This takes a moment.',
      ways: [], none: ['Nothing to do here.', 'The PC opens by itself.']
    },
    account_not_linked: {
      icon: 'account', title: 'This account is not set up yet.',
      detail: 'LiMo does not know whose account this is.',
      ways: [], none: ['A guardian can set it up.', 'In LiMo on this PC, a guardian says whose account this is, or that it has no limit.']
    },
    cannot_check: {
      icon: 'alert', title: 'LiMo cannot check the time.',
      detail: 'LiMo cannot read how much time is left just now.',
      ways: [], none: ['Ask a guardian.', 'A guardian can look at LiMo on this PC to see what is wrong.']
    },
    opened: {
      icon: 'unlock', stamp: 'Open', stampKind: 'teal', title: '30 minutes added.',
      detail: 'The PC opens now.', ways: [], sample: true
    }
  };

  var WAYS = {
    ask: ['hand', 'Ask for more time', 'A guardian sees your request in LiMo'],
    code: ['code', 'Type a one time code', 'A guardian who is away can read you 8 digits'],
    pin: ['pin', 'Guardian PIN', 'For a guardian who is at this PC']
  };
  var ASK_MINUTES = [[15, '15 min'], [30, '30 min'], [45, '45 min'], [60, '1 hour']];
  var PIN_ADD = [['add15', '15 min'], ['add30', '30 min'], ['add60', '1 hour']];
  var PIN_OPEN = [['open60', '1 hour'], ['open120', '2 hours'], ['today', 'Today']];

  function fresh(over) {
    var s = {
      why: 'time_used_up', panel: 'ways', child: 'Mira', now: '9:42 pm', date: 'Wednesday 18 November',
      request: null,       // { minutes, at, answer: 'waiting' | 'refused' | 'granted', given }
      waitUntil: null,     // '9:47 pm' when wrong tries have set a wait
      askMinutes: 30, reason: '', typed: '', pinAct: 'add30', error: '', wrong: 0, second: false
    };
    for (var k in over || {}) { s[k] = over[k]; }
    return s;
  }

  function label(text, hint) {
    return '<div class="lk-label">' + text + (hint ? '<em>' + hint + '</em>' : '') + '</div>';
  }
  function chips(list, picked, act) {
    return '<div class="lk-chips">' + list.map(function (c) {
      return '<button class="lk-chip' + (c[0] === picked ? ' on' : '') + '" data-act="' + act + '" data-v="' + c[0] + '">' + c[1] + '</button>';
    }).join('') + '</div>';
  }
  function head(title) {
    return '<div class="lk-head"><button class="lk-back" data-act="back">' + MARK.left + 'Back</button><h2>' + title + '</h2></div>';
  }
  function pad() {
    var keys = ['1', '2', '3', '4', '5', '6', '7', '8', '9'].map(function (d) {
      return '<button class="lk-key" data-act="key" data-v="' + d + '">' + d + '</button>';
    });
    keys.push('<span class="lk-key none"></span>');
    keys.push('<button class="lk-key" data-act="key" data-v="0">0</button>');
    keys.push('<button class="lk-key" data-act="del" aria-label="Remove the last digit">' + MARK.del + '</button>');
    return '<div class="lk-pad">' + keys.join('') + '</div>';
  }

  function requestLine(r) {
    if (!r) { return ''; }
    var st = r.answer === 'granted' ? '<span class="stamp teal">Given ' + r.given + ' min</span>'
      : r.answer === 'refused' ? '<span class="stamp coral">Not this time</span>'
        : '<span class="stamp">Waiting</span>';
    return '<div class="lk-req">' + icon('hand', 24) + '<span>You asked for ' + r.minutes + ' min at ' + r.at + '</span>' + st + '</div>';
  }

  function panelWays(s, w) {
    if (!w.ways.length) {
      return '<div class="lk-none">' + icon(w.sample ? 'check' : 'info', 36) + '<div><b>' +
        (w.sample ? 'This is a design sample.' : w.none[0]) + '</b><span>' +
        (w.sample ? 'On a real PC the lock screen goes away here.' : w.none[1]) + '</span></div></div>' +
        (w.sample ? '<div class="lk-sample"><button data-act="again">Show the lock again</button></div>' : '');
    }
    var waiting = s.request && s.request.answer === 'waiting';
    var rows = w.ways.filter(function (k) { return !(k === 'ask' && waiting); }).map(function (k) {
      var off = s.waitUntil && k !== 'ask';
      return '<button class="lk-row' + (off ? ' off' : '') + '"' + (off ? '' : ' data-act="open" data-v="' + k + '"') + '>' +
        '<span class="lk-tile">' + icon(WAYS[k][0], 34) + '</span><span><b>' + WAYS[k][1] + '</b><i>' + (off ? 'Wait until ' + s.waitUntil : WAYS[k][2]) + '</i></span>' +
        (off ? '' : MARK.right) + '</button>';
    }).join('');
    return label('What you can do') + requestLine(s.request) +
      (s.waitUntil ? '<div class="lk-wait">' + icon('alert', 24) + '<span>Too many wrong tries. The next try is at ' + s.waitUntil + '.</span></div>' : '') +
      '<div class="lk-rows">' + rows + '</div>' +
      // The closing line gives way when a request and a wait are both shown.
      (s.request && s.waitUntil ? '' : '<div class="lk-also">A guardian who is at home can also open the PC from their own phone.</div>');
  }

  function panelAsk(s) {
    return head('Ask for more time') +
      label('How long') + chips(ASK_MINUTES, s.askMinutes, 'minutes') +
      label('Why', 'you can leave this empty') +
      '<textarea class="lk-text" maxlength="200" placeholder="To finish my science project">' + esc(s.reason) + '</textarea>' +
      '<div class="lk-count"><span id="lk-count">' + s.reason.length + '</span> / 200</div>' +
      '<div class="lk-actions"><p class="lk-small">A guardian can say yes, give a different amount, or say no.</p>' +
      '<button class="lk-btn go" data-act="send">Send request</button></div>';
  }

  function panelCode(s) {
    var cells = '';
    for (var i = 0; i < 8; i += 1) {
      cells += '<span class="lk-cell' + (i === 4 ? ' gap' : '') + (i === s.typed.length ? ' at' : '') + '">' + (s.typed.charAt(i) || '') + '</span>';
    }
    return head('Type a one time code') +
      '<p class="lk-note">A guardian who is away from home can read you a code. It has 8 digits.</p>' +
      '<div class="lk-cells">' + cells + '</div><div class="lk-err">' + esc(s.error) + '</div>' +
      '<div class="lk-entry">' + pad() + '<div class="lk-side"><p class="lk-small">A code works once, only today, and only on this PC.</p>' +
      '<button class="lk-btn go" data-act="use-code">Use this code</button></div></div>';
  }

  function panelPin(s) {
    var dots = '';
    for (var i = 0; i < s.typed.length; i += 1) { dots += '<span class="dot"></span>'; }
    return head('Guardian PIN') +
      '<p class="lk-note">For a guardian. Type your PIN and choose what to give.</p>' +
      '<div class="lk-pinbox at">' + (dots || '<span class="ph">6 to 8 digits</span>') + '</div><div class="lk-err">' + esc(s.error) + '</div>' +
      '<div class="lk-entry">' + pad() + '<div class="lk-side">' +
      label('Add time') + chips(PIN_ADD, s.pinAct, 'pin-act') +
      label('Or keep it open for') + chips(PIN_OPEN, s.pinAct, 'pin-act') +
      '<button class="lk-btn go" data-act="use-pin">Unlock</button></div></div>';
  }

  /** The card in the middle of the lock screen. */
  function card(s) {
    var w = WHY[s.why] || WHY.time_used_up;
    var until = w.until ? '<div class="lk-until"><span>Opens again</span><b>' + w.until + '</b></div>'
      : w.untilPlain ? '<div class="lk-until"><span>Opens again</span><b class="plain">' + w.untilPlain + '</b></div>' : '';
    var right = s.panel === 'ask' ? panelAsk(s) : s.panel === 'code' ? panelCode(s) : s.panel === 'pin' ? panelPin(s) : panelWays(s, w);
    return '<div class="lk-card">' +
      '<section class="lk-why"><div class="lk-why-top">' + icon(w.icon, 88) +
      '<span class="lk-stamp' + (w.stampKind ? ' ' + w.stampKind : '') + '">' + (w.stamp || 'Locked') + '</span></div>' +
      '<div class="lk-eyebrow">' + esc(s.child) + ' · this PC</div>' +
      '<h1>' + (s.title || w.title) + '</h1><p>' + (s.detail || w.detail) + '</p>' + until + '</section>' +
      '<section class="lk-do">' + right + '</section></div>';
  }

  /** The whole lock screen. On any screen but the main one it only says so. */
  function lock(s) {
    var top = '<div class="lk-top"><div class="lk-mark">' + icon('hourglass', 30) + '<span>LiMo</span></div>' +
      '<div class="lk-now"><b>' + s.now + '</b><span>' + s.date + '</span></div></div>';
    if (s.second) {
      return '<div class="lk">' + top + '<div class="lk-second">' + icon('lock', 72) +
        '<span class="lk-stamp">Locked</span><p>Look at the main screen.</p></div><div class="lk-foot">&nbsp;</div></div>';
    }
    return '<div class="lk">' + top + '<div class="lk-mid">' + card(s) + '</div>' +
      '<div class="lk-foot">Nothing was closed. Your programs are still open behind this screen.</div></div>';
  }

  // ---------- The time left display ----------

  var PILLS = {
    counting: ['hourglass', '', '1 h 12 min', 'left'],
    paused: ['hourglass', 'paused', '1 h 12 min', 'paused'],
    soon: ['hourglass', 'soon', '14 min', 'left'],
    last: ['hourglass', 'last', '0:42', 'left'],
    nolimit: ['check', 'open', 'No limit today', ''],
    unlocked: ['unlock', 'open', 'Open until 9:30 pm', '']
  };
  function pill(kind) {
    var p = PILLS[kind] || PILLS.counting;
    return '<span class="tl' + (p[1] ? ' ' + p[1] : '') + '">' + icon(p[0], 24) + '<b>' + p[2] + '</b>' + (p[3] ? '<small>' + p[3] + '</small>' : '') + '</span>';
  }

  // The headings are the engine's own words (orders.rs, WarningShown).
  var NOTICES = {
    warn15: ['bell', '15 minutes of screen time left today.', 'The PC locks when the time is used up.', 'Locks about 9:57 pm', ''],
    warn5: ['bell', '5 minutes of screen time left today.', 'Save what you are doing.', 'Locks about 9:47 pm', ''],
    warn1: ['bell', '1 minute of screen time left today.', 'Save what you are doing now.', '', '0:42'],
    bed15: ['moon', 'The PC locks in 15 minutes, for the hours it is not used.', 'Finish what you are doing.', 'Locks at 9:00 pm', ''],
    unlock5: ['unlock', 'The unlock ends in 5 minutes, and the PC locks then.', 'Save what you are doing.', 'Locks at 9:30 pm', ''],
    closed: ['block', 'Game Launcher was closed.', 'It is blocked by your family’s rules.', '', '']
  };
  function notice(kind) {
    var n = NOTICES[kind] || NOTICES.warn15;
    return '<div class="nt"><div class="nt-top"><span class="lk-tile">' + icon(n[0], 34) + '</span><div><h3>' + n[1] + '</h3><p>' + n[2] + '</p></div></div>' +
      '<div class="nt-foot">' + (n[4] ? '<span class="count">' + n[4] + '</span>' : '<span class="when">' + n[3] + '</span>') +
      '<button class="lk-btn" data-act="ok">OK</button></div></div>';
  }

  /** A plain desktop with the display on it: 'doc', 'game' or 'video'. */
  function desk(kind, pillKind, noticeKind) {
    var inside = '';
    if (kind === 'doc') {
      var lines = '<i class="h"></i>';
      [100, 96, 100, 88, 100, 97, 62, 0, 100, 94, 100, 90, 71].forEach(function (wd) {
        lines += wd ? '<i style="width:' + wd + '%"></i>' : '<i style="background:none"></i>';
      });
      inside = '<div class="desk-strip"></div><div class="desk-page">' + lines + '</div>';
    } else if (kind === 'game') {
      [[8, 12], [22, 30], [37, 9], [52, 22], [66, 14], [81, 27], [91, 8], [15, 44], [73, 40]].forEach(function (p) {
        inside += '<span class="desk-star" style="left:' + p[0] + '%;top:' + p[1] + '%"></span>';
      });
      inside += '<div class="desk-hill"></div><div class="desk-hill b"></div>';
    } else {
      inside = '<div class="desk-play"></div>';
    }
    return '<div class="desk ' + kind + '">' + inside + '<div class="desk-bar">9:42 pm<br>18 Nov 2026</div>' +
      (noticeKind ? notice(noticeKind) : '') + pill(pillKind) + '</div>';
  }

  root.LIMO_PC = {
    WHY: WHY, PILLS: PILLS, NOTICES: NOTICES,
    fresh: fresh, lock: lock, card: card, pill: pill, notice: notice, desk: desk, icon: icon
  };
}(typeof window !== 'undefined' ? window : this));
