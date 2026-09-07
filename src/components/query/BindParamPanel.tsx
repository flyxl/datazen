import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { ChevronDown, X } from 'lucide-react';
import { useI18n } from '../../hooks/useI18n';
import { cn } from '../../lib/cn';
import { getParamLabel, type SqlParam } from '../../lib/sqlBindParams';
import type { ParamHistoryEntry } from '../../windows/connection/query/useBindParameters';
import { Input } from '../ui/Input';

interface BindParamPanelProps {
  params: SqlParam[];
  values: Record<string, string>;
  /**
   * When provided, the panel uses stableId as keys (new S5-B mode).
   * When omitted, the panel derives labels from params and uses param.name as keys
   * (backward-compatible with existing callers).
   */
  labels?: Record<string, string>;
  onChange: (key: string, value: string) => void;
  /** Set of param stable IDs present in the currently executing statement. */
  activeStableIds?: Set<string>;
  /** History entries for each param (keyed by stable ID). */
  history?: Record<string, ParamHistoryEntry[]>;
  /** Clear history for a specific param stableId. */
  onClearHistory?: (stableId: string) => void;
  /** Apply a history value (stableId, value). */
  onApplyHistory?: (stableId: string, value: string) => void;
}

/**
 * Compact horizontal bar for SQL bind parameters.
 *
 * Supports two modes:
 * - **Legacy** (no `labels`): keys by param.name, derives label from syntax.
 * - **S5-B** (with `labels`): keys by stableId, uses explicit label map.
 *
 * History and active-statement highlighting are optional enhancements.
 */
export function BindParamPanel({
  params,
  values,
  labels,
  onChange,
  activeStableIds,
  history,
  onClearHistory,
  onApplyHistory,
}: BindParamPanelProps) {
  const { t } = useI18n();
  const useStableIds = labels !== undefined;

  // Fallback label map for legacy mode
  const fallbackLabels = useMemo(() => {
    if (labels) return labels;
    const map: Record<string, string> = {};
    for (const p of params) {
      map[p.stableId] = getParamLabel(p);
    }
    return map;
  }, [labels, params]);

  if (params.length === 0) return null;

  return (
    <div className="flex items-center gap-1 border-b border-edge bg-surface px-2 py-1 text-xs">
      <span className="mr-1 shrink-0 font-semibold uppercase tracking-wider text-fg-muted">
        {t('query.params')}:
      </span>
      <div className="flex flex-wrap items-center gap-1">
        {params.map((p) => {
          const key = useStableIds ? p.stableId : p.name;
          const valueKey = useStableIds ? p.stableId : p.name;
          return (
            <ParamInput
              key={key}
              param={p}
              label={fallbackLabels[p.stableId] ?? p.name}
              value={values[valueKey] ?? ''}
              isActive={activeStableIds?.has(p.stableId) ?? false}
              onChange={(stableId, val) => {
                // In legacy mode, map stableId back to name
                onChange(useStableIds ? stableId : p.name, val);
              }}
              historyEntries={history?.[p.stableId]}
              onClearHistory={onClearHistory}
              onApplyHistory={onApplyHistory}
            />
          );
        })}
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Individual param input with history dropdown
// ---------------------------------------------------------------------------

interface ParamInputProps {
  param: SqlParam;
  label: string;
  value: string;
  isActive: boolean;
  onChange: (stableId: string, value: string) => void;
  historyEntries?: ParamHistoryEntry[];
  onClearHistory?: (stableId: string) => void;
  onApplyHistory?: (stableId: string, value: string) => void;
}

function ParamInput({
  param,
  label,
  value,
  isActive,
  onChange,
  historyEntries,
  onClearHistory,
  onApplyHistory,
}: ParamInputProps) {
  const { t } = useI18n();
  const [dropdownOpen, setDropdownOpen] = useState(false);
  const [highlightIdx, setHighlightIdx] = useState(-1);
  const containerRef = useRef<HTMLDivElement>(null);

  const entries = historyEntries ?? [];
  const hasHistory = entries.length > 0;

  const closeDropdown = useCallback(() => {
    setDropdownOpen(false);
    setHighlightIdx(-1);
  }, []);

  // Close on outside click
  useEffect(() => {
    if (!dropdownOpen) return;
    const handler = (e: MouseEvent) => {
      if (containerRef.current && !containerRef.current.contains(e.target as Node)) {
        closeDropdown();
      }
    };
    document.addEventListener('mousedown', handler);
    return () => document.removeEventListener('mousedown', handler);
  }, [dropdownOpen, closeDropdown]);

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (!dropdownOpen) {
        if (hasHistory && (e.key === 'ArrowDown' || e.key === 'ArrowUp')) {
          e.preventDefault();
          setDropdownOpen(true);
          setHighlightIdx(0);
          return;
        }
        return;
      }

      switch (e.key) {
        case 'ArrowDown':
          e.preventDefault();
          setHighlightIdx((prev) => (prev < entries.length - 1 ? prev + 1 : 0));
          break;
        case 'ArrowUp':
          e.preventDefault();
          setHighlightIdx((prev) => (prev > 0 ? prev - 1 : entries.length - 1));
          break;
        case 'Enter':
          e.preventDefault();
          if (highlightIdx >= 0 && highlightIdx < entries.length && onApplyHistory) {
            onApplyHistory(param.stableId, entries[highlightIdx].value);
            closeDropdown();
          }
          break;
        case 'Escape':
          e.preventDefault();
          closeDropdown();
          break;
      }
    },
    [
      dropdownOpen,
      hasHistory,
      entries,
      highlightIdx,
      onApplyHistory,
      param.stableId,
      closeDropdown,
    ],
  );

  return (
    <div ref={containerRef} className="relative flex items-center gap-1">
      <span className={cn('font-mono', isActive ? 'text-accent' : 'text-fg-secondary')}>
        {label}
      </span>
      <div className="relative flex items-center">
        <Input
          value={value}
          onChange={(e) => onChange(param.stableId, e.target.value)}
          placeholder={t('query.paramValue')}
          className="h-6 w-28 text-xs pr-5"
          onKeyDown={handleKeyDown}
          onFocus={() => {
            if (hasHistory) setDropdownOpen(true);
          }}
        />
        {hasHistory && (
          <button
            type="button"
            className="absolute right-0.5 top-1/2 -translate-y-1/2 text-fg-muted hover:text-fg"
            tabIndex={-1}
            onMouseDown={(e) => {
              e.preventDefault();
              setDropdownOpen((prev) => !prev);
            }}
          >
            <ChevronDown size={12} />
          </button>
        )}
      </div>
      {dropdownOpen && hasHistory && (
        <HistoryDropdown
          entries={entries}
          highlightIdx={highlightIdx}
          onSelect={(val) => {
            if (onApplyHistory) onApplyHistory(param.stableId, val);
            closeDropdown();
          }}
          onClear={() => {
            if (onClearHistory) onClearHistory(param.stableId);
            closeDropdown();
          }}
        />
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// History dropdown
// ---------------------------------------------------------------------------

interface HistoryDropdownProps {
  entries: ParamHistoryEntry[];
  highlightIdx: number;
  onSelect: (value: string) => void;
  onClear: () => void;
}

function HistoryDropdown({ entries, highlightIdx, onSelect, onClear }: HistoryDropdownProps) {
  const { t } = useI18n();
  return (
    <div className="absolute left-0 top-full z-50 mt-1 min-w-[160px] max-h-48 overflow-y-auto rounded-md border border-edge bg-surface shadow-lg">
      <div className="flex items-center justify-between border-b border-edge px-2 py-1">
        <span className="text-[10px] font-semibold uppercase text-fg-muted">
          {t('query.editor.param.historyLabel')}
        </span>
        <button
          type="button"
          className="text-fg-muted hover:text-fg"
          onClick={onClear}
          title={t('query.editor.param.clearHistory')}
        >
          <X size={10} />
        </button>
      </div>
      {entries.map((entry, idx) => (
        <button
          key={`${entry.timestamp}-${idx}`}
          type="button"
          className={cn(
            'block w-full px-2 py-1 text-left text-xs',
            idx === highlightIdx
              ? 'bg-accent/15 text-accent'
              : 'text-fg-secondary hover:bg-elevated hover:text-fg',
          )}
          onMouseDown={(e) => {
            e.preventDefault();
            onSelect(entry.value);
          }}
        >
          {entry.value}
        </button>
      ))}
    </div>
  );
}
