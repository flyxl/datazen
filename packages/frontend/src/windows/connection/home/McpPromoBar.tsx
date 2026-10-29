import { useMemo } from 'react';
import { Check, Copy, Sparkles, Terminal } from 'lucide-react';
import { Button } from '../../../components/ui/Button';
import { useCopyFeedback } from '../../../components/ui/useCopyFeedback';
import { useI18n } from '../../../hooks/useI18n';
import { formatMcpCliCommand, useAppExecutablePath } from '../../../lib/mcpAgentConfig';

/** How long the "copied" label stays before reverting to the copy action. */
const COPIED_FEEDBACK_MS = 2000;

/**
 * Single-row MCP promo bar (compressed from the former full-height card):
 * icon + title + one-line description + CLI command + Copy button.
 */
export function McpPromoBar() {
  const { t } = useI18n();
  const { copied, copy } = useCopyFeedback(COPIED_FEEDBACK_MS);

  const appExecutablePath = useAppExecutablePath();
  const mcpCliCommand = useMemo(() => formatMcpCliCommand(appExecutablePath), [appExecutablePath]);

  const handleCopy = () => copy(mcpCliCommand);

  return (
    <div
      className="flex items-center gap-3 rounded-xl border border-edge bg-surface-alt px-4 py-3"
      data-testid="home-mcp-promo-bar"
    >
      <div className="flex h-8 w-8 shrink-0 items-center justify-center rounded-lg bg-accent/10 text-accent">
        <Sparkles className="h-4 w-4" />
      </div>

      <div className="min-w-0 shrink-0">
        <div className="text-sm font-semibold text-fg">DataZen MCP Server</div>
        <p className="truncate text-xs text-fg-muted">{t('connWin.home.mcp.shortDesc')}</p>
      </div>

      <div className="ml-auto flex min-w-0 flex-1 items-center justify-end gap-3">
        <div className="flex min-w-0 items-center gap-2 rounded-lg border border-edge bg-surface px-3 py-1.5">
          <Terminal className="h-3.5 w-3.5 shrink-0 text-fg-muted" />
          <code className="truncate font-mono text-xs text-fg" title={mcpCliCommand}>
            {mcpCliCommand}
          </code>
        </div>
        <Button
          variant="ghost"
          size="sm"
          onClick={handleCopy}
          className="h-7 shrink-0 gap-1 px-2 text-xs"
          data-testid="home-mcp-copy-button"
        >
          {copied ? (
            <>
              <Check className="h-3.5 w-3.5 text-success" />
              <span className="text-success">{t('connWin.home.aiIntegration.copied')}</span>
            </>
          ) : (
            <>
              <Copy className="h-3.5 w-3.5" />
              <span>{t('connWin.home.aiIntegration.copy')}</span>
            </>
          )}
        </Button>
      </div>
    </div>
  );
}
