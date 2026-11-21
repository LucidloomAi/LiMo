/* LiMo, the guardian's screens: sign in, the first start and Today, as
   functions that return HTML. Drawings to agree the look, not the product.

   One set of screens arranged by the width they get. A screen returns its
   page, and what is pinned to the bottom or floats over it; frame() puts it
   in the console's window on the family PC or on a guardian's phone. The
   markup of a screen is the same in both, and console.css arranges it.

   Icons are the light clay set in ../icons/svg. The sample family: a
   guardian, the children Asha and Mira, the family PC and Asha's phone. */
(function (root) {
  'use strict';

  function icon(name, size) {
    return '<img src="../icons/svg/' + name + '.svg" width="' + size + '" height="' + size + '" alt="">';
  }

  // Small marks beside text are plain ink strokes, never icons.
  function stroke(w, h, d, cls, width) {
    return '<svg class="mk' + (cls ? ' ' + cls : '') + '" width="' + w + '" height="' + h + '" viewBox="0 0 ' + w + ' ' + h +
      '" fill="none" stroke="currentColor" stroke-width="' + (width || 1.8) + '" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">' + d + '</svg>';
  }
  var MARK = {
    left: stroke(16, 16, '<path d="M10 3L5 8l5 5"/>'),
    down: stroke(16, 16, '<path d="M3.5 6l4.5 4.5L12.5 6"/>'),
    plus: stroke(18, 18, '<path d="M9 3.5v11M3.5 9h11"/>', '', 2),
    minus: stroke(18, 18, '<path d="M3.5 9h11"/>', '', 2),
    tick: stroke(18, 18, '<path d="M3.5 9.5l3.6 3.6L14.5 5"/>', 'tk', 2)
  };
  // Windows draws its own window buttons; they are drawn here in its manner.
  function winMark(d) {
    return '<span>' + stroke(10, 10, d, '', 1) + '</span>';
  }
  var WIN_CONTROLS = winMark('<path d="M0 5.5h10"/>') + winMark('<rect x=".5" y=".5" width="9" height="9" rx="1.5"/>') +
    winMark('<path d="M.5 .5l9 9M9.5 .5l-9 9"/>');

  var CLOCK = '6:12';
  var DATE = 'Saturday 21 November';

  // A time or a span of time never breaks between its parts.
  function nb(html) {
    return html.replace(/(\d) (am|pm|h|min)\b/g, '$1 $2').replace(/\bh (\d)/g, 'h $1');
  }

  function label(text, cls, hint) {
    return '<div class="cn-label' + (cls ? ' ' + cls : '') + '">' + text + (hint ? '<em>' + hint + '</em>' : '') + '</div>';
  }
  function dots(n) {
    var out = '';
    for (var i = 0; i < n; i += 1) { out += '<span class="dot"></span>'; }
    return out;
  }
  var CARET = '<span class="cn-caret"></span>';
  function field(inner, cls) {
    return '<div class="cn-field' + (cls ? ' ' + cls : '') + '">' + inner + '</div>';
  }
  function chips(list, picked, cls) {
    return '<div class="cn-chips' + (cls ? ' ' + cls : '') + '">' + list.map(function (c) {
      return '<span class="cn-chip' + (c[0] === picked ? ' on' : '') + '">' + c[1] + '</span>';
    }).join('') + '</div>';
  }
  function btn(text, cls) {
    return '<span class="cn-btn' + (cls ? ' ' + cls : '') + '">' + text + '</span>';
  }

  /** The top row: LiMo's own name first, and what belongs at the right. */
  function top(right) {
    return '<div class="cn-top"><div class="cn-mark">' + icon('hourglass', 24) + '<span>LiMo</span></div>' + (right || '') + '</div>';
  }

  // The maker's mark: said once, small, at the end of a screen that fills
  // the display, after LiMo's own name at the top.
  var MAKER = '<div class="cn-maker" aria-label="Made by LucidLoom AI"><span>Made by</span>' +
    '<img src="../../docs/images/lucidloom-logo.png" alt="" width="26" height="26"><b>LucidLoom AI</b></div>';

  // ---------- The frame ----------

  /** A screen in a frame. 'pc' is the console's window on the family PC,
      1100 by 760, with Windows' own title bar in its top 32. 'phone' is a
      guardian's phone, 390 by 844, with Android's status bar and gesture
      bar. The screen's own markup is the same in both. With whole, the
      page is drawn unrolled, at its full length. */
  function frame(screen, width, opt) {
    opt = opt || {};
    var pc = width === 'pc';
    var cls = 'cn-frame ' + (pc ? 'pc' : 'ph') + (screen.pin ? ' pinned' : '') + (screen.pins === 2 ? ' pin2' : '') +
      (screen.tabs ? ' tabbed' : '') + (opt.whole ? ' whole' : '');
    var chrome = pc
      ? '<div class="cn-win"><span class="nm">' + icon('hourglass', 16) + 'LiMo</span><span class="ctl">' + WIN_CONTROLS + '</span></div>'
      : '<div class="cn-status">' + CLOCK + '</div>';
    return nb('<div class="' + cls + '"' + (opt.id ? ' data-id="' + opt.id + '"' : '') + '>' + chrome +
      '<div class="cn-view"><div class="cn-page">' + screen.page + '</div>' +
      (screen.pin ? '<div class="cn-pin"><div class="cn-pin-in">' + screen.pin + '</div></div>' : '') +
      (screen.tabs || '') + '</div>' +
      (pc ? '' : '<div class="cn-gesture"><i></i></div>') + '</div>');
  }

  // ---------- C0. Sign in ----------

  var FORGOT = 'A forgotten password cannot be found again, because LiMo keeps it nowhere but this PC. ' +
    'An administrator of this PC can set a new one.';

  /** state: 'ready' (typing), 'wrong', 'wait' (after too many wrong
      tries), or 'child' (opened from a child's Windows account). */
  function signIn(state) {
    var child = state === 'child';
    var wait = state === 'wait';
    var msg = state === 'wrong' ? 'That password is not right. 3 more tries before a wait.'
      : wait ? 'Too many wrong tries. Try again at 6:17 pm.' : '';
    var body = child
      ? '<div class="cn-refuse"><span class="cn-tile">' + icon('account', 32) + '</span>' +
        '<p>LiMo’s settings cannot be opened from Asha’s account. A guardian opens them from their own.</p></div>'
      : label('Guardian password', 'gap') +
        field(wait ? '' : state === 'ready' ? dots(9) + CARET : CARET, wait ? 'off' : 'at') +
        '<div class="cn-err">' + msg + '</div>' +
        btn('Sign in', 'go wide' + (wait ? ' off' : '')) +
        '<div class="cn-forgot">' + label('Forgotten it?') + '<p>' + FORGOT + '</p></div>';
    return {
      page: '<div class="cn-wrap">' + top() + '<div class="cn-mid"><div class="cn-card cn-signin">' +
        icon(child ? 'lock' : 'key', 64) +
        '<h1 class="cn-h1">LiMo’s settings are for a guardian.</h1>' + body + '</div></div></div>'
    };
  }

  // ---------- F1 to F8. The first start ----------

  var STEPS = 8;

  /** Back at the left (its room kept empty on the first step), the counter
      at the right: done ones teal, this one marigold, later ones outlined. */
  function stepHead(at) {
    var d = '';
    for (var i = 0; i < STEPS; i += 1) {
      d += '<span class="cn-dot' + (i < at ? ' done' : i === at ? ' at' : '') + '"></span>';
    }
    return '<div class="cn-steps">' + (at > 0 ? '<span class="cn-back">' + MARK.left + 'Back</span>' : '<span></span>') +
      '<div class="cn-count">' + label('Step ' + (at + 1) + ' of ' + STEPS) +
      '<span class="cn-dots" aria-label="' + (at + 1) + ' of ' + STEPS + '">' + d + '</span></div></div>';
  }
  /** A step: the top row, Back and the counter, the card, and `end` (the
      maker's mark) at the end of the scrolling part. */
  function step(at, inner, pin, pins, end) {
    return {
      page: '<div class="cn-wrap">' + top() + '<div class="cn-col">' + stepHead(at) + inner + '</div>' + (end || '') + '</div>',
      pin: pin, pins: pins
    };
  }
  function cardHead(iconName, title) {
    return '<div class="cn-hrow">' + icon(iconName, 48) + '<h1 class="cn-h1">' + title + '</h1></div>';
  }
  function go(text, off) { return btn(text, 'go' + (off ? ' off' : '')); }

  // F1. Welcome: what LiMo does, what it never does, and the guardian agrees.
  var DOES = [
    ['hourglass', 'Counts time', 'On this PC and on phones, while a child uses them.'],
    ['lock', 'Locks', 'When the day’s time is used up, and at bedtime.'],
    ['block', 'Blocks', 'The programs and apps you choose.'],
    ['eyeoff', 'Never looks inside', 'No screenshots, nothing typed, and nothing leaves your home.']
  ];
  var WINDOW_LINE = 'This window is drawn by a part of Windows that contacts Microsoft when it starts. LiMo itself sends nothing anywhere.';
  function welcome(ticked) {
    return step(0,
      '<div class="cn-head">' + label('Welcome') + '<h1 class="cn-h1">What LiMo does</h1></div>' +
      '<div class="cn-card cn-list">' + DOES.map(function (r) {
        return '<div class="cn-li"><span class="cn-tile">' + icon(r[0], 32) + '</span><span><b>' + r[1] + '</b><i>' + r[2] + '</i></span></div>';
      }).join('') + '</div>' +
      // Said only where Windows draws the window: in the console on the PC.
      '<p class="cn-small cn-pc-only">' + WINDOW_LINE + '</p>' +
      '<div class="cn-tick"><span class="cn-box' + (ticked ? ' on' : '') + '">' + (ticked ? MARK.tick : '') + '</span>' +
      '<span>I am a guardian of this family, and I agree.</span></div>',
      go('Continue', !ticked));
  }

  var PASSWORD_LINE = 'You type it to open LiMo’s settings on this PC.';
  var PIN_LINE = 'You type it on a child’s locked PC or phone to give time or open it.';

  // F2. The guardian password, twice.
  function password() {
    return step(1, '<div class="cn-card">' + cardHead('key', 'The guardian password') +
      '<p class="cn-detail">' + PASSWORD_LINE + '</p>' +
      label('Password', 'gap', '8 or more characters') + field(dots(11)) +
      label('The same again', 'gap') + field(dots(6) + CARET, 'at') +
      '<div class="cn-err"></div></div>', go('Continue'));
  }

  // F3. The guardian PIN, twice, with the two secrets side by side.
  function pin() {
    return step(2, '<div class="cn-card">' + cardHead('pin', 'The guardian PIN') +
      '<div class="cn-two">' +
        '<div><span class="cn-tile">' + icon('key', 32) + '</span><p><b>The password</b>' + PASSWORD_LINE + '</p></div>' +
        '<div class="now"><span class="cn-tile">' + icon('pin', 32) + '</span><p><b>The PIN</b>' + PIN_LINE + '</p></div>' +
      '</div>' +
      '<p class="cn-detail">Choose one your child does not know.</p>' +
      label('PIN', 'gap', '6 to 8 digits') + field(dots(6)) +
      label('The same again', 'gap') + field(dots(3) + CARET, 'at') +
      '<div class="cn-err"></div></div>', go('Continue'));
  }

  // F4. The child: a name and an age band.
  var AGES = [['u6', 'Under 6'], ['6', '6 to 9'], ['10', '10 to 12'], ['13', '13 to 17']];
  function child() {
    return step(3, '<div class="cn-card">' + cardHead('child', 'Your child') +
      label('Name', 'gap') + field('<span class="typed">Asha</span>' + CARET, 'at') +
      label('Age', 'gap') + chips(AGES, '10', 'even') +
      '<p class="cn-small">You can add another child later.</p></div>', go('Continue'));
  }

  // F5. Time a day: one figure for every day, or each day on its own.
  var WEEK = [['Monday', '1 h 30 min'], ['Tuesday', '1 h 30 min'], ['Wednesday', '1 h 30 min'], ['Thursday', '1 h 30 min'],
    ['Friday', '2 h'], ['Saturday', '3 h'], ['Sunday', '3 h']];
  function stepper(value, big) {
    return '<div class="cn-stepper"><span class="cn-sq" aria-label="Less">' + MARK.minus + '</span>' +
      '<span class="' + (big ? 'cn-bigfig' : 'cn-fig') + '">' + value + '</span>' +
      '<span class="cn-sq" aria-label="More">' + MARK.plus + '</span></div>';
  }
  function timeADay(each) {
    var body = each
      ? '<div class="cn-days">' + WEEK.map(function (d) {
          return '<div class="cn-day"><span>' + d[0] + '</span>' + stepper(d[1]) + '</div>';
        }).join('') + '</div>'
      : '<div class="cn-one">' + stepper('2 h', true) + '</div>';
    return step(4, '<div class="cn-card">' + cardHead('calendar', 'Time a day') +
      '<p class="cn-detail">For all of Asha’s devices together.</p>' +
      '<div class="cn-chips" style="margin-top:16px">' +
        '<span class="cn-chip' + (each ? '' : ' on') + '">Every day</span>' +
        '<span class="cn-chip' + (each ? ' on' : '') + '">Each day on its own</span></div>' +
      body + '</div>', go('Continue'));
  }

  // F6. Bedtime.
  function bedtime() {
    function time(v) { return field('<span class="val">' + v + '</span>' + MARK.down); }
    return step(5, '<div class="cn-card">' + cardHead('moon', 'Bedtime') +
      '<p class="cn-detail">Asha’s devices lock for these hours. Only a guardian’s PIN opens them.</p>' +
      label('When', 'gap') + chips([['every', 'Every day'], ['school', 'School nights only'], ['none', 'No bedtime']], 'every') +
      '<div class="cn-times"><div>' + label('From') + time('9:00 pm') + '</div><div>' + label('To') + time('7:00 am') + '</div></div>' +
      '<div class="cn-err"></div></div>', go('Continue'));
  }

  // F7. The Windows accounts on this PC, and whose each is.
  function accounts(adminChild) {
    // Facts are one line on a phone, and one under the other at the
    // console's width, where the choices stand beside them.
    var list = [
      { name: 'Guardian', facts: '<span>Administrator</span><span class="sep"> · </span><span>signed in now</span>', pick: 'g' },
      { name: 'Asha', facts: adminChild ? 'Administrator' : 'Standard', admin: adminChild, pick: 'a' },
      { name: 'Mira', facts: 'Standard', pick: 'l' }
    ];
    var warn = '<div class="cn-warn"><div class="cn-warn-top">' + icon('alert', 24) +
      '<p><b>Asha’s account is an administrator.</b> LiMo cannot hold against an administrator. ' +
      'Make it a standard account in Windows Settings, Accounts, Other users.</p></div>' +
      '<ol><li>There, choose Asha, then Change account type.</li><li>Pick Standard User, then OK.</li></ol>' +
      '<p class="cn-small">LiMo changes no account itself.</p></div>';
    return step(6, '<div class="cn-card">' + cardHead('account', 'Accounts on this PC') +
      '<p class="cn-detail">Say whose each account is. A guardian’s account has no limit.</p>' +
      '<div class="cn-accts">' + list.map(function (a) {
        return '<div class="cn-acct"><span class="cn-tile">' + icon('account', 32) + '</span>' +
          '<div class="who"><b>' + a.name + '</b>' + label(a.facts, 'facts' + (a.admin ? ' admin' : '')) + '</div>' +
          chips([['a', 'Asha’s'], ['g', 'A guardian’s'], ['l', 'Later']], a.pick) +
          (a.admin ? warn : '') + '</div>';
      }).join('') + '</div>' +
      '<p class="cn-small">An account left for later stays locked until a guardian says whose it is. An administrator’s account is never locked.</p>' +
      '</div>', go('Continue'));
  }

  // F8. Done: add the phone now or later, and the maker's mark at the end.
  function done() {
    var made = ['Guardian password and PIN', 'Asha, 10 to 12', '2 h a day', 'Bedtime from 9:00 pm to 7:00 am', 'Asha’s account on this PC'];
    return step(7, '<div class="cn-card">' + cardHead('check', 'The family is set up.') +
      '<div class="cn-oks">' + made.map(function (m) { return '<div class="cn-ok">' + MARK.tick + '<span>' + m + '</span></div>'; }).join('') + '</div>' +
      '<div class="cn-next"><span class="cn-tile">' + icon('phone', 32) + '</span><span><b>Asha’s phone</b>' +
      '<i>Connect it to this PC with a USB cable.</i></span></div></div>',
      btn('Later') + go('Add Asha’s phone now'), 2, MAKER);
  }

  // ---------- T. Today ----------

  function tabs(picked) {
    return '<nav class="cn-tabs">' + [['today', 'sun', 'Today'], ['rules', 'edit', 'Rules'], ['devices', 'devices', 'Devices'], ['more', 'gear', 'More']]
      .map(function (t) { return '<span class="cn-tab' + (t[0] === picked ? ' on' : '') + '">' + icon(t[1], 24) + t[2] + '</span>'; }).join('') + '</nav>';
  }

  function device(d) {
    return '<div class="cn-dev"><span class="cn-tile">' + icon(d.icon, 32) + '</span>' +
      '<span class="nm"><b>' + d.name + '</b><span class="st' + (d.live ? ' live' : '') + '">' + d.st + '</span>' +
      (d.need ? '<span class="need">' + icon('alert', 20) + d.need + '</span>' : '') + '</span>' +
      '<span class="min">' + d.min + '</span></div>';
  }

  /** One child: the time left, used of the day's time, the bar, each
      device, and the quick actions. */
  function kid(k) {
    return '<div class="cn-card cn-kid"><div class="cn-kid-in">' +
      '<div class="cn-kid-head"><b>' + k.name + '</b>' + (k.stamp ? '<span class="cn-st coral">' + k.stamp + '</span>' : '') + '</div>' +
      '<div class="cn-kid-time"><b class="cn-big">' + k.left + '</b>' + label('Left today') +
        '<div class="cn-bar"><i' + (k.pct > 0 && k.pct < 100 ? ' class="part"' : '') + ' style="width:' + k.pct + '%"></i></div>' +
        '<p class="cn-used">' + k.used + '</p></div>' +
      '<div class="cn-devs">' + k.devices.map(device).join('') +
        (k.addPhone ? '<div class="cn-dev missing"><span class="cn-tile">' + icon('phone', 32) + '</span>' +
          '<p>Asha’s phone is not added yet.</p>' + btn('Add Asha’s phone') + '</div>' : '') + '</div>' +
      '<div class="cn-acts">' + btn('+15 min', 'mono') + btn('+30 min', 'mono') + btn('Lock now', 'mono coral') +
        btn('Open until' + MARK.down, 'mono span2') + btn('Lift', 'mono dim') + '</div>' +
      '</div></div>';
  }

  var PC = 'pc';
  var ASHA = {
    name: 'Asha', left: '1 h 12 min', pct: 40, used: '48 min used of 2 h',
    devices: [
      { icon: PC, name: 'This PC', st: 'Not in use', min: '17 min' },
      { icon: 'phone', name: 'Asha’s phone', st: 'In use', live: true, min: '31 min' }
    ]
  };

  /** kind: 'first' (right after the first start), 'one' (one child, an
      ordinary afternoon) or 'two' (two children, a request waiting, and a
      phone that needs a guardian). */
  function today(kind) {
    var kids, aside;
    if (kind === 'first') {
      kids = [{ name: 'Asha', left: '2 h', pct: 0, used: '0 min used of 2 h', addPhone: true,
        devices: [{ icon: PC, name: 'This PC', st: 'Not in use', min: '0 min' }] }];
      aside = '<div class="cn-quiet"><b>Nothing is waiting for you.</b>When Asha asks for more time, the request shows here.</div>';
    } else if (kind === 'one') {
      kids = [ASHA];
      aside = '<div class="cn-quiet"><b>Nothing is waiting for you.</b></div>';
    } else {
      kids = [
        { name: 'Asha', left: '1 h 12 min', pct: 40, used: '48 min used of 2 h',
          devices: [ASHA.devices[0], { icon: 'phone', name: 'Asha’s phone', st: 'Last heard 3 min ago', min: '31 min', need: 'Accessibility is off' }] },
        { name: 'Mira', left: '0 min', pct: 100, used: '1 h used of 1 h', stamp: 'Locked',
          devices: [{ icon: PC, name: 'This PC', st: 'Not in use', min: '1 h' }] }
      ];
      // A request and something that needs a guardian both wait for a
      // guardian, so they share one heading; the coral stamp says which.
      aside = label('Waiting for you', 'cn-sec') +
        '<div class="cn-card cn-req"><div class="cn-req-top"><span class="cn-tile lg">' + icon('hand', 32) + '</span>' +
          '<span><b>Mira asks for 30 min</b>' + label('On this PC · 6:04 pm') + '</span></div>' +
          '<p class="cn-quote">“To finish my science project”</p>' +
          '<div class="cn-answers">' + btn('Give 30 min', 'go') + btn('Give 15 min') + btn('No') + '</div></div>' +
        '<div class="cn-card cn-need"><div class="cn-need-top"><span class="cn-tile lg">' + icon('alert', 32) + '</span>' +
          '<span class="cn-st coral">Needs a guardian</span></div>' +
          '<b>Asha’s phone</b><p>Accessibility is switched off, so LiMo cannot see which app is open.</p>' +
          btn('See what to do', 'wide') + '</div>';
    }
    return {
      page: '<div class="cn-wrap tabbed">' + top(label(DATE)) +
        '<div class="cn-today"><div class="cn-aside">' + aside + '</div>' +
        '<div class="cn-kids">' + kids.map(kid).join('') + '</div></div></div>',
      tabs: tabs('today')
    };
  }

  root.LIMO_CONSOLE = {
    frame: frame,
    signIn: signIn,
    welcome: welcome, password: password, pin: pin, child: child, timeADay: timeADay,
    bedtime: bedtime, accounts: accounts, done: done,
    today: today
  };
}(typeof window !== 'undefined' ? window : this));
