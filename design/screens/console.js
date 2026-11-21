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
    right: stroke(16, 16, '<path d="M6 3l5 5-5 5"/>'),
    down: stroke(16, 16, '<path d="M3.5 6l4.5 4.5L12.5 6"/>'),
    cross: stroke(18, 18, '<path d="M5 5l8 8M13 5l-8 8"/>', '', 2),
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
  /** screen: { page, pin, pins, pinCls ('pinfoot' with the cable's state,
      'pinsave' with a line above the buttons), pinWide (the strip as wide
      as the page, not the reading column), tabs, over (something floating
      above the page) }. opt: { id, whole, scrollTo (a selector the page is
      drawn scrolled to) }. */
  function frame(screen, width, opt) {
    opt = opt || {};
    var pc = width === 'pc';
    var cls = 'cn-frame ' + (pc ? 'pc' : 'ph') + (screen.pin ? ' pinned' : '') + (screen.pins === 2 ? ' pin2' : '') +
      (screen.pinCls ? ' ' + screen.pinCls : '') + (screen.tabs ? ' tabbed' : '') + (opt.whole ? ' whole' : '');
    var chrome = pc
      ? '<div class="cn-win"><span class="nm">' + icon('hourglass', 16) + 'LiMo</span><span class="ctl">' + WIN_CONTROLS + '</span></div>'
      : '<div class="cn-status">' + CLOCK + '</div>';
    return nb('<div class="' + cls + '"' + (opt.id ? ' data-id="' + opt.id + '"' : '') + '>' + chrome +
      '<div class="cn-view"><div class="cn-page"' + (opt.scrollTo && !opt.whole ? ' data-scroll-to="' + opt.scrollTo + '"' : '') + '>' + screen.page + '</div>' +
      (screen.pin ? '<div class="cn-pin"><div class="cn-pin-in' + (screen.pinWide ? ' wide' : '') + '">' + screen.pin + '</div></div>' : '') +
      (screen.over || '') + (screen.tabs || '') + '</div>' +
      (pc ? '' : '<div class="cn-gesture"><i></i></div>') + '</div>');
  }

  /** Only the pinned strip of a screen, as wide as the window or the
      phone: to show the cable's state in each of its cases. */
  function strip(screen, width, opt) {
    opt = opt || {};
    var pc = width === 'pc';
    return nb('<div class="cn-frame strip ' + (pc ? 'pc' : 'ph') + ' pinned ' + (screen.pinCls || '') + '"' +
      (opt.id ? ' data-id="' + opt.id + '"' : '') + '><div class="cn-view">' +
      '<div class="cn-pin"><div class="cn-pin-in">' + screen.pin + '</div></div></div></div>');
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
  function stepHead(at, total, backFirst) {
    total = total || STEPS;
    var d = '';
    for (var i = 0; i < total; i += 1) {
      d += '<span class="cn-dot' + (i < at ? ' done' : i === at ? ' at' : '') + '"></span>';
    }
    return '<div class="cn-steps">' + (at > 0 || backFirst ? '<span class="cn-back">' + MARK.left + 'Back</span>' : '<span></span>') +
      '<div class="cn-count">' + label('Step ' + (at + 1) + ' of ' + total) +
      '<span class="cn-dots" aria-label="' + (at + 1) + ' of ' + total + '">' + d + '</span></div></div>';
  }
  /** A step: the top row, Back and the counter, the card, and `end` (the
      maker's mark) at the end of the scrolling part. `more` sets the
      total of steps and the kind of pinned strip. */
  function step(at, inner, pin, pins, end, more) {
    more = more || {};
    return {
      page: '<div class="cn-wrap">' + top() + '<div class="cn-col">' + stepHead(at, more.total, more.backFirst) + inner + '</div>' + (end || '') + '</div>',
      pin: pin, pins: pins, pinCls: more.pinCls
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
    return '<nav class="cn-tabs">' + [['today', 'sun', 'Today'], ['rules', 'rules', 'Rules'], ['devices', 'devices', 'Devices'], ['more', 'gear', 'More']]
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
        '<p class="cn-used">' + k.used + '</p>' + (k.who ? '<p class="cn-who">' + icon('guardianphone', 20) + '<span>' + k.who + '</span></p>' : '') + '</div>' +
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

  // ---------- Shared parts of the screens with tabs ----------

  /** A card's head on a screen with tabs: the icon on a tile, the heading,
      and what stands at its right. */
  function head2(iconName, title, right) {
    return '<div class="cn-head2"><span class="cn-tile">' + icon(iconName, 32) + '</span><h2 class="cn-h2">' + title + '</h2>' + (right || '') + '</div>';
  }
  /** Back, and the place it goes back to. */
  function subHead(to) {
    return '<div class="cn-steps"><span class="cn-back">' + MARK.left + 'Back</span>' + label(to) + '</div>';
  }
  /** A fact: its name in mono, its value, and what can be done about it. */
  function kv(k, v, after, vcls) {
    return '<div class="cn-kv"><span class="k">' + k + '</span><span class="v' + (vcls ? ' ' + vcls : '') + '">' + v + '</span>' + (after || '') + '</div>';
  }
  function letter(name, sm) {
    return '<span class="cn-letter' + (sm ? ' sm' : '') + '">' + name.charAt(0) + '</span>';
  }
  function tabbed(picked, inner, more) {
    more = more || {};
    return {
      page: '<div class="cn-wrap tabbed">' + top(more.right === undefined ? label(DATE) : more.right) + inner + (more.end || '') + '</div>',
      tabs: more.noTabs ? '' : tabs(picked), over: more.over,
      pin: more.pin, pinCls: more.pinCls, pinWide: more.pinWide
    };
  }

  // ---------- R. A child's rules ----------

  /** An app or program in a list: its first letter on a tile (the real
      screen shows its own icon), its name, and a way to take it off. */
  function appRow(name, opt) {
    opt = opt || {};
    return '<div class="cn-app' + (opt.bad ? ' bad' : '') + '">' + letter(name) + '<span class="nm">' + name + '</span>' +
      (opt.fixed ? '<span class="cn-fixed">' + opt.fixed + '</span>' : '<span class="cn-sq" aria-label="Take it off">' + MARK.cross + '</span>') +
      '</div>' + (opt.bad ? '<p class="cn-problem">' + opt.bad + '</p>' : '');
  }
  function addChip(name) { return '<span class="cn-chip add">' + MARK.plus + name + '</span>'; }
  function useRows(list) {
    return list.map(function (u) {
      return '<div class="cn-urow">' + letter(u[0], true) + '<span class="nm">' + u[0] + '</span><span class="min">' + u[1] + '</span></div>';
    }).join('');
  }

  /** state: 'view', 'problem' (Save found something wrong) or 'saved'. */
  function rules(state) {
    var problem = state === 'problem';
    var time = '<div class="cn-card">' + head2('calendar', 'Time a day') +
      '<div class="cn-chips cn-first"><span class="cn-chip">Every day</span><span class="cn-chip on">Each day on its own</span></div>' +
      '<div class="cn-days">' + WEEK.map(function (d) {
        return '<div class="cn-day"><span>' + d[0] + '</span>' + stepper(d[1]) + '</div>';
      }).join('') + '</div></div>';
    var bed = '<div class="cn-card">' + head2('moon', 'Bedtime') +
      chips([['every', 'Every day'], ['school', 'School nights only'], ['none', 'No bedtime']], 'every', 'cn-first') +
      '<div class="cn-times"><div>' + label('From') + field('<span class="val">9:00 pm</span>' + MARK.down) + '</div>' +
      '<div>' + label('To') + field('<span class="val">7:00 am</span>' + MARK.down) + '</div></div>' +
      '<div class="cn-err"></div></div>';
    var blocked = '<div class="cn-card cn-blocked">' + head2('block', 'Blocked') +
      label('On this PC', 'grp') + appRow('Game Launcher') +
      '<div class="cn-offer">' + label('Add from what Asha used today') +
        '<div class="cn-chips">' + addChip('Chrome') + addChip('Paint') + '</div></div>' +
      label('On Asha’s phone', 'grp') + appRow('Game Launcher') +
      (problem ? appRow('Maps', { bad: 'Maps is both always allowed and blocked. Choose one.' }) : '') +
      btn(MARK.plus + 'Add an app', 'mono wide cn-addapp') + '</div>';
    var allowed = '<div class="cn-card">' + head2('star', 'Always allowed') +
      '<p class="cn-detail">They never use up Asha’s time, and stay open when Asha’s devices are locked.</p>' +
      label('On this PC', 'grp') + appRow('Calculator') +
      label('On Asha’s phone', 'grp') + appRow('Phone', { fixed: 'Never blocked' }) + appRow('Messages') + appRow('Clock') + appRow('Maps') +
      '<p class="cn-small">The phone call app and emergency calls are never blocked.</p>' +
      btn(MARK.plus + 'Add an app', 'mono wide cn-addapp') + '</div>';
    var away = '<div class="cn-card">' + head2('away', 'Away from home') +
      '<p class="cn-detail">When Asha’s phone cannot reach this PC, it may use up to half of the day.</p>' +
      chips([['0', 'None'], ['25', 'A quarter'], ['50', 'Half'], ['100', 'All']], '50', 'cn-first') + '</div>';
    var use = '<div class="cn-card">' + head2('chart', 'Today’s use') +
      label('On this PC · 17 min', 'grp') + useRows([['Chrome', '11 min'], ['Paint', '6 min']]) +
      label('On Asha’s phone · 31 min', 'grp') + useRows([['Maps', '12 min'], ['Chrome', '9 min'], ['Gallery', '6 min'], ['Clock', '4 min']]) + '</div>';
    var more = '<div class="cn-card">' + head2('gear', 'More for Asha') +
      '<div class="cn-kvs">' + kv('A new day starts', 'At midnight', btn('Change', 'mono')) +
      kv('Name and age', 'Asha, 10 to 12', btn('Change', 'mono')) + '</div>' +
      btn('Remove Asha', 'coral wide cn-gap') + '<p class="cn-small">Asks for the password again.</p></div>';
    var who = chips([['a', 'Asha'], ['m', 'Mira']], 'a', 'cn-who');
    // The time rules first, then the apps, then today's use and the rest.
    var page = '<div class="cn-cols">' + time + bed + away + blocked + allowed + use + more + '</div>';
    if (problem) {
      return tabbed('rules', page, {
        right: who, noTabs: true, pinCls: 'pinsave', pinWide: true,
        pin: '<div class="cn-pinmsg">Not saved. One thing to fix.</div><div class="cn-pinrow">' + btn('Undo changes') + go('Save') + '</div>'
      });
    }
    return tabbed('rules', page, {
      right: who,
      over: state === 'saved' ? '<div class="cn-saved"><span class="cn-tile">' + icon('check', 32) + '</span>' +
        '<p>Saved. Asha’s devices have the new rules within a minute.</p></div>' : ''
    });
  }

  // ---------- D. Devices ----------

  var SIX = ['Notifications', 'Phone', 'Usage access', 'Accessibility', 'Battery', 'Protection'];
  var RING = stroke(18, 18, '<circle cx="9" cy="9" r="5.5"/>', 'ring', 2);

  /** The six steps of a phone's setup: a teal tick when on, a hollow ring
      and a word at the end when not. */
  function six(off, word) {
    return '<div class="cn-oks cn-six">' + SIX.map(function (s, i) {
      var isOff = off.indexOf(i) >= 0;
      return '<div class="cn-ok">' + (isOff ? RING : MARK.tick) + '<span>' + s + '</span>' +
        '<span class="cn-okst ' + (isOff ? word[1] : 'teal') + '">' + (isOff ? word[0] : 'On') + '</span></div>';
    }).join('') + '</div>';
  }
  function warn(title, text, after) {
    return '<div class="cn-warn cn-gap"><div class="cn-warn-top">' + icon('alert', 24) + '<p><b>' + title + '</b> ' + text + '</p></div>' + (after || '') + '</div>';
  }

  function phoneCard(trouble) {
    return '<div class="cn-card">' + head2('phone', 'Asha’s phone', btn('Rename', 'mono')) +
      '<div class="cn-kvs">' +
        kv('Child', 'Asha') + kv('Phone', 'Galaxy S20 Ultra, Android 13') + kv('Paired', '21 Nov') +
        (trouble ? kv('Last heard', '2 h ago') + kv('Where', 'Away from home')
          : kv('Last heard', 'Just now') + kv('Now', 'In use, at home', '', 'teal')) +
        kv('Protection', 'On', '', 'teal') + kv('LiMo on it', '0.1.0') +
      '</div>' +
      label('What LiMo needs on it', 'grp') + six(trouble ? [3] : [], ['Not on', 'coral']) +
      (trouble
        ? warn('USB debugging is still on.', 'While it is on, any PC with a cable could stop LiMo on this phone.',
            '<div class="cn-warn-act">' + btn('Switch it off', 'mono') + '</div>') +
          warn('Its clock is 9 min ahead of this PC’s.', 'LiMo keeps its own count, so no time is gained.') +
          '<div class="cn-update cn-gap">' + label('A newer LiMo') + '<p>This PC carries LiMo 0.2.0 for phones.</p>' +
            btn('Update LiMo on this phone', 'wide') + '</div>'
        : '') +
      btn('Remove this phone', 'coral wide cn-gap') +
      '<p class="cn-small">Asks for the password again. The phone is told at its next contact, lets go of its limits, and says LiMo can be removed.</p>' +
      '</div>';
  }

  function pcCard() {
    var accts = [
      ['Guardian', 'Administrator · signed in now', 'A guardian’s, no limit'],
      ['Asha', 'Standard', 'Asha’s'],
      ['Mira', 'Standard', 'Mira’s']
    ];
    return '<div class="cn-card">' + head2('pc', 'This PC') +
      label('Windows accounts', 'grp') +
      '<div class="cn-accs">' + accts.map(function (a) {
        return '<div class="cn-acc"><span class="cn-tile">' + icon('account', 32) + '</span>' +
          '<span class="who"><b>' + a[0] + '</b>' + label(a[1]) + '<span class="whose">' + a[2] + '</span></span>' +
          btn('Change', 'mono') + '</div>';
      }).join('') + '</div>' +
      label('Pause the count', 'grp') +
      '<div class="cn-stepline">' + stepper('3 min') + '</div>' +
      '<p class="cn-small">After this long with no keyboard or mouse input, unless something plays: in full screen, with sound, or keeping the screen awake.</p>' +
      label('An account LiMo was not told about', 'grp') +
      chips([['locked', 'Locked'], ['free', 'Used freely']], 'locked') +
      '<p class="cn-small">It stays locked until a guardian says whose it is. An administrator’s account is never locked.</p>' +
      '</div>';
  }

  /** trouble: the phone needs a guardian. */
  function devices(trouble) {
    return tabbed('devices',
      '<div class="cn-devs2"><div class="cn-stack ph">' + phoneCard(trouble) +
        btn(MARK.plus + 'Add a phone', 'wide') + '</div>' +
      '<div class="cn-stack pcs">' + pcCard() + '</div></div>');
  }

  // ---------- A. Add a phone ----------

  var ADD_STEPS = 9;

  // The cable's state, read once a second while these screens are open.
  var CABLE = {
    none: ['', 'No phone on a USB port.', 'If it is plugged in, the cable may only charge it. Try the one that came with the phone.'],
    off: ['', 'A phone is plugged in.', 'USB debugging is off on it.'],
    driver: ['coral', 'No working driver for this phone.', 'Install its maker’s USB driver for Windows, such as Samsung’s Android USB Driver.'],
    allow: ['', 'A phone is plugged in.', 'Waiting for Allow on the phone.'],
    ok: ['teal', 'Connected: Galaxy S20 Ultra.', 'Android 13.']
  };
  function foot(c) {
    var f = CABLE[c];
    return '<div class="cn-foot' + (f[0] ? ' ' + f[0] : '') + '"><span class="cn-tile">' + icon('usb', 32) + '</span>' +
      '<span><b>' + f[1] + '</b><i>' + f[2] + '</i></span></div>';
  }
  /** One step of Add a phone: its card, the cable's state at the foot, and
      the main button beside it (under it on a phone). */
  function addStep(at, iconName, title, body, button, cable, off) {
    return step(at, '<div class="cn-card">' + cardHead(iconName, title) + body + '</div>',
      foot(cable) + go(button, off), 1, '', { total: ADD_STEPS, pinCls: 'pinfoot' });
  }
  function ol(items) {
    return '<ol class="cn-ol">' + items.map(function (i) {
      return '<li>' + i[0] + (i[1] ? '<span class="sub">' + i[1] + '</span>' : '') + '</li>';
    }).join('') + '</ol>';
  }
  function oks(list) {
    return '<div class="cn-oks">' + list.map(function (m) { return '<div class="cn-ok">' + MARK.tick + '<span>' + m + '</span></div>'; }).join('') + '</div>';
  }

  function addPhone(at) {
    switch (at) {
      case 0:
        return addStep(0, 'phone', 'Add a phone',
          label('Whose phone', 'gap') + chips([['a', 'Asha'], ['m', 'Mira']], 'a') +
          label('You need', 'gap') +
          '<div class="cn-need-list">' +
            '<div class="cn-li"><span class="cn-tile">' + icon('phone', 32) + '</span><span><b>Asha’s phone</b></span></div>' +
            '<div class="cn-li"><span class="cn-tile">' + icon('usb', 32) + '</span><span><b>Its USB cable</b><i>One that moves files, ideally the one that came with it.</i></span></div>' +
            '<div class="cn-li"><span class="cn-tile">' + icon('clock', 32) + '</span><span><b>About ten minutes</b></span></div>' +
          '</div>',
          'Start', 'none');
      case 1:
        return addStep(1, 'usb', 'Switch on USB debugging',
          '<p class="cn-detail">Only a person can switch it on, on the phone itself. On most phones:</p>' +
          ol([
            ['Open Settings, then About phone.', 'On a Samsung phone, then Software information.'],
            ['Tap Build number seven times.', 'The phone may ask for its own PIN, then says that developer mode is on.'],
            ['Go back to Settings, open Developer options, and switch on USB debugging.', 'Developer options is near the end of Settings.'],
            ['Plug the phone into this PC.', 'This screen moves on by itself when it sees USB debugging.']
          ]) +
          '<div class="cn-note">' + label('On a newer Samsung phone') +
            '<p>Also switch off Auto Blocker, in Settings, Security and privacy. It blocks installs over the cable.</p></div>',
          'Next', 'off', true);
      case 2:
        return addStep(2, 'key', 'Allow this PC on the phone',
          '<p class="cn-detail">Unlock the phone. It asks: Allow USB debugging? Tick Always allow from this computer, then tap Allow.</p>' +
          label('The phone shows this PC’s key', 'gap') +
          '<div class="cn-finger">3C:91:5A:07:E2:4B:D8:16:<br>AF:62:0C:B9:75:18:4E:D3</div>' +
          '<p class="cn-small">It should read the same. If it does not, tap Cancel on the phone. The phone asks once; after that it knows this PC.</p>',
          'Next', 'allow', true);
      case 3:
        return addStep(3, 'install', 'Put LiMo on Asha’s phone',
          oks(['One phone plugged in', 'Android 13, recent enough for LiMo', 'LiMo is not on it yet']) +
          '<div class="cn-progress">' + label('Putting LiMo on Asha’s phone') +
            '<div class="cn-prog"><i style="width:62%"></i></div></div>' +
          '<p class="cn-small">Installing asks nothing on the phone.</p>',
          'Next', 'ok', true);
      case 4:
        return addStep(4, 'link', 'This phone is now Asha’s.',
          '<div class="cn-statusrow"><span class="cn-st teal">Paired</span><span>Over the cable, with this PC.</span></div>' +
          '<p class="cn-detail">LiMo opened on the phone by itself, with Asha’s name on it.</p>' +
          label('Its name', 'gap') + field('<span class="typed">Asha’s Galaxy S20 Ultra</span>' + CARET, 'at'),
          'Next', 'ok');
      case 5:
        return addStep(5, 'phone', 'Now set up LiMo on the phone',
          '<p class="cn-detail">It asks for six things. Each one ticks here when the phone says it is on.</p>' +
          six([3, 4, 5], ['Not yet', 'ink']) +
          '<p class="cn-small">This screen moves on by itself when the phone says it is set up.</p>',
          'Next', 'ok', true);
      case 6:
        return addStep(6, 'wifi', 'At home',
          '<p class="cn-detail">Checking that Asha’s phone can reach this PC on the home Wi-Fi.</p>' +
          '<div class="cn-statusrow"><span class="cn-st teal">Reached over the home Wi-Fi</span></div>',
          'Next', 'ok');
      case 7:
        return addStep(7, 'usb', 'Switch USB debugging off',
          '<p class="cn-detail">While it is on, any PC with a cable could stop LiMo on Asha’s phone. LiMo can switch it off now, over the cable.</p>' +
          btn('Leave it on', 'wide cn-gap') +
          '<p class="cn-small">Then Devices says that it is still on.</p>',
          'Switch it off for me', 'ok');
      default:
        return addStep(8, 'check', 'Asha’s phone is added.',
          oks(['LiMo is on the phone', 'Paired with this PC', 'Set up on the phone, all six on', 'Reached over the home Wi-Fi', 'USB debugging is off']) +
          '<p class="cn-detail">You can unplug the phone.</p>',
          'Done', 'off');
    }
  }
  /** Only the pinned strip of Add a phone, in one case of the cable. */
  function cableStrip(c) {
    return { pin: foot(c) + go('Next', c !== 'ok'), pinCls: 'pinfoot' };
  }

  // ---------- K. A one time code ----------

  function code(made) {
    var card = made
      ? '<div class="cn-card">' + cardHead('code', 'The code') +
          label('Asha’s phone · 30 min', 'gap') +
          '<div class="cn-code"><span>4718</span><span>2093</span></div>' +
          '<p class="cn-detail">Read it to Asha. It works once, today only, on Asha’s phone, when its time is used up.</p>' +
          '<p class="cn-small">You can make 9 more for this phone today.</p>' +
          '<div class="cn-pinrow cn-gap">' + btn('Make another') + go('Done') + '</div></div>'
      : '<div class="cn-card">' + cardHead('code', 'A one time code') +
          '<p class="cn-detail">Time for a child whose device is out of reach or locked, read out over a call.</p>' +
          label('For', 'gap') + chips([['a', 'Asha'], ['m', 'Mira']], 'a') +
          label('On', 'gap') + chips([['pc', 'This PC'], ['ph', 'Asha’s phone']], 'ph') +
          label('How much', 'gap') + chips([['15', '15 min'], ['30', '30 min'], ['60', '1 hour']], '30') +
          '<div class="cn-gap">' + btn('Make a code', 'go wide') + '</div>' +
          '<p class="cn-small">A guardian who goes out can note a code now and read it out later today.</p></div>';
    return tabbed('more', '<div class="cn-col">' + subHead('More') + card + '</div>');
  }

  // ---------- M. More ----------

  function more() {
    var rows = [
      ['code', 'A one time code', 'Time for a child out of reach, read out over a call.'],
      ['key', 'Password and PIN', 'Change either one.'],
      ['info', 'About LiMo', 'Its version, its licences, and removing it.']
    ];
    return tabbed('more', '<div class="cn-col cn-moretop"><div class="cn-card cn-list cn-menu">' + rows.map(function (r) {
      return '<div class="cn-li"><span class="cn-tile">' + icon(r[0], 32) + '</span><span><b>' + r[1] + '</b><i>' + r[2] + '</i></span>' + MARK.right + '</div>';
    }).join('') + '</div></div>');
  }

  function secrets() {
    return tabbed('more', '<div class="cn-col wide">' + subHead('More') + '<div class="cn-pair2">' +
      '<div class="cn-card">' + head2('key', 'The guardian password') +
        '<p class="cn-detail">' + PASSWORD_LINE + '</p>' +
        label('The one you use now', 'gap') + field(dots(10)) +
        label('A new one', 'gap', '8 or more characters') + field(dots(12)) +
        label('The same again', 'gap') + field(dots(4) + CARET, 'at') +
        '<div class="cn-err"></div>' + btn('Change the password', 'wide') + '</div>' +
      '<div class="cn-card">' + head2('pin', 'The guardian PIN') +
        '<p class="cn-detail">' + PIN_LINE + '</p>' +
        '<p class="cn-small">Choose one your child does not know.</p>' +
        label('A new PIN', 'gap', '6 to 8 digits') + field('') +
        label('The same again', 'gap') + field('') +
        '<div class="cn-err"></div>' + btn('Change the PIN', 'wide') + '</div>' +
      '</div></div>');
  }

  function about() {
    return tabbed('more', '<div class="cn-col">' + subHead('More') +
      '<div class="cn-card">' + head2('info', 'About LiMo') +
        '<div class="cn-kvs">' + kv('For Windows', '0.1.0') + kv('For phones', '0.1.0') + '</div>' +
        '<p class="cn-small">This PC carries LiMo for phones, and puts it on a phone over the cable.</p></div>' +
      '<div class="cn-card cn-gap">' + head2('list', 'Licences') +
        '<div class="cn-kvs">' + kv('LiMo', 'Apache License 2.0', btn('Read', 'mono')) +
        kv('Others', 'Third party notices', btn('Read', 'mono')) + '</div>' +
        '<p class="cn-detail">More info on <span class="cn-addr">https://lucidloom.ai</span></p></div>' +
      '<div class="cn-card cn-gap">' + head2('trash', 'Remove') +
        btn('Remove LiMo from this PC', 'wide cn-first') +
        '<p class="cn-small">Asks for the password, then opens Windows’ own uninstaller.</p>' +
        btn('Delete the family', 'coral wide cn-gap') +
        '<p class="cn-small">Asks for the password again.</p></div>' +
      '</div>', { end: MAKER });
  }

  function deleteFamily() {
    return tabbed('more', '<div class="cn-col">' + subHead('About LiMo') +
      '<div class="cn-card">' + cardHead('trash', 'Delete the family?') +
        '<p class="cn-detail">Everything LiMo keeps on this PC is deleted: Asha and Mira, their rules, their time and their devices. It cannot be undone.</p>' +
        '<p class="cn-small">Asha’s phone is told at its next contact, lets go of its limits, and says LiMo can be removed. LiMo stays on this PC and starts again from the first step.</p>' +
        label('Guardian password', 'gap') + field(CARET, 'at') +
        '<div class="cn-err"></div>' + btn('Delete the family', 'danger wide') + '</div></div>');
  }

  // ---------- Sheet three: guardians by name, and a guardian's own phone ----------

  var GUARDIAN = 'Devi';
  var GPHONE = 'Devi’s Pixel';
  var GMODEL = 'Google Pixel 8, Android 15';

  /** A list of things a phone can do, or where they stay: each its mark and its words. */
  function marks(list, iconName) {
    return '<div class="cn-oks">' + list.map(function (m) {
      return '<div class="cn-ok">' + (iconName ? '<span class="cn-okic">' + icon(iconName, 20) + '</span>' : MARK.tick) + '<span>' + m + '</span></div>';
    }).join('') + '</div>';
  }

  // W1. The first start gains a step after the password: the first
  // guardian's name. Nine steps now.
  function yourName() {
    return step(2, '<div class="cn-card">' + cardHead('guardian', 'Your name') +
      '<p class="cn-detail">As the family says it. LiMo shows it beside what is done from your phone, such as “+30 min from Devi’s phone”.</p>' +
      label('Name', 'gap') + field('<span class="typed">Devi</span>' + CARET, 'at') +
      '<div class="cn-err"></div>' +
      '<p class="cn-small">Another guardian can be added later, in More.</p></div>',
      go('Continue'), 1, '', { total: 9 });
  }

  // W1b. A family made before guardians had names: asked once, after sign in.
  function nameOnce() {
    return {
      page: '<div class="cn-wrap">' + top() + '<div class="cn-col cn-moretop"><div class="cn-card">' + cardHead('guardian', 'What is your name?') +
        '<p class="cn-detail">LiMo now keeps the family’s guardians by name. Yours shows beside what is done from your phone, such as “+30 min from Devi’s phone”.</p>' +
        label('Your name', 'gap') + field('<span class="typed">Devi</span>' + CARET, 'at') +
        '<div class="cn-err"></div>' +
        '<p class="cn-small">Later asks again the next time you sign in.</p></div></div></div>',
      pin: btn('Later') + go('Save'), pins: 2
    };
  }

  // W2a. More gains a row: Guardians.
  function moreGuardians() {
    var rows = [
      ['guardian', 'Guardians', 'Who looks after Asha and Mira, and their phones.'],
      ['code', 'A one time code', 'Time for a child out of reach, read out over a call.'],
      ['key', 'Password and PIN', 'Change either one.'],
      ['info', 'About LiMo', 'Its version, its licences, and removing it.']
    ];
    return tabbed('more', '<div class="cn-col cn-moretop"><div class="cn-card cn-list cn-menu">' + rows.map(function (r) {
      return '<div class="cn-li"><span class="cn-tile">' + icon(r[0], 32) + '</span><span><b>' + r[1] + '</b><i>' + r[2] + '</i></span>' + MARK.right + '</div>';
    }).join('') + '</div></div>');
  }

  // W2. Guardians: each guardian, their phones, and the two ways to add.
  function guardians() {
    return tabbed('more', '<div class="cn-col">' + subHead('More') +
      '<div class="cn-card">' + head2('family', 'Guardians') +
        '<p class="cn-detail">The people who look after Asha and Mira. They share the family’s one password and one PIN.</p>' +
        '<div class="cn-gds">' +
          '<div class="cn-acc"><span class="cn-tile">' + icon('guardian', 32) + '</span>' +
            '<span class="who"><b>' + GUARDIAN + '</b>' + label('A guardian · 1 phone') + '</span>' + btn('Rename', 'mono') + '</div>' +
          '<div class="cn-acc cn-sub"><span class="cn-tile">' + icon('guardianphone', 32) + '</span>' +
            '<span class="who"><b>' + GPHONE + '</b>' + label('Google Pixel 8') + '<span class="whose live">At home now</span></span>' +
            btn('See it', 'mono') + '</div>' +
        '</div>' +
        btn(MARK.plus + 'Add a guardian’s phone', 'wide cn-gap') +
        btn(MARK.plus + 'Add a guardian', 'wide cn-addapp') +
        '<p class="cn-small">A guardian’s phone answers requests and gives time behind its own screen lock. Rules ask the password there; the family’s settings stay on this PC.</p>' +
      '</div></div>');
  }

  // W3. Add a guardian: a name, and what a guardian is in LiMo.
  function addGuardian() {
    return tabbed('more', '<div class="cn-col">' + subHead('Guardians') +
      '<div class="cn-card">' + cardHead('guardian', 'Add a guardian') +
        '<p class="cn-detail">Someone else who looks after Asha and Mira, such as another parent or a grandparent.</p>' +
        label('Name', 'gap') + field(CARET, 'at') +
        '<div class="cn-two">' +
          '<div><span class="cn-tile">' + icon('key', 32) + '</span><p><b>One password</b>The family has one. Every guardian types it to open these settings on this PC.</p></div>' +
          '<div><span class="cn-tile">' + icon('pin', 32) + '</span><p><b>One PIN</b>The family has one. Every guardian types it on a child’s locked PC or phone.</p></div>' +
        '</div>' +
        '<p class="cn-small">Tell the new guardian both, or add their phone next: it answers requests without either.</p>' +
        '<div class="cn-gap">' + btn('Add the guardian', 'go wide') + '</div></div></div>');
  }

  // W4 to W8. Add a guardian's phone: over the cable, as a child's phone is
  // added, in Add a phone's nine steps, so a guardian who added a child's
  // phone knows it already. Only the steps that differ are drawn: the start,
  // putting LiMo Guardian on the phone, the phone's own part, and Done. The
  // others are Add a phone's A2, A3, A5, A7 and A8, with Devi's name and
  // phone in place of Asha's.

  // The cable's state with Devi's phone connected.
  CABLE.gok = ['teal', 'Connected: Pixel 8.', 'Android 15.'];

  // W4. The start: whose phone, what is needed, and what goes on it.
  function gStart() {
    return addStep(0, 'guardianphone', 'Add a guardian’s phone',
      label('Whose phone', 'gap') +
      '<div class="cn-chips"><span class="cn-chip on">' + GUARDIAN + '</span><span class="cn-chip add">' + MARK.plus + 'Another guardian</span></div>' +
      '<p class="cn-small">This puts LiMo Guardian on it: LiMo for a guardian, not the one for a child.</p>' +
      label('You need', 'gap') +
      '<div class="cn-need-list">' +
        '<div class="cn-li"><span class="cn-tile">' + icon('guardianphone', 32) + '</span><span><b>Devi’s own Android phone</b></span></div>' +
        '<div class="cn-li"><span class="cn-tile">' + icon('usb', 32) + '</span><span><b>Its USB cable</b><i>One that moves files, ideally the one that came with it.</i></span></div>' +
        '<div class="cn-li"><span class="cn-tile">' + icon('clock', 32) + '</span><span><b>About ten minutes</b></span></div>' +
      '</div>',
      'Start', 'none');
  }

  // W5. Step 4: LiMo Guardian goes on the phone, as A4.
  function gInstall() {
    return addStep(3, 'install', 'Put LiMo Guardian on Devi’s phone',
      oks(['One phone plugged in', 'Android 15, recent enough for LiMo', 'LiMo Guardian is not on it yet']) +
      '<div class="cn-progress">' + label('Putting LiMo Guardian on Devi’s phone') +
        '<div class="cn-prog"><i style="width:62%"></i></div></div>' +
      '<p class="cn-small">Installing asks nothing on the phone.</p>',
      'Next', 'gok', true);
  }

  // W6. Step 6: the phone's own part, done on the phone, ticking as A6 does.
  function gOwn() {
    var rows = [['Its own screen lock', 'Set', true], ['Tell me of requests at home', 'Not yet', false]];
    return addStep(5, 'guardianphone', 'Now set up LiMo Guardian on the phone',
      '<p class="cn-detail">It asks for two things. Each one ticks here when the phone says it is done.</p>' +
      '<div class="cn-oks cn-six">' + rows.map(function (r) {
        return '<div class="cn-ok">' + (r[2] ? MARK.tick : RING) + '<span>' + r[0] + '</span>' +
          '<span class="cn-okst ' + (r[2] ? 'teal' : 'ink') + '">' + r[1] + '</span></div>';
      }).join('') + '</div>' +
      '<p class="cn-small">LiMo Guardian opens behind the phone’s own screen lock, so the phone needs one. Tell me of requests at home is Devi’s choice: on, a request reaches the phone within a minute at home; off, only while LiMo Guardian is open.</p>' +
      '<p class="cn-small">This screen moves on by itself when the phone says it is set up.</p>',
      'Next', 'gok', true);
  }

  // W8. Step 9: done, what the phone can do, and what stays on this PC.
  function gDone() {
    return addStep(8, 'check', 'Devi’s phone is a guardian’s phone now.',
      label('On the phone, behind its own screen lock', 'gap') +
      marks(['See Today, Devices and requests', 'Answer a request, give time, lock and open', 'Make a one time code, at home or away']) +
      label('On the phone, with the password', 'gap') +
      marks(['Change a child’s rules', 'Rename a child or a device']) +
      label('Only on this PC', 'gap') +
      marks(['The password and the PIN', 'Adding or removing a child, a device or a guardian', 'Windows accounts and this PC’s settings', 'Removing LiMo'], 'pc') +
      '<p class="cn-detail">You can unplug the phone.</p>',
      'Done', 'off');
  }

  // W9. Devices: a guardian's phone, how it stands, and removing it.
  function gPhoneCard() {
    return '<div class="cn-card">' + head2('guardianphone', GPHONE, btn('Rename', 'mono')) +
      '<div class="cn-kvs">' +
        kv('Guardian', GUARDIAN) + kv('Phone', GMODEL) + kv('Paired', '21 Nov') + kv('Last seen', '2 h ago') +
        kv('LiMo Guardian', '0.1.0') + kv('Codes kept', '3 a day for each child’s device') +
      '</div>' +
      '<p class="cn-small">It answers requests and gives time behind its own screen lock. Rules ask the password. The family’s settings stay on this PC.</p>' +
      btn('Remove this phone', 'coral wide cn-gap') +
      '<p class="cn-small">Asks for the password again. The phone is refused at its next contact and forgets the family; the codes it kept stop working.</p>' +
      '</div>';
  }
  function devicesGuardian() {
    return tabbed('devices',
      '<div class="cn-devs2"><div class="cn-stack ph">' + gPhoneCard() + phoneCard(false) +
        btn(MARK.plus + 'Add a phone', 'wide') + '</div>' +
      '<div class="cn-stack pcs">' + pcCard() + '</div></div>');
  }

  // W10. Today says who did what from a phone.
  function todayWho() {
    var kids = [
      { name: 'Asha', left: '1 h 42 min', pct: 32, used: '48 min used of 2 h 30 min', who: '+30 min from Devi’s phone<span class="nw"> · 6:02 pm</span>',
        devices: ASHA.devices },
      { name: 'Mira', left: '30 min', pct: 67, used: '1 h used of 1 h 30 min',
        devices: [{ icon: PC, name: 'This PC', st: 'In use', live: true, min: '1 h' }] }
    ];
    var aside = label('Answered', 'cn-sec') +
      '<div class="cn-card cn-req"><div class="cn-req-top"><span class="cn-tile lg">' + icon('hand', 32) + '</span>' +
        '<span><b>Mira asked for 30 min</b>' + label('On this PC · 6:04 pm') + '</span></div>' +
        '<p class="cn-quote">“To finish my science project”</p>' +
        '<div class="cn-statusrow"><span class="cn-st teal">Given 30 min</span><span>Answered from Devi’s phone · 6:05 pm</span></div></div>' +
      '<div class="cn-quiet"><b>Nothing else is waiting.</b></div>';
    return {
      page: '<div class="cn-wrap tabbed">' + top(label(DATE)) +
        '<div class="cn-today"><div class="cn-aside">' + aside + '</div>' +
        '<div class="cn-kids">' + kids.map(kid).join('') + '</div></div></div>',
      tabs: tabs('today')
    };
  }

  // W11. Add a phone, step 1: says where a guardian's own phone is added.
  function addPhoneWhose() {
    return addStep(0, 'phone', 'Add a phone',
      label('Whose phone', 'gap') + chips([['a', 'Asha'], ['m', 'Mira']], 'a') +
      '<p class="cn-small">This adds a child’s phone. For your own, use Add a guardian’s phone in More.</p>' +
      label('You need', 'gap') +
      '<div class="cn-need-list">' +
        '<div class="cn-li"><span class="cn-tile">' + icon('phone', 32) + '</span><span><b>Asha’s phone</b></span></div>' +
        '<div class="cn-li"><span class="cn-tile">' + icon('usb', 32) + '</span><span><b>Its USB cable</b><i>One that moves files, ideally the one that came with it.</i></span></div>' +
        '<div class="cn-li"><span class="cn-tile">' + icon('clock', 32) + '</span><span><b>About ten minutes</b></span></div>' +
      '</div>',
      'Start', 'none');
  }

  // W12. A one time code, and the codes left: here, and kept for a guardian's phone.
  function codeLeft() {
    var card = '<div class="cn-card">' + cardHead('code', 'A one time code') +
      '<p class="cn-detail">Time for a child whose device is out of reach or locked, read out over a call.</p>' +
      label('For', 'gap') + chips([['a', 'Asha'], ['m', 'Mira']], 'a') +
      label('On', 'gap') + chips([['pc', 'This PC'], ['ph', 'Asha’s phone']], 'ph') +
      '<p class="cn-small">For Asha’s phone today: 7 codes left here, and 3 kept for Devi’s phone.</p>' +
      label('How much', 'gap') + chips([['15', '15 min'], ['30', '30 min'], ['60', '1 hour']], '30') +
      '<div class="cn-gap">' + btn('Make a code', 'go wide') + '</div>' +
      '<p class="cn-small">A guardian who goes out can note a code now and read it out later today, or make one on their own phone.</p></div>';
    return tabbed('more', '<div class="cn-col">' + subHead('More') + card + '</div>');
  }

  root.LIMO_CONSOLE = {
    frame: frame, strip: strip,
    signIn: signIn,
    welcome: welcome, password: password, pin: pin, child: child, timeADay: timeADay,
    bedtime: bedtime, accounts: accounts, done: done,
    today: today,
    rules: rules, devices: devices, addPhone: addPhone, cableStrip: cableStrip,
    code: code, more: more, secrets: secrets, about: about, deleteFamily: deleteFamily,
    yourName: yourName, nameOnce: nameOnce, moreGuardians: moreGuardians, guardians: guardians, addGuardian: addGuardian,
    gStart: gStart, gInstall: gInstall, gOwn: gOwn, gDone: gDone, devicesGuardian: devicesGuardian,
    todayWho: todayWho, addPhoneWhose: addPhoneWhose, codeLeft: codeLeft
  };
}(typeof window !== 'undefined' ? window : this));
