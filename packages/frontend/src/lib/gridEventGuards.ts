/**
 * Pure event guards shared by every grid interaction surface (DB-01 §3.10 / §4.5).
 *
 * Extracted from `useGridSelection` so the Wave 2 wiring (`DataTable` / VirtualBody) and the
 * later selection sub-volumes read the same rules instead of re-deriving them: an event that
 * belongs to an editor, or to an IME composition, is never a grid-level event.
 */

/**
 * Editable controls and open cell editors own their pointer and keyboard events. Both the
 * inline `<input>`/`<textarea>` editors and the `[data-dt-editing="true"]` cell wrapper are
 * covered so a click on the editor's padding does not move the grid selection.
 */
export function isEditableTarget(target: EventTarget | null): boolean {
  if (!(target instanceof Element)) return false;
  return (
    target.closest(
      'input, textarea, select, [contenteditable="true"], [data-dt-editing="true"]',
    ) !== null
  );
}

/**
 * Shape shared by native events (`KeyboardEvent`) and React synthetic events
 * (`React.KeyboardEvent`), which expose the same flags one level down in `nativeEvent`.
 */
export interface ComposingEventLike {
  readonly isComposing?: boolean;
  readonly keyCode?: number;
  readonly nativeEvent?: ComposingEventLike;
}

/**
 * True while an IME composition is active. Browsers disagree on the signal — Chrome/Safari
 * set `isComposing`, older engines and some IMEs only send the legacy `keyCode === 229` — so
 * both are checked, and the synthetic/native nesting is unwrapped explicitly.
 */
export function isComposingEvent(event: ComposingEventLike): boolean {
  if (event.isComposing === true) return true;
  if (event.keyCode === 229) return true;
  const native = event.nativeEvent;
  if (native === undefined) return false;
  return native.isComposing === true || native.keyCode === 229;
}
