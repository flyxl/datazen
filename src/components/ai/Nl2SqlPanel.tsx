import { useCallback, useEffect, useRef, useState } from 'react';
import { Loader2, Sparkles, Trash2, Settings } from 'lucide-react';
import { Button } from '../ui/Button';
import { useI18n } from '../../hooks/useI18n';
import { useAiStore } from '../../stores/aiStore';
import { openSettingsWindow } from '../../lib/windowManager';
import { AiInput } from './AiInput';
import { splitContextItems } from '../../lib/contextItems';
import type { ContextItem } from '../../types';

interface Nl2SqlPanelProps {
  dbSessionId: string;
  database: string;
  currentTable?: string;
  /** Stream / write generated SQL into the SQL editor. */
  onSqlChange: (sql: string) => void;
}

export function Nl2SqlPanel({
  dbSessionId,
  database,
  currentTable,
  onSqlChange,
}: Nl2SqlPanelProps) {
  const { t } = useI18n();
  const nl2sql = useAiStore((s) => s.nl2sql);
  const nl2sqlError = useAiStore((s) => s.nl2sqlError);
  const isConfigured = useAiStore((s) => s.isConfigured);
  const setNl2SqlInput = useAiStore((s) => s.setNl2SqlInput);
  const generateSql = useAiStore((s) => s.generateSql);
  const clearNl2Sql = useAiStore((s) => s.clearNl2Sql);

  const [contextItems, setContextItems] = useState<ContextItem[]>([]);
  const lastWrittenRef = useRef('');

  // Write SQL into the editor only when generation finishes (not streaming).
  useEffect(() => {
    if (nl2sql.isGenerating) return;
    if (!nl2sql.generatedSql) return;
    if (nl2sql.generatedSql === lastWrittenRef.current) return;
    lastWrittenRef.current = nl2sql.generatedSql;
    onSqlChange(nl2sql.generatedSql);
  }, [nl2sql.isGenerating, nl2sql.generatedSql, onSqlChange]);

  const handleGenerate = useCallback(() => {
    if (!nl2sql.input.trim() || nl2sql.isGenerating || !database) return;
    const { contextFiles, contextTables } = splitContextItems(contextItems);
    lastWrittenRef.current = '';
    void generateSql({
      dbSessionId,
      database,
      currentTable,
      contextFiles: contextFiles.length > 0 ? contextFiles : undefined,
      contextTables: contextTables.length > 0 ? contextTables : undefined,
    });
    setContextItems([]);
  }, [
    generateSql,
    dbSessionId,
    database,
    currentTable,
    nl2sql.input,
    nl2sql.isGenerating,
    contextItems,
  ]);

  if (!isConfigured) {
    return (
      <div className="flex items-center gap-2 px-3 py-2 text-xs text-fg-muted border-b border-edge bg-surface-alt">
        <Sparkles className="h-3.5 w-3.5" />
        <span className="flex-1">{t('nl2sql.notConfigured')}</span>
        <Button
          variant="primary"
          className="h-6 gap-1 px-2 text-[11px]"
          onClick={() => openSettingsWindow('ai')}
        >
          <Settings className="h-3 w-3" />
          {t('settings.ai.goToConfigure')}
        </Button>
      </div>
    );
  }

  return (
    <div className="flex shrink-0 flex-col border-b border-edge bg-surface-alt">
      <div className="flex shrink-0 items-center gap-2 p-2">
        <AiInput
          className="min-w-0 flex-1 [&_>div]:rounded [&_>div]:border"
          value={nl2sql.input}
          onChange={setNl2SqlInput}
          onSubmit={handleGenerate}
          placeholder={database ? undefined : t('nl2sql.selectDatabaseFirst')}
          disabled={!database || nl2sql.isGenerating}
          isLoading={nl2sql.isGenerating}
          contextItems={contextItems}
          onContextItemsChange={setContextItems}
          dbSessionId={dbSessionId}
          database={database}
          pickerPosition="below"
          hideSubmit
        />
        <div className="flex shrink-0 gap-1">
          <Button
            variant="primary"
            className="h-7 gap-1 px-2 text-xs"
            disabled={!nl2sql.input.trim() || nl2sql.isGenerating || !database}
            onClick={handleGenerate}
          >
            {nl2sql.isGenerating ? (
              <Loader2 className="h-3.5 w-3.5 animate-spin" />
            ) : (
              <Sparkles className="h-3.5 w-3.5" />
            )}
            {nl2sql.isGenerating ? t('nl2sql.generating') : t('nl2sql.generate')}
          </Button>
          {nl2sql.input && (
            <Button variant="ghost" className="h-7 px-1.5 text-xs" onClick={clearNl2Sql}>
              <Trash2 className="h-3.5 w-3.5" />
            </Button>
          )}
        </div>
      </div>

      {nl2sqlError && (
        <div className="mx-2 mb-2 shrink-0 rounded border border-red-500/20 bg-red-500/10 px-3 py-2 text-xs text-red-400">
          {nl2sqlError}
        </div>
      )}
    </div>
  );
}
