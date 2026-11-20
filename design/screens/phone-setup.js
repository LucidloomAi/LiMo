/* LiMo on the phone: the setup screens, the child's own introduction, the
   How it works tab, and Today when something is switched off, as functions
   that return HTML. Drawings to agree the look, not the product.

   Needs phone.js loaded first: it draws the phone itself (frame, crop). */
(function (root) {
  'use strict';

  var P = root.LIMO_PHONE;
  var CHILD = 'Mira';

  function icon(name, size) {
    return '<img src="../icons/svg/' + name + '.svg" width="' + size + '" height="' + size + '" alt="">';
  }

  // Small marks beside text are plain ink strokes, never icons.
  function stroke(w, h, d, cls) {
    return '<svg class="mk' + (cls ? ' ' + cls : '') + '" width="' + w + '" height="' + h + '" viewBox="0 0 ' + w + ' ' + h +
      '" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">' + d + '</svg>';
  }
  var TICK = stroke(18, 18, '<path d="M3.5 9.5l3.6 3.6L14.5 5"/>', 'su-tickmark');

  function label(text, cls) {
    return '<div class="ph-label' + (cls ? ' ' + cls : '') + '">' + text + '</div>';
  }

  /** The top row: LiMo's own mark first, and what belongs at the right. */
  function top(right) {
    return '<div class="ph-top"><div class="ph-mark">' + icon('hourglass', 24) + '<span>LiMo</span></div>' + (right || '') + '</div>';
  }

  /** A counter: done ones teal, this one marigold, later ones outlined. */
  function dots(count, at) {
    var out = '';
    for (var i = 0; i < count; i += 1) {
      out += '<span class="su-dot' + (i < at ? ' done' : i === at ? ' at' : '') + '"></span>';
    }
    return '<span class="su-dots" aria-label="' + (at + 1) + ' of ' + count + '">' + out + '</span>';
  }
  function stepCount(at) {
    return '<div class="su-count">' + label('Step ' + (at + 1) + ' of ' + STEPS.length) + dots(STEPS.length, at) + '</div>';
  }

  // The maker's mark: said once, small, at the end of a screen that fills
  // the display, after LiMo's own name at the top.
  var MAKER = '<div class="ph-maker" aria-label="Made by LucidLoom AI"><span>Made by</span>' +
    '<img src="../../docs/images/lucidloom-logo.png" alt="" width="22" height="22"><b>LucidLoom AI</b></div>';
  function tail() { return '<div class="ph-tail">' + MAKER + '</div>'; }

  /** A setup screen: the page, and its main button pinned to the bottom. */
  function screen(page, button, off) {
    return {
      page: '<div class="ph-lock">' + page + '</div>',
      pin: '<div class="ph-btn go' + (off ? ' off' : '') + '">' + button + '</div>'
    };
  }

  /** A card that opens a screen: the icon, then the words. */
  function card(iconName, eyebrow, heading, text, more) {
    return '<div class="ph-card su-card">' + icon(iconName, 64) +
      (eyebrow ? label(eyebrow, 'ph-eyebrow') : '') +
      '<h1 class="ph-h1">' + heading + '</h1>' +
      (text ? '<p class="ph-detail">' + text + '</p>' : '') + (more || '') + '</div>';
  }

  // ---------- S0. Not set up ----------

  function notSetUp() {
    return '<div class="ph-lock">' + top() +
      card('usb', '', 'This phone is not set up yet.',
        'Connect it to the family PC with a USB cable, then choose <strong class="su-name">Add a phone</strong> in LiMo on the PC.') +
      tail() + '</div>';
  }

  // ---------- S1. Welcome ----------

  function welcome() {
    return screen(top() +
      card('phone', 'For a guardian', 'This phone is now ' + CHILD + '’s.',
        'Six short steps finish the setup. Each one asks Android for one thing, and says why.'),
      'Start');
  }

  // ---------- S2. What LiMo does on this phone ----------

  var DOES = [
    ['hourglass', 'Counts time', 'How long each app is open, while the screen is on.'],
    ['lock', 'Covers and locks', 'A blocked app is covered. The phone locks when today’s time is used up, or at bedtime.'],
    ['eyeoff', 'Never reads', 'Messages, web addresses or anything typed. It knows which app is open, never what is in it.']
  ];
  function rows(list) {
    return '<div class="ph-card su-list">' + list.map(function (r) {
      return '<div class="su-li"><span class="ph-tile">' + icon(r[0], 32) + '</span><span><b>' + r[1] + '</b><i>' + r[2] + '</i></span></div>';
    }).join('') + '</div>';
  }
  function tickRow(on) {
    return '<div class="su-tick"><span class="su-box' + (on ? ' on' : '') + '">' + (on ? TICK : '') + '</span>' +
      '<span>I am ' + CHILD + '’s guardian, and I agree.</span></div>';
  }
  function agree() {
    return screen(top() +
      '<div class="su-head">' + label('Before you start') + '<h1 class="ph-h1">What LiMo does here</h1></div>' +
      rows(DOES) +
      '<p class="su-line">Nothing leaves your home. This phone talks only to your family PC.</p>' +
      tickRow(false),
      'Continue', true);
  }
  /** Only the tick box, ticked, and the button switched on. */
  function agreed() {
    return tickRow(true) + '<div class="su-pinned"><div class="ph-btn go">Continue</div></div>';
  }

  // ---------- S3 to S8. One permission each ----------

  var STEPS = [
    { icon: 'bell', title: 'Notifications',
      why: 'So ' + CHILD + ' sees the time left, and a warning before the phone locks.',
      what: 'Android asks once. Choose <strong class="su-name">Allow</strong>.', button: 'Allow notifications' },
    // Only to know that a call is ringing or on, so a lock never hides it.
    { icon: 'call', title: 'Phone',
      why: 'So a lock never hides a call. LiMo is told that a call is ringing or on, never who is calling.',
      what: 'Android asks once. Choose <strong class="su-name">Allow</strong>.', button: 'Allow',
      note: ['What Android asks', 'Android’s words are “make and manage phone calls”. LiMo never makes a call. It only learns that one is ringing or on.'] },
    { icon: 'chart', title: 'Usage access',
      why: 'So LiMo can count the time in each app. It gets app names and minutes from Android, nothing else.',
      what: 'Android opens its own settings. Find LiMo in the list and switch it on.', button: 'Open Android’s settings' },
    { icon: 'shield', title: 'Accessibility',
      why: 'So LiMo can see which app is in front, and cover it when it is blocked or the time is used up. It is told the app’s name only, never what is on the screen.',
      what: 'Android opens its own settings and shows a warning of its own. Find LiMo under Installed apps and switch it on.', button: 'Open Android’s settings' },
    { icon: 'battery', title: 'Battery',
      why: 'So Android does not put LiMo to sleep to save battery.',
      what: 'Android asks once. Choose <strong class="su-name">Allow</strong>.', button: 'Allow',
      note: ['On a Samsung phone', 'Also open Settings, then Battery, then Background usage limits, then Never sleeping apps, and add LiMo there.'] },
    { icon: 'key', title: 'Protection',
      why: 'So LiMo cannot be switched off or removed without the guardian PIN.',
      what: 'Android asks you to make LiMo a device admin app. Choose <strong class="su-name">Activate</strong>.', button: 'Turn on protection' }
  ];

  /** Step `at` (0 to 5), not yet on, or done. With stuck, Android's screen
      has been opened twice and the step is still off, so the guardian may go
      on without it; the main button stays, to try again. */
  function step(at, done, stuck) {
    var s = STEPS[at];
    var line = done ? 'Android says it is on.' : stuck ? 'Android has not turned this on. You can go on and come back to it from Today.' : '';
    return screen(top(stepCount(at)) +
      '<div class="ph-card su-card">' + icon(s.icon, 64) +
        '<h1 class="ph-h1">' + s.title + '</h1>' +
        label('Why', 'su-gap') + '<p class="ph-detail">' + s.why + '</p>' +
        '<div class="su-what">' + label('What happens') + '<p class="ph-detail">' + s.what + '</p>' +
          (s.note ? '<div class="su-note">' + label(s.note[0]) + '<p>' + s.note[1] + '</p></div>' : '') +
        '</div>' +
        // The line under the stamp has its room kept, so nothing moves.
        '<div class="su-state">' + (done ? '<span class="ph-st teal">On</span>' : '<span class="ph-st coral">Not on yet</span>') +
          '<p>' + line + '</p></div>' +
        // The way on without the step comes under the line, where nothing else is.
        (stuck && !done ? '<div class="ph-btn su-goon">Go on without it</div>' : '') +
      '</div>',
      done ? 'Next' : s.button);
  }

  // ---------- S9. Done ----------

  function done() {
    var list = STEPS.map(function (s) { return '<div class="su-ok">' + TICK + '<span>' + s.title + '</span></div>'; }).join('');
    return screen(top() +
      '<div class="ph-card su-card">' + icon('check', 64) +
        '<h1 class="ph-h1">This phone is set up.</h1>' +
        '<div class="su-oks">' + list + '</div>' +
        // The app knows the child's name, not how the child is spoken of,
        // so the line says the name again instead of "her" or "him".
        '<p class="ph-detail su-after">Hand the phone to ' + CHILD + '. The next three screens are ' + CHILD + '’s.</p>' +
      '</div>' + tail(),
      'Show ' + CHILD + ' how it works');
  }

  // ---------- C1 to C3. The child's own introduction ----------

  var INTRO = [
    ['hourglass', 'For ' + CHILD, 'What LiMo counts', 'The time your screen is on with an app open. Calls, the home screen and LiMo itself do not count.', 'Next'],
    ['eyeoff', '', 'What LiMo never looks at', 'Your messages, what you type, and the pages you visit. It only knows which app is open, and for how long.', 'Next'],
    ['hand', '', 'How to ask for more time', 'When the time is used up, tap <strong class="su-name">Ask for more time</strong>. A guardian answers. A guardian who is away can read you a code to type.', 'Got it']
  ];
  function intro(at) {
    var c = INTRO[at];
    return screen(top('<div class="su-count three">' + dots(3, at) + '</div>') + card(c[0], c[1], c[2], c[3]), c[4]);
  }

  // ---------- The child's app: the floating glass tab bar ----------

  function tabs(picked) {
    return '<nav class="ph-tabs">' +
      '<span class="ph-tab' + (picked === 'today' ? ' on' : '') + '">' + icon('sun', 24) + 'Today</span>' +
      '<span class="ph-tab' + (picked === 'how' ? ' on' : '') + '">' + icon('help', 24) + 'How it works</span></nav>';
  }
  var WHO = '<span class="ph-who">' + CHILD + '</span>';

  // ---------- H. How it works ----------

  var RULES = [['Time a day', '2 h'], ['Screens off', '9:00 pm to 7:00 am'], ['Always open', 'Phone, Messages, Clock, Maps'], ['Not allowed', 'Game Launcher']];

  function howItWorks() {
    return '<div class="ph-lock ph-today">' + top(WHO) +
      label('Your rules', 'ph-sec') +
      '<div class="ph-card su-rules">' + RULES.map(function (r) {
        return '<div class="su-rule">' + label(r[0]) + '<span class="v">' + r[1] + '</span></div>';
      }).join('') + '</div>' +
      label('What LiMo does', 'ph-sec') +
      rows(INTRO.map(function (c) { return [c[0], c[2], c[3]]; })) +
      label('About', 'ph-sec') +
      '<div class="ph-card su-about"><b>LiMo 0.1.0</b>' +
        '<p>Free and open source, under the Apache 2.0 licence.</p>' +
        '<div class="su-repo">github.com/LucidloomAi/LiMo</div></div>' +
      tail() + '</div>';
  }

  // ---------- T. Today when something is switched off ----------

  // Today's markup is the first sheet's (phone.js), with a banner above the
  // time left.
  var USED = [['Maps', 12], ['Chrome', 9], ['Gallery', 6], ['Clock', 4]];

  function todayOff() {
    return '<div class="ph-lock ph-today">' + top(WHO) +
      '<div class="ph-card su-banner">' +
        '<div class="ph-why-top"><span class="ph-tile lg">' + icon('alert', 32) + '</span><span class="ph-st coral">Needs a guardian</span></div>' +
        '<p>Accessibility is switched off, so LiMo cannot see which app is open.</p>' +
        '<div class="ph-btn">Show a guardian what to do</div>' +
      '</div>' +
      '<div class="ph-card">' +
        '<b class="ph-big">1 h 12 min</b>' + label('Left today', 'ph-big-label') +
        '<p class="ph-quiet">Looking at this screen does not use your time.</p>' +
        '<div class="ph-used"><div class="ph-bar"><i style="width:40%"></i></div>' +
        '<p>48 min used of 2 h</p>' + label('On this phone 31 min · on the PC 17 min') + '</div>' +
      '</div>' +
      '<div class="ph-card ph-hours"><span class="ph-tile lg">' + icon('moon', 32) + '</span>' +
        '<p>Screens are off from 9:00 pm to 7:00 am.</p></div>' +
      '<div class="ph-btn">Ask for more time</div>' +
      label('Today on this phone', 'ph-sec') +
      '<div class="ph-card ph-list">' + USED.map(function (u) {
        return '<div class="ph-li"><span class="ph-letter sm">' + u[0].charAt(0) + '</span><span>' + u[0] + '</span><span class="min">' + u[1] + ' min</span></div>';
      }).join('') + '</div>' +
      '</div>';
  }

  root.LIMO_SETUP = {
    frame: P.frame, crop: P.crop,
    notSetUp: notSetUp, welcome: welcome, agree: agree, agreed: agreed,
    step: step, done: done, intro: intro, howItWorks: howItWorks, todayOff: todayOff, tabs: tabs
  };
}(typeof window !== 'undefined' ? window : this));
