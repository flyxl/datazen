/**
 * Keep normal application text selectable by mouse drag while preventing
 * Cmd/Ctrl+A from selecting the entire WebView.
 *
 * Native editable controls and CodeMirror keep their own select-all behavior.
 */
export function installGlobalTextSelectionPolicy(): () => void {
  const onKeyDown = (event: KeyboardEvent) => {
    if (!(event.metaKey || event.ctrlKey) || event.key.toLowerCase() !== 'a') {
      return;
    }

    const target = event.target;
    if (!(target instanceof Element)) {
      event.preventDefault();
      return;
    }

    // Let native controls, contenteditable elements, and CodeMirror handle
    // their own select-all behavior.
    if (
      target.closest(
        'input, textarea, select, [contenteditable="true"], [contenteditable="plaintext-only"], .cm-editor',
      )
    ) {
      return;
    }

    // Do not let Cmd/Ctrl+A select all text in the surrounding WebView.
    // Normal mouse-drag text selection remains fully native.
    event.preventDefault();
  };

  document.addEventListener('keydown', onKeyDown, true);

  return () => {
    document.removeEventListener('keydown', onKeyDown, true);
  };
}

/**
 * Suppress the browser's right-button drag selection.
 *
 * Chromium/WebKit select text while the *right* button is held and dragged,
 * painting a large blue block even though the user only wants the context
 * menu. A plain right-click (no movement) must NOT clear an existing
 * left-button selection, so "select text, then right-click to copy" still
 * works — only movement with the right button held clears the selection.
 */
export function installRightDragSelectionSuppressor(): () => void {
  let rightDown = false;

  const onMouseDown = (e: MouseEvent) => {
    if (e.button === 2) rightDown = true;
  };

  // A mousemove while the right button is held means the user is dragging:
  // drop whatever the webview selected so no blue block is painted. A plain
  // right-click never fires mousemove, so an existing left-button selection
  // survives for "select → right-click to copy".
  const onMouseMove = (e: MouseEvent) => {
    if (rightDown && e.buttons & 2) clearSelection();
  };

  const onMouseUp = (e: MouseEvent) => {
    if (e.button === 2) rightDown = false;
  };

  const onBlur = () => {
    rightDown = false;
  };

  document.addEventListener('mousedown', onMouseDown);
  document.addEventListener('mousemove', onMouseMove);
  document.addEventListener('mouseup', onMouseUp);
  window.addEventListener('blur', onBlur);

  return () => {
    document.removeEventListener('mousedown', onMouseDown);
    document.removeEventListener('mousemove', onMouseMove);
    document.removeEventListener('mouseup', onMouseUp);
    window.removeEventListener('blur', onBlur);
  };
}

function clearSelection() {
  const sel = window.getSelection();
  if (sel && !sel.isCollapsed) sel.removeAllRanges();
}

/**
 * Guard against stray left-button drags painting a huge selection block.
 *
 * Selection is only allowed to start on content that is actually selectable
 * (computed `user-select: text` — tables, logs, code, `.selectable` etc.).
 * When the left button goes down on a blank area, background, scrollbar area
 * or control (`user-select: none`), the webview would otherwise select every
 * piece of text the drag passes over. If the press started on such an element
 * and the mouse moves, the selection is cleared.
 *
 * It also keeps an existing selection stable: a single press inside a
 * non-collapsed selection makes the browser restart the selection on the next
 * drag, so a double-clicked word's highlight "flashes" away. Presses with
 * `detail === 2` (the second press of a double-click) are left untouched so
 * double-click word selection still works.
 */
export function installDragSelectionGuard(): () => void {
  let pressStart: { target: Element; selectable: boolean } | null = null;

  const isSelectable = (el: Element | null): boolean => {
    if (!el) return false;
    return getComputedStyle(el).userSelect === 'text';
  };

  const onMouseDown = (e: MouseEvent) => {
    if (e.button !== 0) return;

    if (e.detail === 1) {
      const sel = window.getSelection();
      if (sel && !sel.isCollapsed && sel.rangeCount > 0 && e.target instanceof Element) {
        const range = sel.getRangeAt(0);
        if (range.intersectsNode(e.target)) {
          e.preventDefault();
        }
      }
    }

    const target = e.target instanceof Element ? e.target : null;
    pressStart = { target: target ?? document.body, selectable: isSelectable(target) };
  };

  const onMouseMove = (e: MouseEvent) => {
    if (!pressStart || !(e.buttons & 1)) return;
    // The press did not begin on selectable content → this is a stray drag
    // (window/scrollbar/blank area), drop whatever got selected.
    if (!pressStart.selectable) clearSelection();
  };

  const onMouseUp = () => {
    pressStart = null;
  };

  const onBlur = () => {
    pressStart = null;
  };

  document.addEventListener('mousedown', onMouseDown);
  document.addEventListener('mousemove', onMouseMove);
  document.addEventListener('mouseup', onMouseUp);
  window.addEventListener('blur', onBlur);

  return () => {
    document.removeEventListener('mousedown', onMouseDown);
    document.removeEventListener('mousemove', onMouseMove);
    document.removeEventListener('mouseup', onMouseUp);
    window.removeEventListener('blur', onBlur);
  };
}
