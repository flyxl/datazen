import { useEffect, useRef, useState } from 'react';
import { Loader2, Sparkles, X } from 'lucide-react';
import { useI18n } from '../../hooks/useI18n';
import { useAiKeyboard } from '../../hooks/useAiKeyboard';
import { useAiStore } from '../../stores/aiStore';
import { useTableDataStore } from '../../stores/tableDataStore';
import { cn } from '../../lib/cn';

interface NlFilterInputProps {
  dbSessionId: string;
  database: string;
  tableName: string;
}

export function NlFilterInput({ dbSessionId, database, tableName }: NlFilterInputProps) {
  const { t } = useI18n();
  const [expanded, setExpanded] = useState(false);
  const [validationError, setValidationError] = useState<string | null>(null);
  const abortRef = useRef(false);

  const isConfigured = useAiStore((s) => s.isConfigured);
  const nlFilterInput = useAiStore((s) => s.nlFilterInput);
  const setNlFilterInput = useAiStore((s) => s.setNlFilterInput);
  const parsedFilters = useAiStore((s) => s.parsedFilters);
  const isParsingFilter = useAiStore((s) => s.isParsingFilter);
  const nlFilterError = useAiStore((s) => s.nlFilterError);
  const parseFilter = useAiStore((s) => s.parseFilter);
  const clearNlFilter = useAiStore((s) => s.clearNlFilter);

  const setFilters = useTableDataStore((s) => s.setFilters);
  const clearFilters = useTableDataStore((s) => s.clearFilters);
  const columns = useTableDataStore((s) => s.columns);

  useEffect(() => {
    clearNlFilter();
    setValidationError(null);
    setExpanded(false);
  }, [tableName, clearNlFilter]);

  useEffect(() => {
    abortRef.current = false;
    return () => {
      abortRef.current = true;
    };
  }, [tableName]);

  const handleParse = async () => {
    abortRef.current = false;
    const targetTable = tableName;
    const filters = await parseFilter({ dbSessionId, database, table: targetTable });

    if (abortRef.current) return;

    const currentActive = useTableDataStore.getState().activeTable;
    if (currentActive !== targetTable) return;

    if (filters && filters.length > 0) {
      // Metadata may still be loading when the user parses a filter. Only
      // reject fields once the table has reported a non-empty column list;
      // an empty list must not turn a valid filter into a false warning.
      if (columns.length > 0) {
        const knownColumns = new Set(columns.map((column) => column.name.toLowerCase()));
        const unknownColumns = [
          ...new Set(
            filters
              .map((filter) => filter.column.trim())
              .filter((column) => column && !knownColumns.has(column.toLowerCase())),
          ),
        ];
        if (unknownColumns.length > 0) {
          setValidationError(
            t('smartFilter.invalidColumns').replace('{columns}', unknownColumns.join(', ')),
          );
          return;
        }
      }
      setValidationError(null);
      setFilters(filters);
    }
  };

  const aiKeyboard = useAiKeyboard(() => void handleParse());

  const handleClear = () => {
    clearNlFilter();
    clearFilters();
    setValidationError(null);
  };

  const iconBtnClass =
    'flex h-7 w-7 shrink-0 items-center justify-center rounded text-xs text-fg-muted transition-colors hover:bg-surface-alt hover:text-fg';

  if (!isConfigured) {
    return (
      <button
        type="button"
        onMouseDown={(e) => e.preventDefault()}
        className={cn(iconBtnClass, 'w-auto max-w-full gap-1 px-1.5')}
        onClick={() => {
          void import('../../lib/windowManager').then((m) => m.openSettingsWindow('ai'));
        }}
        title={t('common.aiNotConfigured')}
      >
        <Sparkles className="h-3.5 w-3.5 shrink-0" />
        <span className="max-w-[14rem] truncate text-[11px]">{t('common.aiNotConfigured')}</span>
      </button>
    );
  }

  if (!expanded) {
    return (
      <button
        type="button"
        onMouseDown={(e) => e.preventDefault()}
        className={iconBtnClass}
        onClick={() => setExpanded(true)}
        title={t('smartFilter.title')}
        aria-label={t('smartFilter.title')}
        data-testid="smart-filter-toggle"
      >
        <Sparkles className="h-3.5 w-3.5" />
      </button>
    );
  }

  return (
    <div className="flex min-w-0 flex-1 flex-col gap-1">
      <div className="flex h-7 min-w-0 items-center gap-2">
        <span className="flex h-7 w-7 shrink-0 items-center justify-center">
          <Sparkles className="h-3.5 w-3.5 text-accent" />
        </span>
        <input
          className="min-w-0 flex-1 bg-transparent text-sm outline-none placeholder:text-fg-muted"
          placeholder={t('smartFilter.placeholder')}
          value={nlFilterInput}
          onChange={(e) => setNlFilterInput(e.target.value)}
          {...aiKeyboard}
          disabled={isParsingFilter}
        />
        {isParsingFilter ? (
          <span className="flex shrink-0 items-center gap-1 text-xs text-fg-muted">
            <Loader2 className="h-4 w-4 animate-spin" />
            {t('smartFilter.parsing')}
          </span>
        ) : (
          <button
            type="button"
            onMouseDown={(e) => e.preventDefault()}
            className="rounded bg-accent px-2 py-0.5 text-xs text-white transition-colors hover:bg-accent/90 disabled:opacity-50"
            onClick={() => void handleParse()}
            disabled={!nlFilterInput.trim()}
          >
            {t('smartFilter.parse')}
          </button>
        )}
        <button
          type="button"
          onMouseDown={(e) => e.preventDefault()}
          className="rounded p-0.5 text-fg-muted transition-colors hover:text-fg"
          onClick={() => {
            handleClear();
            setExpanded(false);
          }}
          title={t('smartFilter.clear')}
        >
          <X className="h-3.5 w-3.5" />
        </button>
      </div>

      {nlFilterError && (
        <div className="pl-9 text-xs text-danger" role="alert">
          {nlFilterError}
        </div>
      )}

      {validationError && (
        <div className="pl-9 text-xs text-warning" role="alert">
          {validationError}
        </div>
      )}

      {parsedFilters && parsedFilters.length === 0 && (
        <div className="pl-9 text-xs text-fg-muted">{t('smartFilter.noFilters')}</div>
      )}

      {parsedFilters && parsedFilters.length > 0 && !validationError && (
        <div className="pl-9 text-xs text-success" role="status">
          {t('smartFilter.parsed').replace('{count}', String(parsedFilters.length))}
        </div>
      )}
    </div>
  );
}
