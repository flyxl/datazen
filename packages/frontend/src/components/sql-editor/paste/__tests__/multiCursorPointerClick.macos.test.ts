/**
 * macOS branch of the multi-cursor POINTER gesture.
 *
 * Two platform facts make this file separate from the main journey suite:
 *
 *  1. CodeMirror's `browser.mac` is computed from `navigator.platform` at
 *     MODULE LOAD time, and `currentPlatform` then decides whether `Mod`
 *     normalizes to `Cmd` or `Ctrl`. jsdom reports an empty
 *     `navigator.platform`, so the main suite only ever exercises the non-mac
 *     branch. `vi.hoisted` is hoisted above this file's `import` statements,
 *     so the stub is in place when `@codemirror/view` is first evaluated;
 *     `afterAll` restores the original descriptor.
 *
 *  2. The deliberate decision under test: on macOS the multi-cursor chord is
 *     **Cmd**, and **Control is never a chord** because Control+click is the
 *     secondary-click / `contextmenu` gesture. See `multiCursorPointer.ts`.
 *
 * `isMacOS()` reads `navigator.platform` at CALL time, so the predicate picks
 * up the same stub; the point of importing everything dynamically here is to
 * keep the two mechanisms honest about the same module-load window.
 */
import { describe, it, expect, beforeAll, afterAll, afterEach, vi } from 'vitest';
// Type-only imports: erased at compile time, so they cannot trigger the
// module evaluations that `vi.hoisted` has to precede.
import type { EditorView } from '@codemirror/view';
import type { MountedEditor } from './multiCursorJourneyHarness';

const platformBackup = vi.hoisted(() => {
  const own = Object.getOwnPropertyDescriptor(navigator, 'platform');
  Object.defineProperty(navigator, 'platform', { value: 'MacIntel', configurable: true });
  return own;
});

afterAll(() => {
  if (platformBackup) {
    Object.defineProperty(navigator, 'platform', platformBackup);
  } else {
    Reflect.deleteProperty(navigator, 'platform');
  }
});

const { clickAddsCursor } = await import('../multiCursorPointer');
const {
  JOURNEY_DOC,
  clickAtPosition,
  flushMeasure,
  heads,
  installEditorLayoutShim,
  mountEditor,
  press,
  renderedCursors,
} = await import('./multiCursorJourneyHarness');

const P = { line1: 7, line2: 20, line3: 32, docStart: 0 } as const;

let mounted: MountedEditor | null = null;

beforeAll(() => {
  installEditorLayoutShim();
});

afterEach(() => {
  mounted?.destroy();
  mounted = null;
});

function open(anchor: number): EditorView {
  mounted = mountEditor({ anchor });
  return mounted.view;
}

describe('macOS pointer branch: the chord is Cmd, Control is not', () => {
  const evt = (over: Partial<Record<'altKey' | 'ctrlKey' | 'metaKey' | 'shiftKey', boolean>>) => ({
    altKey: false,
    ctrlKey: false,
    metaKey: false,
    shiftKey: false,
    ...over,
  });

  it('agrees with every row of the documented macOS table', () => {
    // The inversion versus the non-mac branch is the whole point: Ctrl and
    // Cmd swap roles, and Ctrl+click is excluded because the browser turns it
    // into a `contextmenu` (the statement context menu), not a cursor add.
    expect(clickAddsCursor(evt({ metaKey: true }))).toBe(true);
    expect(clickAddsCursor(evt({ ctrlKey: true }))).toBe(false);
    expect(clickAddsCursor(evt({ altKey: true }))).toBe(true);
    expect(clickAddsCursor(evt({ shiftKey: true }))).toBe(false);
    expect(clickAddsCursor(evt({ altKey: true, metaKey: true }))).toBe(true);
    expect(clickAddsCursor(evt({ altKey: true, ctrlKey: true }))).toBe(true);
    expect(clickAddsCursor(evt({}))).toBe(false);
  });

  it('Cmd+click adds a rendered second cursor and Escape exits it', async () => {
    const view = open(P.line1);
    expect(heads(view)).toEqual([P.line1]);

    await clickAtPosition(view, P.line2, { metaKey: true });
    expect(heads(view)).toEqual([P.line1, P.line2]);
    // Rendered, not merely logical.
    expect(renderedCursors(view)).toEqual({
      total: 2,
      primary: 1,
      secondary: 1,
      hasLayer: true,
    });

    await clickAtPosition(view, P.line3, { metaKey: true });
    expect(heads(view)).toEqual([P.line1, P.line2, P.line3]);
    expect(renderedCursors(view).total).toBe(3);

    press(view, { key: 'Escape', code: 'Escape' });
    expect(heads(view)).toEqual([P.line3]);
    await flushMeasure();
    expect(renderedCursors(view)).toEqual({
      total: 1,
      primary: 1,
      secondary: 0,
      hasLayer: true,
    });
  });

  it('Control+click does NOT add a cursor — it is the context-menu gesture', async () => {
    // Ctrl+click on macOS delivers `button: 0` with `ctrlKey: true` and the
    // browser then fires `contextmenu`; the SQL editor turns that into the
    // statement context menu. Adding a cursor there would fight the menu.
    const view = open(P.line1);
    await clickAtPosition(view, P.line2, { ctrlKey: true });
    expect(heads(view)).toEqual([P.line2]);
    expect(renderedCursors(view).total).toBe(1);
  });

  it('Option+click keeps working on macOS, alongside Cmd+click', async () => {
    const view = open(P.line1);
    await clickAtPosition(view, P.line2, { altKey: true });
    expect(heads(view)).toEqual([P.line1, P.line2]);
    await clickAtPosition(view, P.line3, { metaKey: true });
    expect(heads(view)).toEqual([P.line1, P.line2, P.line3]);
    press(view, { key: 'Escape', code: 'Escape' });
    expect(heads(view)).toEqual([P.line3]);
  });

  it('the journey document is the shared fixture, not a mac-only one', () => {
    // Guards the constant the other suite shares.
    expect(JOURNEY_DOC).toBe('SELECT alpha\nSELECT beta\nSELECT gamma');
  });
});
