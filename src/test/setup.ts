// Vitest 4 note: registering through "@testing-library/jest-dom/vitest"
// silently no-ops in this environment (its side-effect entry ends up with
// an empty matcher set), so every toBeInTheDocument-style assertion fails
// with "Invalid Chai property". Extending from the standalone "./matchers"
// export with the runner's own `expect` registers correctly.

// `expect.extend` above only registers the matchers at *runtime*; it does not
// widen vitest's `Assertion` interface, so every `toBeInTheDocument` call in the
// suites type-checked as a missing property. This reference pulls in the
// package's `declare module 'vitest'` augmentation and nothing else — a
// triple-slash types reference is compile-time only, so the broken runtime
// entry noted above is still never loaded.
/// <reference types="@testing-library/jest-dom/vitest" />
import { expect } from 'vitest';
import * as jestDomMatchers from '@testing-library/jest-dom/matchers';

expect.extend(jestDomMatchers as unknown as Record<string, never>);

// jsdom does not implement getClientRects on Range/Element — CodeMirror 6
// calls it during gutter measurement. Return a minimal empty rect list.
if (typeof Range !== 'undefined' && !Range.prototype.getClientRects) {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  (Range.prototype as any).getClientRects = () => ({
    length: 0,
    item: () => null,
    [Symbol.iterator]: function* () {},
  });
}

// jsdom does not implement scrollIntoView either. Components that scroll an
// element into view (the filter editor does so after a filter was added) call
// it from a requestAnimationFrame callback, and the missing method turns into
// a TypeError that vitest reports as an *unhandled* error — every test passes,
// yet the run exits 1. Whether the callback fires before or after the suite
// ends is a timing question, so the failure showed up in some full runs and
// not in others. The stub is a no-op: jsdom has no layout, so there is
// nothing to scroll.
if (typeof Element !== 'undefined' && !Element.prototype.scrollIntoView) {
  Element.prototype.scrollIntoView = () => {};
}

// ── Platform ports (form-split F2) ────────────────────────────────────────────
// Shared code reaches window/event capabilities through the platform registry
// (`src/platform/contracts.ts`). Production wires the desktop implementations
// in the assembly root before first paint; tests configure a browser-shaped
// no-op platform so unconfigured-port errors never mask the behavior under
// test. This mirrors what browser dev mode resolves to via the compile-time
// `IS_DESKTOP_BUILD` branches.
import { configurePlatform, clearPlatform } from '../platform/contracts';
import type { PlatformBindings } from '../platform/contracts';

const testPlatform: PlatformBindings = {
  services: {
    saveTextWithDialog: async () => false,
    saveBinaryWithDialog: async () => false,
    openTextWithDialog: async () => null,
    openDirectoryWithDialog: async () => null,
    writeClipboard: async () => undefined,
    readClipboard: async () => '',
    openExternal: async () => undefined,
  },
  windows: {
    openSingletonWindow: () => undefined,
    openMultiInstanceWindow: () => undefined,
    focusMainWindow: async () => undefined,
    hasOpenChildWindows: async () => false,
    windowChrome: {
      isFullscreen: async () => false,
      onFullscreenChanged: async () => () => undefined,
      setBackgroundRootColor: async () => undefined,
      onCloseRequested: async () => () => undefined,
      minimizeCurrent: async () => undefined,
      closeCurrent: async () => undefined,
    },
  },
  events: {
    emitCrossWindow: async () => undefined,
    listenCrossWindow: async () => () => undefined,
  },
};

configurePlatform(testPlatform);

// Suites that exercise the *unconfigured* port path rebind with these.
export { testPlatform, clearPlatform, configurePlatform };
