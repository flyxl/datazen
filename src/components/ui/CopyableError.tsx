import { useCallback, useState } from 'react';
import { Check, Copy } from 'lucide-react';
import { useI18n } from '../../hooks/useI18n';
import { cn } from '../../lib/cn';

export interface CopyableErrorProps {
  message: string;
  className?: string;
  /** When true, renders a copy-to-clipboard button beside the message. */
  copyButton?: boolean;
  /** Use monospace pre formatting (for SQL/technical errors). */
  mono?: boolean;
  'data-testid'?: string;
}

/**
 * Renders an error message as selectable text. Optionally adds a copy action.
 * Pair with `whitespace-pre-wrap break-words` styling so long messages stay readable.
 */
export function CopyableError({
  message,
  className,
  copyButton = false,
  mono = false,
  'data-testid': testId = 'copyable-error-message',
}: Readonly<CopyableErrorProps>) {
  const { t } = useI18n();
  const [copied, setCopied] = useState(false);

  const handleCopy = useCallback(() => {
    void navigator.clipboard.writeText(message);
    setCopied(true);
    window.setTimeout(() => setCopied(false), 1500);
  }, [message]);

  const textClass = cn(
    'selectable whitespace-pre-wrap break-words',
    mono && 'font-mono text-sm leading-relaxed',
    copyButton && 'min-w-0 flex-1',
  );

  const messageEl = mono ? (
    <pre data-testid={testId} className={textClass}>
      {message}
    </pre>
  ) : (
    <div data-testid={testId} className={textClass} role="alert">
      {message}
    </div>
  );

  if (copyButton) {
    return (
      <div className={cn('flex items-start gap-2', className)}>
        {messageEl}
        <button
          type="button"
          data-testid="copyable-error-copy"
          className="flex shrink-0 items-center gap-1 rounded px-2 py-0.5 text-[11px] text-fg-secondary hover:bg-red-500/10"
          onClick={handleCopy}
          title={t('common.copy')}
        >
          {copied ? (
            <>
              <Check className="h-3 w-3 text-green-400" />
              {t('common.copied')}
            </>
          ) : (
            <>
              <Copy className="h-3 w-3" />
              {t('common.copy')}
            </>
          )}
        </button>
      </div>
    );
  }

  if (className) {
    return (
      <div className={className} role={mono ? undefined : 'alert'}>
        {messageEl}
      </div>
    );
  }

  return messageEl;
}
