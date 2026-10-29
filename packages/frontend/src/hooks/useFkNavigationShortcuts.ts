import { useCallback } from 'react';
import { isComposingEvent, isEditableTarget } from '../lib/gridEventGuards';

/**
 * The keyboard of the foreign-key navigation (DB-07).
 *
 * `⌘/Ctrl+⌥+←` goes back, `⌘/Ctrl+⌥+→` goes forward. The handler is bound to
 * the **grid container**, never to the window: a global listener would fight
 * the SQL editor and every text field in the app.
 *
 * Two guards decide whether the key belongs to the navigation:
 *
 * - an **IME composition** owns every key (a Chinese input in the middle of a
 *   word must not navigate away);
 * - an **editable target** owns its keys (an open cell editor must not lose
 *   the navigation because the user pressed an arrow).
 *
 * When the stack is empty the key is left alone: no `preventDefault`, no
 * error. Swallowing a shortcut that did nothing is worse than letting the
 * system have it.
 */
export interface UseFkNavigationShortcutsOptions {
  /** The navigation is available at all (a tab that was reached by a jump). */
  enabled: boolean;
  /** Go back one jump. */
  onBack: () => void;
  /** Go forward one jump. */
  onForward: () => void;
}

/** The key handler of the navigation; bind it to the grid container. */
export type FkNavigationKeyHandler = (event: React.KeyboardEvent) => void;

export function useFkNavigationShortcuts({
  enabled,
  onBack,
  onForward,
}: UseFkNavigationShortcutsOptions): FkNavigationKeyHandler {
  return useCallback(
    (event: React.KeyboardEvent) => {
      if (!enabled) return;
      if (isComposingEvent(event)) return;
      if (isEditableTarget(event.target)) return;
      const mod = event.metaKey || event.ctrlKey;
      if (!mod || !event.altKey) return;
      if (event.key === 'ArrowLeft') {
        event.preventDefault();
        onBack();
        return;
      }
      if (event.key === 'ArrowRight') {
        event.preventDefault();
        onForward();
      }
    },
    [enabled, onBack, onForward],
  );
}
