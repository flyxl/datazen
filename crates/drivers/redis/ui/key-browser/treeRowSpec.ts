import {
  ancestorIndexes,
  firstChildIndex as sharedFirstChildIndex,
  indentOf,
  nextNavigableIndex as sharedNextNavigableIndex,
  parentIndexOf as sharedParentIndexOf,
  stepIndex,
} from '@datazen/ui';
import type { KeyTreeRow } from './keyTree';

/**
 * Geometry of the key-tree rows (PRD §3.2 行规格 / task book D-4).
 *
 * Lives apart from the renderer because both the virtualizer (fixed row height)
 * and the sticky group-header stack have to agree on the same numbers, and
 * because "which folders are pinned right now" is pure arithmetic over the flat
 * row list — so it is unit-testable without a DOM.
 *
 * The spec is deliberately *tight*: 30 px rows and a 4 + depth×10 px indent.
 * The previous 8 + depth×16 indent burned a third of the tree column on nesting
 * before the key name was readable.
 *
 * Indent and the index walks are the shared tree core's, not this file's: the
 * connection navigator asks the same questions over the same flat pre-order
 * shape, and keeping private copies is what let the two drift. What stays here
 * is what is genuinely the key browser's own — the row height, the pixel scale
 * the indent is expressed in, the scroll maths that depends on a fixed-height
 * virtualizer, and the keyboard map.
 */
export const ROW_HEIGHT = 30;
export const INDENT_BASE = 4;
export const INDENT_STEP = 10;

/** A `folder` row owns a child list; a `key` row never does. */
const isFolder = (row: KeyTreeRow): boolean => row.kind === 'folder';

/** The same predicate, for the shared tree shell's `isBranch` probe. */
export const isFolderRow = isFolder;

/**
 * Stable identity of a row, derived from the row itself and never from its
 * position.
 *
 * The renderer used to build these keys inline while mapping the virtual
 * window, which put the key contract one indentation level away from the data
 * that has to satisfy it — and left the *folder* keys unprefixed while the key
 * rows used the raw Redis name, so a key literally called `folder:app` and a
 * folder at `app` produced the same React key.
 *
 * The prefix matters because a virtualizer recycles DOM nodes by key: a key
 * that changes when a sibling is inserted, removed, expanded or collapsed
 * silently re-points a live node (and its focus, its checkbox state and its
 * in-flight click) at a different row. `path` and `entry.key` are the row's
 * own identity, so neither fold nor filter nor scroll can change them.
 */
export function keyTreeRowKey(row: KeyTreeRow): string {
  return row.kind === 'folder' ? `folder:${row.path}` : `key:${row.entry.key}`;
}

/** Left padding of a row at `depth` (root rows are depth 0). */
export function rowIndent(depth: number): number {
  return indentOf(depth, INDENT_BASE, INDENT_STEP);
}

/** Row index sitting at the top of the viewport for a uniform-height list. */
export function firstVisibleIndex(scrollTop: number, rowCount: number): number {
  if (rowCount <= 0) return 0;
  const raw = Math.floor(Math.max(0, scrollTop) / ROW_HEIGHT);
  return Math.min(raw, rowCount - 1);
}

/**
 * Folder rows pinned above the viewport while the list is scrolled
 * (multi-level sticky group headers).
 *
 * The chain holds the *strict ancestors* of the row at `topIndex`: a folder row
 * itself is excluded because it is already painted at the top of the viewport,
 * which keeps the transition into the pinned state visually seamless (enter:
 * the folder scrolls above the top edge; exit: it scrolls back below it).
 *
 * Rows are pre-order (a folder is always followed by its expanded children), so
 * the running stack behind this is exact.
 */
export function stickyFolderChain(rows: KeyTreeRow[], topIndex: number): KeyTreeRow[] {
  return ancestorIndexes(rows, topIndex, isFolder).map((index) => rows[index]!);
}

/**
 * Index of the row `offset` steps away from `from`, clamped to the list.
 * Clamping (instead of wrapping) is the exit transition of keyboard navigation:
 * ↑ on the first row and ↓ on the last row stay put rather than jumping the
 * viewport to the other end of a 10k-key tree.
 */
export function nextActiveIndex(from: number, offset: number, rowCount: number): number {
  return stepIndex(from, offset, rowCount);
}

/**
 * Row index of the folder that owns `rows[from]`, or `-1`.
 * ← on a leaf or a collapsed folder moves the selection to its parent; ← on a
 * root row has nowhere to go and returns `from`.
 */
export function parentIndexOf(rows: KeyTreeRow[], from: number): number {
  return sharedParentIndexOf(rows, from, isFolder);
}

/**
 * Row index of the first child of the folder at `from`, or `-1` when it is a
 * leaf / an empty collapsed folder. → on an open folder steps into its subtree
 * instead of doing nothing.
 */
export function firstChildIndex(rows: KeyTreeRow[], from: number): number {
  return sharedFirstChildIndex(rows, from, isFolder);
}

/* ── I-9 keyboard map ─────────────────────────────────────────────────────── */

/**
 * What a key press asks the tree to do. Named as *intents*, not keys, so the
 * renderer owns the state machine and this map stays testable without a DOM:
 * `↑/↓` move, `→` expand-or-step-in, `←` fold-or-parent, `Enter` activate,
 * `⌘/Ctrl+A` select what is loaded, `⌘/Ctrl+R` refresh, `Esc` leave selection.
 */
export type TreeNavAction =
  | 'next'
  | 'previous'
  | 'expand'
  | 'fold'
  | 'activate'
  | 'select-all'
  | 'refresh'
  | 'clear';

const PLAIN_NAV_KEYS: Record<string, TreeNavAction> = {
  ArrowDown: 'next',
  ArrowUp: 'previous',
  ArrowRight: 'expand',
  ArrowLeft: 'fold',
  Enter: 'activate',
  Escape: 'clear',
};

export interface TreeNavKeyEvent {
  key: string;
  metaKey?: boolean;
  ctrlKey?: boolean;
}

/** `null` ⇒ this key belongs to somebody else (a text input, the browser…). */
export function treeNavAction(event: TreeNavKeyEvent): TreeNavAction | null {
  if (event.metaKey || event.ctrlKey) {
    if (event.key === 'a' || event.key === 'A') return 'select-all';
    if (event.key === 'r' || event.key === 'R') return 'refresh';
    // Any other chord (⌘←, ⌘⇧A, a IME commit) must reach the browser untouched.
    return null;
  }
  return PLAIN_NAV_KEYS[event.key] ?? null;
}

/**
 * Starting at `first` (inclusive) and moving in `direction`, return the first
 * index `isNavigable` accepts, or `-1` when the walk leaves the list without
 * finding one (redis-tree-ui-BUG-004: extracted from `KeyTreeList`, where two
 * inline copies of this walk existed — the ↑/↓ step and `→`-into-subtree — and
 * no test ever reached either, so the round-1 claim "navigation passes
 * breadcrumbs through" had zero execution evidence).
 *
 * `first` is the *candidate*, not the current row: callers advance first
 * (`nextActiveIndex`, `firstChildIndex`) and this helper only skips what the
 * pattern filter marked as decoration. `isNavigable` is injected rather than
 * imported so this module stays free of the row type — the tree passes "not a
 * breadcrumb", a test can pass anything.
 *
 * A return of `-1` means "nowhere to go": callers must stay put (or leave
 * selection when the current row is itself no longer navigable). Returning the
 * row the walk skipped past would defeat it, which is exactly the round-1 defect.
 */
export function nextNavigableIndex(
  first: number,
  direction: 1 | -1,
  rowCount: number,
  isNavigable: (index: number) => boolean,
): number {
  return sharedNextNavigableIndex(first, direction, rowCount, isNavigable);
}
