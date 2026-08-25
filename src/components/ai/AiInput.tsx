import { forwardRef, useCallback, useImperativeHandle, useRef, useState } from 'react';
import { ArrowUp, File, Folder, Square, Table2 } from 'lucide-react';
import { cn } from '../../lib/cn';
import { useAiKeyboard } from '../../hooks/useAiKeyboard';
import { useI18n } from '../../hooks/useI18n';
import { ContextPicker } from './ContextPicker';
import type { ContextItem } from '../../types';

function TokenIcon({ kind }: { kind: ContextItem['kind'] }) {
  const className = 'h-3 w-3 shrink-0 opacity-80';
  if (kind === 'table') return <Table2 className={className} aria-hidden />;
  if (kind === 'dir') return <Folder className={className} aria-hidden />;
  return <File className={className} aria-hidden />;
}

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
  contextItems?: ContextItem[];
  onContextItemsChange?: (items: ContextItem[]) => void;
  dbSessionId?: string;
  database?: string;
  /** Where the @ picker opens relative to the input. Default: above. */
  pickerPosition?: 'above' | 'below';
  /** Hide the built-in send/stop control (e.g. NL2SQL uses an external Generate button). */
  hideSubmit?: boolean;
}

function itemKey(item: ContextItem): string {
  return `${item.kind}:${item.id}`;
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
    contextItems,
    onContextItemsChange,
    dbSessionId,
    database,
    pickerPosition = 'above',
    hideSubmit = false,
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
  const hasContext = onContextItemsChange !== undefined;

  const handleChange = useCallback(
    (e: React.ChangeEvent<HTMLTextAreaElement>) => {
      const newValue = e.target.value;
      onChange(newValue);

      if (!hasContext) return;

      const cursorPos = e.target.selectionStart ?? newValue.length;
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

      const textarea = textareaRef.current;
      if (
        onContextItemsChange &&
        contextItems &&
        contextItems.length > 0 &&
        textarea &&
        (e.key === 'Backspace' || (e.key === 'Delete' && value.length === 0))
      ) {
        const start = textarea.selectionStart ?? 0;
        const end = textarea.selectionEnd ?? 0;
        if (start === 0 && end === 0) {
          e.preventDefault();
          onContextItemsChange(contextItems.slice(0, -1));
          return;
        }
      }

      aiKeyboard.onKeyDown?.(e as unknown as React.KeyboardEvent<HTMLTextAreaElement>);
    },
    [showPicker, disabled, aiKeyboard, onContextItemsChange, contextItems, value.length],
  );

  const handleSelect = useCallback(
    (item: ContextItem) => {
      if (!onContextItemsChange || !contextItems) return;

      if (!contextItems.some((i) => itemKey(i) === itemKey(item))) {
        onContextItemsChange([...contextItems, item]);
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
    [onContextItemsChange, contextItems, value, onChange],
  );

  const hasTokens = Boolean(contextItems && contextItems.length > 0);
  const showPlaceholder = !hasTokens && value.length === 0;

  return (
    <div ref={wrapperRef} className={cn('relative', className)}>
      <div
        className={cn(
          'relative rounded-lg border border-edge bg-surface',
          'transition-colors focus-within:border-accent',
          disabled && 'opacity-50',
        )}
        onMouseDown={(e) => {
          // Keep focus in the textarea when clicking chips / padding (span-like field).
          if (e.target === e.currentTarget || (e.target as HTMLElement).closest('[data-testid="context-token"]')) {
            e.preventDefault();
            textareaRef.current?.focus();
          }
        }}
      >
        {showPicker && hasContext && (
          <ContextPicker
            query={pickerQuery}
            onSelect={handleSelect}
            onClose={() => setShowPicker(false)}
            anchorRef={wrapperRef}
            position={pickerPosition}
            dbSessionId={dbSessionId}
            database={database}
          />
        )}

        {/* Chips + text share one wrapping line (Android span-like), not a stacked block. */}
        <div
          data-testid="ai-input-field"
          className={cn(
            'flex flex-wrap items-center gap-x-1.5 gap-y-1',
            'px-3 py-2',
            !hideSubmit && 'pr-10',
          )}
        >
          {hasTokens &&
            contextItems!.map((item) => (
              <span
                key={itemKey(item)}
                data-testid="context-token"
                data-kind={item.kind}
                data-id={item.id}
                className={cn(
                  'inline-flex max-w-full shrink-0 items-center gap-1',
                  'rounded-md bg-accent/15 px-1.5 py-0.5',
                  'text-[12px] leading-5 text-accent',
                )}
              >
                <TokenIcon kind={item.kind} />
                <span className="truncate">@{item.name}</span>
              </span>
            ))}

          <textarea
            ref={textareaRef}
            value={value}
            onChange={handleChange}
            onKeyDown={handleKeyDown}
            onCompositionStart={aiKeyboard.onCompositionStart}
            onCompositionEnd={aiKeyboard.onCompositionEnd}
            placeholder={
              showPlaceholder
                ? placeholder !== undefined
                  ? placeholder
                  : hasContext
                    ? t('context.placeholder')
                    : undefined
                : undefined
            }
            rows={rows}
            disabled={disabled}
            autoCapitalize="off"
            autoCorrect="off"
            spellCheck={false}
            className={cn(
              // Grow on the same flex line as chips; wrap only when the line is full.
              'min-h-[1.5rem] min-w-[6rem] flex-1 basis-[6rem] resize-none',
              'bg-transparent py-0 text-sm leading-5 text-fg',
              'placeholder:text-fg-muted focus:outline-none',
              'disabled:cursor-not-allowed',
            )}
          />
        </div>

        {!hideSubmit && (
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
        )}
      </div>
    </div>
  );
});
