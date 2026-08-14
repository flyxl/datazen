/**
 * Make application text selectable while keeping Cmd/Ctrl+A scoped to
 * native editable controls and CodeMirror editors.
 */
export function installGlobalTextSelectionPolicy(): () => void {
  const body = document.body;
  const previousUserSelect = body.style.userSelect;
  const previousWebkitUserSelect = body.style.webkitUserSelect;

  body.style.userSelect = 'text';
  body.style.webkitUserSelect = 'text';

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
    // their own select-all behavior. CodeMirror's Cmd/Ctrl+A selects only its
    // editor document rather than the surrounding WebView.
    if (
      target.closest(
        'input, textarea, select, [contenteditable="true"], [contenteditable="plaintext-only"], .cm-editor',
      )
    ) {
      return;
    }

    // Prevent the browser WebView from selecting every selectable text node.
    event.preventDefault();
  };

  document.addEventListener('keydown', onKeyDown, true);

  return () => {
    document.removeEventListener('keydown', onKeyDown, true);
    body.style.userSelect = previousUserSelect;
    body.style.webkitUserSelect = previousWebkitUserSelect;
  };
}
