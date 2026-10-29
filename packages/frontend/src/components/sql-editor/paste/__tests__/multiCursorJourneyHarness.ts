/**
 * Shared harness for the multi-cursor journey tests.
 *
 * Not a test file (`*.test.ts` is what vitest collects), so it can be imported
 * by both the default-platform and the macOS journey suites.
 *
 * ## Why a layout shim
 *
 * CodeMirror draws cursors as `.cm-cursor` decorations positioned from
 * `view.coordsAtPos()`, which reads `Range.getClientRects()`. jsdom has no
 * layout engine, so that returns nothing and the cursor layer emits **zero**
 * widgets — a test that asserts "the second cursor is rendered" would pass for
 * the wrong reason. The shim below gives the document a deterministic
 * monospace geometry.
 *
 * The one non-obvious requirement: every `.cm-line` must get a **distinct**
 * `top`. CodeMirror's decoration layers are keyed by block, so a shim that
 * returns `top: 0` for every line collapses all cursors onto one block and
 * silently renders a single caret.
 *
 * ## Why clicks are expressed as document positions
 *
 * The project rule is that interaction is identified without viewport
 * geometry, and a real `clientX/clientY` in jsdom resolves to nothing anyway.
 * So the click helpers pin `posAtCoords`/`posAndSideAtCoords` to a document
 * position and assert on editor state plus `.cm-cursor*` DOM nodes. No test
 * here depends on a pixel.
 */
import { EditorState } from '@codemirror/state';
import type { Extension } from '@codemirror/state';
import { EditorView, keymap } from '@codemirror/view';
import type { EditorView as EditorViewType } from '@codemirror/view';
import { searchKeymap } from '@codemirror/search';
import { defaultKeymap, historyKeymap } from '@codemirror/commands';
import { createMultipleSelectionsExtension } from '../multipleSelections';

/** The three-line SQL document every journey in this feature walks. */
export const JOURNEY_DOC = 'SELECT alpha\nSELECT beta\nSELECT gamma';

const CHAR_WIDTH = 8;
const LINE_HEIGHT = 16;

function makeRect(left: number, top: number, width: number, height: number): DOMRect {
  return {
    x: left,
    y: top,
    left,
    top,
    width,
    height,
    right: left + width,
    bottom: top + height,
    toJSON: () => ({}),
  } as DOMRect;
}

function rectList(rects: DOMRect[]): DOMRectList {
  const list = rects.slice();
  return Object.assign(list, {
    item: (i: number): DOMRect | null => list[i] ?? null,
  }) as unknown as DOMRectList;
}

function lineTopOf(node: Node): number {
  let el: HTMLElement | null =
    node.nodeType === Node.ELEMENT_NODE ? (node as HTMLElement) : node.parentElement;
  while (el && !el.classList.contains('cm-line')) el = el.parentElement;
  if (!el?.parentElement) return 0;
  return Array.prototype.indexOf.call(el.parentElement.children, el) * LINE_HEIGHT;
}

let shimInstalled = false;

/** Idempotent: installs the fake monospace layout onto `Range.prototype`. */
export function installEditorLayoutShim(): void {
  if (shimInstalled) return;
  shimInstalled = true;
  Range.prototype.getClientRects = function getClientRects(this: Range): DOMRectList {
    const start = this.startContainer;
    const end = this.endContainer;
    if (start.nodeType !== Node.TEXT_NODE || end.nodeType !== Node.TEXT_NODE) {
      return rectList([makeRect(0, 0, CHAR_WIDTH, LINE_HEIGHT)]);
    }
    const left = this.startOffset * CHAR_WIDTH;
    const width = Math.max(0, this.endOffset - this.startOffset) * CHAR_WIDTH;
    return rectList([makeRect(left, lineTopOf(start), width || 1, LINE_HEIGHT)]);
  };
  Range.prototype.getBoundingClientRect = function getBoundingClientRect(this: Range): DOMRect {
    return this.getClientRects()[0] ?? makeRect(0, 0, 0, LINE_HEIGHT);
  };
}

/**
 * The real mount order used by the SQL editor.
 *
 * `createBaseEditorExtensions()` registers
 * `keymap.of([...defaultKeymap, ...historyKeymap, ...searchKeymap])` first
 * (editorExtensions.ts:286) and the multi-cursor extension arrives afterwards
 * through `compartments.paste.of(...)`. Within one precedence the earlier
 * registration wins, so a standalone extension would never reproduce real
 * dispatch. Every test mounts in this order.
 */
export function realOrderExtensions(): Extension[] {
  return [keymap.of([...defaultKeymap, ...historyKeymap, ...searchKeymap])];
}

export interface MountedEditor {
  view: EditorViewType;
  parent: HTMLElement;
  destroy(): void;
}

export interface EditorMountOptions {
  doc?: string;
  anchor?: number;
  head?: number;
  /** Mounted after `realOrderExtensions()`, before the multi-cursor extension. */
  extraBefore?: Extension[];
  /** Mounted after the multi-cursor extension — the Pro compartment position. */
  extraAfter?: Extension[];
}

/** Mounts an editor whose keymap precedence matches the real SQL editor. */
export function mountEditor(options: EditorMountOptions = {}): MountedEditor {
  const parent = document.createElement('div');
  document.body.appendChild(parent);
  const anchor = options.anchor ?? 0;
  const state = EditorState.create({
    doc: options.doc ?? JOURNEY_DOC,
    selection: { anchor, head: options.head ?? anchor },
    extensions: [
      ...realOrderExtensions(),
      ...(options.extraBefore ?? []),
      // NOTE: `drawSelection()` is deliberately NOT added here. It must come
      // from `createMultipleSelectionsExtension()` alone, otherwise these tests
      // would assert against a harness-supplied cursor layer and stay green
      // even if the product stopped rendering secondary cursors.
      createMultipleSelectionsExtension(),
      ...(options.extraAfter ?? []),
    ],
  });
  const view = new EditorView({ state, parent });
  return {
    view,
    parent,
    destroy(): void {
      view.destroy();
      parent.remove();
    },
  };
}

/**
 * Waits for CodeMirror's measure cycle.
 *
 * Decoration layers are rebuilt in a `requestMeasure` callback, so immediately
 * after a dispatch the `.cm-cursorLayer` still holds the *previous* state's
 * widgets. Without this await, "are the cursors rendered?" silently reads the
 * pre-dispatch DOM and every assertion about it is a lie.
 */
export function flushMeasure(): Promise<void> {
  return new Promise((resolve) => {
    requestAnimationFrame(() => resolve(undefined));
  });
}

/** A real `keydown` through CodeMirror's real keymap dispatch. */
export function press(view: EditorViewType, init: KeyboardEventInit): KeyboardEvent {
  const event = new KeyboardEvent('keydown', { bubbles: true, cancelable: true, ...init });
  view.contentDOM.dispatchEvent(event);
  return event;
}

let clickSeq = 0;

/**
 * Drives one mouse gesture with coordinate→position resolution pinned to a
 * document position, so a gesture can be described as "at position N" instead
 * of as a pair of pixel coordinates that jsdom cannot resolve.
 *
 * `posFor` receives the gesture phase so a drag can start at one position and
 * finish at another; a click ignores it and stays put.
 */
async function driveGesture(
  view: EditorViewType,
  posFor: (phase: 'down' | 'move' | 'up') => number,
  init: MouseEventInit,
): Promise<void> {
  const originalPosAt = view.posAtCoords.bind(view);
  const originalPosAndSide = view.posAndSideAtCoords.bind(view);
  let phase: 'down' | 'move' | 'up' = 'down';
  const current = (): number => posFor(phase);
  view.posAtCoords = () => current();
  view.posAndSideAtCoords = () => ({ pos: current(), assoc: 0 }) as never;
  // Vary the coordinates per gesture: CodeMirror's multi-click detector treats
  // two clicks at the same spot within 400ms as a double/triple click, which
  // would silently turn a later click into a word/line selection.
  clickSeq += 1;
  const cx = 5 + clickSeq;
  const cy = 5 + clickSeq;
  const mouse = (type: string, buttons: number): MouseEvent =>
    new MouseEvent(type, {
      bubbles: true,
      cancelable: true,
      button: 0,
      buttons,
      detail: 1,
      clientX: cx,
      clientY: cy,
      ...init,
    });
  try {
    phase = 'down';
    view.contentDOM.dispatchEvent(mouse('mousedown', 1));
    phase = 'move';
    document.dispatchEvent(mouse('mousemove', 1));
    phase = 'up';
    document.dispatchEvent(mouse('mouseup', 0));
    await flushMeasure();
  } finally {
    view.posAtCoords = originalPosAt;
    view.posAndSideAtCoords = originalPosAndSide;
  }
}

/**
 * A primary-button click at a document position, driven as a real
 * mousedown/mousemove/mouseup triple.
 *
 * `init` supplies the modifier flags under test (ctrlKey, metaKey, altKey,
 * shiftKey) — the same fields a real browser fills in.
 */
export async function clickAtPosition(
  view: EditorViewType,
  pos: number,
  init: MouseEventInit = {},
): Promise<void> {
  await driveGesture(view, () => pos, init);
}

/** A primary-button drag from one document position to another. */
export async function dragAcross(
  view: EditorViewType,
  from: number,
  to: number,
  init: MouseEventInit = {},
): Promise<void> {
  await driveGesture(view, (phase) => (phase === 'down' ? from : to), init);
}

/* -------------------------------------------------------------------------- */
/*  Observable-state readers                                                   */
/* -------------------------------------------------------------------------- */

/** The rendered cursor count, split into primary/secondary by CodeMirror. */
export interface RenderedCursors {
  total: number;
  primary: number;
  secondary: number;
  /** True when a `.cm-cursorLayer` exists at all (i.e. `drawSelection` is on). */
  hasLayer: boolean;
}

/** Reads the cursor decorations the user can actually see. */
export function renderedCursors(view: EditorViewType): RenderedCursors {
  return {
    total: view.dom.querySelectorAll('.cm-cursor').length,
    primary: view.dom.querySelectorAll('.cm-cursor-primary').length,
    secondary: view.dom.querySelectorAll('.cm-cursor-secondary').length,
    hasLayer: view.dom.querySelector('.cm-cursorLayer') !== null,
  };
}

/** Every selection range as `[from, to]`, in document order. */
export function rangeTuples(view: EditorViewType): Array<[number, number]> {
  return view.state.selection.ranges.map((r) => [r.from, r.to]);
}

/** Every cursor head, in document order. */
export function heads(view: EditorViewType): number[] {
  return view.state.selection.ranges.map((r) => r.head);
}

/** Absolute offset at which each line of `doc` starts. */
export function lineStarts(doc: string): number[] {
  const starts = [0];
  for (let i = 0; i < doc.length; i++) {
    if (doc.charCodeAt(i) === 10) starts.push(i + 1);
  }
  return starts;
}

/**
 * The column (offset from its own line start) of every range, in order.
 *
 * A column selection is defined by every range landing on the SAME column
 * across lines; comparing raw offsets would be true only by accident.
 */
export function columnsOf(view: EditorViewType): number[] {
  const starts = lineStarts(view.state.doc.toString());
  return view.state.selection.ranges.map((range) => {
    let line = 0;
    for (let i = 0; i < starts.length; i++) {
      if (starts[i] <= range.from) line = i;
    }
    return range.from - starts[line];
  });
}
