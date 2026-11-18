// Self check: the page tells the Rust side what it drew, so the proof can be
// checked without anyone watching (`limo-console-proof.exe --self-check <log>`).
//
// It runs only when both are true:
// 1. the Rust side put `__LIMO_SELF_CHECK__` on the page, which it does only
//    when it was started with --self-check, and
// 2. the Rust side answers `self_check_state` with active=true. Outside self
//    check mode Rust does not grant the window the self check commands at all,
//    so the call is refused and nothing here runs.
// In a browser or on a phone, step 1 is already false.
//
// It reads what is on screen from the DOM (React Native Web turns testID into
// data-testid) and presses buttons with a DOM click, which goes through the
// same onPress handlers, host.request and Rust command a person's click does.

import { host } from './host';

type State = { active: boolean; reported: string[] };

const moduleMs = Math.round(nowMs());
const cspViolations: string[] = [];

if (typeof document !== 'undefined') {
  document.addEventListener('securitypolicyviolation', (e) => {
    cspViolations.push(`${e.violatedDirective} ${e.blockedURI || '(inline)'}`);
  });
}

let started = false;

export function startSelfCheckIfAsked(): void {
  if (started) return;
  started = true;
  if (host.where() !== 'tauri') return;
  if (!(globalThis as Record<string, unknown>).__LIMO_SELF_CHECK__) return;
  run().catch((e: unknown) => {
    // Rust writes this line and still reports what is missing at the deadline.
    report('error', String(e)).catch(() => undefined);
  });
}

function nowMs(): number {
  return typeof performance !== 'undefined' ? performance.now() : 0;
}

function report(key: string, text: string): Promise<void> {
  return host.request<void>('self_check_report', {
    key,
    text: `${text} | page_ms=${Math.round(nowMs())}`,
  });
}

function el(testId: string): HTMLElement | null {
  return document.querySelector<HTMLElement>(`[data-testid="${testId}"]`);
}

function text(testId: string): string | null {
  const e = el(testId);
  return e ? (e.textContent ?? '') : null;
}

function waitFor<T>(what: string, check: () => T | null | undefined | false, ms = 8000): Promise<T> {
  const until = nowMs() + ms;
  return new Promise((resolve, reject) => {
    const tick = () => {
      const v = check();
      if (v) {
        resolve(v);
      } else if (nowMs() > until) {
        reject(new Error(`timed out waiting for ${what}`));
      } else {
        setTimeout(tick, 10);
      }
    };
    tick();
  });
}

function csp(): string {
  return cspViolations.length === 0 ? 'csp_violations=0' : `csp_violations=${cspViolations.length} [${cspViolations.join('; ')}]`;
}

async function run(): Promise<void> {
  const state = await host.request<State>('self_check_state');
  if (!state.active) return;
  const done = new Set(state.reported);

  if (!done.has('reload_requested')) {
    // First load of the page.
    await report('js_start', `href=${location.href} bundle_ran_at_page_ms=${moduleMs}`);

    await waitFor('the Today list', () => text('time-left-1'));
    const nav = performance.getEntriesByType('navigation')[0] as PerformanceNavigationTiming | undefined;
    await report(
      'today_rendered',
      `route=${location.pathname} where="${text('where')}" ` +
        `child0="${text('child-name-0')}, ${text('time-used-0')}, ${text('time-left-0')}" ` +
        `child1="${text('child-name-1')}, ${text('time-used-1')}, ${text('time-left-1')}" ` +
        `dom_content_loaded_page_ms=${nav ? Math.round(nav.domContentLoadedEventEnd) : 'n/a'} ${csp()}`,
    );

    const answer = await host.request<unknown>('family_view');
    // A command the window was not granted (Tauri's own app plugin) must be refused.
    const probe = await host.request<unknown>('plugin:app|version').then(
      (v) => `ALLOWED (${JSON.stringify(v)})`,
      (e: unknown) => `refused (${String(e)})`,
    );
    await report('rust_answer', `family_view=${JSON.stringify(answer)} core_command_probe=${probe}`);

    const before = text('time-left-0');
    const t0 = nowMs();
    el('give-15-0')?.click();
    const after = await waitFor('a new time left after Give 15 minutes', () => {
      const t = text('time-left-0');
      return t !== before ? t : null;
    });
    await report('give_15', `child0 before="${before}" after="${after}" click_to_screen_ms=${Math.round(nowMs() - t0)}`);

    const t1 = nowMs();
    el('child-link-0')?.click();
    await waitFor('the Child route', () => location.pathname.startsWith('/child/') && text('child-heading'));
    await report(
      'child_route',
      `href=${location.href} heading="${text('child-heading')}" left="${text('child-time-left')}" ` +
        `status="${text('child-status')}" devices="${text('device-0')}; ${text('device-1')}" ` +
        `click_to_screen_ms=${Math.round(nowMs() - t1)}`,
    );

    const statusBefore = text('child-status');
    const t2 = nowMs();
    el('lock-now')?.click();
    const statusAfter = await waitFor('Locked after Lock now', () => {
      const s = text('child-status');
      return s !== statusBefore ? s : null;
    });
    await report(
      'lock_now',
      `status before="${statusBefore}" after="${statusAfter}" devices="${text('device-0')}; ${text('device-1')}" ` +
        `click_to_screen_ms=${Math.round(nowMs() - t2)}`,
    );

    await report('reload_requested', `reloading at ${location.pathname}`);
    location.reload();
    return;
  }

  if (!done.has('after_reload')) {
    // Second load of the page, after location.reload() on the Child route.
    await waitFor('the Child screen after reload', () => text('child-heading'));
    await report(
      'after_reload',
      `href=${location.href} heading="${text('child-heading')}" left="${text('child-time-left')}" ` +
        `status="${text('child-status')}" bundle_ran_at_page_ms=${moduleMs} ${csp()}`,
    );
  }
}
