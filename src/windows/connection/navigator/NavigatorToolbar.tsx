import {
  ChevronsDownUp,
  Download,
  FolderPlus,
  PanelLeftClose,
  Plus,
  RefreshCw,
  Search,
  Upload,
} from 'lucide-react';
import type { I18nKey } from '../../../locales';

export interface NavigatorToolbarProps {
  t: (key: I18nKey) => string;
  searchQuery: string;
  setSearchQuery: (value: string) => void;
  onNewConnection: () => void;
  onExportConnections?: () => void;
  onImportConnections?: () => void;
  onRefresh?: () => void;
  onCollapseSidebar?: () => void;
  onNewGroup: () => void;
  onCollapseAll: () => void;
}

export function NavigatorToolbar({
  t,
  searchQuery,
  setSearchQuery,
  onNewConnection,
  onExportConnections,
  onImportConnections,
  onRefresh,
  onCollapseSidebar,
  onNewGroup,
  onCollapseAll,
}: NavigatorToolbarProps) {
  return (
    <>
      <div className="flex h-12 min-h-[48px] shrink-0 items-center justify-between border-b border-edge px-2">
        <span className="text-[13px] font-semibold text-fg">{t('nav.connections')}</span>
        <div className="flex items-center gap-0.5">
          {onExportConnections && (
            <button
              type="button"
              className="flex h-6 w-6 items-center justify-center rounded-md text-fg-muted hover:bg-surface-raised hover:text-fg"
              onClick={onExportConnections}
              title={t('common.exportConnections')}
            >
              <Upload className="h-3.5 w-3.5" />
            </button>
          )}
          {onImportConnections && (
            <button
              type="button"
              className="flex h-6 w-6 items-center justify-center rounded-md text-fg-muted hover:bg-surface-raised hover:text-fg"
              onClick={onImportConnections}
              title={t('common.importConnections')}
            >
              <Download className="h-3.5 w-3.5" />
            </button>
          )}
          <button
            type="button"
            className="flex h-6 w-6 items-center justify-center rounded-md text-fg-muted hover:bg-surface-raised hover:text-fg"
            onClick={onNewConnection}
            title={t('common.newConnection')}
          >
            <Plus className="h-3.5 w-3.5" />
          </button>
          <button
            type="button"
            className="flex h-6 w-6 items-center justify-center rounded-md text-fg-muted hover:bg-surface-raised hover:text-fg"
            onClick={onNewGroup}
            title={t('common.newGroup')}
          >
            <FolderPlus className="h-3.5 w-3.5" />
          </button>
          <button
            type="button"
            className="flex h-6 w-6 items-center justify-center rounded-md text-fg-muted hover:bg-surface-raised hover:text-fg"
            onClick={onCollapseAll}
            title={t('connWin.collapseAll')}
          >
            <ChevronsDownUp className="h-3.5 w-3.5" />
          </button>
          {onRefresh && (
            <button
              type="button"
              className="flex h-6 w-6 items-center justify-center rounded-md text-fg-muted hover:bg-surface-raised hover:text-fg"
              onClick={onRefresh}
              title={t('connWin.refresh')}
            >
              <RefreshCw className="h-3.5 w-3.5" />
            </button>
          )}
          {onCollapseSidebar && (
            <button
              type="button"
              className="flex h-6 w-6 items-center justify-center rounded-md text-fg-muted hover:bg-surface-raised hover:text-fg"
              onClick={onCollapseSidebar}
              title={t('connWin.collapseSidebar')}
            >
              <PanelLeftClose className="h-3.5 w-3.5" />
            </button>
          )}
        </div>
      </div>

      <div className="flex shrink-0 items-center border-b border-edge px-2 py-1.5">
        <div className="relative flex-1">
          <Search className="absolute left-2 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-fg-muted" />
          <input
            type="text"
            className="h-7 w-full rounded-md bg-surface pl-7 pr-2 text-xs text-fg placeholder:text-fg-muted focus:outline-none focus:ring-1 focus:ring-accent"
            placeholder={t('main.searchPlaceholder')}
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
          />
        </div>
      </div>
    </>
  );
}
