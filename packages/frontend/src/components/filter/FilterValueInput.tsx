import { useEffect, useRef, useState } from 'react';
import { Input } from '../ui/Input';

/** Delay before writing a typed value into the draft (IME-safe). */
export const FILTER_VALUE_DEBOUNCE_MS = 350;

/**
 * Local draft + IME-aware value field.
 * Commits into the filter draft only (no query); parent Apply runs the query.
 *
 * The IME handling is the reason this component exists: a Chinese input
 * method confirms a candidate with Enter, and committing that keystroke would
 * cut the word in half. `composingRef` plus `nativeEvent.isComposing` keeps
 * the keystroke out of the draft until the composition ends.
 */
export function FilterValueInput({
  value,
  onCommit,
  placeholder,
  autoFocus,
  disabled,
  onEnterComplete,
}: {
  value: string;
  onCommit: (value: string) => void;
  placeholder: string;
  autoFocus?: boolean;
  disabled?: boolean;
  /** Called after Enter commits a non-empty value (collapse to chip). */
  onEnterComplete?: () => void;
}) {
  const [draft, setDraft] = useState(value);
  const composingRef = useRef(false);
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const draftRef = useRef(draft);
  draftRef.current = draft;

  useEffect(() => {
    if (!composingRef.current) setDraft(value);
  }, [value]);

  useEffect(
    () => () => {
      if (timerRef.current) clearTimeout(timerRef.current);
    },
    [],
  );

  const clearTimer = () => {
    if (timerRef.current) {
      clearTimeout(timerRef.current);
      timerRef.current = null;
    }
  };

  const flush = (next: string) => {
    clearTimer();
    if (next !== value) onCommit(next);
  };

  const schedule = (next: string) => {
    clearTimer();
    timerRef.current = setTimeout(() => flush(next), FILTER_VALUE_DEBOUNCE_MS);
  };

  return (
    <Input
      autoFocus={autoFocus}
      disabled={disabled}
      value={draft}
      onChange={(e) => {
        const next = e.target.value;
        setDraft(next);
        if (composingRef.current || (e.nativeEvent as InputEvent).isComposing) return;
        schedule(next);
      }}
      onCompositionStart={() => {
        composingRef.current = true;
        clearTimer();
      }}
      onCompositionEnd={(e) => {
        const next = e.currentTarget.value;
        setDraft(next);
        requestAnimationFrame(() => {
          composingRef.current = false;
        });
        schedule(next);
      }}
      onBlur={() => {
        if (composingRef.current) return;
        flush(draftRef.current);
      }}
      onKeyDown={(e) => {
        if (e.key === 'Enter' && !composingRef.current && !e.nativeEvent.isComposing) {
          e.preventDefault();
          flush(draftRef.current);
          if (draftRef.current.trim() !== '') onEnterComplete?.();
        }
      }}
      placeholder={placeholder}
      className="!h-7 w-24 shrink-0 !px-2 !text-xs"
      data-testid="filter-value"
    />
  );
}
