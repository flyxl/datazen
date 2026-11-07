import { useEffect, useRef, useState } from 'react';
import { X } from 'lucide-react';
import { Input } from '../ui/Input';

/**
 * The value editor of a set operator (`in` / `notIn`): a list of chips plus a
 * text field that turns into a chip on Enter or on a comma.
 *
 * **IME discipline**: a Chinese input method confirms the candidate word with
 * Enter, and turning that keystroke into a chip would cut the word in half.
 * The composition flag therefore blocks the Enter shortcut; the chip is only
 * created once the composition ended.
 *
 * The value is a list, never a comma joined string: a value that contains a
 * comma has to survive the round trip (the back end splits a comma string,
 * so joining here would change the meaning of the value).
 */
export function FilterTokenInput({
  value,
  onCommit,
  placeholder,
  autoFocus,
  disabled,
  onEnterComplete,
  removeLabel,
}: {
  value: string[];
  onCommit: (tokens: string[]) => void;
  placeholder: string;
  autoFocus?: boolean;
  disabled?: boolean;
  /** Called after a value was committed and the list is not empty any more. */
  onEnterComplete?: () => void;
  /** `aria-label` of the button that removes one token. */
  removeLabel: string;
}) {
  const [text, setText] = useState('');
  const composingRef = useRef(false);

  useEffect(
    () => () => {
      composingRef.current = false;
    },
    [],
  );

  const commitText = () => {
    const token = text.trim();
    if (token === '') return false;
    setText('');
    onCommit([...value, token]);
    return true;
  };

  return (
    <div
      className="flex min-w-0 max-w-[16rem] flex-nowrap items-center gap-1"
      data-testid="filter-token-input"
    >
      <div className="flex min-w-0 flex-1 flex-wrap items-center gap-0.5">
        {value.map((token, index) => (
          <span
            key={`${token}-${index}`}
            className="inline-flex max-w-[8rem] items-center gap-0.5 rounded bg-accent/15 px-1 py-px text-[11px] text-accent"
            data-testid="filter-token"
          >
            <span className="truncate">{token}</span>
            <button
              type="button"
              className="shrink-0 rounded-full p-0.5 hover:bg-accent/20"
              disabled={disabled}
              onClick={() => onCommit(value.filter((_, position) => position !== index))}
              aria-label={removeLabel}
              data-testid="filter-token-remove"
            >
              <X className="h-2.5 w-2.5" />
            </button>
          </span>
        ))}
        <Input
          autoFocus={autoFocus}
          disabled={disabled}
          value={text}
          onChange={(e) => setText(e.target.value)}
          onCompositionStart={() => {
            composingRef.current = true;
          }}
          onCompositionEnd={() => {
            // The flag is cleared in the next frame: the composition end event
            // and the keydown that follows it belong to the same keystroke.
            requestAnimationFrame(() => {
              composingRef.current = false;
            });
          }}
          onKeyDown={(e) => {
            const composing = composingRef.current || e.nativeEvent.isComposing;
            if (e.key === ',' && !composing) {
              e.preventDefault();
              commitText();
              return;
            }
            if (e.key === 'Enter' && !composing) {
              e.preventDefault();
              if (commitText()) onEnterComplete?.();
              return;
            }
            if (e.key === 'Backspace' && text === '' && value.length > 0) {
              e.preventDefault();
              onCommit(value.slice(0, -1));
            }
          }}
          onBlur={() => {
            if (composingRef.current) return;
            commitText();
          }}
          placeholder={placeholder}
          className="!h-7 w-20 min-w-0 shrink-0 !px-2 !text-xs"
          data-testid="filter-token-field"
        />
      </div>
    </div>
  );
}
