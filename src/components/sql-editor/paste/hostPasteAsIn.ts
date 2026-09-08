/**
 * Host (community) Paste-as-IN fallback (§4.4).
 *
 * The parsing and literal-formatting rules already live in this package
 * (`parseDelimitedValues`), so the community build only lacked the editor
 * command. Pro ships a richer version with value-mode submenus; when Pro is
 * active its extensions take precedence and this one is not registered, so the
 * `Mod-Shift-v` binding is never bound twice.
 */
import { EditorView, keymap } from '@codemirror/view';
import type { Extension } from '@codemirror/state';
import { formatInClause, parseDelimitedValues, type ValueMode } from './parseDelimitedValues';

async function readClipboard(): Promise<string | null> {
  try {
    const { invoke } = await import('@tauri-apps/api/core');
    return await invoke<string>('read_clipboard');
  } catch {
    // Not running under Tauri (tests, browser dev server) — fall back to the
    // web Clipboard API, which may still be denied.
    try {
      if (typeof navigator !== 'undefined' && navigator.clipboard?.readText) {
        return await navigator.clipboard.readText();
      }
    } catch {
      /* clipboard permission denied */
    }
    return null;
  }
}

/**
 * Builds the `(...)` fragment for the current clipboard contents.
 * Returns null when the clipboard is unreadable or holds no usable values.
 */
export async function buildInClauseFromClipboard(
  mode: ValueMode = 'auto-type',
): Promise<string | null> {
  const text = await readClipboard();
  if (text == null || text.trim() === '') return null;

  const parsed = parseDelimitedValues(text);
  if (!('ok' in parsed)) return null;

  return `(${formatInClause(parsed.values, mode)})`;
}

/** Replaces the selection (or inserts at the cursor) with an IN clause. */
export async function pasteAsInCondition(
  view: EditorView,
  mode: ValueMode = 'auto-type',
): Promise<boolean> {
  const clause = await buildInClauseFromClipboard(mode);
  if (clause == null) return false;

  const { from, to } = view.state.selection.main;
  view.dispatch({
    changes: { from, to, insert: clause },
    selection: { anchor: from + clause.length },
    scrollIntoView: true,
  });
  return true;
}

export function createHostPasteAsInExtension(): Extension[] {
  return [
    keymap.of([
      {
        key: 'Mod-Shift-v',
        run: (view) => {
          // Claim the keystroke synchronously so the platform paste handler
          // does not also fire while the clipboard read is in flight.
          void pasteAsInCondition(view);
          return true;
        },
      },
    ]),
  ];
}
