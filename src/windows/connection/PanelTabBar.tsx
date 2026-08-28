import type { MouseEvent } from 'react';
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
  if (panels.length === 0) return null;

  return (
    <div className="flex shrink-0 items-center border-b border-edge bg-surface-alt">
      <div
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
                'group relative flex items-center gap-1.5 border-r border-edge px-3 py-2 text-xs transition-colors',
                isActive
                  ? 'bg-surface text-fg'
                  : 'text-fg-secondary hover:bg-surface-raised hover:text-fg',
              )}
              title={tabLabel}
              onContextMenu={(e) => onContextMenu(panel.id, e)}
            >
              <button
                type="button"
                className="flex items-center gap-1.5"
                onClick={() => onSelectPanel(panel.id)}
              >
                {getPanelIcon(panel)}
                <span className="max-w-[200px] truncate">{tabLabel}</span>
              </button>
              <button
                type="button"
                data-testid="panel-tab-close"
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
