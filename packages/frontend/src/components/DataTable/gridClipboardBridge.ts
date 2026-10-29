/**
 * The vocabulary the grid and its owner use to talk about the clipboard (DB-03 §3.6).
 *
 * It lives in its own module rather than in `DataTable.tsx` so that the owner's hook
 * (`src/hooks/useGridClipboard.ts`) can type its result without importing a component —
 * a type-only import would work, but a hook that names a component module makes the layering
 * harder to read than it has to be. `DataTable.tsx` re-exports these names, so the chapter's
 * documented import path stays valid.
 */
import type { GridClipboardAnchor, GridClipboardRange } from '../../lib/gridClipboard';

/** What a copy gesture should put on the clipboard. */
export type GridCopyTarget =
  | { mode: 'row'; rowIndices: number[] }
  | { mode: 'cell'; range: GridClipboardRange };

/**
 * The grid's clipboard bridge.
 *
 * `DataTable` knows neither `ColumnSchema` nor the store: it reports gestures and renders the
 * text it is handed. Every rule about *what* to copy and *where* to write lives in the owner
 * (see §3.2b, §3.4).
 */
export interface GridClipboardHandlers {
  /** `copy` event. Returns the text to put on the clipboard; `null` = do not intercept. */
  buildCopyText: (target: GridCopyTarget) => string | null;
  /** Copy result feedback (the owner holds the timer). */
  onCopied: (target: GridCopyTarget, ok: boolean) => void;
  /** Native `paste` event. Returns `true` = consumed (the caller already called `preventDefault`). */
  onPasteText: (text: string, anchor: GridClipboardAnchor) => boolean;
  /**
   * Right-click "paste": the system clipboard has to be read asynchronously.
   *
   * Optional, because a host may bind only the native gesture (nothing to read
   * asynchronously then) and because it keeps a partially-implemented bridge from
   * having to fabricate a no-op.
   */
  onPasteFromClipboard?: (anchor: GridClipboardAnchor) => void;
  /** Copy entry availability (read-only connection / no selection ⇒ `false`); never hidden. */
  canCopy: boolean;
  /** Paste entry availability (read-only connection / table without primary key ⇒ `false`). */
  canPaste: boolean;
}
