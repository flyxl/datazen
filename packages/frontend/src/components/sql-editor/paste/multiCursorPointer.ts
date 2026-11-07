/**
 * Pointer gestures that grow a multi-cursor selection.
 *
 * `EditorView.clickAddsSelectionRange` REPLACES CodeMirror's own predicate
 * rather than extending it (`@codemirror/view:4841`):
 *
 * ```js
 * function addsSelectionRange(view, event) {
 *   let facet = view.state.facet(clickAddsSelectionRange);
 *   return facet.length ? facet[0](event) : browser.mac ? event.metaKey : event.ctrlKey;
 * }
 * ```
 *
 * The host registered `(e) => e.altKey && !e.shiftKey`, which silently dropped
 * the platform chord: Cmd+click on macOS and Ctrl+click on Windows/Linux did
 * not add a cursor. This module restores that chord alongside Alt+click instead
 * of inventing a new mapping, so the host and a stock CodeMirror agree on what
 * a bare modified click means.
 *
 * ## Why the platform chord rather than one chord everywhere
 *
 * - **macOS ⌘+click.** System-wide this is the "click through / ignore"
 *   gesture, and in a browser it opens a link in a background tab. Inside this
 *   app it is unclaimed: the SQL editor renders no links, and the webview is a
 *   WKWebView whose click-through gesture is not delivered to page content.
 *   This is the same chord Sublime Text and VS Code use for "add cursor", so
 *   the muscle memory transfers.
 * - **macOS ⌃+click is deliberately NOT a multi-cursor chord.** On macOS
 *   Control+click is the secondary-click gesture: WebKit raises `contextmenu`
 *   for it. The SQL editor binds `contextmenu` to the statement menu
 *   (`createDomEventHandlers`), so claiming ⌃+click would fire a cursor-add and
 *   a context menu from one gesture. CodeMirror's own default agrees, which is
 *   why the platform switch is one-sided.
 * - **Windows/Linux Ctrl+click.** No click-through or secondary-click meaning
 *   exists for Ctrl in WinWebView2, so it is free.
 *
 * ## Modifier conflict table
 *
 * | gesture              | behaviour                                              |
 * | -------------------- | ------------------------------------------------------ |
 * | click                | replace the selection with one cursor (CodeMirror)     |
 * | Alt/Option + click   | add a cursor                                           |
 * | ⌘ + click (macOS)    | add a cursor                                           |
 * | Ctrl + click (Win/Linux) | add a cursor                                       |
 * | Shift + click        | extend the selection — never a multi-cursor chord      |
 * | ⌘⌥ / Ctrl+Alt + click| add a cursor (Alt decides; Alt+**drag** is rectangular) |
 * | ⌘ + drag (macOS)     | add a range while dragging (restores the CM default)    |
 * | Alt + drag           | rectangular / column selection (unchanged)              |
 *
 * A click is not a drag, so the ⌘⌥ and Ctrl+Alt combinations resolve to
 * "add a cursor" here while their drag counterparts stay rectangular.
 */
import { isMacOS } from '../extensions/types';

/**
 * Whether a pointer event should grow the selection with a new cursor.
 *
 * Exported as a pure predicate over the modifier flags (the platform is read
 * through {@link isMacOS}) so the modifier table above can be asserted
 * directly, without mounting an editor.
 */
export function clickAddsCursor(e: {
  altKey: boolean;
  ctrlKey: boolean;
  metaKey: boolean;
  shiftKey: boolean;
}): boolean {
  // Shift+click extends the current selection. It is deliberately excluded on
  // every platform: it was excluded before this change and is a distinct,
  // still-supported gesture.
  if (e.shiftKey) return false;
  // Alt/Option+click is the pre-existing host chord and wins over everything
  // else, so ⌘⌥ and Ctrl+Alt are "add a cursor" and never ambiguous.
  if (e.altKey) return true;
  // The platform chord, restoring CodeMirror's overridden default.
  return isMacOS() ? e.metaKey : e.ctrlKey;
}
