import { useEffect, useRef, useState } from 'react';
import { Filter, Loader2, Sparkles, X } from 'lucide-react';
import { useI18n } from '../../hooks/useI18n';
import { useAiStore } from '../../stores/aiStore';
import { useTableDataStore } from '../../stores/tableDataStore';

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

  if (!isConfigured) {
    // Still render the toggle button so users know the feature exists
    return (
      <div className="flex items-center gap-2 border-b border-edge px-2 py-1">
        <button
          type="button"
          className="flex items-center gap-1 rounded px-1.5 py-0.5 text-[11px] text-fg-muted hover:bg-surface-alt hover:text-fg"
          onClick={() => {
            import('../../lib/windowManager').then((m) => m.openSettingsWindow('ai'));
          }}
        >
          <Sparkles className="h-3 w-3" />
          {t('smartFilter.notConfigured')}
        </button>
      </div>
    );
  }

  const handleParse = async () => {
    abortRef.current = false;
    const targetTable = tableName;
    const filters = await parseFilter({ connectionId, database, table: targetTable });

    if (abortRef.current) return;

    const currentActive = useTableDataStore.getState().activeTable;
    if (currentActive !== targetTable) return;

    if (filters && filters.length > 0) {
      setFilters(filters);
    }
  };

  const handleClear = () => {
    clearNlFilter();
    clearFilters();
  };

  if (!expanded) {
    return (
      <button
        type="button"
        className="flex items-center gap-1 px-2 py-1 text-xs text-fg-muted hover:text-fg rounded transition-colors"
        onClick={() => setExpanded(true)}
        title={t('smartFilter.title')}
      >
        <Sparkles className="h-3.5 w-3.5" />
        <Filter className="h-3.5 w-3.5" />
      </button>
    );
  }

  return (
    <div className="flex flex-col gap-1.5 px-3 py-2 border-b border-border bg-muted/30">
      <div className="flex items-center gap-2">
        <Sparkles className="h-4 w-4 text-accent shrink-0" />
        <input
          className="flex-1 bg-transparent text-sm outline-none placeholder:text-fg-muted"
          placeholder={t('smartFilter.placeholder')}
          value={nlFilterInput}
          onChange={(e) => setNlFilterInput(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter' && !e.shiftKey) {
              e.preventDefault();
              void handleParse();
            }
          }}
          disabled={isParsingFilter}
        />
        {isParsingFilter ? (
          <span className="flex items-center gap-1 text-xs text-fg-muted shrink-0">
            <Loader2 className="h-4 w-4 animate-spin" />
            {t('smartFilter.parsing')}
          </span>
        ) : (
          <button
            type="button"
            className="px-2 py-0.5 text-xs bg-accent text-white rounded hover:bg-accent/90 disabled:opacity-50 transition-colors"
            onClick={() => void handleParse()}
            disabled={!nlFilterInput.trim()}
          >
            {t('smartFilter.parse')}
          </button>
        )}
        <button
          type="button"
          className="p-0.5 text-fg-muted hover:text-fg rounded transition-colors"
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
        <div className="text-xs text-red-400 pl-6">{nlFilterError}</div>
      )}

      {parsedFilters && parsedFilters.length === 0 && (
        <div className="text-xs text-fg-muted pl-6">{t('smartFilter.noFilters')}</div>
      )}

      {parsedFilters && parsedFilters.length > 0 && (
        <div className="flex items-center gap-2 pl-6">
          <span className="text-xs text-green-400">
            {t('smartFilter.parsed').replace('{count}', String(parsedFilters.length))}
          </span>
        </div>
      )}
    </div>
  );
}
