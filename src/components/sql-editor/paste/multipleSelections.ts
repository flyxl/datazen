/**
 * Multi-cursor extension for CodeMirror 6.
 *
 * Enables multiple selections, Mod+D next occurrence, and rectangular
 * (Alt+drag / Option+drag) selection. Handles keymap priority to avoid breaking
 * CodeMirror's built-in find-next behavior.
 *
 * §Track S5-A step 8, §6.6
 */

import { keymap, rectangularSelection, EditorView } from '@codemirror/view';
import { EditorState } from '@codemirror/state';
import type { Extension } from '@codemirror/state';
import { selectNextOccurrence } from '@codemirror/search';
import { addCursorAbove, addCursorBelow } from '@codemirror/commands';

/**
 * Multi-cursor extension for CodeMirror 6.
 *
 * Enables multiple selections, Mod+D next occurrence, Option+Click multi-cursor,
 * and rectangular (Alt/Option+drag) column selection compatible with macOS Magic Trackpads.
 * Also provides Option+Cmd+Up/Down (Shift+Alt+Up/Down) to insert vertical cursors directly.
 */
export function createMultipleSelectionsExtension(): Extension[] {
  return [
    EditorState.allowMultipleSelections.of(true),
    rectangularSelection({
      eventFilter: (e: MouseEvent) => {
        // Support macOS Option+drag, Shift+Option+drag, and Cmd+Option+drag on Trackpad
        return (
          (e.altKey || (e.altKey && e.shiftKey) || (e.metaKey && e.altKey)) &&
          (e.button === 0 || e.buttons === 1)
        );
      },
    }),
    EditorView.clickAddsSelectionRange.of((e) => e.altKey && !e.shiftKey),
    EditorView.domEventHandlers({
      keydown(e, view) {
        // High-priority direct handler for Mod+D / Cmd+D across macOS WKWebView and web
        if (
          (e.metaKey || e.ctrlKey) &&
          !e.shiftKey &&
          !e.altKey &&
          (e.key === 'd' || e.key === 'D' || e.code === 'KeyD')
        ) {
          const handled = selectNextOccurrence(view);
          if (handled) {
            e.preventDefault();
            e.stopPropagation();
            return true;
          }
        }
        return false;
      },
    }),
    keymap.of([
      {
        key: 'Mod-d',
        run: selectNextOccurrence,
        preventDefault: true,
      },
      {
        key: 'Mod-D',
        run: selectNextOccurrence,
        preventDefault: true,
      },
      {
        key: 'Alt-Mod-ArrowUp',
        mac: 'Alt-Cmd-ArrowUp',
        run: addCursorAbove,
        preventDefault: true,
      },
      {
        key: 'Alt-Mod-ArrowDown',
        mac: 'Alt-Cmd-ArrowDown',
        run: addCursorBelow,
        preventDefault: true,
      },
      {
        key: 'Shift-Alt-ArrowUp',
        run: addCursorAbove,
        preventDefault: true,
      },
      {
        key: 'Shift-Alt-ArrowDown',
        run: addCursorBelow,
        preventDefault: true,
      },
    ]),
  ];
}
