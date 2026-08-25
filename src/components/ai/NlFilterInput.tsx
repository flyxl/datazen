import { useEffect, useRef, useState } from 'react';
import { Loader2, Sparkles, X } from 'lucide-react';
import { useI18n } from '../../hooks/useI18n';
import { useAiKeyboard } from '../../hooks/useAiKeyboard';
import { useAiStore } from '../../stores/aiStore';
import { useTableDataStore } from '../../stores/tableDataStore';
import { cn } from '../../lib/cn';

interface NlFilterInputProps {
  connectionId: string;
  database: string;
  tableName: string;
}

export function NlFilterInput({ connectionId, database, tableName }: NlFilterInputProps) {
  const { t } = useI18n();
  const [expanded, setExpanded] = useState(false);
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

  useEffect(() => {
    clearNlFilter();
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
    const filters = await parseFilter({ dbSessionId: connectionId, database, table: targetTable });

    if (abortRef.current) return;

    const currentActive = useTableDataStore.getState().activeTable;
    if (currentActive !== targetTable) return;

    if (filters && filters.length > 0) {
      setFilters(filters);
    }
  };

  const aiKeyboard = useAiKeyboard(() => void handleParse());

  const handleClear = () => {
    clearNlFilter();
    clearFilters();
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
        title={t('smartFilter.notConfigured')}
      >
        <Sparkles className="h-3.5 w-3.5 shrink-0" />
        <span className="max-w-[14rem] truncate text-[11px]">{t('smartFilter.notConfigured')}</span>
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

      {nlFilterError && <div className="pl-9 text-xs text-red-400">{nlFilterError}</div>}

      {parsedFilters && parsedFilters.length === 0 && (
        <div className="pl-9 text-xs text-fg-muted">{t('smartFilter.noFilters')}</div>
      )}

      {parsedFilters && parsedFilters.length > 0 && (
        <div className="pl-9 text-xs text-green-400">
          {t('smartFilter.parsed').replace('{count}', String(parsedFilters.length))}
        </div>
      )}
    </div>
  );
}
