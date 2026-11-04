import type { ReactNode } from 'react';
import { TriangleAlert } from 'lucide-react';
import { Button, cn, useI18n, Spinner } from '@datazen/ui';
import type { OverviewSourceStatus } from './useOverviewData';

/**
 * Shared 屏 A card shell: header (title + data-source tag + optional action) and
 * a body that resolves to exactly one of five named states.
 *
 * The five-state resolution lives here rather than in each card so PRD I-11
 * ("每个区块都有具名空态，禁止留白") cannot be forgotten by a later card, and so
 * tests can locate a card and its state through stable `data-overview-*`
 * attributes instead of rendered copy (PRD §7-6).
 */
export type OverviewCardState = OverviewSourceStatus | 'empty';

export interface OverviewCardProps {
  /** Stable id used in `data-overview-card` — one per 屏 A block. */
  cardId: 'server' | 'memory' | 'keyspace' | 'slowlog' | 'performance' | 'actions' | 'recent';
  titleKey: string;
  /** Backend command name this block reads (raw token, not UI copy). */
  source?: string;
  /** Loading/授权/失败 status of that source. */
  status?: OverviewSourceStatus;
  /** When `status === 'ready'` but there is nothing to show (I-11). */
  empty?: boolean;
  emptyKey?: string;
  /** 具名"未授权"文案 key（无权限 ⇒ 空态而不是错误条）。 */
  unauthorizedKey?: string;
  /** Extra hint appended to the unauthorized state (e.g. where to enable it). */
  unauthorizedHintKey?: string;
  /** Refresh target for the failed state. */
  onRetry?: () => void;
  /** Optional inline permission hint (never an error). */
  notice?: ReactNode;
  className?: string;
  children?: ReactNode;
}

function resolveState({
  status,
  empty,
}: {
  status?: OverviewSourceStatus;
  empty?: boolean;
}): OverviewCardState {
  if (status === 'loading') return 'loading';
  if (status === 'unauthorized') return 'unauthorized';
  if (status === 'failed') return 'failed';
  if (empty) return 'empty';
  return 'ready';
}

export function OverviewCard({
  cardId,
  titleKey,
  source,
  status = 'ready',
  empty = false,
  emptyKey,
  unauthorizedKey,
  unauthorizedHintKey,
  onRetry,
  notice,
  className,
  children,
}: OverviewCardProps) {
  const { t } = useI18n();
  const state = resolveState({ status, empty });

  return (
    <section
      data-overview-card={cardId}
      data-overview-card-state={state}
      className={cn(
        'flex min-w-0 flex-col overflow-hidden rounded-lg border border-edge bg-surface',
        className,
      )}
    >
      <header className="flex h-8 shrink-0 items-center gap-2 border-b border-edge bg-surface-alt px-3">
        <h3
          data-overview-card-title={cardId}
          className="truncate text-xs font-semibold uppercase tracking-wide text-fg-secondary"
        >
          {t(titleKey)}
        </h3>
        {source ? (
          <code
            data-overview-card-source={cardId}
            className="truncate rounded bg-surface-raised px-1.5 py-0.5 font-mono text-[10px] text-fg-muted"
          >
            {source}
          </code>
        ) : null}
        <span className="flex-1" />
      </header>

      <div className="min-h-0 flex-1">
        {state === 'loading' ? (
          <div
            data-overview-loading={cardId}
            className="flex items-center gap-2 px-3 py-4 text-xs text-fg-muted"
          >
            <Spinner size="md" />
            {t('redis.overview.loading')}
          </div>
        ) : null}

        {state === 'unauthorized' ? (
          <div
            data-overview-unauthorized={cardId}
            className="flex flex-col gap-1 px-3 py-4 text-xs"
          >
            <span className="flex items-center gap-2 text-warning">
              <TriangleAlert className="h-3.5 w-3.5 shrink-0" aria-hidden="true" />
              {t(unauthorizedKey ?? 'redis.overview.unauthorized.default')}
            </span>
            {unauthorizedHintKey ? (
              <span className="text-fg-muted">{t(unauthorizedHintKey)}</span>
            ) : null}
          </div>
        ) : null}

        {state === 'failed' ? (
          <div
            data-overview-failed={cardId}
            className="flex flex-col gap-2 px-3 py-4 text-xs text-fg-muted"
          >
            <span className="flex items-center gap-2 text-danger">
              <TriangleAlert className="h-3.5 w-3.5 shrink-0" aria-hidden="true" />
              {t('redis.overview.loadFailed')}
            </span>
            {onRetry ? (
              <Button
                type="button"
                variant="secondary"
                size="sm"
                data-overview-retry={cardId}
                className="self-start"
                onClick={onRetry}
              >
                {t('redis.overview.retry')}
              </Button>
            ) : null}
          </div>
        ) : null}

        {state === 'empty' ? (
          <div
            data-overview-empty={cardId}
            className="flex flex-col gap-1 px-3 py-4 text-xs text-fg-muted"
          >
            {t(emptyKey ?? 'redis.overview.empty.default')}
          </div>
        ) : null}

        {state === 'ready' ? (
          <>
            {notice}
            {children}
          </>
        ) : null}
      </div>
    </section>
  );
}
