import { useState, useRef, useEffect, useMemo, type KeyboardEvent } from 'react';
import { Columns3, Search, X, RotateCcw } from 'lucide-react';
import type { ColumnSchema } from '../../types';
import { useI18n } from '../../hooks/useI18n';
import { cn } from '../../lib/cn';

export interface TableColumnFilterProps {
  columns: ColumnSchema[];
  visibleColumns: string[] | null;
  onChange: (visibleColumns: string[] | null) => void;
  disabled?: boolean;
}

export function TableColumnFilter({
  columns,
  visibleColumns,
  onChange,
  disabled = false,
}: TableColumnFilterProps) {
  const { t } = useI18n();
  const [open, setOpen] = useState(false);
  const [searchQuery, setSearchQuery] = useState('');

  const containerRef = useRef<HTMLDivElement>(null);
  const buttonRef = useRef<HTMLButtonElement>(null);
  const searchInputRef = useRef<HTMLInputElement>(null);

  const allColumnNames = useMemo(() => columns.map((c) => c.name), [columns]);
  const activeVisible = useMemo(
    () => visibleColumns ?? allColumnNames,
    [visibleColumns, allColumnNames],
  );
  const isFiltered = visibleColumns !== null && visibleColumns.length < columns.length;
  const visibleCount = activeVisible.length;
  const totalCount = columns.length;

  // Filter columns based on search input
  const filteredColumns = useMemo(() => {
    const q = searchQuery.trim().toLowerCase();
    if (!q) return columns;
    return columns.filter(
      (c) => c.name.toLowerCase().includes(q) || c.dataType.toLowerCase().includes(q),
    );
  }, [columns, searchQuery]);

  // Click outside to close popover
  useEffect(() => {
    if (!open) return;

    const handleMouseDown = (e: MouseEvent) => {
      if (containerRef.current && !containerRef.current.contains(e.target as Node)) {
        setOpen(false);
      }
    };

    document.addEventListener('mousedown', handleMouseDown);
    return () => {
      document.removeEventListener('mousedown', handleMouseDown);
    };
  }, [open]);

  // Focus search input when popover opens
  useEffect(() => {
    if (open) {
      setTimeout(() => {
        searchInputRef.current?.focus();
      }, 50);
    } else {
      setSearchQuery('');
    }
  }, [open]);

  const handleKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    if (e.key === 'Escape') {
      e.preventDefault();
      setOpen(false);
      buttonRef.current?.focus();
    }
  };

  const handleToggleColumn = (columnName: string) => {
    if (activeVisible.includes(columnName)) {
      const next = activeVisible.filter((c) => c !== columnName);
      onChange(next);
    } else {
      const next = [...activeVisible, columnName];
      if (next.length === allColumnNames.length) {
        onChange(null);
      } else {
        onChange(next);
      }
    }
  };

  const handleSelectAll = () => {
    onChange(null);
  };

  const handleClearAll = () => {
    onChange([]);
  };

  const handleReset = () => {
    onChange(null);
  };

  return (
    <div
      ref={containerRef}
      className="relative inline-flex shrink-0 items-center"
      onKeyDown={handleKeyDown}
    >
      <button
        ref={buttonRef}
        type="button"
        disabled={disabled || columns.length === 0}
        onClick={() => setOpen((prev) => !prev)}
        aria-expanded={open}
        aria-haspopup="true"
        title={t('tableData.columnFilter')}
        data-testid="table-column-filter-toggle"
        className={cn(
          'flex items-center gap-1 rounded px-1.5 py-0.5 text-xs transition-colors',
          isFiltered
            ? 'bg-accent/15 text-accent font-medium'
            : open
              ? 'bg-surface-raised text-fg'
              : 'text-fg-secondary hover:bg-surface-raised hover:text-fg',
          (disabled || columns.length === 0) && 'pointer-events-none opacity-50',
        )}
      >
        <Columns3 className="h-3 w-3 shrink-0" />
        <span>{t('tableData.columnFilter')}</span>
        {isFiltered && (
          <span
            data-testid="column-filter-count-badge"
            className="rounded bg-accent/20 px-1 text-[10px] font-mono leading-tight text-accent"
          >
            {visibleCount}/{totalCount}
          </span>
        )}
      </button>

      {open && (
        <div
          data-testid="table-column-filter-popover"
          className="absolute right-0 top-full z-50 mt-1 flex w-72 flex-col rounded-xl border border-edge bg-surface p-2.5 shadow-xl animate-in fade-in zoom-in-95 duration-100"
        >
          {/* Header */}
          <div className="flex items-center justify-between border-b border-edge/80 pb-2">
            <div className="flex items-center gap-1.5 text-xs font-semibold text-fg">
              <span>{t('tableData.visibleColumns')}</span>
              <span className="rounded bg-surface-raised px-1.5 py-0.5 text-[10px] font-mono text-fg-muted">
                {visibleCount}/{totalCount}
              </span>
            </div>
            <div className="flex items-center gap-1">
              <button
                type="button"
                data-testid="table-column-filter-select-all"
                onClick={handleSelectAll}
                className="rounded px-1.5 py-0.5 text-[11px] text-accent hover:bg-accent/10 hover:underline"
              >
                {t('tableData.selectAllColumns')}
              </button>
              <span className="text-edge">|</span>
              <button
                type="button"
                data-testid="table-column-filter-clear-all"
                onClick={handleClearAll}
                className="rounded px-1.5 py-0.5 text-[11px] text-fg-muted hover:bg-surface-raised hover:text-fg"
              >
                {t('tableData.deselectAllColumns')}
              </button>
              {isFiltered && (
                <>
                  <span className="text-edge">|</span>
                  <button
                    type="button"
                    data-testid="table-column-filter-reset"
                    onClick={handleReset}
                    title={t('tableData.resetColumns')}
                    className="flex items-center gap-1 rounded px-1.5 py-0.5 text-[11px] text-fg-muted hover:bg-surface-raised hover:text-fg"
                  >
                    <RotateCcw className="h-3 w-3" />
                    <span>{t('tableData.resetColumns')}</span>
                  </button>
                </>
              )}
            </div>
          </div>

          {/* Search Input */}
          <div className="relative mt-2">
            <Search className="pointer-events-none absolute left-2 top-2 h-3.5 w-3.5 text-fg-muted" />
            <input
              ref={searchInputRef}
              type="text"
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              placeholder={t('tableData.searchColumns')}
              data-testid="table-column-filter-search"
              className="h-7 w-full rounded border border-edge bg-surface-alt pl-7 pr-7 text-xs text-fg placeholder:text-fg-muted focus:border-accent focus:outline-none"
            />
            {searchQuery && (
              <button
                type="button"
                onClick={() => setSearchQuery('')}
                className="absolute right-1.5 top-1.5 flex h-4 w-4 items-center justify-center rounded-full text-fg-muted hover:bg-surface-raised hover:text-fg"
              >
                <X className="h-3 w-3" />
              </button>
            )}
          </div>

          {/* Columns Checkbox List */}
          <div className="mt-2 max-h-60 overflow-y-auto pr-0.5 space-y-0.5">
            {filteredColumns.length === 0 ? (
              <div className="py-4 text-center text-xs text-fg-muted">
                {t('tableData.noColumnsFound')}
              </div>
            ) : (
              filteredColumns.map((col) => {
                const isChecked = activeVisible.includes(col.name);
                return (
                  <label
                    key={col.name}
                    data-testid={`column-filter-item-${col.name}`}
                    className="flex cursor-pointer items-center gap-2 rounded px-2 py-1.5 text-xs transition-colors hover:bg-surface-raised select-none"
                  >
                    <input
                      type="checkbox"
                      checked={isChecked}
                      onChange={() => handleToggleColumn(col.name)}
                      data-testid={`column-filter-checkbox-${col.name}`}
                      className="h-3.5 w-3.5 rounded accent-accent"
                    />
                    <span
                      className={cn(
                        'min-w-0 flex-1 truncate',
                        isChecked ? 'font-medium text-fg' : 'text-fg-muted',
                      )}
                    >
                      {col.name}
                    </span>
                    {col.isPrimaryKey && (
                      <span className="shrink-0 rounded border border-accent/20 bg-accent/15 px-1 py-0.2 text-[9px] font-mono font-semibold text-accent">
                        PK
                      </span>
                    )}
                    <span className="shrink-0 font-mono text-[10px] text-fg-muted">
                      {col.dataType}
                    </span>
                  </label>
                );
              })
            )}
          </div>
        </div>
      )}
    </div>
  );
}
