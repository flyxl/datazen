import { useCallback, useRef, useState } from 'react';
import { Loader2, Sparkles, Copy, Check, Trash2, ArrowDownToLine } from 'lucide-react';
import { Button } from '../ui/Button';
import { useI18n } from '../../hooks/useI18n';
import { useAiStore } from '../../stores/aiStore';
import { cn } from '../../lib/cn';

interface Nl2SqlPanelProps {
  connectionId: string;
  database: string;
  currentTable?: string;
  onApplySql: (sql: string) => void;
}

export function Nl2SqlPanel({ connectionId, database, currentTable, onApplySql }: Nl2SqlPanelProps) {
  const { t } = useI18n();
  const nl2sql = useAiStore((s) => s.nl2sql);
  const nl2sqlError = useAiStore((s) => s.nl2sqlError);
  const isConfigured = useAiStore((s) => s.isConfigured);
  const setNl2SqlInput = useAiStore((s) => s.setNl2SqlInput);
  const generateSql = useAiStore((s) => s.generateSql);
  const clearNl2Sql = useAiStore((s) => s.clearNl2Sql);

  const inputRef = useRef<HTMLTextAreaElement>(null);
  const [copied, setCopied] = useState(false);

  const handleGenerate = useCallback(() => {
    void generateSql({ connectionId, database, currentTable });
  }, [generateSql, connectionId, database, currentTable]);

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (e.key === 'Enter' && !e.shiftKey) {
        e.preventDefault();
        if (nl2sql.input.trim() && !nl2sql.isGenerating) {
          handleGenerate();
        }
      }
    },
    [nl2sql.input, nl2sql.isGenerating, handleGenerate],
  );

  const handleCopy = useCallback(() => {
    void navigator.clipboard.writeText(nl2sql.generatedSql);
    setCopied(true);
    setTimeout(() => setCopied(false), 1500);
  }, [nl2sql.generatedSql]);

  const handleApply = useCallback(() => {
    if (nl2sql.generatedSql) {
      onApplySql(nl2sql.generatedSql);
    }
  }, [nl2sql.generatedSql, onApplySql]);

  if (!isConfigured) {
    return (
      <div className="flex items-center gap-2 px-3 py-2 text-xs text-fg-muted">
        <Sparkles className="h-3.5 w-3.5" />
        {t('nl2sql.notConfigured')}
      </div>
    );
  }

  return (
    <div className="border-b border-edge bg-surface-alt">
      <div className="flex items-start gap-2 p-2">
        <Sparkles className="mt-1.5 h-3.5 w-3.5 shrink-0 text-blue-400" />
        <div className="flex min-w-0 flex-1 flex-col gap-1.5">
          <textarea
            ref={inputRef}
            value={nl2sql.input}
            onChange={(e) => setNl2SqlInput(e.target.value)}
            onKeyDown={handleKeyDown}
            placeholder={t('nl2sql.placeholder')}
            rows={1}
            className={cn(
              'w-full resize-none rounded border border-edge bg-surface px-2 py-1.5',
              'text-sm text-fg placeholder:text-fg-muted',
              'focus:border-accent focus:outline-none',
            )}
          />
          {nl2sql.generatedSql && (
            <div className="rounded border border-edge bg-surface p-2">
              <pre className="whitespace-pre-wrap font-mono text-xs text-fg-secondary">
                {nl2sql.generatedSql}
              </pre>
              {!nl2sql.isGenerating && (
                <div className="mt-2 flex gap-1.5">
                  <Button
                    variant="primary"
                    className="h-6 gap-1 px-2 text-[11px]"
                    onClick={handleApply}
                  >
                    <ArrowDownToLine className="h-3 w-3" />
                    {t('nl2sql.apply')}
                  </Button>
                  <Button
                    variant="ghost"
                    className="h-6 gap-1 px-2 text-[11px]"
                    onClick={handleCopy}
                  >
                    {copied ? <Check className="h-3 w-3" /> : <Copy className="h-3 w-3" />}
                    {t('nl2sql.copy')}
                  </Button>
                </div>
              )}
            </div>
          )}
          {nl2sqlError && (
            <div className="rounded border border-red-500/20 bg-red-500/10 px-3 py-2 text-xs text-red-400">
              {nl2sqlError}
            </div>
          )}
        </div>
        <div className="flex shrink-0 gap-1">
          <Button
            variant="primary"
            className="h-7 gap-1 px-2 text-xs"
            disabled={!nl2sql.input.trim() || nl2sql.isGenerating}
            onClick={handleGenerate}
          >
            {nl2sql.isGenerating ? (
              <Loader2 className="h-3.5 w-3.5 animate-spin" />
            ) : (
              <Sparkles className="h-3.5 w-3.5" />
            )}
            {nl2sql.isGenerating ? t('nl2sql.generating') : t('nl2sql.generate')}
          </Button>
          {(nl2sql.generatedSql || nl2sql.input) && (
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
    </div>
  );
}
