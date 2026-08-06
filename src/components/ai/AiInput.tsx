import { forwardRef, useCallback, useImperativeHandle, useRef, useState } from 'react';
import { ArrowUp, Square, X } from 'lucide-react';
import { cn } from '../../lib/cn';
import { useAiKeyboard } from '../../hooks/useAiKeyboard';
import { useI18n } from '../../hooks/useI18n';
import { ContextPicker } from './ContextPicker';
import type { ContextEntry } from '../../types';

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
  contextFiles?: ContextEntry[];
  onContextFilesChange?: (files: ContextEntry[]) => void;
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
    contextFiles,
    onContextFilesChange,
  },
  ref,
) {
  const { t } = useI18n();
  const aiKeyboard = useAiKeyboard(onSubmit);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const wrapperRef = useRef<HTMLDivElement>(null);

  useImperativeHandle(ref, () => textareaRef.current!, []);

  const [showPicker, setShowPicker] = useState(false);
  const [pickerQuery, setPickerQuery] = useState('');

  const canSend = value.trim().length > 0 && !isLoading;
  const showStop = isLoading && onStop;
  const hasContext = onContextFilesChange !== undefined;

  const handleChange = useCallback(
    (e: React.ChangeEvent<HTMLTextAreaElement>) => {
      const newValue = e.target.value;
      onChange(newValue);

      if (!hasContext) return;

      const cursorPos = e.target.selectionStart;
      const textBefore = newValue.substring(0, cursorPos);
      const atMatch = textBefore.match(/@([^\s@]*)$/);

      if (atMatch) {
        setShowPicker(true);
        setPickerQuery(atMatch[1]);
      } else {
        setShowPicker(false);
        setPickerQuery('');
      }
    },
    [onChange, hasContext],
  );

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
      if (showPicker && (e.key === 'ArrowUp' || e.key === 'ArrowDown' || e.key === 'Enter' || e.key === 'Escape')) {
        return;
      }
      if (disabled) return;
      aiKeyboard.onKeyDown?.(e as unknown as React.KeyboardEvent<HTMLTextAreaElement>);
    },
    [showPicker, disabled, aiKeyboard],
  );

  const handleSelect = useCallback(
    (entry: ContextEntry) => {
      if (!onContextFilesChange || !contextFiles) return;

      if (!contextFiles.some((f) => f.path === entry.path)) {
        onContextFilesChange([...contextFiles, entry]);
      }

      const textarea = textareaRef.current;
      if (textarea) {
        const cursorPos = textarea.selectionStart;
        const textBefore = value.substring(0, cursorPos);
        const textAfter = value.substring(cursorPos);
        const atStart = textBefore.lastIndexOf('@');
        if (atStart >= 0) {
          const newValue = textBefore.substring(0, atStart) + textAfter.replace(/^\s*/, '');
          onChange(newValue.trimStart() === '' ? '' : newValue);
        }
      }

      setShowPicker(false);
      setPickerQuery('');
      textarea?.focus();
    },
    [onContextFilesChange, contextFiles, value, onChange],
  );

  const handleRemoveContext = useCallback(
    (path: string) => {
      if (onContextFilesChange && contextFiles) {
        onContextFilesChange(contextFiles.filter((f) => f.path !== path));
      }
    },
    [onContextFilesChange, contextFiles],
  );

  const hasChips = contextFiles && contextFiles.length > 0;

  return (
    <div ref={wrapperRef} className={cn('relative', className)}>
      <div
        className={cn(
          'relative flex flex-wrap items-end rounded-lg border border-edge bg-surface',
          'transition-colors focus-within:border-accent',
          disabled && 'opacity-50',
        )}
      >
        {showPicker && hasContext && (
          <ContextPicker
            query={pickerQuery}
            onSelect={handleSelect}
            onClose={() => setShowPicker(false)}
            anchorRef={wrapperRef}
          />
        )}

        {/* Context chips — inline at the start of the input */}
        {hasChips && (
          <div className="flex flex-wrap gap-1 pl-2 pt-2">
            {contextFiles.map((f) => (
              <span
                key={f.path}
                className="inline-flex items-center gap-0.5 rounded bg-accent/10 px-1.5 py-0.5 text-[11px] text-accent"
              >
                @{f.name}
                <button
                  type="button"
                  className="rounded hover:bg-accent/20"
                  onMouseDown={(e) => e.preventDefault()}
                  onClick={() => handleRemoveContext(f.path)}
                >
                  <X className="h-2.5 w-2.5" />
                </button>
              </span>
            ))}
          </div>
        )}

        <textarea
          ref={textareaRef}
          value={value}
          onChange={handleChange}
          onKeyDown={handleKeyDown}
          onCompositionStart={aiKeyboard.onCompositionStart}
          onCompositionEnd={aiKeyboard.onCompositionEnd}
          placeholder={hasContext ? t('context.placeholder') : placeholder}
          rows={rows}
          disabled={disabled}
          autoCapitalize="off"
          autoCorrect="off"
          spellCheck={false}
          className={cn(
            'min-w-0 flex-1 resize-none bg-transparent px-3 py-2 text-sm text-fg',
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
    </div>
  );
});
