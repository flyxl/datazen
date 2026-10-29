import { useRef, type KeyboardEvent, type MouseEvent } from 'react';
import { X } from 'lucide-react';
import { cn } from '../../lib/cn';
import type { Panel } from '../../stores/panelStore';
import { getPanelIcon, getPanelTabLabel } from './contentViewHelpers';
import { useI18n } from '../../hooks/useI18n';
import { useSchemaStore } from '../../stores/schemaStore';

export interface PanelTabBarProps {
  panels: Panel[];
  activePanelId: string | null;
  onSelectPanel: (panelId: string) => void;
  onClosePanel: (panelId: string) => void;
  onContextMenu: (panelId: string, e: MouseEvent) => void;
}

export function PanelTabBar({
  panels,
  activePanelId,
  onSelectPanel,
  onClosePanel,
  onContextMenu,
}: PanelTabBarProps) {
  const { t } = useI18n();
  const schemas = useSchemaStore((s) => s.schemas);
  const tabRefs = useRef<Record<string, HTMLButtonElement | null>>({});
  if (panels.length === 0) return null;

  const focusPanel = (index: number) => {
    const panel = panels[index];
    if (!panel) return;
    tabRefs.current[panel.id]?.focus();
    onSelectPanel(panel.id);
  };

  const handleTabKeyDown = (event: KeyboardEvent<HTMLButtonElement>, index: number) => {
    let nextIndex: number | undefined;
    if (event.key === 'ArrowRight') nextIndex = (index + 1) % panels.length;
    if (event.key === 'ArrowLeft') nextIndex = (index - 1 + panels.length) % panels.length;
    if (event.key === 'Home') nextIndex = 0;
    if (event.key === 'End') nextIndex = panels.length - 1;

    if (nextIndex !== undefined) {
      event.preventDefault();
      focusPanel(nextIndex);
    }
  };

  return (
    // Height contract: this bar and the navigator's "Databases" title row are
    // the window's two column headers, so both are pinned to `h-12` instead of
    // each being sized by whatever its own buttons happened to add up to.
    // `NavigatorToolbar` is the reference; `PanelTabBarHeight.test.tsx` is what
    // keeps the two literals from drifting apart again.
    //
    // `items-stretch` (not `items-center`) is what lets the tab's `h-full`
    // resolve at all: the scrollable tab list is the only child, and a
    // percentage height against an `auto` parent is just `auto`.
    <div
      className="flex h-12 min-h-[48px] shrink-0 items-stretch border-b border-edge bg-surface-alt"
      data-testid="panel-tab-bar"
    >
      <div
        role="tablist"
        aria-label={t('panel.tabListLabel')}
        className="scrollbar-hide flex min-w-0 flex-1 overflow-x-auto"
        onWheel={(e) => {
          if (Math.abs(e.deltaY) <= Math.abs(e.deltaX)) return;
          e.currentTarget.scrollLeft += e.deltaY;
        }}
      >
        {panels.map((panel) => {
          const isActive = panel.id === activePanelId;
          const sessionDatabase = schemas.get(panel.dbSessionId)?.currentDatabase ?? null;
          const tabLabel = getPanelTabLabel(panel, sessionDatabase, t);
          return (
            <div
              key={panel.id}
              data-testid="panel-tab"
              className={cn(
                // `h-full` rather than `py-2`: the bar owns the height, and the
                // active-tab underline resolves against this box, so the tab has
                // to reach the bar's bottom edge to sit on the border.
                'group relative flex h-full items-center gap-1.5 border-r border-edge px-3 text-xs transition-colors',
                isActive
                  ? 'bg-surface text-fg'
                  : 'text-fg-secondary hover:bg-surface-raised hover:text-fg',
              )}
              title={tabLabel}
              onContextMenu={(e) => onContextMenu(panel.id, e)}
            >
              <button
                ref={(element) => {
                  tabRefs.current[panel.id] = element;
                }}
                type="button"
                data-testid="panel-tab-select"
                role="tab"
                aria-selected={isActive}
                tabIndex={isActive ? 0 : -1}
                aria-label={tabLabel}
                className="flex items-center gap-1.5 rounded-sm outline-none focus-visible:ring-2 focus-visible:ring-accent/60"
                onClick={() => onSelectPanel(panel.id)}
                onKeyDown={(event) => handleTabKeyDown(event, panels.indexOf(panel))}
              >
                {getPanelIcon(panel)}
                <span className="max-w-[200px] truncate">{tabLabel}</span>
              </button>
              <button
                type="button"
                data-testid="panel-tab-close"
                aria-label={t('panel.closeTab', { title: tabLabel })}
                className="rounded p-0.5 text-fg-muted opacity-0 hover:bg-surface-raised hover:text-fg group-hover:opacity-100"
                onClick={() => onClosePanel(panel.id)}
              >
                <X className="h-3 w-3" />
              </button>
              <span
                className={cn(
                  'absolute inset-x-0 bottom-0 h-0.5 bg-accent transition-opacity duration-300',
                  isActive ? 'opacity-100' : 'opacity-0',
                )}
              />
            </div>
          );
        })}
      </div>
    </div>
  );
}
