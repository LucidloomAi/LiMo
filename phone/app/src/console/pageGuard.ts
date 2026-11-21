// The console page's own guard against a browser's keys and its right click
// menu. WebView2 is told to switch both off (the console's Rust side sets
// AreBrowserAcceleratorKeysEnabled and AreDefaultContextMenusEnabled to
// false); this is the second guard, in the page, for any WebView2 that does
// not honour them. Keys a guardian needs in a field (typing, Tab, Enter,
// copy, paste, undo, select all) are never touched.

/** The parts of a key press the guard looks at. */
export type KeyPress = {
  key: string;
  ctrlKey: boolean;
  metaKey: boolean;
  altKey: boolean;
  shiftKey: boolean;
};

/** Keys that do something to the browser rather than to the page. */
const ALONE = new Set(['F1', 'F3', 'F5', 'F6', 'F7', 'F11', 'F12', 'BrowserBack', 'BrowserForward', 'BrowserRefresh', 'BrowserSearch', 'BrowserHome', 'BrowserFavorites', 'BrowserStop']);
/** With Ctrl: reload, print, find, find again, history, downloads, source, save, open, new window or tab, close, zoom, caret browsing. */
const WITH_CTRL = new Set(['r', 'p', 'f', 'g', 'h', 'j', 'u', 's', 'o', 'n', 't', 'w', 'd', 'l', 'k', 'e', '+', '-', '=', '0', 'F5']);
/** With Ctrl and Shift: developer tools, reload without the cache, and the rest. */
const WITH_CTRL_SHIFT = new Set(['i', 'j', 'c', 'r', 'delete', 'n', 't', 'p', 'b', 'o', 'k', 'm', 'y']);
/** With Alt: back, forward, home, and the browser's menu. */
const WITH_ALT = new Set(['ArrowLeft', 'ArrowRight', 'Home', 'f', 'e']);

/** True for a key press that would act on the browser, not on the page. */
export function isBrowserKey(press: KeyPress): boolean {
  const key = press.key.length === 1 ? press.key.toLowerCase() : press.key;
  const ctrl = press.ctrlKey || press.metaKey;
  if (ALONE.has(press.key) && !ctrl && !press.altKey) return true;
  if (ctrl && press.shiftKey && WITH_CTRL_SHIFT.has(key.toLowerCase())) return true;
  if (ctrl && !press.shiftKey && !press.altKey && WITH_CTRL.has(key)) return true;
  if (press.altKey && !ctrl && WITH_ALT.has(key)) return true;
  return false;
}

let installed = false;

/**
 * Puts the guard on the page, once. Leaves it off when the self check asks
 * for the page without it, to show what WebView2's own settings do alone.
 */
export function guardThePage(): void {
  if (installed || typeof document === 'undefined') return;
  if ((globalThis as Record<string, unknown>).__LIMO_SELF_CHECK_NO_PAGE_GUARD__) return;
  installed = true;
  document.addEventListener('contextmenu', (e) => e.preventDefault(), true);
  document.addEventListener(
    'keydown',
    (e) => {
      if (isBrowserKey(e)) {
        e.preventDefault();
        e.stopPropagation();
      }
    },
    true,
  );
  // Dropping a file on the window would open it in place of the console.
  document.addEventListener('dragover', (e) => e.preventDefault(), true);
  document.addEventListener('drop', (e) => e.preventDefault(), true);
}

/** Whether the guard is on. */
export function pageGuardOn(): boolean {
  return installed;
}
