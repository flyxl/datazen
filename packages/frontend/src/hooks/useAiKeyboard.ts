import { useCallback, useRef } from 'react';

/**
 * Shared keyboard + IME composition handler for AI input fields.
 * Enter submits, Shift+Enter inserts newline, IME composition is respected.
 *
 * Returns an object that can be spread directly onto `<input>` / `<textarea>`:
 * ```tsx
 * const aiKeyboard = useAiKeyboard(handleSubmit);
 * <textarea {...aiKeyboard} />
 * ```
 *
 * Uses both `compositionstart/end` tracking AND `nativeEvent.isComposing`
 * for maximum cross-platform reliability (WKWebView, Chromium, Firefox).
 */
export function useAiKeyboard(onSubmit: () => void) {
  const composingRef = useRef(false);

  const onCompositionStart = useCallback(() => {
    composingRef.current = true;
  }, []);

  const onCompositionEnd = useCallback(() => {
    // Use requestAnimationFrame to clear after any pending keydown.
    // In Chromium, compositionend fires BEFORE the final keydown;
    // in WebKit/Firefox, it fires AFTER. rAF covers both cases.
    requestAnimationFrame(() => {
      composingRef.current = false;
    });
  }, []);

  const onKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (
        e.key === 'Enter' &&
        !e.shiftKey &&
        !composingRef.current &&
        !e.nativeEvent.isComposing
      ) {
        e.preventDefault();
        onSubmit();
      }
    },
    [onSubmit],
  );

  return { onKeyDown, onCompositionStart, onCompositionEnd };
}
