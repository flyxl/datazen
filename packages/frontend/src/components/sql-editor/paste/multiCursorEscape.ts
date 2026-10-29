/**
 * Escape: leave the multi-cursor state.
 *
 * ## Why a dedicated command instead of `simplifySelection`
 *
 * Escape is bound five times over (see the full inventory in
 * `multipleSelections.ts`). The one that used to answer in the editor content
 * is `simplifySelection` from `defaultKeymap`
 * (`@codemirror/commands@6.10.3:1774`), which for a multi-range selection does:
 *
 * ```js
 * if (cur.ranges.length > 1) selection = EditorSelection.create([cur.main]);
 * ```
 *
 * That reduces the range count but keeps the surviving range **as it is** — so
 * after a `Mod-d` chain (where every range is a selected word) Escape leaves a
 * still-highlighted word behind, and a second Escape is needed to reach a bare
 * caret. The exit state is therefore ambiguous. This command collapses to an
 * empty cursor instead, which makes the post-Escape state unambiguous: exactly
 * one bare cursor, and a second Escape is a defined no-op.
 *
 * ## State machine
 *
 *   S0  one cursor, empty or not      Escape -> handled by `simplifySelection`
 *                                      (a non-empty range becomes a cursor;
 *                                       an empty one is a no-op)
 *   S1  two or more cursors           Escape -> THIS command: collapse to one
 *                                      empty cursor at the main range's head,
 *                                      dropping any rectangular state
 *   S2  autocomplete open             Escape -> `closeCompletion` at
 *                                      `Prec.highest`, above this binding, so
 *                                      the multi-cursor survives; the next
 *                                      Escape runs this command
 *   S3  snippet session open          Escape -> `clearSnippet` at
 *                                      `Prec.highest`, same shape as S2
 *   S4  search panel focused          Escape -> `closeSearchPanel`, scoped to
 *                                      `"editor search-panel"`, so it only
 *                                      applies while focus is inside the panel
 *
 * There is no zero-cursor state: `EditorSelection` always holds at least one
 * range, so "Escape with nothing to collapse" is exactly S0, and this command
 * returns `false` there and lets `simplifySelection` decide.
 */
import { EditorSelection } from '@codemirror/state';
import type { Command } from '@codemirror/view';
import type { EditorView } from '@codemirror/view';

/**
 * Collapse a multi-cursor selection to a single empty cursor.
 *
 * Returns `false` — leaving Escape to `simplifySelection` — whenever there is
 * nothing to exit, so this command never shadows the single-cursor case.
 */
export const exitMultiCursor: Command = (view: EditorView): boolean => {
  const { selection } = view.state;
  if (selection.ranges.length <= 1) return false;
  // `main` is the range the user last moved, i.e. the one CodeMirror already
  // treats as primary. It becomes a bare cursor at its head, so a multi-cursor
  // that started from a selected word (`Mod-d`) does not keep that highlight.
  // `EditorSelection.create` also drops any rectangular state the selection
  // carried, which is what makes Escape leave column selection as well.
  view.dispatch({
    selection: EditorSelection.single(view.state.selection.main.head),
    scrollIntoView: true,
  });
  return true;
};
