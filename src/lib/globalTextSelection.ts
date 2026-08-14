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
