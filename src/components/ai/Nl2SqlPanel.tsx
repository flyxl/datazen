import { useCallback, useEffect, useRef, useState } from 'react';
import { Loader2, Sparkles, Trash2, Settings, X } from 'lucide-react';
import { Button } from '../ui/Button';
import { useI18n } from '../../hooks/useI18n';
import { useAiKeyboard } from '../../hooks/useAiKeyboard';
import { useAiStore } from '../../stores/aiStore';
import { cn } from '../../lib/cn';
import { openSettingsWindow } from '../../lib/windowManager';
import { ContextPicker } from './ContextPicker';
import type { ContextEntry, ContextItem } from '../../types';

interface Nl2SqlPanelProps {
  connectionId: string;
  database: string;
  currentTable?: string;
  /** Stream / write generated SQL into the SQL editor. */
  onSqlChange: (sql: string) => void;
}

export function Nl2SqlPanel({ connectionId, database, currentTable, onSqlChange }: Nl2SqlPanelProps) {
  const { t } = useI18n();
  const nl2sql = useAiStore((s) => s.nl2sql);
  const nl2sqlError = useAiStore((s) => s.nl2sqlError);
  const isConfigured = useAiStore((s) => s.isConfigured);
  const setNl2SqlInput = useAiStore((s) => s.setNl2SqlInput);
  const generateSql = useAiStore((s) => s.generateSql);
  const clearNl2Sql = useAiStore((s) => s.clearNl2Sql);

  const [contextFiles, setContextFiles] = useState<ContextEntry[]>([]);
  const [showPicker, setShowPicker] = useState(false);
  const [pickerQuery, setPickerQuery] = useState('');
  const inputWrapperRef = useRef<HTMLDivElement>(null);
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
    const ctxPaths = contextFiles.length > 0 ? contextFiles.map((f) => f.path) : undefined;
    lastWrittenRef.current = '';
    void generateSql({ connectionId, database, currentTable, contextFiles: ctxPaths });
    setContextFiles([]);
  }, [generateSql, connectionId, database, currentTable, nl2sql.input, nl2sql.isGenerating, contextFiles]);

  const aiKeyboard = useAiKeyboard(handleGenerate);

  if (!isConfigured) {
    return (
      <div className="flex items-center gap-2 px-3 py-2 text-xs text-fg-muted border-b border-edge bg-surface-alt">
        <Sparkles className="h-3.5 w-3.5" />
        <span className="flex-1">{t('nl2sql.notConfigured')}</span>
        <Button variant="primary" className="h-6 gap-1 px-2 text-[11px]" onClick={() => openSettingsWindow('ai')}>
          <Settings className="h-3 w-3" />
          {t('settings.ai.goToConfigure')}
        </Button>
      </div>
    );
  }

  return (
    <div className="flex shrink-0 flex-col border-b border-edge bg-surface-alt">
      <div className="flex shrink-0 items-start gap-2 p-2">
        <div ref={inputWrapperRef} className="relative min-w-0 flex-1">
          {showPicker && (
            <ContextPicker
              query={pickerQuery}
              position="below"
              connectionId={connectionId}
              database={database}
              onSelect={(item: ContextItem) => {
                if (item.kind !== 'file' && item.kind !== 'dir') return;
                const entry: ContextEntry = {
                  name: item.name,
                  path: item.path ?? item.id,
                  isDir: item.kind === 'dir',
                };
                if (!contextFiles.some((f) => f.path === entry.path)) {
                  setContextFiles((prev) => [...prev, entry]);
                }
                const input = nl2sql.input;
                const atStart = input.lastIndexOf('@');
                if (atStart >= 0) {
                  setNl2SqlInput(input.substring(0, atStart).trimEnd());
                }
                setShowPicker(false);
                setPickerQuery('');
              }}
              onClose={() => setShowPicker(false)}
              anchorRef={inputWrapperRef}
            />
          )}
          <div
            className={cn(
              'flex flex-wrap items-center rounded border border-edge bg-surface',
              'transition-colors focus-within:border-accent',
            )}
          >
            {contextFiles.length > 0 && (
              <div className="flex flex-wrap gap-1 pl-2 pt-1">
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
                      onClick={() => setContextFiles((prev) => prev.filter((c) => c.path !== f.path))}
                    >
                      <X className="h-2.5 w-2.5" />
                    </button>
                  </span>
                ))}
              </div>
            )}
            <textarea
              value={nl2sql.input}
              onChange={(e) => {
                const val = e.target.value;
                setNl2SqlInput(val);
                const cursorPos = e.target.selectionStart;
                const before = val.substring(0, cursorPos);
                const atMatch = before.match(/@([^\s@]*)$/);
                if (atMatch) {
                  setShowPicker(true);
                  setPickerQuery(atMatch[1]);
                } else {
                  setShowPicker(false);
                  setPickerQuery('');
                }
              }}
              onKeyDown={(e) => {
                if (showPicker && ['ArrowUp', 'ArrowDown', 'Enter', 'Escape'].includes(e.key)) return;
                aiKeyboard.onKeyDown?.(e);
              }}
              onCompositionStart={aiKeyboard.onCompositionStart}
              onCompositionEnd={aiKeyboard.onCompositionEnd}
              placeholder={database ? t('context.placeholder') : t('nl2sql.selectDatabaseFirst')}
              rows={1}
              className={cn(
                'min-w-0 flex-1 resize-none bg-transparent px-2 py-1.5',
                'text-sm text-fg placeholder:text-fg-muted',
                'focus:outline-none',
              )}
            />
          </div>
        </div>
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
            <Button
              variant="ghost"
              className="h-7 px-1.5 text-xs"
              onClick={clearNl2Sql}
            >
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
