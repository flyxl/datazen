import { useCallback, useEffect, useId, useMemo, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { ChevronDown } from 'lucide-react';
import { cn } from '../../lib/cn';
import { useI18n } from '../../hooks/useI18n';

export interface SelectOption {
  readonly value: string;
  readonly label: string;
  readonly disabled?: boolean;
  readonly title?: string;
}

export interface SelectProps {
  readonly value: string | number;
  readonly options: readonly SelectOption[];
  readonly onChange: (value: string) => void;
  readonly placeholder?: string;
  readonly disabled?: boolean;
  readonly className?: string;
  readonly title?: string;
  /** Combobox: type in the trigger field to filter options by label or value. */
  readonly searchable?: boolean;
  /** Shrink trigger width to the current label/query; pair with `max-w-*` on `className`. */
  readonly fitContent?: boolean;
  /** Keep the option list readable when a compact trigger is used. */
  readonly listMinWidth?: number;
}

function filterOptions(options: readonly SelectOption[], query: string): SelectOption[] {
  const q = query.trim().toLowerCase();
  if (!q) return [...options];
  return options.filter(
    (o) => o.label.toLowerCase().includes(q) || o.value.toLowerCase().includes(q),
  );
}

const triggerShellClass =
  'flex items-center rounded-md border border-edge bg-surface text-left text-sm text-fg outline-none focus-within:border-blue-500 focus-within:ring-2 focus-within:ring-blue-500/25 disabled:cursor-not-allowed disabled:opacity-50';

function fitContentCharCount(text: string, min = 2, max = 12): number {
  return Math.max(min, Math.min(text.length, max));
}

function OptionList({
  filteredOptions,
  strValue,
  highlightIdx,
  setHighlightIdx,
  handleSelect,
  optionId,
  noMatchesLabel,
}: {
  filteredOptions: SelectOption[];
  strValue: string;
  highlightIdx: number;
  setHighlightIdx: (idx: number) => void;
  handleSelect: (opt: SelectOption) => void;
  optionId: (value: string, index: number) => string;
  noMatchesLabel: string;
}) {
  if (filteredOptions.length === 0) {
    return <div className="px-2.5 py-2 text-sm text-fg-muted">{noMatchesLabel}</div>;
  }
  return (
    <>
      {filteredOptions.map((opt, idx) => {
        const isSelected = opt.value === strValue;
        const isHighlighted = idx === highlightIdx;
        return (
          <div
            key={opt.value}
            id={optionId(opt.value, idx)}
            role="option"
            data-option-idx={idx}
            tabIndex={opt.disabled ? undefined : -1}
            aria-selected={isSelected}
            aria-disabled={opt.disabled || undefined}
            title={opt.title}
            className={cn(
              'flex cursor-pointer items-center px-2.5 py-1.5 text-sm transition-colors',
              opt.disabled && 'cursor-not-allowed opacity-40',
              isHighlighted && !opt.disabled && 'bg-surface-raised',
              isSelected && !isHighlighted && 'text-accent',
            )}
            onMouseEnter={() => {
              if (!opt.disabled) setHighlightIdx(idx);
            }}
            onMouseDown={(e) => {
              e.preventDefault();
              handleSelect(opt);
            }}
          >
            <span className="min-w-0 truncate">{opt.label}</span>
            {isSelected && <span className="ml-auto pl-2 text-accent">✓</span>}
          </div>
        );
      })}
    </>
  );
}

export function Select({
  value,
  options,
  onChange,
  placeholder,
  disabled,
  className,
  title,
  searchable = false,
  fitContent = false,
  listMinWidth,
}: SelectProps) {
  const { t } = useI18n();
  const [open, setOpen] = useState(false);
  const [highlightIdx, setHighlightIdx] = useState(-1);
  const [filterQuery, setFilterQuery] = useState('');
  const selectId = useId().replace(/:/g, '');
  const listId = `dz-select-listbox-${selectId}`;
  const accessibleLabel = title ?? placeholder ?? t('select.placeholder');
  const triggerRef = useRef<HTMLDivElement | HTMLButtonElement>(null);
  const listRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  const [pos, setPos] = useState<{ top: number; left: number; width: number }>({
    top: 0,
    left: 0,
    width: 0,
  });

  const strValue = String(value);
  const selectedOption = options.find((o) => o.value === strValue);
  const filteredOptions = useMemo(
    () => (searchable ? filterOptions(options, filterQuery) : [...options]),
    [options, filterQuery, searchable],
  );
  const optionId = useCallback(
    (optionValue: string, index: number) =>
      `${listId}-option-${index}-${optionValue.replace(/[^a-zA-Z0-9_-]/g, '-')}`,
    [listId],
  );
  const activeDescendant =
    highlightIdx >= 0 && filteredOptions[highlightIdx]
      ? optionId(filteredOptions[highlightIdx].value, highlightIdx)
      : undefined;

  const updatePosition = useCallback(() => {
    if (!triggerRef.current) return;
    const rect = triggerRef.current.getBoundingClientRect();
    const spaceBelow = globalThis.innerHeight - rect.bottom;
    const spaceAbove = rect.top;
    const listHeight = Math.min(filteredOptions.length * 34 + 8, 240);
    const goUp = spaceBelow < listHeight && spaceAbove > spaceBelow;
    const preferredWidth = Math.max(rect.width, listMinWidth ?? 0);
    const availableRight = globalThis.innerWidth - rect.left - 8;
    const width =
      availableRight > 0
        ? Math.min(preferredWidth, Math.max(rect.width, availableRight))
        : preferredWidth;
    setPos({
      top: goUp ? rect.top - listHeight - 4 : rect.bottom + 4,
      left: rect.left,
      width,
    });
  }, [filteredOptions.length, listMinWidth]);

  const pickHighlightIndex = useCallback(
    (list: readonly SelectOption[]) => {
      const enabled = list.filter((o) => !o.disabled);
      const idx = enabled.findIndex((o) => o.value === strValue);
      if (idx >= 0) return list.indexOf(enabled[idx]);
      return list.findIndex((o) => !o.disabled);
    },
    [strValue],
  );

  const handleOpen = useCallback(() => {
    if (disabled) return;
    updatePosition();
    setFilterQuery('');
    setOpen(true);
    setHighlightIdx(pickHighlightIndex(filterOptions(options, '')));
  }, [disabled, updatePosition, options, pickHighlightIndex]);

  const handleClose = useCallback(() => {
    setOpen(false);
    setFilterQuery('');
  }, []);

  const handleSelect = useCallback(
    (opt: SelectOption) => {
      if (opt.disabled) return;
      onChange(opt.value);
      handleClose();
      if (searchable) {
        inputRef.current?.focus();
      } else {
        (triggerRef.current as HTMLButtonElement | null)?.focus();
      }
    },
    [onChange, handleClose, searchable],
  );

  useEffect(() => {
    if (!open) return;
    const handler = (e: MouseEvent) => {
      if (
        triggerRef.current?.contains(e.target as Node) ||
        listRef.current?.contains(e.target as Node)
      )
        return;
      handleClose();
    };
    globalThis.addEventListener('mousedown', handler);
    return () => globalThis.removeEventListener('mousedown', handler);
  }, [open, handleClose]);

  useEffect(() => {
    if (!open) return;
    updatePosition();
    if (searchable) {
      inputRef.current?.focus();
    }
  }, [open, searchable, updatePosition]);

  useEffect(() => {
    if (!open) return;
    setHighlightIdx(pickHighlightIndex(filteredOptions));
  }, [filterQuery, open, filteredOptions, pickHighlightIndex]);

  useEffect(() => {
    if (!open || highlightIdx < 0 || !listRef.current) return;
    const item = listRef.current.querySelector(`[data-option-idx="${highlightIdx}"]`) as
      | HTMLElement
      | undefined;
    item?.scrollIntoView?.({ block: 'nearest' });
  }, [open, highlightIdx, filteredOptions]);

  const moveHighlight = useCallback(
    (direction: 1 | -1) => {
      if (filteredOptions.length === 0) return;
      const enabledIndexes = filteredOptions.reduce<number[]>((indexes, option, index) => {
        if (!option.disabled) indexes.push(index);
        return indexes;
      }, []);
      if (enabledIndexes.length === 0) return;
      let next = highlightIdx < 0 ? enabledIndexes[direction === 1 ? 0 : enabledIndexes.length - 1] : highlightIdx;
      for (let i = 0; i < filteredOptions.length; i += 1) {
        next = (next + direction + filteredOptions.length) % filteredOptions.length;
        if (!filteredOptions[next]?.disabled) break;
      }
      setHighlightIdx(next);
    },
    [filteredOptions, highlightIdx],
  );

  const closeFromKeyboard = useCallback(() => {
    handleClose();
    (triggerRef.current as HTMLElement | null)?.focus();
  }, [handleClose]);

  const handleListKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (!open) return;
      switch (e.key) {
        case 'ArrowDown': {
          e.preventDefault();
          moveHighlight(1);
          break;
        }
        case 'ArrowUp': {
          e.preventDefault();
          moveHighlight(-1);
          break;
        }
        case 'Home': {
          e.preventDefault();
          setHighlightIdx(filteredOptions.findIndex((option) => !option.disabled));
          break;
        }
        case 'End': {
          e.preventDefault();
          setHighlightIdx(
            filteredOptions.reduce((last, option, index) => (option.disabled ? last : index), -1),
          );
          break;
        }
        case 'Enter': {
          e.preventDefault();
          const opt = filteredOptions[highlightIdx];
          if (opt && !opt.disabled) handleSelect(opt);
          break;
        }
        case 'Escape':
          e.preventDefault();
          closeFromKeyboard();
          break;
        case 'Tab':
          handleClose();
          break;
      }
    },
    [open, highlightIdx, filteredOptions, handleSelect, handleClose, closeFromKeyboard, moveHighlight],
  );

  const listPortal =
    open &&
    createPortal(
      <div
        ref={listRef}
        id={listId}
        role="listbox"
        aria-label={accessibleLabel}
        className="fixed z-[9999] overflow-y-auto rounded-lg border border-edge bg-surface-alt py-1 shadow-xl"
        style={{ top: pos.top, left: pos.left, width: pos.width, maxHeight: 240 }}
        onKeyDown={handleListKeyDown}
      >
        <OptionList
          filteredOptions={filteredOptions}
          strValue={strValue}
          highlightIdx={highlightIdx}
          setHighlightIdx={setHighlightIdx}
          handleSelect={handleSelect}
          optionId={optionId}
          noMatchesLabel={t('select.noMatches')}
        />
      </div>,
      document.body,
    );

  if (searchable) {
    const editing = open || filterQuery.length > 0;
    const inputValue = editing ? filterQuery : (selectedOption?.label ?? '');
    const showPlaceholder = !editing && !selectedOption;
    const sizingText = editing ? filterQuery : (selectedOption?.label ?? placeholder ?? '');
    const fitMinChars = fitContent ? Math.max(6, Math.min(placeholder?.length ?? 6, 12)) : 2;
    const inputCh = fitContent ? fitContentCharCount(sizingText, fitMinChars) : undefined;

    return (
      <>
        <div
          ref={triggerRef as React.RefObject<HTMLDivElement>}
          title={title}
          className={cn(
            triggerShellClass,
            fitContent ? 'inline-flex w-auto' : 'w-full',
            'h-9 gap-0.5',
            className,
          )}
        >
          <input
            ref={inputRef}
            type="text"
            role="combobox"
            disabled={disabled}
            aria-expanded={open}
            aria-haspopup="listbox"
            aria-controls={open ? listId : undefined}
            aria-activedescendant={open ? activeDescendant : undefined}
            aria-autocomplete="list"
            aria-label={accessibleLabel}
            className={cn(
              'bg-transparent outline-none',
              fitContent ? 'shrink-0 pl-2.5 pr-0' : 'min-w-0 flex-1 px-2.5',
              showPlaceholder && 'text-fg-muted',
            )}
            style={inputCh !== undefined ? { width: `${inputCh}ch` } : undefined}
            value={inputValue}
            placeholder={showPlaceholder ? (placeholder ?? '') : undefined}
            onChange={(e) => {
              setFilterQuery(e.target.value);
              if (!open) {
                updatePosition();
                setOpen(true);
                setHighlightIdx(pickHighlightIndex(filterOptions(options, e.target.value)));
              }
            }}
            onFocus={() => {
              if (!open) handleOpen();
            }}
            onKeyDown={(e) => {
              if (!open && (e.key === 'ArrowDown' || e.key === 'ArrowUp')) {
                e.preventDefault();
                handleOpen();
                return;
              }
              handleListKeyDown(e);
            }}
          />
          <button
            type="button"
            disabled={disabled}
            aria-label={t('select.toggleOptions')}
            className="flex shrink-0 items-center justify-center self-stretch px-1 text-fg-muted hover:text-fg disabled:pointer-events-none"
            onMouseDown={(e) => e.preventDefault()}
            onClick={() => (open ? handleClose() : handleOpen())}
          >
            <ChevronDown className={cn('h-3.5 w-3.5 transition-transform', open && 'rotate-180')} />
          </button>
        </div>
        {listPortal}
      </>
    );
  }

  const handleButtonKeyDown = (e: React.KeyboardEvent) => {
    if (!open) {
      if (e.key === 'ArrowDown' || e.key === 'ArrowUp' || e.key === 'Enter' || e.key === ' ') {
        e.preventDefault();
        handleOpen();
      }
      return;
    }
    handleListKeyDown(e);
  };

  const buttonSizingText = selectedOption?.label ?? placeholder ?? '';
  const buttonFitMinChars = fitContent ? Math.max(6, Math.min(placeholder?.length ?? 6, 12)) : 2;

  return (
    <>
      <button
        ref={triggerRef as React.RefObject<HTMLButtonElement>}
        type="button"
        role="combobox"
        aria-expanded={open}
        aria-haspopup="listbox"
        aria-controls={open ? listId : undefined}
        aria-activedescendant={open ? activeDescendant : undefined}
        aria-label={accessibleLabel}
        disabled={disabled}
        title={title}
        className={cn(
          triggerShellClass,
          fitContent ? 'inline-flex w-auto' : 'w-full',
          'h-9 justify-between gap-1 px-2.5',
          className,
        )}
        onClick={() => (open ? handleClose() : handleOpen())}
        onKeyDown={handleButtonKeyDown}
      >
        <span
          className={cn(
            fitContent ? 'shrink-0 truncate' : 'min-w-0 truncate',
            !selectedOption && 'text-fg-muted',
          )}
          style={
            fitContent
              ? { width: `${fitContentCharCount(buttonSizingText, buttonFitMinChars)}ch` }
              : undefined
          }
        >
          {selectedOption?.label ?? placeholder ?? ''}
        </span>
        <ChevronDown
          className={cn(
            'h-3.5 w-3.5 shrink-0 text-fg-muted transition-transform',
            open && 'rotate-180',
          )}
        />
      </button>
      {listPortal}
    </>
  );
}
