// The console's self check: the page drives its own screen as a guardian
// would and tells the console's Rust side what it drew, so the whole way in
// is checked without anyone at the PC (`limo-console.exe --self-check`,
// console/README.md, "The self check").
//
// It runs only when both are true:
// 1. the Rust side put `__LIMO_SELF_CHECK__` on the page, which it does only
//    when started with --self-check, and
// 2. the Rust side answers the self check's "state" with active true. Without
//    --self-check it refuses every self check request.
// In a browser or on a phone, step 1 is already false.
//
// It reads the screen from the DOM (react-native-web turns testID into
// data-testid), types with the input's own value setter and an input event,
// and presses with a DOM click: the same handlers, the same host.request and
// the same Rust command a guardian's typing and click reach.

import { host } from '../host';
import { pageGuardOn } from './pageGuard';

type State = {
  active: boolean;
  reported: string[];
  password: string;
  probe: 'none' | 'waiting' | 'done';
};

const moduleMs = Math.round(nowMs());
const cspViolations: string[] = [];
const seen = { beforeprint: 0, contextmenu: 0, keydown: [] as string[] };

if (typeof document !== 'undefined') {
  document.addEventListener('securitypolicyviolation', (e) => {
    cspViolations.push(`${e.violatedDirective} ${e.blockedURI || '(inline)'}`);
  });
  // What the key probe could cause, as the page sees it: the print dialog
  // (beforeprint), a right click reaching the page, and each key that
  // reached it. Listened for in the capture phase, ahead of the guard.
  // The first two are also said at once, for a control run, where F5 is
  // meant to reload the page before it can report.
  window.addEventListener('beforeprint', () => {
    seen.beforeprint += 1;
    sayAtOnce('beforeprint: the print dialog is opening');
  });
  document.addEventListener('contextmenu', () => {
    seen.contextmenu += 1;
    sayAtOnce('contextmenu: a right click reached the page');
  }, true);
  document.addEventListener('keydown', (e) => {
    seen.keydown.push(`${e.ctrlKey ? 'Ctrl+' : ''}${e.shiftKey ? 'Shift+' : ''}${e.key}`);
  }, true);
}

let started = false;

function sayAtOnce(what: string): void {
  if (!(globalThis as Record<string, unknown>).__LIMO_SELF_CHECK__ || host.place() !== 'console') return;
  report('saw', what).catch(() => undefined);
}

export function startSelfCheckIfAsked(): void {
  if (started) return;
  started = true;
  if (host.place() !== 'console') return;
  if (!(globalThis as Record<string, unknown>).__LIMO_SELF_CHECK__) return;
  run().catch((e: unknown) => {
    report('error', String(e)).catch(() => undefined);
  });
}

function nowMs(): number {
  return typeof performance !== 'undefined' ? performance.now() : 0;
}

async function ask(args: Parameters<typeof host.request<'console.selfCheck'>>[1]): Promise<unknown> {
  return host.request('console.selfCheck', args);
}

function report(key: string, text: string): Promise<unknown> {
  return ask({ action: 'report', key, text: `${text} | page_ms=${Math.round(nowMs())}` });
}

function el(testId: string): HTMLElement | null {
  return document.querySelector<HTMLElement>(`[data-testid="${testId}"]`);
}

function text(testId: string): string {
  return el(testId)?.textContent ?? '';
}

function waitFor<T>(what: string, check: () => T | null | undefined | false, ms = 8000): Promise<T> {
  const until = nowMs() + ms;
  return new Promise((resolve, reject) => {
    const tick = () => {
      const v = check();
      if (v) resolve(v);
      else if (nowMs() > until) reject(new Error(`timed out waiting for ${what}`));
      else setTimeout(tick, 10);
    };
    tick();
  });
}

/** Types into a react-native-web TextInput as a person would: its value, then an input event. */
function type(testId: string, value: string): void {
  const input = el(testId) as HTMLInputElement | null;
  if (!input) throw new Error(`no ${testId} to type into`);
  const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')?.set;
  setter?.call(input, value);
  input.dispatchEvent(new Event('input', { bubbles: true }));
}

function fonts(): string {
  const f = document.fonts;
  const check = (family: string) => (f && f.check(`16px "${family}"`) ? 'loaded' : 'NOT loaded');
  return `fonts: body ${check('Instrument Sans')}, mono ${check('Spline Sans Mono')}, display ${check('Fraunces 28')}`;
}

/** What the browser really drew the sign in with: proof that the styles applied. */
function drawnWith(): string {
  const heading = document.querySelector<HTMLElement>('[data-testid="sign-in"] [role="heading"]');
  const field = el('password');
  const button = el('open');
  const look = (e: HTMLElement | null, props: string[]) => {
    if (!e) return 'missing';
    const s = getComputedStyle(e);
    return props.map((p) => `${p}: ${s.getPropertyValue(p)}`).join(', ');
  };
  return (
    `heading {${look(heading, ['font-family', 'font-size'])}} ` +
    `field {${look(field, ['height', 'border-top-width', 'background-color'])}} ` +
    `button {${look(button?.firstElementChild as HTMLElement | null, ['background-color', 'min-height'])}} ` +
    `window ${window.innerWidth} by ${window.innerHeight}`
  );
}

function csp(): string {
  return cspViolations.length === 0 ? 'csp_violations=0' : `csp_violations=${cspViolations.length} [${cspViolations.join('; ')}]`;
}

async function state(): Promise<State> {
  return (await ask({ action: 'state' })) as State;
}

async function run(): Promise<void> {
  const s = await state();
  if (!s.active) return;
  const done = new Set(s.reported);

  if (!done.has('signed_in')) {
    await report('js_start', `href=${location.href} bundle_ran_at_page_ms=${moduleMs} page_guard=${pageGuardOn() ? 'on' : 'off'}`);
    await waitFor('the sign in form, after the door answered about', () => el('password'));
    if (document.fonts) await document.fonts.ready;
    const nav = performance.getEntriesByType('navigation')[0] as PerformanceNavigationTiming | undefined;
    await report(
      'sign_in_drawn',
      `field and button on screen; ${fonts()}; ${drawnWith()}; dom_content_loaded_page_ms=${nav ? Math.round(nav.domContentLoadedEventEnd) : 'n/a'} ${csp()}`,
    );

    type('password', 'not the password');
    let t0 = nowMs();
    el('open')?.click();
    const wrong = await waitFor('the wrong password line', () => (text('message').startsWith('That password') ? text('message') : null));
    await report('wrong_password', `message="${wrong}" field_after="${(el('password') as HTMLInputElement | null)?.value ?? '?'}" click_to_screen_ms=${Math.round(nowMs() - t0)}`);

    type('password', s.password);
    t0 = nowMs();
    el('open')?.click();
    await waitFor('the signed in card', () => el('signed-in'));
    await report('signed_in', `card="${text('signed-in')}" click_to_screen_ms=${Math.round(nowMs() - t0)}`);

    // The key probe: the Rust side sends the keys and the right click to
    // WebView2's own window, and says when it has.
    await report('ready_for_keys', 'the page listens for what the keys could cause');
    for (let i = 0; i < 800; i += 1) {
      const now = await state();
      if (now.probe === 'done' || now.probe === 'none') break;
      await new Promise((r) => setTimeout(r, 25));
    }
    await report(
      'keys',
      `beforeprint=${seen.beforeprint} contextmenu_reached_page=${seen.contextmenu} keys_reached_page=[${seen.keydown.join(' ')}] still_signed_in=${el('signed-in') ? 'yes' : 'no'}`,
    );

    await ask({ action: 'use', door: 'child' });
    await report('reload_requested', 'to the door that marks this account as a child’s');
    location.reload();
    return;
  }

  if (!done.has('child_account')) {
    await waitFor('the refusal for a child’s account', () => el('refusal'));
    await report('child_account', `refusal="${text('refusal')}" password_field_shown=${el('password') ? 'yes' : 'no'}`);
    await ask({ action: 'use', door: 'impostor' });
    await report('reload_requested', 'to the door checked by the rule for LiMo’s service');
    location.reload();
    return;
  }

  if (!done.has('not_the_service')) {
    await waitFor('the words for a door that is not LiMo’s service', () => (text('message').includes('not LiMo') ? text('message') : null));
    await report('not_the_service', `message="${text('message')}" button_off=${el('open')?.getAttribute('aria-disabled') ?? '?'} ${csp()}`);
  }
}
