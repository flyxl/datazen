/**
 * Multi-cursor extension for CodeMirror 6.
 *
 * Enables multiple selections, Mod+D next occurrence, and rectangular
 * (Alt+drag / Option+drag) selection. Handles keymap priority to avoid breaking
 * CodeMirror's built-in find-next behavior.
 *
 * ## ESCAPE — the full binding inventory
 *
 * Escape is bound by five bindings. The host used to own none of them, so the
 * editor content silently fell through to `simplifySelection` and multi-cursor
 * exit was an accident of registration order rather than a decision — and it
 * left the main selection non-empty, i.e. the selected word stayed selected
 * after "leaving" multi-cursor mode. Row 3 below is the deliberate replacement.
 *
 * | # | binding                     | source                                    | precedence | scope                 |
 * |---|-----------------------------|-------------------------------------------|------------|-----------------------|
 * | 1 | `clearSnippet`              | `@codemirror/autocomplete` snippetKeymap  | highest    | editor                |
 * | 2 | `closeCompletion`           | `@codemirror/autocomplete` completionKeymap| highest   | editor                |
 * | 3 | `exitMultiCursor` (this file)| `multiCursorEscape.ts`                    | high       | editor                |
 * | 4 | `simplifySelection`         | `@codemirror/commands` defaultKeymap      | default    | editor                |
 * | 5 | `closeSearchPanel`          | `@codemirror/search` searchKeymap         | default    | `"editor search-panel"`|
 *
 * Rows 1-2 sit at `Prec.highest` and answer only when a snippet or completion
 * is actually live (both return `false` otherwise), so they always win the
 * first Escape and the multi-cursor survives until the panel is gone — the
 * intended two-step dismissal. Row 5 is scope-gated to the panel's own
 * content, so it never competes in the editor body.
 *
 * That leaves row 4 as the only competitor for row 3, and it is registered
 * FIRST (`editorExtensions.ts`) at default precedence, so an Escape binding
 * added at default precedence here would lose every time and do nothing. It is
 * therefore mounted at `Prec.high`, the same lever already used below for
 * Shift-Alt-Arrow. `Prec.high` rather than `Prec.highest` on purpose: highest
 * would put multi-cursor exit above the autocomplete dismissal in row 1-2, and
 * a first Escape that both closes a panel and throws away every cursor is a
 * worse state machine than the two-step one above.
 *
 * A Pro extension binding Escape at `Prec.highest` (the precedence
 * `createExtraKeymap` and the fold keymap already use) would outrank row 3.
 * That is the intended seam: a privileged binding wins, and row 3 only owns
 * Escape when nothing above it claims the key. Note that a `Prec.highest`
 * binding which DECLINES (`run` returns false) falls through to row 3, so Pro
 * can inspect Escape and hand it back.
 *
 * ## POINTER — the modifier table
 *
 * `EditorView.clickAddsSelectionRange` REPLACES CodeMirror's own default rather
 * than extending it, so this host decides the whole chord. The table and the
 * platform reasoning live in `multiCursorPointer.ts`; the short version is that
 * Option/Alt+click keeps its existing meaning, ⌘/Ctrl+click is added to restore
 * the chord CodeMirror would have used, and ⌃+click is never a chord on macOS
 * because that is the secondary-click/context-menu gesture.
 */

import { keymap, rectangularSelection, EditorView, drawSelection } from '@codemirror/view';
import { EditorState, Prec } from '@codemirror/state';
import type { Extension } from '@codemirror/state';
import { selectNextOccurrence } from '@codemirror/search';
import { addCursorAbove, addCursorBelow, copyLineUp, copyLineDown } from '@codemirror/commands';
import { clickAddsCursor } from './multiCursorPointer';
import { exitMultiCursor } from './multiCursorEscape';

/**
 * Multi-cursor extension for CodeMirror 6.
 *
 * Enables multiple selections, Mod+D next occurrence, Option/Alt+Click and
 * ⌘/Ctrl+Click multi-cursor, and rectangular (Alt/Option+drag) column selection
 * compatible with macOS Magic Trackpads.
 * Also provides Option+Cmd+Up/Down (Shift+Alt+Up/Down) to insert vertical
 * cursors directly, and Escape to leave the multi-cursor state.
 */
export function createMultipleSelectionsExtension(): Extension[] {
  return [
    EditorState.allowMultipleSelections.of(true),
    /*
     * RENDERING — secondary cursors have to be drawn, not just selected.
     *
     * Measured before this change: with two selection ranges and no
     * `drawSelection`, the view has no `.cm-cursorLayer` at all and renders
     * zero `.cm-cursor` nodes. Only the browser's native caret for the main
     * range is visible, so a second cursor is logically present and visually
     * absent — the user clicks, sees one caret, and cannot tell whether the
     * click landed. Every other multi-cursor entry point (Mod-D, Alt+Click,
     * Alt+Shift+Arrow, column selection) has the same invisible result.
     *
     * `drawSelection()` contributes the `.cm-cursorLayer` /
     * `.cm-selectionLayer` decorations plus `hideNativeSelection`, and
     * `RectangleMarker.forRange` already emits one `.cm-cursor-secondary` per
     * extra range, so it covers column selection too.
     *
     * It lives here rather than in `createBaseEditorExtensions()` because the
     * multi-cursor feature is what is broken without it, and the base list is
     * shared with the Pro compartments and their precedence tests. One
     * consequence to be aware of: the main caret is now drawn by CodeMirror
     * instead of by the OS, and it blinks on the CodeMirror schedule.
     */
    drawSelection(),
    rectangularSelection({
      eventFilter: (e: MouseEvent) => {
        // Support macOS Option+drag, Shift+Option+drag, and Cmd+Option+drag on Trackpad
        return (
          (e.altKey || (e.altKey && e.shiftKey) || (e.metaKey && e.altKey)) &&
          (e.button === 0 || e.buttons === 1)
        );
      },
    }),
    // Alt/Option+click, plus the platform chord CodeMirror's own default uses.
    // See multiCursorPointer.ts for the modifier conflict table.
    EditorView.clickAddsSelectionRange.of(clickAddsCursor),
    EditorView.domEventHandlers({
      keydown(e, view) {
        // High-priority direct handler for Mod+D / Cmd+D across macOS WKWebView and web
        if (
          (e.metaKey || e.ctrlKey) &&
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
        key: 'Shift-Mod-d',
        run: selectNextOccurrence,
        preventDefault: true,
      },
      {
        key: 'Shift-Mod-D',
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
      /*
       * copy line keeps a home now that Option+Shift+Up/Down is multi-cursor.
       *
       * `Mod-Shift-ArrowUp/Down` is free on Windows/Linux, where it is
       * registered as a normal-precedence binding so it can never shadow the
       * editor's own custom shortcuts. On macOS that chord is NOT free:
       * `standardKeymap` owns it as { mac: "Cmd-ArrowUp", shift: selectDocStart },
       * i.e. Cmd+Shift+Up selects to the start of the document. Leaving it
       * alone (no Prec) preserves that core macOS gesture instead of breaking
       * it — hence the mac-only four-modifier fallback below.
       */
      {
        key: 'Mod-Shift-ArrowUp',
        run: copyLineUp,
        preventDefault: true,
      },
      {
        key: 'Mod-Shift-ArrowDown',
        run: copyLineDown,
        preventDefault: true,
      },
      {
        key: 'Alt-Shift-Mod-ArrowUp',
        mac: 'Alt-Shift-Cmd-ArrowUp',
        run: copyLineUp,
        preventDefault: true,
      },
      {
        key: 'Alt-Shift-Mod-ArrowDown',
        mac: 'Alt-Shift-Cmd-ArrowDown',
        run: copyLineDown,
        preventDefault: true,
      },
    ]),
    /*
     * KEYMAP PRECEDENCE — the actual fix for the dead Shift-Alt-Arrow* keys.
     *
     * CodeMirror resolves a keymap by facet precedence first and by
     * registration order within one precedence. This extension is mounted
     * through `compartments.paste.of(...)`, which comes AFTER
     * `createBaseEditorExtensions()` in the SqlEditor extension array, so its
     * plain `keymap.of(...)` above lost every conflict to the defaultKeymap
     * registered in `editorExtensions.ts`.
     *
     * `@codemirror/commands` binds `Shift-Alt-ArrowUp/Down` to
     * `copyLineUp/copyLineDown` with NO platform variant, so the conflict is
     * NOT macOS-only — it exists on every platform.
     *
     * `Prec.high` lifts ONLY these two bindings above defaultKeymap. Raising
     * the whole keymap would also outrank the editor's own custom shortcuts
     * (execute / save / Tab), which this extension must not do.
     *
     * `Escape` joins them for the reason tabulated in the file header: the
     * only competitor is `defaultKeymap`'s `simplifySelection`, which is
     * registered first at default precedence and would otherwise always win.
     * `exitMultiCursor` returns `false` when there is at most one range, so
     * adding it here steals nothing from `simplifySelection`'s single-cursor
     * behaviour, and it stays strictly below the autocomplete/snippet
     * dismissals at `Prec.highest`.
     *
     * Deliberately NO `preventDefault` on the Escape binding. CodeMirror
     * applies a binding's `preventDefault` even when its `run` returns false
     * (`runHandlers`, @codemirror/view index.js:9164), so declaring it would
     * make Escape swallow the event unconditionally — including the
     * degenerate one-cursor case where our command declines. Omitting it
     * matches `defaultKeymap`'s own Escape binding and keeps "nobody handled
     * this" observable, which is what lets `simplifySelection` and any outer
     * listener keep their turn.
     */
    Prec.high(
      keymap.of([
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
        {
          key: 'Escape',
          run: exitMultiCursor,
        },
      ]),
    ),
  ];
}
