import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { Button } from './ui/Button';
import { aiCommands } from '../commands/ai';
import { useI18n } from '../hooks/useI18n';
import { requirePlatform } from '../platform/contracts';
import { getWindowKind } from '../lib/windowKind';
import type { McpApprovalRequest } from '../types';

/**
 * Global host for MCP per-call approvals.
 *
 * The backend pauses a non-read-only/high-risk call and emits
 * `mcp:approval-requested`; this dialog shows the exact SQL/arguments already
 * redacted by the host. Allow once resumes the call; Deny / Stop / the
 * five-minute timeout resolve it without executing the tool.
 */
export function McpApprovalHost() {
  const { t } = useI18n();
  const [queue, setQueue] = useState<McpApprovalRequest[]>([]);
  const [remainingMs, setRemainingMs] = useState(0);
  const [responding, setResponding] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [auditError, setAuditError] = useState<string | null>(null);
  const timedOutRef = useRef<Set<string>>(new Set());
  const current = queue[0];

  useEffect(() => {
    if (getWindowKind() !== 'main') return;
    let disposed = false;
    const unlisteners: Array<() => void> = [];
    void (async () => {
      const events = requirePlatform().events;
      const unRequest = await events.listenCrossWindow('mcp:approval-requested', (raw) => {
        const request = raw as McpApprovalRequest;
        if (!request || request.tool.length === 0) return;
        setQueue((previous) =>
          previous.some((item) => item.requestId === request.requestId)
            ? previous
            : [...previous, request],
        );
      });
      const unAuditError = await events.listenCrossWindow('mcp:audit-error', (raw) => {
        setAuditError(typeof raw === 'string' ? raw : t('mcp.approval.auditFailed'));
      });
      if (disposed) {
        unRequest();
        unAuditError();
      } else {
        unlisteners.push(unRequest, unAuditError);
      }
    })().catch(() => {});
    return () => {
      disposed = true;
      for (const unlisten of unlisteners) unlisten();
    };
  }, [t]);

  const publishDecision = useCallback(
    async (requestId: string, decision: 'allow_once' | 'deny' | 'cancel' | 'timeout') => {
      setResponding(true);
      setError(null);
      try {
        await aiCommands.mcpRespondApproval(requestId, decision);
        setQueue((previous) => previous.filter((item) => item.requestId !== requestId));
      } catch (cause) {
        setError(
          cause instanceof Error && cause.message ? cause.message : t('mcp.approval.respondFailed'),
        );
      } finally {
        setResponding(false);
      }
    },
    [t],
  );

  const handleStop = useCallback(async () => {
    if (!current) return;
    setResponding(true);
    setError(null);
    try {
      await aiCommands.mcpCancelApproval(current.requestId);
      setQueue((previous) => previous.filter((item) => item.requestId !== current.requestId));
    } catch (cause) {
      setError(
        cause instanceof Error && cause.message ? cause.message : t('mcp.approval.respondFailed'),
      );
    } finally {
      setResponding(false);
    }
  }, [current, t]);

  useEffect(() => {
    if (!current) {
      setRemainingMs(0);
      return;
    }
    const expiresAt = Date.parse(current.expiresAt);
    const tick = () => {
      const next = Number.isFinite(expiresAt) ? expiresAt - Date.now() : 0;
      setRemainingMs(Math.max(0, next));
      if (next <= 0 && !timedOutRef.current.has(current.requestId)) {
        timedOutRef.current.add(current.requestId);
        void publishDecision(current.requestId, 'timeout');
      }
    };
    tick();
    const timer = window.setInterval(tick, 1000);
    return () => window.clearInterval(timer);
  }, [current, publishDecision]);

  const seconds = useMemo(() => Math.ceil(remainingMs / 1000), [remainingMs]);
  if (!current && !auditError) return null;

  return (
    <>
      {current && (
        <div
          className="fixed inset-0 z-[10000] flex items-center justify-center bg-black/50 p-4"
          role="presentation"
        >
          <div
            role="dialog"
            aria-modal="true"
            aria-labelledby="mcp-approval-title"
            className="w-full max-w-2xl space-y-4 rounded-lg border border-edge bg-surface p-5 shadow-xl"
          >
            <div className="space-y-1">
              <h2 id="mcp-approval-title" className="text-base font-semibold text-fg">
                {t('mcp.approval.title')}
              </h2>
              <p className="text-xs text-fg-muted">{t('mcp.approval.description')}</p>
            </div>

            <dl className="grid grid-cols-[8rem_1fr] gap-x-3 gap-y-1 text-xs">
              <dt className="text-fg-muted">{t('mcp.approval.tool')}</dt>
              <dd className="font-mono text-fg">{current.tool}</dd>
              <dt className="text-fg-muted">{t('mcp.approval.connection')}</dt>
              <dd className="font-mono text-fg">{current.connectionId ?? '—'}</dd>
              <dt className="text-fg-muted">{t('mcp.approval.database')}</dt>
              <dd className="font-mono text-fg">{current.database ?? '—'}</dd>
              <dt className="text-fg-muted">{t('mcp.approval.namespace')}</dt>
              <dd className="font-mono text-fg">{current.namespace ?? '—'}</dd>
            </dl>

            {current.sql && (
              <div className="space-y-1">
                <div className="text-xs text-fg-muted">{t('mcp.approval.sql')}</div>
                <pre className="max-h-64 overflow-auto rounded-md border border-edge bg-surface-raised p-3 font-mono text-xs whitespace-pre-wrap break-all text-fg">
                  {current.sql}
                </pre>
              </div>
            )}

            <div className="space-y-1">
              <div className="text-xs text-fg-muted">{t('mcp.approval.arguments')}</div>
              <pre className="max-h-56 overflow-auto rounded-md border border-edge bg-surface-raised p-3 font-mono text-xs whitespace-pre-wrap break-all text-fg-secondary">
                {JSON.stringify(current.arguments, null, 2)}
              </pre>
            </div>

            {error && <p className="text-xs text-red-500">{error}</p>}
            <div className="flex items-center justify-between gap-3">
              <span
                className={seconds <= 30 ? 'text-xs text-amber-500' : 'text-xs text-fg-muted'}
                aria-live="polite"
              >
                {seconds > 0 ? t('mcp.approval.remaining', { seconds }) : t('mcp.approval.expired')}
              </span>
              <div className="flex items-center gap-2">
                <Button variant="secondary" disabled={responding} onClick={() => void handleStop()}>
                  {t('mcp.approval.stop')}
                </Button>
                <Button
                  variant="secondary"
                  disabled={responding}
                  onClick={() => void publishDecision(current.requestId, 'deny')}
                >
                  {t('mcp.approval.deny')}
                </Button>
                <Button
                  variant="primary"
                  disabled={responding || seconds <= 0}
                  onClick={() => void publishDecision(current.requestId, 'allow_once')}
                >
                  {t('mcp.approval.allowOnce')}
                </Button>
              </div>
            </div>
          </div>
        </div>
      )}

      {auditError && (
        <div
          role="alert"
          className="fixed bottom-4 right-4 z-[10001] max-w-md rounded-md border border-red-500/40 bg-surface p-3 text-xs text-red-500 shadow-lg"
        >
          <div className="flex items-start justify-between gap-3">
            <span>
              {t('mcp.approval.auditFailed')}: {auditError}
            </span>
            <button
              type="button"
              className="text-fg-muted hover:text-fg"
              onClick={() => setAuditError(null)}
              aria-label={t('common.close')}
            >
              ×
            </button>
          </div>
        </div>
      )}
    </>
  );
}
