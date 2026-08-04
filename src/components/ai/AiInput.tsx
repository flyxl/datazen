import { forwardRef } from 'react';
import { ArrowUp, Square } from 'lucide-react';
import { cn } from '../../lib/cn';
import { useAiKeyboard } from '../../hooks/useAiKeyboard';
import { useI18n } from '../../hooks/useI18n';

interface AiInputProps {
  value: string;
  onChange: (value: string) => void;
  onSubmit: () => void;
  placeholder?: string;
  disabled?: boolean;
  rows?: number;
  isLoading?: boolean;
  onStop?: () => void;
  className?: string;
}

export const AiInput = forwardRef<HTMLTextAreaElement, AiInputProps>(function AiInput(
  {
    value,
    onChange,
    onSubmit,
    placeholder,
    disabled,
    rows = 1,
    isLoading,
    onStop,
    className,
  },
  ref,
) {
  const { t } = useI18n();
  const aiKeyboard = useAiKeyboard(onSubmit);

  const canSend = value.trim().length > 0 && !isLoading;
  const showStop = isLoading && onStop;

  return (
    <div
      className={cn(
        'relative flex rounded-lg border border-edge bg-surface',
        'transition-colors focus-within:border-accent',
        disabled && 'opacity-50',
        className,
      )}
    >
      <textarea
        ref={ref}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        {...(disabled ? {} : aiKeyboard)}
        placeholder={placeholder}
        rows={rows}
        disabled={disabled}
        autoCapitalize="off"
        autoCorrect="off"
        spellCheck={false}
        className={cn(
          'flex-1 resize-none bg-transparent px-3 py-2 text-sm text-fg',
          'placeholder:text-fg-muted focus:outline-none',
          'disabled:cursor-not-allowed',
        )}
      />
      <div className="absolute bottom-1.5 right-1.5 flex items-end">
        {showStop ? (
          <button
            type="button"
            onMouseDown={(e) => e.preventDefault()}
            onClick={onStop}
            title={t('chat.stop')}
            className={cn(
              'flex h-6 w-6 items-center justify-center rounded-md',
              'bg-fg-muted/20 text-fg-muted hover:bg-fg-muted/30 hover:text-fg',
              'transition-colors',
            )}
          >
            <Square className="h-3 w-3" />
          </button>
        ) : (
          <button
            type="button"
            onMouseDown={(e) => e.preventDefault()}
            onClick={onSubmit}
            disabled={!canSend}
            className={cn(
              'flex h-6 w-6 items-center justify-center rounded-md transition-colors',
              canSend
                ? 'bg-accent text-white hover:bg-accent/90'
                : 'bg-fg-muted/10 text-fg-muted/40 cursor-not-allowed',
            )}
          >
            <ArrowUp className="h-3.5 w-3.5" />
          </button>
        )}
      </div>
    </div>
  );
});
