import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { ShieldAlert, ShieldCheck } from 'lucide-react';
import { Button } from './ui/Button';
import { aiCommands } from '../commands/ai';
import { useI18n } from '../hooks/useI18n';
import { requirePlatform } from '../platform/contracts';
import { getWindowKind } from '../lib/windowKind';
import type { AiWriteApplyReceipt, AiWriteProposal } from '../types';

/**
 * Global host for T24 confirmed writes.
 *
 * The agent never applies a write itself. `propose_write` emits
 * `ai:write-proposal-requested`; this dialog shows the exact SQL, optional
 * review diff, and static impact. Only "Allow once" calls `ai_apply_write`
 * with the exact proposal id, connection, database and SQL. Deny removes the
 * proposal; expiry disables the allow path.
 */
export function AiWriteProposalHost() {
  const { t } = useI18n();
  const [queue, setQueue] = useState<AiWriteProposal[]>([]);
  const [remainingMs, setRemainingMs] = useState(0);
  const [responding, setResponding] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [receipt, setReceipt] = useState<AiWriteApplyReceipt | null>(null);
  const [auditError, setAuditError] = useState<string | null>(null);
  const timedOutRef = useRef<Set<string>>(new Set());
  const current = queue[0];

  useEffect(() => {
    if (getWindowKind() !== 'main') return;
    let disposed = false;
    const unlisteners: Array<() => void> = [];
    void (async () => {
      const events = requirePlatform().events;
      const unProposal = await events.listenCrossWindow('ai:write-proposal-requested', (raw) => {
        const proposal = raw as AiWriteProposal;
        if (!proposal || !proposal.proposalId || !proposal.sql) return;
        setQueue((previous) =>
          previous.some((item) => item.proposalId === proposal.proposalId)
            ? previous
            : [...previous, proposal],
        );
      });
      const unAuditError = await events.listenCrossWindow('ai:write-audit-error', (raw) => {
        setAuditError(typeof raw === 'string' ? raw : t('ai.write.auditFailed'));
      });
      if (disposed) {
        unProposal();
        unAuditError();
      } else {
        unlisteners.push(unProposal, unAuditError);
      }
    })().catch(() => {});
    return () => {
      disposed = true;
      for (const unlisten of unlisteners) unlisten();
    };
  }, [t]);

  const dropCurrent = useCallback(() => {
    setQueue((previous) => previous.filter((item) => item.proposalId !== current?.proposalId));
  }, [current?.proposalId]);

  const deny = useCallback(async () => {
    if (!current) return;
    setResponding(true);
    setError(null);
    try {
      await aiCommands.aiDenyWrite(current.proposalId);
      dropCurrent();
    } catch (cause) {
      setError(
        cause instanceof Error && cause.message ? cause.message : t('ai.write.respondFailed'),
      );
    } finally {
      setResponding(false);
    }
  }, [current, dropCurrent, t]);

  const allow = useCallback(async () => {
    if (!current || !current.applyAllowed) return;
    setResponding(true);
    setError(null);
    try {
      const applied = await aiCommands.aiApplyWrite({
        proposalId: current.proposalId,
        connectionId: current.connectionId,
        database: current.database,
        sql: current.sql,
      });
      setReceipt(applied);
      dropCurrent();
    } catch (cause) {
      setError(
        cause instanceof Error && cause.message ? cause.message : t('ai.write.respondFailed'),
      );
    } finally {
      setResponding(false);
    }
  }, [current, dropCurrent, t]);

  useEffect(() => {
    if (!current) {
      setRemainingMs(0);
      return;
    }
    const expiresAt = Date.parse(current.expiresAt);
    const tick = () => {
      const next = Number.isFinite(expiresAt) ? expiresAt - Date.now() : 0;
      setRemainingMs(Math.max(0, next));
      if (next <= 0 && !timedOutRef.current.has(current.proposalId)) {
        timedOutRef.current.add(current.proposalId);
        void deny();
      }
    };
    tick();
    const timer = window.setInterval(tick, 1000);
    return () => window.clearInterval(timer);
  }, [current, deny]);

  const seconds = useMemo(() => Math.ceil(remainingMs / 1000), [remainingMs]);
  const expired = remainingMs <= 0;
  if (!current && !receipt && !auditError) return null;

  return (
    <>
      {current && (
        <div
          className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 p-4"
          data-testid="ai-write-proposal-backdrop"
        >
          <div
            role="dialog"
            aria-modal="true"
            aria-labelledby="ai-write-proposal-title"
            className="flex max-h-[85vh] w-full max-w-2xl flex-col rounded-lg border border-edge bg-surface shadow-xl"
            data-testid="ai-write-proposal-dialog"
          >
            <div className="flex items-center gap-2 border-b border-edge px-4 py-3">
              {current.productionProtected ? (
                <ShieldAlert className="h-4 w-4 text-danger" />
              ) : (
                <ShieldCheck className="h-4 w-4 text-accent" />
              )}
              <div className="min-w-0">
                <h2 id="ai-write-proposal-title" className="text-sm font-medium text-fg">
                  {t('ai.write.title')}
                </h2>
                <p className="text-[11px] text-fg-muted">{t('ai.write.description')}</p>
              </div>
              <span className="ml-auto shrink-0 text-[11px] text-fg-muted">
                {expired ? t('ai.write.expired') : t('ai.write.remaining', { seconds })}
              </span>
            </div>

            <div className="min-h-0 flex-1 overflow-auto px-4 py-3 text-xs text-fg-secondary">
              <div className="mb-3 grid grid-cols-[auto_1fr] gap-x-3 gap-y-1">
                <span className="text-fg-muted">{t('ai.write.connection')}</span>
                <span data-testid="ai-write-proposal-connection">{current.connectionId}</span>
                <span className="text-fg-muted">{t('ai.write.database')}</span>
                <span data-testid="ai-write-proposal-database">
                  {current.database || t('ai.write.noDatabase')}
                </span>
                <span className="text-fg-muted">{t('ai.write.hash')}</span>
                <code className="truncate font-mono text-[10px]" title={current.sqlHash}>
                  {current.sqlHash}
                </code>
              </div>

              {current.productionProtected && (
                <div
                  className="mb-3 rounded border border-danger/40 bg-danger/10 px-3 py-2 text-danger"
                  data-testid="ai-write-proposal-production-warning"
                >
                  {t('ai.write.productionWarning')}
                </div>
              )}
              {!current.productionProtected && !current.applyAllowed && (
                <div
                  className="mb-3 rounded border border-warning/40 bg-warning/10 px-3 py-2 text-warning"
                  data-testid="ai-write-proposal-readonly-warning"
                >
                  {t('ai.write.readOnlyWarning')}
                </div>
              )}

              <div className="mb-1 font-medium text-fg">{t('ai.write.exactSql')}</div>
              <pre
                className="mb-3 max-h-56 overflow-auto rounded border border-edge bg-surface-alt px-3 py-2 font-mono text-[11px] whitespace-pre-wrap"
                data-testid="ai-write-proposal-sql"
              >
                {current.sql}
              </pre>

              {current.diff && (
                <>
                  <div className="mb-1 font-medium text-fg">{t('ai.write.diff')}</div>
                  <pre
                    className="mb-3 max-h-40 overflow-auto rounded border border-edge bg-surface-alt px-3 py-2 font-mono text-[11px] whitespace-pre-wrap"
                    data-testid="ai-write-proposal-diff"
                  >
                    {current.diff}
                  </pre>
                </>
              )}

              <div className="mb-1 font-medium text-fg">{t('ai.write.impact')}</div>
              <div
                className="rounded border border-edge bg-surface-alt px-3 py-2"
                data-testid="ai-write-proposal-impact"
              >
                <div data-testid="ai-write-proposal-summary">{current.impact.summary}</div>
                <div className="mt-1 text-[11px] text-fg-muted">
                  {t('ai.write.statements', { count: current.impact.statementCount })}
                  {current.impact.tables.length > 0
                    ? ` · ${t('ai.write.tables', { tables: current.impact.tables.join(', ') })}`
                    : ''}
                </div>
                {(current.impact.destructive || !current.impact.hasWhere) && (
                  <div className="mt-1 text-[11px] text-danger">
                    {current.impact.destructive
                      ? t('ai.write.destructiveWarning')
                      : t('ai.write.noWhereWarning')}
                  </div>
                )}
              </div>

              {error && (
                <div className="mt-3 text-[11px] text-danger" data-testid="ai-write-proposal-error">
                  {error}
                </div>
              )}
            </div>

            <div className="flex items-center justify-end gap-2 border-t border-edge px-4 py-3">
              <Button
                variant="ghost"
                onClick={() => void deny()}
                disabled={responding}
                data-testid="ai-write-proposal-deny"
              >
                {t('ai.write.deny')}
              </Button>
              <Button
                variant="primary"
                onClick={() => void allow()}
                disabled={responding || expired || !current.applyAllowed}
                data-testid="ai-write-proposal-allow"
              >
                {t('ai.write.allowOnce')}
              </Button>
            </div>
          </div>
        </div>
      )}

      {receipt && !current && (
        <div
          className="fixed bottom-4 left-1/2 z-40 -translate-x-1/2 rounded border border-edge bg-surface px-4 py-2 text-xs text-fg-secondary shadow-lg"
          data-testid="ai-write-applied-receipt"
        >
          {receipt.rowCount == null
            ? t('ai.write.appliedUnknown')
            : t('ai.write.applied', { count: receipt.rowCount })}
          <button
            type="button"
            className="ml-3 text-fg-muted hover:text-fg"
            onClick={() => setReceipt(null)}
          >
            {t('ai.write.dismiss')}
          </button>
        </div>
      )}

      {auditError && (
        <div
          className="fixed bottom-4 right-4 z-40 rounded border border-danger/40 bg-danger/10 px-3 py-2 text-xs text-danger"
          data-testid="ai-write-audit-error"
        >
          {auditError}
          <button type="button" className="ml-3 underline" onClick={() => setAuditError(null)}>
            {t('ai.write.dismiss')}
          </button>
        </div>
      )}
    </>
  );
}
