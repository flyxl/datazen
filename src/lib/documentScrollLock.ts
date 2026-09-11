/**
 * Lock the document (html/body) scroll position at (0, 0).
 *
 * The app renders fixed full-viewport chrome: `html/body/#root` are
 * `height: 100%; overflow: hidden` and every scrollable region is an inner
 * container. But `overflow: hidden` only removes scrollbars and user
 * scrolling — the root element stays a scroll container that programmatic
 * scrolls can still move:
 *
 *   - WebDriver / WebdriverIO auto-scrolls every scrollable ancestor
 *     (including the document) before `click()` and for `scrollIntoView()`;
 *   - `element.focus()` may scroll ancestors to reveal the caret.
 *
 * Once shifted, the user cannot scroll back (no scrollbars): the fixed
 * title bar disappears above the viewport and an unpainted gap appears at
 * the bottom. This is most visible under E2E automation, but any focus()
 * on freshly mounted offscreen content could trigger it.
 *
 * The shim snaps the document back to (0, 0) whenever anything moves it.
 * Inner containers are untouched (scroll events of inner elements do not
 * reach the Document/Window targets without the capture flag). Snapping
 * fires one extra no-op scroll event, so the listener settles immediately.
 * Remove never — the viewport must stay anchored for the app lifetime.
 */
interface ScrollLockGlobal {
  __datazenScrollLockInstalled?: boolean;
}

export function installDocumentScrollLock(): void {
  if (typeof window === 'undefined' || typeof document === 'undefined') return;
  const g = globalThis as typeof globalThis & ScrollLockGlobal;
  if (g.__datazenScrollLockInstalled) return;
  g.__datazenScrollLockInstalled = true;

  const snap = () => {
    const root = document.documentElement;
    const body = document.body;
    if (root.scrollTop !== 0) root.scrollTop = 0;
    if (root.scrollLeft !== 0) root.scrollLeft = 0;
    if (body) {
      if (body.scrollTop !== 0) body.scrollTop = 0;
      if (body.scrollLeft !== 0) body.scrollLeft = 0;
    }
  };

  // Root-element scrolling fires the scroll event at the Document (and
  // surfaces on Window). Non-capture on purpose: scroll events of inner
  // containers must not be observed here. Passive: the handler never calls
  // preventDefault().
  window.addEventListener('scroll', snap, { passive: true });
  document.addEventListener('scroll', snap, { passive: true });

  // Snap once on install in case something scrolled before bootstrap.
  snap();
}
