/**
 * Best-effort release of the runtime sessions a page owns when that page goes away.
 *
 * A runtime session holds physical connections against the desktop budget, and the
 * only thing that ever gave one back was the thirty-minute idle deadline. Nothing
 * told the host that the page which opened a session had vanished — a reload, a
 * navigation, or the teardown of a WebDriver session left the session (and its
 * resources) behind, which is exactly how a multi-page run saturated the budget
 * (measured: seven sessions, fourteen of sixteen slots, the next claim refused).
 *
 * This is deliberately *not* a lifecycle owner: the controller still closes its own
 * session, and this only speaks when the page cannot. A page that dies mid-teardown
 * may lose the message; that is why it is a fallback and not the mechanism.
 */

type PageUnloadCloser = () => void;

const closers = new Set<PageUnloadCloser>();
let installed = false;

/**
 * Run every registered closer, once. Exported so a test can observe the teardown
 * directly; production reaches it through the listeners installed below.
 */
export function releaseOwnedSessions(): void {
  const pending = [...closers];
  closers.clear();
  for (const closer of pending) {
    try {
      closer();
    } catch {
      // A page on its way out must not fail because one closer did. The runtime
      // still owns the session's teardown, so swallowing here loses only the hint.
    }
  }
}

function install(): void {
  if (installed || typeof window === 'undefined') return;
  installed = true;
  // `pagehide` is the one that fires for reload/navigation and for a browser
  // context being torn down; `beforeunload` is the belt for engines that skip it.
  window.addEventListener('pagehide', releaseOwnedSessions);
  window.addEventListener('beforeunload', releaseOwnedSessions);
}

/**
 * Register a closer for this page, returning the function that unregisters it.
 *
 * Callers unregister as soon as they have released their session themselves:
 * closing twice is harmless but pointless, and a stale closer would target a
 * handle the controller no longer owns.
 */
export function registerPageUnloadCloser(closer: PageUnloadCloser): () => void {
  install();
  closers.add(closer);
  return () => {
    closers.delete(closer);
  };
}
