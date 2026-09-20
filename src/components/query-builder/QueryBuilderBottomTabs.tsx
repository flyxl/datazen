import type { ReactNode } from 'react';
import { cn } from '@datazen/ui';
import { useI18n } from '../../hooks/useI18n';
import type { QbBottomTab } from '../../stores/queryBuilderStore';

export interface QueryBuilderBottomTabsProps {
  /** Active tab. */
  tab: QbBottomTab;
  onTabChange: (tab: QbBottomTab) => void;
  /** Show a warning dot on the Build tab (SQL generation problem). */
  hasBuildError?: boolean;
  /** Content for the Build tab — only mounted while active. */
  buildContent: ReactNode;
  /** Content for the Preview tab — only mounted while active. */
  previewContent: ReactNode;
}

/**
 * Bottom region of the visual builder: two mutually-exclusive tabs.
 *
 * Only the active tab is mounted, which is what buys back the vertical space
 * the previous stacked layout (canvas + criteria grid + preview) wasted.
 * The footer lives outside this component so it never scrolls away.
 */
export function QueryBuilderBottomTabs({
  tab,
  onTabChange,
  hasBuildError = false,
  buildContent,
  previewContent,
}: QueryBuilderBottomTabsProps) {
  const { t } = useI18n();

  const tabs: Array<{ id: QbBottomTab; label: string; badge?: boolean }> = [
    { id: 'build', label: t('query.visualBuilder.tabBuild'), badge: hasBuildError },
    { id: 'preview', label: t('query.visualBuilder.tabPreview') },
  ];

  return (
    <div className="flex min-h-0 flex-1 flex-col" data-testid="qb-bottom-tabs">
      <div
        className="flex shrink-0 items-center border-b border-edge bg-surface-alt"
        role="tablist"
        data-testid="qb-tabbar"
      >
        {tabs.map((item) => {
          const active = tab === item.id;
          return (
            <button
              key={item.id}
              type="button"
              role="tab"
              aria-selected={active}
              onClick={() => onTabChange(item.id)}
              className={cn(
                'relative flex items-center gap-1.5 px-5 py-2 text-[13px] transition-colors',
                active ? 'bg-surface font-medium text-fg' : 'text-fg-secondary hover:text-fg',
              )}
              data-testid={`qb-tab-${item.id}`}
            >
              {item.label}
              {item.badge && (
                <span
                  className="inline-block h-1.5 w-1.5 rounded-full bg-danger"
                  title={t('query.visualBuilder.buildErrorBadge')}
                  data-testid={`qb-tab-${item.id}-badge`}
                />
              )}
              <span
                className={cn(
                  'absolute inset-x-0 bottom-0 h-0.5 bg-accent transition-opacity duration-300',
                  active ? 'opacity-100' : 'opacity-0',
                )}
              />
            </button>
          );
        })}
      </div>

      {/* Mutually exclusive content — only the active tab is mounted. */}
      {/* The region itself is a flex column so the preview can fill it via
          flex-1; each tab keeps its own scroll container so the Build tab
          behaves exactly as before. */}
      <div
        className="flex min-h-0 flex-1 flex-col"
        role="tabpanel"
        data-testid="qb-tab-content"
        data-active-tab={tab}
      >
        {tab === 'build' ? (
          <div className="min-h-0 flex-1 overflow-auto" data-testid="qb-build-scroll">
            {buildContent}
          </div>
        ) : (
          previewContent
        )}
      </div>
    </div>
  );
}
