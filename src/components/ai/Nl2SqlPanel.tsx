import { useCallback, useEffect, useRef, useState } from 'react';
import { Loader2, Sparkles, Trash2, Settings } from 'lucide-react';
import { Button } from '../ui/Button';
import { useI18n } from '../../hooks/useI18n';
import { useAiKeyboard } from '../../hooks/useAiKeyboard';
import { useAiStore } from '../../stores/aiStore';
import { cn } from '../../lib/cn';
import { openSettingsWindow } from '../../lib/windowManager';
import { ContextPicker } from './ContextPicker';
import { splitContextItems } from '../../lib/contextItems';
import type { ContextItem } from '../../types';

interface Nl2SqlPanelProps {
  connectionId: string;
  database: string;
  currentTable?: string;
  /** Stream / write generated SQL into the SQL editor. */
  onSqlChange: (sql: string) => void;
}

function itemKey(item: ContextItem): string {
  return `${item.kind}:${item.id}`;
}

export function Nl2SqlPanel({ connectionId, database, currentTable, onSqlChange }: Nl2SqlPanelProps) {
  const { t } = useI18n();
  const nl2sql = useAiStore((s) => s.nl2sql);
  const nl2sqlError = useAiStore((s) => s.nl2sqlError);
  const isConfigured = useAiStore((s) => s.isConfigured);
  const setNl2SqlInput = useAiStore((s) => s.setNl2SqlInput);
  const generateSql = useAiStore((s) => s.generateSql);
  const clearNl2Sql = useAiStore((s) => s.clearNl2Sql);

  const [contextItems, setContextItems] = useState<ContextItem[]>([]);
  const [showPicker, setShowPicker] = useState(false);
  const [pickerQuery, setPickerQuery] = useState('');
  const inputWrapperRef = useRef<HTMLDivElement>(null);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
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
      connectionId,
      database,
      currentTable,
      contextFiles: contextFiles.length > 0 ? contextFiles : undefined,
      contextTables: contextTables.length > 0 ? contextTables : undefined,
    });
    setContextItems([]);
  }, [generateSql, connectionId, database, currentTable, nl2sql.input, nl2sql.isGenerating, contextItems]);

  const aiKeyboard = useAiKeyboard(handleGenerate);

  const handleSelect = useCallback(
    (item: ContextItem) => {
      if (!contextItems.some((i) => itemKey(i) === itemKey(item))) {
        setContextItems((prev) => [...prev, item]);
      }
      const input = nl2sql.input;
      const textarea = textareaRef.current;
      const cursorPos = textarea?.selectionStart ?? input.length;
      const textBefore = input.substring(0, cursorPos);
      const textAfter = input.substring(cursorPos);
      const atStart = textBefore.lastIndexOf('@');
      if (atStart >= 0) {
        const newValue = textBefore.substring(0, atStart) + textAfter.replace(/^\s*/, '');
        setNl2SqlInput(newValue.trimStart() === '' ? '' : newValue);
      }
      setShowPicker(false);
      setPickerQuery('');
      textarea?.focus();
    },
    [contextItems, nl2sql.input, setNl2SqlInput],
  );

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
              onSelect={handleSelect}
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
            {contextItems.length > 0 && (
              <div className="flex flex-wrap gap-1 pl-2 pt-1">
                {contextItems.map((item) => (
                  <span
                    key={itemKey(item)}
                    data-testid="context-token"
                    data-kind={item.kind}
                    data-id={item.id}
                    className="inline-flex items-center rounded bg-accent/10 px-1.5 py-0.5 text-[11px] text-accent"
                  >
                    @{item.name}
                  </span>
                ))}
              </div>
            )}
            <textarea
              ref={textareaRef}
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
                const textarea = textareaRef.current;
                if (
                  contextItems.length > 0 &&
                  textarea &&
                  (e.key === 'Backspace' || (e.key === 'Delete' && nl2sql.input.length === 0))
                ) {
                  const start = textarea.selectionStart ?? 0;
                  const end = textarea.selectionEnd ?? 0;
                  if (start === 0 && end === 0) {
                    e.preventDefault();
                    setContextItems((prev) => prev.slice(0, -1));
                    return;
                  }
                }
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
