import { Boxes, Download, Plus, Radio, Terminal, X } from 'lucide-react';
import { Button, cn, useI18n } from '@datazen/ui';
import { OverviewCard } from './OverviewCard';
import { relativeTimeParts, type BrowseHistoryEntry } from '../lib/redisBrowseHistory';
import type { OverviewJumpHandler, OverviewJumpTarget } from './overviewNavigation';
import { jumpStateAttribute, typeBadgeClass } from './overviewNavigation';

/**
 * Combined navigation card: Quick Actions + Recently Browsed Keys.
 * Merges two navigation-related sections into one compact card
 * to reduce vertical space waste in the overview grid.
 */
export interface NavigationCardProps {
  defaultDbIndex: number;
  recentEntries: BrowseHistoryEntry[];
  now?: number;
  onJump: (target: OverviewJumpTarget) => void;
  onClearRecent: () => void;
  jumpHandler?: OverviewJumpHandler;
}

interface QuickAction {
  id: 'browseDb' | 'console' | 'pubsub' | 'importExport' | 'newKey';
  labelKey: string;
  icon: typeof Boxes;
  target: OverviewJumpTarget;
}

function buildQuickActions(defaultDbIndex: number): QuickAction[] {
  return [
    {
      id: 'browseDb',
      labelKey: 'redis.overview.action.browseDb',
      icon: Boxes,
      target: { kind: 'database', dbIndex: defaultDbIndex },
    },
    {
      id: 'console',
      labelKey: 'redis.overview.action.console',
      icon: Terminal,
      target: { kind: 'console' },
    },
    {
      id: 'pubsub',
      labelKey: 'redis.overview.action.pubsub',
      icon: Radio,
      target: { kind: 'pubsub' },
    },
    {
      id: 'importExport',
      labelKey: 'redis.overview.action.importExport',
      icon: Download,
      target: { kind: 'importExport' },
    },
    {
      id: 'newKey',
      labelKey: 'redis.overview.action.newKey',
      icon: Plus,
      target: { kind: 'newKey', dbIndex: defaultDbIndex },
    },
  ];
}

export function NavigationCard({
  defaultDbIndex,
  recentEntries,
  now,
  onJump,
  onClearRecent,
  jumpHandler,
}: NavigationCardProps) {
  const { t } = useI18n();
  const jumpState = jumpStateAttribute(jumpHandler);
  const actions = buildQuickActions(defaultDbIndex);

  return (
    <OverviewCard cardId="actions" titleKey="redis.overview.navigation.title" className="h-full">
      <div className="flex flex-col divide-y divide-edge/50 text-xs">
        {/* Quick Actions */}
        <div className="p-2.5">
          <h4 className="mb-1.5 text-[11px] font-semibold uppercase tracking-wide text-fg-secondary">
            {t('redis.overview.actions.title')}
          </h4>
          <ul className="flex flex-wrap gap-1.5">
            {actions.map((action) => (
              <li key={action.id}>
                <button
                  type="button"
                  data-overview-action={action.id}
                  data-overview-action-target={action.target.kind}
                  data-overview-jump={jumpState}
                  className="flex h-7 items-center gap-1.5 rounded-full border border-edge bg-surface-alt px-2.5 text-fg-secondary transition-colors hover:border-accent/45 hover:text-fg"
                  onClick={() => onJump(action.target)}
                >
                  <action.icon className="h-3.5 w-3.5 shrink-0" aria-hidden="true" />
                  {t(action.labelKey)}
                </button>
              </li>
            ))}
          </ul>
        </div>

        {/* Recently Browsed Keys */}
        <div className="p-2.5">
          <div className="mb-1.5 flex items-center justify-between gap-2">
            <h4 className="text-[11px] font-semibold uppercase tracking-wide text-fg-secondary">
              {t('redis.overview.recent.title')}
            </h4>
            {recentEntries.length > 0 ? (
              <Button
                type="button"
                variant="ghost"
                size="sm"
                data-overview-recent-clear
                className="h-5 px-1 text-[10px]"
                onClick={onClearRecent}
              >
                <X className="h-2.5 w-2.5" aria-hidden="true" />
                {t('redis.overview.recent.clear')}
              </Button>
            ) : null}
          </div>

          {recentEntries.length === 0 ? (
            <p className="text-fg-muted">{t('redis.overview.recent.empty')}</p>
          ) : (
            <ul className="flex flex-col">
              {recentEntries.map((entry) => {
                const relative = relativeTimeParts(entry.visitedAt, now);
                return (
                  <li key={`${entry.dbIndex}\u0000${entry.key}`}>
                    <button
                      type="button"
                      data-overview-recent-key={entry.key}
                      data-overview-db-index={entry.dbIndex}
                      data-overview-key-type={entry.keyType ?? 'unknown'}
                      data-overview-jump={jumpState}
                      className="flex w-full items-baseline gap-1.5 rounded px-1 py-0.5 text-left transition-colors hover:bg-surface-raised"
                      onClick={() =>
                        onJump({
                          kind: 'key',
                          dbIndex: entry.dbIndex,
                          key: entry.key,
                          keyType: entry.keyType ?? null,
                        })
                      }
                    >
                      <span className="min-w-0 flex-1 truncate font-mono text-fg" title={entry.key}>
                        {entry.key}
                      </span>
                      <span
                        className={cn(
                          'shrink-0 rounded border px-1 py-0.5 font-mono text-[10px] leading-none',
                          typeBadgeClass(entry.keyType),
                        )}
                      >
                        {entry.keyType ?? t('redis.overview.typeUnknown')}
                      </span>
                      <span className="shrink-0 font-mono text-[10px] text-fg-muted">
                        {`db${entry.dbIndex}`}
                      </span>
                      <span
                        data-overview-recent-time={entry.key}
                        data-overview-relative-unit={relative.unitKey}
                        data-overview-relative-value={relative.value}
                        className="shrink-0 text-[10px] text-fg-muted"
                      >
                        {t(relative.unitKey, { value: relative.value })}
                      </span>
                    </button>
                  </li>
                );
              })}
            </ul>
          )}
        </div>
      </div>
    </OverviewCard>
  );
}
