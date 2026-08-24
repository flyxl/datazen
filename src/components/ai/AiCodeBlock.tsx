import { useCallback, useState } from 'react';
import { Check, Copy, Code2 } from 'lucide-react';
import { SqlCodeBlock } from '../SqlCodeBlock';
import { useI18n } from '../../hooks/useI18n';
import { cn } from '../../lib/cn';
import { isSqlCodeBlock } from '../../lib/aiMessageBlocks';

interface AiCodeBlockProps {
  language: string;
  code: string;
  sqlDialect?: string;
  onInsertSql?: (sql: string) => void;
  showActions?: boolean;
  isStreaming?: boolean;
}

export function AiCodeBlock({
  language,
  code,
  sqlDialect = 'postgresql',
  onInsertSql,
  showActions = true,
  isStreaming = false,
}: AiCodeBlockProps) {
  const { t } = useI18n();
  const [copied, setCopied] = useState(false);
  const isSql = isSqlCodeBlock(language, code);
  const label = language || (isSql ? 'sql' : 'code');

  const handleCopy = useCallback(() => {
    void navigator.clipboard.writeText(code);
    setCopied(true);
    setTimeout(() => setCopied(false), 1500);
  }, [code]);

  return (
    <div
      className={cn(
        'my-2 overflow-hidden rounded-md border border-edge bg-surface',
        isStreaming && 'animate-pulse',
      )}
      data-testid="ai-code-block"
    >
      <div className="flex items-center justify-between gap-2 border-b border-edge bg-surface-alt px-2 py-1">
        <div className="flex items-center gap-1.5 min-w-0">
          <Code2 className="h-3 w-3 shrink-0 text-fg-muted" />
          <span className="truncate text-[10px] font-medium uppercase text-fg-muted">{label}</span>
        </div>
        {showActions && !isStreaming && (
          <div className="flex shrink-0 items-center gap-1">
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
            {onInsertSql && isSql && (
              <button
                type="button"
                onMouseDown={(e) => e.preventDefault()}
                className="rounded px-1.5 py-0.5 text-[10px] text-blue-400 hover:bg-surface hover:text-blue-300"
                onClick={() => onInsertSql(code)}
                data-testid="ai-code-insert"
              >
                {t('chat.insertSql')}
              </button>
            )}
          </div>
        )}
      </div>
      <div className={cn('overflow-x-auto', isSql ? 'max-h-60' : '')}>
        {isSql ? (
          <div className="h-32 min-h-[4rem]">
            <SqlCodeBlock code={code} dialect={sqlDialect} />
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
