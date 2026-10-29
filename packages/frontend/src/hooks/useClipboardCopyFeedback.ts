/**
 * Request-bound "copy" feedback that **also** has a fallback chain (DB-03 §3.5).
 *
 * `@datazen/ui`'s `useCopyFeedback` is the same semantics but deliberately cannot carry the
 * downgrade chain: `packages/ui` is not allowed to import `@tauri-apps/api`, and the chain
 * needs `invoke('write_clipboard')`. On Tauri's WKWebView the async Clipboard API is
 * refused as soon as the user gesture is gone — which is exactly the case for a context-menu
 * callback that went through IPC — so a grid copy that only used `writeText` would fail
 * silently ("I clicked Copy and nothing happened", defect G5). This host-side hook therefore
 * keeps the three guarantees of the shared one (optimistic feedback with rollback, every
 * click owns its whole window, a late rejection cannot erase a newer success) and swaps the
 * write for `copyToClipboard`'s three-step chain.
 *
 * Empty text stays a **failure**: `copyToClipboard('')` returns `false` by contract, and
 * lighting up "copied" for a write that put nothing on the clipboard would be a lie.
 */
import { useCallback, useEffect, useRef, useState } from 'react';
import { copyRichTextToClipboard, copyToClipboard } from '../lib/fetchRelationDdl';

export interface ClipboardCopyFeedback {
  /** Whether the last copy succeeded; the success state lasts `feedbackMs`. */
  copied: boolean;
  /**
   * Perform the copy. Never throws; returns whether the text reached the clipboard.
   *
   * With `html` (DB-10) the write carries both flavors: `text/html` for
   * targets that render tables (Notion / Excel / Google Sheets) and the
   * plain text as the fallback everything else reads.
   */
  copy: (text: string, rich?: { html?: string }) => Promise<boolean>;
}

export function useClipboardCopyFeedback(feedbackMs: number): ClipboardCopyFeedback {
  const [copied, setCopied] = useState(false);
  // `number`, not `ReturnType<typeof window.setTimeout>`: with @types/node in scope that
  // resolves to `NodeJS.Timeout` while the DOM one returns a numeric id.
  const timerRef = useRef<number | null>(null);
  /** Monotonic id of the newest request; older ones are stale and must not roll back. */
  const requestIdRef = useRef(0);

  useEffect(
    () => () => {
      if (timerRef.current !== null) {
        window.clearTimeout(timerRef.current);
        timerRef.current = null;
      }
    },
    [],
  );

  const copy = useCallback(
    async (text: string, rich?: { html?: string }): Promise<boolean> => {
      if (!text && !rich?.html) {
        // The existing helper's semantics, kept: an empty copy is not a success.
        return false;
      }
      const requestId = ++requestIdRef.current;
      // Optimistic: the click has to read as instant. Rolled back below if the write fails.
      setCopied(true);
      if (timerRef.current !== null) window.clearTimeout(timerRef.current);
      timerRef.current = window.setTimeout(() => {
        timerRef.current = null;
        setCopied(false);
      }, feedbackMs);

      const ok = await (
        rich?.html ? copyRichTextToClipboard(rich.html, text) : copyToClipboard(text)
      ).catch(() => false);
      if (!ok && requestId === requestIdRef.current) {
        // Bound to this request: a failure of an older copy leaves the newer one's window
        // (and its timer) alone.
        if (timerRef.current !== null) {
          window.clearTimeout(timerRef.current);
          timerRef.current = null;
        }
        setCopied(false);
      }
      return ok;
    },
    [feedbackMs],
  );

  return { copied, copy };
}
