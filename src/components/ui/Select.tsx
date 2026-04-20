import { useCallback, useEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { ChevronDown } from 'lucide-react';
import { cn } from '../../lib/cn';

export interface SelectOption {
  readonly value: string;
  readonly label: string;
  readonly disabled?: boolean;
}

export interface SelectProps {
  readonly value: string | number;
  readonly options: readonly SelectOption[];
  readonly onChange: (value: string) => void;
  readonly placeholder?: string;
  readonly disabled?: boolean;
  readonly className?: string;
}

const LIST_ID = 'dz-select-listbox';

export function Select({ value, options, onChange, placeholder, disabled, className }: SelectProps) {
  const [open, setOpen] = useState(false);
  const [highlightIdx, setHighlightIdx] = useState(-1);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const listRef = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState<{ top: number; left: number; width: number }>({
    top: 0, left: 0, width: 0,
  });

  const strValue = String(value);
  const selectedOption = options.find((o) => o.value === strValue);

  const updatePosition = useCallback(() => {
    if (!triggerRef.current) return;
    const rect = triggerRef.current.getBoundingClientRect();
    const spaceBelow = globalThis.innerHeight - rect.bottom;
    const spaceAbove = rect.top;
    const listHeight = Math.min(options.length * 34 + 8, 240);
    const goUp = spaceBelow < listHeight && spaceAbove > spaceBelow;
    setPos({
      top: goUp ? rect.top - listHeight - 4 : rect.bottom + 4,
      left: rect.left,
      width: rect.width,
    });
  }, [options.length]);

  const handleOpen = useCallback(() => {
    if (disabled) return;
    updatePosition();
    setOpen(true);
    const enabledOptions = options.filter((o) => !o.disabled);
    const idx = enabledOptions.findIndex((o) => o.value === strValue);
    setHighlightIdx(idx >= 0 ? options.indexOf(enabledOptions[idx]) : 0);
  }, [disabled, updatePosition, options, strValue]);

  const handleSelect = useCallback((opt: SelectOption) => {
    if (opt.disabled) return;
    onChange(opt.value);
    setOpen(false);
    triggerRef.current?.focus();
  }, [onChange]);

  useEffect(() => {
    if (!open) return;
    const handler = (e: MouseEvent) => {
      if (
        triggerRef.current?.contains(e.target as Node) ||
        listRef.current?.contains(e.target as Node)
      ) return;
      setOpen(false);
    };
    globalThis.addEventListener('mousedown', handler);
    return () => globalThis.removeEventListener('mousedown', handler);
  }, [open]);

  useEffect(() => {
    if (!open || highlightIdx < 0 || !listRef.current) return;
    const item = listRef.current.children[highlightIdx] as HTMLElement | undefined;
    item?.scrollIntoView({ block: 'nearest' });
  }, [open, highlightIdx]);

  const handleKeyDown = useCallback((e: React.KeyboardEvent) => {
    if (!open) {
      if (e.key === 'ArrowDown' || e.key === 'ArrowUp' || e.key === 'Enter' || e.key === ' ') {
        e.preventDefault();
        handleOpen();
      }
      return;
    }

    switch (e.key) {
      case 'ArrowDown': {
        e.preventDefault();
        let next = highlightIdx;
        do { next = (next + 1) % options.length; } while (options[next].disabled && next !== highlightIdx);
        setHighlightIdx(next);
        break;
      }
      case 'ArrowUp': {
        e.preventDefault();
        let next = highlightIdx;
        do { next = (next - 1 + options.length) % options.length; } while (options[next].disabled && next !== highlightIdx);
        setHighlightIdx(next);
        break;
      }
      case 'Enter':
      case ' ': {
        e.preventDefault();
        const opt = options[highlightIdx];
        if (opt && !opt.disabled) handleSelect(opt);
        break;
      }
      case 'Escape':
      case 'Tab':
        setOpen(false);
        break;
    }
  }, [open, highlightIdx, options, handleOpen, handleSelect]);

  return (
    <>
      <button
        ref={triggerRef}
        type="button"
        aria-expanded={open}
        aria-haspopup="listbox"
        aria-controls={open ? LIST_ID : undefined}
        disabled={disabled}
        className={cn(
          'flex h-9 w-full items-center justify-between gap-1 rounded-md border border-edge bg-surface px-2.5 text-left text-sm text-fg outline-none',
          'focus:border-blue-500 focus:ring-2 focus:ring-blue-500/25',
          'disabled:cursor-not-allowed disabled:opacity-50',
          className,
        )}
        onClick={() => (open ? setOpen(false) : handleOpen())}
        onKeyDown={handleKeyDown}
      >
        <span className={cn('min-w-0 truncate', !selectedOption && 'text-fg-muted')}>
          {selectedOption?.label ?? placeholder ?? '请选择'}
        </span>
        <ChevronDown className={cn('h-3.5 w-3.5 shrink-0 text-fg-muted transition-transform', open && 'rotate-180')} />
      </button>

      {open && createPortal(
        <div
          ref={listRef}
          id={LIST_ID}
          className="fixed z-[9999] overflow-y-auto rounded-lg border border-edge bg-surface-alt py-1 shadow-xl"
          style={{ top: pos.top, left: pos.left, width: pos.width, maxHeight: 240 }}
        >
          {options.map((opt, idx) => {
            const isSelected = opt.value === strValue;
            const isHighlighted = idx === highlightIdx;
            return (
              <div
                key={opt.value}
                tabIndex={opt.disabled ? undefined : -1}
                aria-selected={isSelected}
                className={cn(
                  'flex cursor-pointer items-center px-2.5 py-1.5 text-sm transition-colors',
                  opt.disabled && 'cursor-not-allowed opacity-40',
                  isHighlighted && !opt.disabled && 'bg-surface-raised',
                  isSelected && !isHighlighted && 'text-accent',
                )}
                onMouseEnter={() => { if (!opt.disabled) setHighlightIdx(idx); }}
                onMouseDown={(e) => { e.preventDefault(); handleSelect(opt); }}
              >
                <span className="min-w-0 truncate">{opt.label}</span>
                {isSelected && <span className="ml-auto pl-2 text-accent">✓</span>}
              </div>
            );
          })}
        </div>,
        document.body,
      )}
    </>
  );
}
