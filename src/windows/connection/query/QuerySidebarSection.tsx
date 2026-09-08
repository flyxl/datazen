import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type MouseEvent as ReactMouseEvent,
} from 'react';
import { Trash2 } from 'lucide-react';
import { usePanelStore } from '../../../stores/panelStore';
import { nextPanelId, type QueryPanel } from '../../../stores/panelTypes';
import { useSchemaStore } from '../../../stores/schemaStore';
import { useI18n } from '../../../hooks/useI18n';
import { showNativeContextMenu } from '../../../lib/nativeContextMenu';
import {
  buildFavoriteSidebarContextMenuItems,
  buildHistorySidebarContextMenuItems,
  buildHistorySidebarHeaderContextMenuItems,
} from '../../../lib/querySidebarContextMenu';
import { findGroupForDatabase, groupQueryHistory } from '../../../lib/historyGroups';
import { queryCommands } from '../../../commands/query';
import { formatLastConnected } from '../../../lib/formatters';
import {
  namespaceRootsFrom,
  pathsEqual,
  resolveQueryContextPath,
  autoCompletePathHierarchyPath,
} from '../../../lib/queryContextPath';
import type { SqlNamespace } from '../../../lib/sqlNamespace';

export interface UseQueryContextPathOptions {
  panelId: string;
  dbSessionId: string;
  panelNamespacePath?: string[];
  isPathHierarchy: boolean;
  selectedDatabase?: string | null;
  namespaceTree: SqlNamespace;
  pathAliases: Record<string, string>;
  databases: string[];
  currentDatabase: string | null;
}

export function useQueryContextPath({
  panelId,
  dbSessionId,
  panelNamespacePath,
  isPathHierarchy,
  selectedDatabase,
  namespaceTree,
  pathAliases,
  databases,
  currentDatabase,
}: UseQueryContextPathOptions) {
  const updatePanel = usePanelStore((s) => s.updatePanel);
  const switchDatabase = useSchemaStore((s) => s.switchDatabase);
  const ensureNamespacePath = useSchemaStore((s) => s.ensureNamespacePath);
  const [contextPath, setContextPath] = useState<string[]>(() => panelNamespacePath ?? []);
  const ensureTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(
    () => () => {
      if (ensureTimer.current) clearTimeout(ensureTimer.current);
    },
    [],
  );

  useEffect(() => {
    void ensureNamespacePath([]);
  }, [dbSessionId, selectedDatabase, ensureNamespacePath]);

  useEffect(() => {
    if (isPathHierarchy) return;
    setContextPath(selectedDatabase ? [selectedDatabase] : []);
  }, [selectedDatabase, isPathHierarchy]);

  const applyContextPath = useCallback(
    async (next: string[]) => {
      setContextPath(next);
      if (isPathHierarchy) {
        updatePanel(panelId, { namespacePath: next.length > 0 ? next : undefined });
        if (next.length > 0) await ensureNamespacePath(next);
        return;
      }
      const db = next[0];
      if (db && db !== currentDatabase) {
        await switchDatabase(db);
      }
    },
    [currentDatabase, ensureNamespacePath, isPathHierarchy, panelId, switchDatabase, updatePanel],
  );

  useEffect(() => {
    if (!isPathHierarchy) return;
    const next = autoCompletePathHierarchyPath(namespaceTree, pathAliases, databases, contextPath);
    if (next) {
      void applyContextPath(next);
    }
  }, [isPathHierarchy, namespaceTree, pathAliases, databases, contextPath, applyContextPath]);

  const handleSelectContextLevel = useCallback(
    (index: number, value: string) => {
      if (!value) return;
      if (!isPathHierarchy) {
        if (index === 0) updatePanel(panelId, { database: value });
        if (index === 1) updatePanel(panelId, { schema: value });
      }
      void applyContextPath([...contextPath.slice(0, index), value]);
    },
    [applyContextPath, contextPath, panelId, updatePanel, isPathHierarchy],
  );

  const handleQualifiedPath = useCallback(
    (parents: string[]) => {
      if (ensureTimer.current) clearTimeout(ensureTimer.current);
      ensureTimer.current = setTimeout(() => {
        void ensureNamespacePath(parents);
      }, 120);
      const roots = new Set(namespaceRootsFrom(namespaceTree, pathAliases, databases));
      if (parents[0] && roots.has(parents[0]) && !pathsEqual(parents, contextPath)) {
        void applyContextPath(parents);
      }
    },
    [applyContextPath, contextPath, databases, ensureNamespacePath, namespaceTree, pathAliases],
  );

  const syncContextFromSql = useCallback(
    async (sql: string) => {
      const resolved = resolveQueryContextPath(sql, {
        databases,
        namespaceRoots: namespaceRootsFrom(namespaceTree, pathAliases, databases),
      });
      if (!resolved || pathsEqual(resolved, contextPath)) return;
      await applyContextPath(resolved);
    },
    [applyContextPath, contextPath, databases, namespaceTree, pathAliases],
  );

  return {
    contextPath,
    handleSelectContextLevel,
    handleQualifiedPath,
    syncContextFromSql,
  };
}

export interface QuerySidebarSectionProps {
  panelId: string;
  connectionId: string;
  selectedDatabase?: string | null;
  favoritesVisible: boolean;
  historyVisible: boolean;
}

export function QuerySidebarSection({
  panelId,
  connectionId,
  selectedDatabase,
  favoritesVisible,
  historyVisible,
}: QuerySidebarSectionProps) {
  const { t } = useI18n();
  const history = usePanelStore((s) => s.queryHistory);
  const favorites = usePanelStore((s) => s.queryFavorites);
  const updateSql = usePanelStore((s) => s.updateSql);
  const loadHistory = usePanelStore((s) => s.loadHistory);
  const deleteFavorite = usePanelStore((s) => s.deleteFavorite);
  const [historySearch, setHistorySearch] = useState('');
  const [historyScopeMode, setHistoryScopeMode] = useState<'current' | 'all'>('current');

  const copySqlToClipboard = useCallback((sql: string) => {
    void navigator.clipboard.writeText(sql);
  }, []);

  const handleOpenFavoriteInNewTab = useCallback(
    (favorite: { id: string; title: string; sql: string }) => {
      const currentPanel = usePanelStore.getState().panels.find((p) => p.id === panelId);
      if (!currentPanel || currentPanel.type !== 'query') {
        updateSql(panelId, favorite.sql);
        return;
      }
      const newPanelId = nextPanelId('qry');
      const newPanel: QueryPanel = {
        ...currentPanel,
        id: newPanelId,
        title: favorite.title || currentPanel.title,
        sql: favorite.sql,
      };
      usePanelStore.getState().addPanel(newPanel, true);
      usePanelStore.getState().updateSql(newPanelId, favorite.sql);
      usePanelStore.getState().setActivePanel(newPanelId);
    },
    [panelId, updateSql],
  );

  const handleFavoriteContextMenu = useCallback(
    (e: ReactMouseEvent, favorite: { id: string; title: string; sql: string }) => {
      e.preventDefault();
      e.stopPropagation();
      void showNativeContextMenu(
        buildFavoriteSidebarContextMenuItems({
          labels: {
            openInNewTab: t('query.openInNewTab'),
            applySql: t('query.applySql'),
            copySql: t('common.copySql'),
            delete: t('common.delete'),
          },
          handlers: {
            onOpenInNewTab: () => handleOpenFavoriteInNewTab(favorite),
            onApplySql: () => updateSql(panelId, favorite.sql),
            onCopySql: () => copySqlToClipboard(favorite.sql),
            onDelete: () => {
              void deleteFavorite(favorite.id);
            },
          },
        }),
        { x: e.clientX, y: e.clientY },
      );
    },
    [handleOpenFavoriteInNewTab, panelId, t, updateSql, copySqlToClipboard, deleteFavorite],
  );

  const handleHistoryContextMenu = useCallback(
    (e: ReactMouseEvent, sql: string) => {
      e.preventDefault();
      e.stopPropagation();
      void showNativeContextMenu(
        buildHistorySidebarContextMenuItems({
          labels: {
            applySql: t('query.applySql'),
            copySql: t('common.copySql'),
          },
          handlers: {
            onApplySql: () => updateSql(panelId, sql),
            onCopySql: () => copySqlToClipboard(sql),
          },
        }),
        { x: e.clientX, y: e.clientY },
      );
    },
    [panelId, t, updateSql, copySqlToClipboard],
  );

  const handleHistoryHeaderContextMenu = useCallback(
    (e: ReactMouseEvent) => {
      e.preventDefault();
      e.stopPropagation();
      void showNativeContextMenu(
        buildHistorySidebarHeaderContextMenuItems({
          labels: { clearHistory: t('query.clearHistory') },
          handlers: {
            onClearHistory: () => {
              void (async () => {
                await queryCommands.clearQueryHistory();
                await loadHistory(connectionId);
              })();
            },
          },
        }),
        { x: e.clientX, y: e.clientY },
      );
    },
    [t, connectionId, loadHistory],
  );

  const handleClearHistory = useCallback(() => {
    void (async () => {
      await queryCommands.clearQueryHistory();
      await loadHistory(connectionId);
    })();
  }, [connectionId, loadHistory]);

  const historyGroups = useMemo(
    () => groupQueryHistory(history, t('query.historyUnknownDb')),
    [history, t],
  );
  const currentDbGroup = useMemo(
    () => findGroupForDatabase(historyGroups, selectedDatabase),
    [historyGroups, selectedDatabase],
  );
  const historyScopeFallback = historyScopeMode === 'current' && !currentDbGroup;
  const scopedSections = useMemo(() => {
    const q = historySearch.trim().toLowerCase();
    const applyQ = (items: typeof history) =>
      q ? items.filter((h) => h.sql.toLowerCase().includes(q)) : items;
    if (historyScopeMode === 'current' && currentDbGroup) {
      return [
        {
          key: currentDbGroup.key,
          label: null as string | null,
          items: applyQ(currentDbGroup.entries),
        },
      ];
    }
    return historyGroups.map((g) => ({ key: g.key, label: g.label, items: applyQ(g.entries) }));
  }, [historyScopeMode, currentDbGroup, historyGroups, historySearch]);
  const filteredCount = scopedSections.reduce((n, s) => n + s.items.length, 0);

  return (
    <>
      {favoritesVisible && (
        <aside className="w-64 shrink-0 overflow-y-auto border-l border-edge bg-surface-alt">
          <div className="border-b border-edge px-3 py-2 text-[11px] font-semibold uppercase tracking-wider text-fg-muted">
            {t('query.favoritesTitle')}
          </div>
          {favorites.length === 0 ? (
            <div className="px-3 py-4 text-center text-xs text-fg-muted">
              {t('query.noFavorites')}
            </div>
          ) : (
            favorites.map((f) => (
              <div
                key={f.id}
                className="group flex w-full items-start border-b border-edge px-3 py-2 hover:bg-surface-raised"
                onContextMenu={(e) => handleFavoriteContextMenu(e, f)}
              >
                <button
                  type="button"
                  className="min-w-0 flex-1 text-left"
                  onClick={() => handleOpenFavoriteInNewTab(f)}
                  title={t('query.openInNewTab')}
                >
                  <div className="truncate text-xs font-medium text-fg">{f.title}</div>
                  <div className="mt-0.5 truncate font-mono text-[11px] text-fg-muted">{f.sql}</div>
                </button>
                <button
                  type="button"
                  className="ml-1 shrink-0 p-1 text-fg-muted opacity-0 hover:text-red-400 group-hover:opacity-100"
                  onClick={() => void deleteFavorite(f.id)}
                >
                  <Trash2 className="h-3 w-3" />
                </button>
              </div>
            ))
          )}
        </aside>
      )}
      {historyVisible && (
        <aside className="w-64 shrink-0 overflow-y-auto border-l border-edge bg-surface-alt">
          <div
            className="flex items-center justify-between border-b border-edge px-3 py-2"
            onContextMenu={handleHistoryHeaderContextMenu}
          >
            <span className="text-[11px] font-semibold uppercase tracking-wider text-fg-muted">
              {t('query.historyTitle')}
            </span>
            {history.length > 0 && (
              <button
                type="button"
                className="p-1 text-fg-muted hover:text-red-400"
                title={t('query.clearHistory')}
                aria-label={t('query.clearHistory')}
                onClick={handleClearHistory}
              >
                <Trash2 className="h-3 w-3" />
              </button>
            )}
          </div>
          {history.length > 0 && (
            <div className="border-b border-edge px-2 py-1.5">
              <input
                type="search"
                value={historySearch}
                onChange={(e) => setHistorySearch(e.target.value)}
                placeholder={t('query.searchHistory')}
                className="w-full rounded border border-edge bg-surface px-2 py-1 text-xs text-fg placeholder:text-fg-muted focus:border-accent focus:outline-none"
                aria-label={t('query.searchHistory')}
              />
            </div>
          )}
          {history.length > 0 && (
            <div className="flex gap-1 border-b border-edge px-2 py-1.5">
              <button
                type="button"
                data-testid="history-scope-current"
                aria-pressed={historyScopeMode === 'current'}
                onClick={() => setHistoryScopeMode('current')}
                className={`rounded px-2 py-0.5 text-[11px] ${historyScopeMode === 'current' ? 'bg-accent text-white' : 'border border-edge text-fg-muted hover:text-fg'}`}
              >
                {t('query.historyScopeCurrent')}
              </button>
              <button
                type="button"
                data-testid="history-scope-all"
                aria-pressed={historyScopeMode === 'all'}
                onClick={() => setHistoryScopeMode('all')}
                className={`rounded px-2 py-0.5 text-[11px] ${historyScopeMode === 'all' ? 'bg-accent text-white' : 'border border-edge text-fg-muted hover:text-fg'}`}
              >
                {t('query.historyScopeAll')}
              </button>
            </div>
          )}
          {history.length > 0 && historyScopeFallback && (
            <div
              data-testid="history-scope-fallback-hint"
              className="border-b border-edge px-3 py-1.5 text-[11px] text-fg-muted"
            >
              {t('query.historyScopeFallbackHint')}
            </div>
          )}
          {history.length === 0 ? (
            <div className="px-3 py-4 text-center text-xs text-fg-muted">
              {t('query.noHistory')}
            </div>
          ) : filteredCount === 0 ? (
            <div className="px-3 py-4 text-center text-xs text-fg-muted">
              {t('query.noHistoryMatch')}
            </div>
          ) : (
            <>
              {historyScopeMode === 'current' && currentDbGroup && (
                <div className="border-b border-edge px-3 py-1.5 text-[11px] text-fg-muted">
                  {t('query.database')}:
                  <span className="ml-1 font-medium text-fg">{currentDbGroup.label}</span>
                </div>
              )}
              {scopedSections.map((section) => (
                <div key={section.key}>
                  {section.label && (
                    <div
                      data-testid="history-group-label"
                      className="sticky top-0 z-10 border-b border-edge bg-surface-alt px-3 py-1 text-[11px] font-semibold text-fg-muted"
                    >
                      {section.label} ({section.items.length})
                    </div>
                  )}
                  {section.items.map((h) => (
                    <button
                      key={h.id}
                      type="button"
                      className="w-full border-b border-edge px-3 py-2 text-left hover:bg-surface-raised"
                      onClick={() => updateSql(panelId, h.sql)}
                      onContextMenu={(e) => handleHistoryContextMenu(e, h.sql)}
                    >
                      <div className="selectable truncate font-mono text-xs text-fg-secondary">
                        {h.sql}
                      </div>
                      <div className="mt-1 flex flex-wrap items-center gap-x-2 gap-y-0.5 text-[11px] text-fg-muted">
                        <span className={h.success ? 'text-green-400' : 'text-red-400'}>
                          {h.success ? t('common.success') : t('common.failed')}
                        </span>
                        <span>{h.executionTimeMs}ms</span>
                        {h.rowsAffected != null && (
                          <span>{t('query.historyRows', { count: h.rowsAffected })}</span>
                        )}
                        <span>{formatLastConnected(h.executedAt)}</span>
                      </div>
                    </button>
                  ))}
                </div>
              ))}
            </>
          )}
        </aside>
      )}
    </>
  );
}
