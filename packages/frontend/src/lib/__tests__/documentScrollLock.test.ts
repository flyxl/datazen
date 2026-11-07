import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { installDocumentScrollLock } from '../documentScrollLock';

/**
 * Document scroll lock regression tests.
 *
 * The app renders fixed full-viewport chrome (html/body overflow:hidden).
 * Programmatic scrolls — WebDriver click()/scrollIntoView() auto-scroll,
 * focus() — can still move the root scroll container; once shifted, the
 * user cannot scroll back and the TitleBar disappears above the viewport.
 * The lock must snap html/body back to (0,0) on every document-level scroll
 * without ever touching inner container scrolling.
 */
describe('installDocumentScrollLock', () => {
  let listeners: Array<[EventTarget, EventListener]>;

  const fireScroll = (target: EventTarget) => {
    for (const [t, fn] of listeners) {
      if (t === target) fn(new Event('scroll'));
    }
  };

  beforeEach(() => {
    listeners = [];
    vi.spyOn(window, 'addEventListener').mockImplementation(((type: string, fn: EventListener) => {
      if (type === 'scroll') listeners.push([window, fn]);
    }) as typeof window.addEventListener);
    vi.spyOn(document, 'addEventListener').mockImplementation(((
      type: string,
      fn: EventListener,
    ) => {
      if (type === 'scroll') listeners.push([document, fn]);
    }) as typeof document.addEventListener);
  });

  afterEach(() => {
    vi.restoreAllMocks();
    document.documentElement.scrollTop = 0;
    document.documentElement.scrollLeft = 0;
    document.body.scrollTop = 0;
    document.body.scrollLeft = 0;
    (globalThis as { __datazenScrollLockInstalled?: boolean }).__datazenScrollLockInstalled =
      undefined;
  });

  it('installs exactly once even when called repeatedly', () => {
    installDocumentScrollLock();
    installDocumentScrollLock();
    expect(listeners.filter(([t]) => t === window)).toHaveLength(1);
    expect(listeners.filter(([t]) => t === document)).toHaveLength(1);
  });

  it('snaps the root element back after a programmatic document scroll', () => {
    installDocumentScrollLock();

    // Simulate WebDriver scrollIntoView shifting the document by one
    // titlebar height + a horizontal component.
    document.documentElement.scrollTop = 40;
    document.documentElement.scrollLeft = 12;
    fireScroll(document);

    expect(document.documentElement.scrollTop).toBe(0);
    expect(document.documentElement.scrollLeft).toBe(0);
  });

  it('snaps the body element back after a programmatic scroll', () => {
    installDocumentScrollLock();

    document.body.scrollTop = 68;
    document.body.scrollLeft = 5;
    fireScroll(document);

    expect(document.body.scrollTop).toBe(0);
    expect(document.body.scrollLeft).toBe(0);
  });

  it('snaps immediately on install when the document was already shifted', () => {
    document.documentElement.scrollTop = 40;
    installDocumentScrollLock();
    expect(document.documentElement.scrollTop).toBe(0);
  });

  it('leaves a scrolled inner container untouched (no capture observation)', () => {
    installDocumentScrollLock();

    const inner = document.createElement('div');
    inner.scrollTop = 250;
    // Inner scroll events do not reach document/window targets: dispatching
    // on the element itself must not trigger the snap handlers.
    inner.dispatchEvent(new Event('scroll'));

    expect(inner.scrollTop).toBe(250);
  });

  it('is a no-op when the document is at origin', () => {
    installDocumentScrollLock();
    fireScroll(document);
    expect(document.documentElement.scrollTop).toBe(0);
    expect(document.body.scrollTop).toBe(0);
  });
});
