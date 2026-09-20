import { useCallback, useState } from 'react';
import { Check, Copy, Code2, Play, Plus, Maximize2, Minimize2 } from 'lucide-react';
import { SqlCodeBlock } from '../SqlCodeBlock';
import { Select } from '../ui/Select';
import { useI18n } from '../../hooks/useI18n';
import { cn } from '../../lib/cn';
import { isSqlCodeBlock } from '../../lib/aiMessageBlocks';
import type { SelectOption } from '../ui/Select';

const SQL_DIALECTS: readonly SelectOption[] = [
  { value: 'postgresql', label: 'PostgreSQL' },
  { value: 'mysql', label: 'MySQL' },
  { value: 'sqlite', label: 'SQLite' },
  { value: 'mariadb', label: 'MariaDB' },
  { value: 'sqlserver', label: 'SQL Server' },
  { value: 'oracle', label: 'Oracle' },
  { value: 'clickhouse', label: 'ClickHouse' },
];

interface AiCodeBlockProps {
  language: string;
  code: string;
  sqlDialect?: string;
  onInsertSql?: (sql: string) => void;
  onRunCode?: (code: string, language: string) => void;
  onNewQuery?: (code: string) => void;
  showActions?: boolean;
  isStreaming?: boolean;
}

export function AiCodeBlock({
  language,
  code,
  sqlDialect = 'postgresql',
  onInsertSql,
  onRunCode,
  onNewQuery,
  showActions = true,
  isStreaming = false,
}: AiCodeBlockProps) {
  const { t } = useI18n();
  const [copied, setCopied] = useState(false);
  const [fullscreen, setFullscreen] = useState(false);
  const [selectedDialect, setSelectedDialect] = useState(sqlDialect);
  const isSql = isSqlCodeBlock(language, code);
  const label = language || (isSql ? 'sql' : 'code');

  const handleCopy = useCallback(() => {
    void navigator.clipboard.writeText(code);
    setCopied(true);
    setTimeout(() => setCopied(false), 1500);
  }, [code]);

  const handleRun = useCallback(() => {
    onRunCode?.(code, language || 'sql');
  }, [code, language, onRunCode]);

  const handleNewQuery = useCallback(() => {
    onNewQuery?.(code);
  }, [code, onNewQuery]);

  const toggleFullscreen = useCallback(() => {
    setFullscreen((prev) => !prev);
  }, []);

  const handleDialectChange = useCallback((value: string) => {
    setSelectedDialect(value);
  }, []);

  if (fullscreen) {
    return (
      <div className="fixed inset-0 z-50 flex flex-col bg-surface" data-testid="ai-code-fullscreen">
        {/* Fullscreen header */}
        <div className="flex items-center justify-between border-b border-edge px-3 py-1.5">
          <div className="flex items-center gap-2">
            <Code2 className="h-3.5 w-3.5 text-fg-muted" />
            <span className="text-xs font-medium uppercase text-fg-muted">{label}</span>
            {isSql && (
              <Select
                value={selectedDialect}
                options={SQL_DIALECTS}
                onChange={handleDialectChange}
                className="h-7 w-32 text-[10px]"
                title={t('chat.selectDialect')}
                triggerDataAttrs={{ 'data-testid': 'ai-code-dialect-select' }}
              />
            )}
          </div>
          <div className="flex items-center gap-1">
            {isSql && onRunCode && (
              <button
                type="button"
                className="flex items-center gap-1 rounded px-2 py-1 text-[10px] text-emerald-400 hover:bg-emerald-400/10"
                onClick={handleRun}
                data-testid="ai-code-run"
              >
                <Play className="h-2.5 w-2.5" />
                {t('chat.runCode')}
              </button>
            )}
            {isSql && onInsertSql && (
              <button
                type="button"
                className="rounded px-2 py-1 text-[10px] text-accent hover:bg-accent/10"
                onClick={() => onInsertSql(code)}
                data-testid="ai-code-insert"
              >
                {t('chat.insertSql')}
              </button>
            )}
            {isSql && onNewQuery && (
              <button
                type="button"
                className="flex items-center gap-1 rounded px-2 py-1 text-[10px] text-fg-muted hover:bg-surface-alt hover:text-fg"
                onClick={handleNewQuery}
                data-testid="ai-code-new-query"
              >
                <Plus className="h-2.5 w-2.5" />
                {t('chat.newQuery')}
              </button>
            )}
            <button
              type="button"
              className="flex items-center gap-1 rounded px-2 py-1 text-[10px] text-fg-muted hover:bg-surface-alt hover:text-fg"
              onClick={handleCopy}
              data-testid="ai-code-copy"
            >
              {copied ? <Check className="h-2.5 w-2.5" /> : <Copy className="h-2.5 w-2.5" />}
              {copied ? '' : t('chat.copyCode')}
            </button>
            <button
              type="button"
              className="flex items-center gap-1 rounded px-2 py-1 text-[10px] text-fg-muted hover:bg-surface-alt hover:text-fg"
              onClick={toggleFullscreen}
              data-testid="ai-code-fullscreen-toggle"
              title={t('chat.exitFullscreen')}
            >
              <Minimize2 className="h-2.5 w-2.5" />
            </button>
          </div>
        </div>
        {/* Fullscreen content */}
        <div className="flex-1 overflow-auto">
          {isSql ? (
            <SqlCodeBlock code={code} dialect={selectedDialect} />
          ) : (
            <pre className="p-4 text-[11px] font-mono text-fg-secondary whitespace-pre-wrap">
              {code}
            </pre>
          )}
        </div>
      </div>
    );
  }

  return (
    <div
      className={cn(
        'my-2 overflow-hidden rounded-md border border-edge bg-surface',
        isStreaming && 'animate-pulse',
      )}
      data-testid="ai-code-block"
    >
      {/* Header with label + toolbar */}
      <div className="flex items-center justify-between gap-2 border-b border-edge bg-surface-alt px-2 py-1">
        <div className="flex items-center gap-1.5 min-w-0">
          <Code2 className="h-3 w-3 shrink-0 text-fg-muted" />
          <span className="truncate text-[10px] font-medium uppercase text-fg-muted">{label}</span>
          {isSql && (
            <Select
              value={selectedDialect}
              options={SQL_DIALECTS}
              onChange={handleDialectChange}
              className="h-6 w-28 text-[9px]"
              title={t('chat.selectDialect')}
              triggerDataAttrs={{ 'data-testid': 'ai-code-dialect-select' }}
            />
          )}
        </div>
        {showActions && !isStreaming && (
          <div className="flex shrink-0 items-center gap-0.5">
            {/* Run (SQL only) */}
            {isSql && onRunCode && (
              <button
                type="button"
                onMouseDown={(e) => e.preventDefault()}
                className="flex items-center gap-1 rounded px-1.5 py-0.5 text-[10px] text-emerald-400 hover:bg-emerald-400/10"
                onClick={handleRun}
                title={t('chat.runCode')}
                data-testid="ai-code-run"
              >
                <Play className="h-2.5 w-2.5" />
              </button>
            )}
            {/* Insert to Editor (SQL only) */}
            {isSql && onInsertSql && (
              <button
                type="button"
                onMouseDown={(e) => e.preventDefault()}
                className="rounded px-1.5 py-0.5 text-[10px] text-accent hover:bg-accent/10"
                onClick={() => onInsertSql(code)}
                title={t('chat.insertSql')}
                data-testid="ai-code-insert"
              >
                {t('chat.insertSql')}
              </button>
            )}
            {/* New Query (SQL only) */}
            {isSql && onNewQuery && (
              <button
                type="button"
                onMouseDown={(e) => e.preventDefault()}
                className="flex items-center gap-1 rounded px-1.5 py-0.5 text-[10px] text-fg-muted hover:bg-surface hover:text-fg"
                onClick={handleNewQuery}
                title={t('chat.newQuery')}
                data-testid="ai-code-new-query"
              >
                <Plus className="h-2.5 w-2.5" />
              </button>
            )}
            {/* Copy */}
            <button
              type="button"
              onMouseDown={(e) => e.preventDefault()}
              className="flex items-center gap-1 rounded px-1.5 py-0.5 text-[10px] text-fg-muted hover:bg-surface hover:text-fg"
              onClick={handleCopy}
              title={t('chat.copyCode')}
              data-testid="ai-code-copy"
            >
              {copied ? <Check className="h-2.5 w-2.5" /> : <Copy className="h-2.5 w-2.5" />}
            </button>
            {/* Fullscreen */}
            <button
              type="button"
              onMouseDown={(e) => e.preventDefault()}
              className="flex items-center gap-1 rounded px-1.5 py-0.5 text-[10px] text-fg-muted hover:bg-surface hover:text-fg"
              onClick={toggleFullscreen}
              title={t('chat.fullscreen')}
              data-testid="ai-code-fullscreen-toggle"
            >
              <Maximize2 className="h-2.5 w-2.5" />
            </button>
          </div>
        )}
      </div>
      {/* Code content */}
      <div className={cn('overflow-x-auto', isSql ? 'max-h-60' : '')}>
        {isSql ? (
          <div className="h-32 min-h-[4rem]">
            <SqlCodeBlock code={code} dialect={selectedDialect} />
          </div>
        ) : (
          <pre className="p-2 text-[11px] font-mono text-fg-secondary whitespace-pre-wrap">
            {code}
          </pre>
        )}
      </div>
    </div>
  );
}
