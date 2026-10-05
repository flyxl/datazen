import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type MouseEvent as ReactMouseEvent,
} from 'react';
import { RefreshCw, Trash2 } from 'lucide-react';
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
import type { EditorSessionController } from '../../../lib/session/EditorSessionController';
import { switchDatabaseSession } from '../../../lib/session/QueryPanelSession';
import { switchResultPromptKey, sessionErrorPromptKey } from '../../../lib/session/sessionPrompts';
import type { SessionView } from '@datazen/backend-client';

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
  /** Two-phase session switch for the QueryPanel editor owned by this pane. */
  sessionController?: EditorSessionController | null;
  /** Confirm switching away from an active transaction. Defaults to abort. */
  confirmTransactionSwitch?: (session: SessionView) => Promise<boolean> | boolean;
  /** Surfaces switch conflicts/failures via the mapped session prompt keys. */
  onSessionPrompt?: (promptKey: string, message?: string) => void;
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
  sessionController,
  confirmTransactionSwitch,
  onSessionPrompt,
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
    void ensureNamespacePath([], dbSessionId);
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
        if (next.length > 0) await ensureNamespacePath(next, dbSessionId);
        return;
      }
      const db = next[0];
      // Guard: in single-database mode the context-path root can be a *schema*
      // (e.g. `public`) rather than a database — never hand a schema name to
      // switchDatabase, which would run listTables('public') and pin the
      // session's currentDatabase to a non-existent database.
      if (db && databases.includes(db) && db !== currentDatabase) {
        // Persist the database to the panel so that `selectedDatabase`
        // (= database ?? currentDatabase) reflects the switch even when the
        // panel already has a bound database from creation time.
        updatePanel(panelId, { database: db });
        // Route the switch through the two-phase session switch: the
        // transaction confirmation and revision-conflict handling live there,
        // and the prompt keys come from the shared sessionPrompts mapping.
        // The legacy session pointer only moves when the switch succeeds.
        if (sessionController) {
          try {
            const result = await switchDatabaseSession({
              controller: sessionController,
              desired: {
                database: db,
                catalog: null,
                schema: null,
                path: [],
              },
              confirmTransaction: confirmTransactionSwitch,
              onRevisionConflict: (session) => {
                onSessionPrompt?.(
                  'query.session.revisionConflict',
                  session?.observedContext?.namespace?.database ?? undefined,
                );
              },
            });
            if (result.status === 'switched') {
              await switchDatabase(db, dbSessionId);
            } else {
              const key = switchResultPromptKey(result);
              if (key) {
                onSessionPrompt?.(
                  key,
                  result.status === 'failed' && result.error instanceof Error
                    ? result.error.message
                    : undefined,
                );
              }
            }
          } catch (error) {
            onSessionPrompt?.(sessionErrorPromptKey(error));
          }
        } else {
          await switchDatabase(db, dbSessionId);
        }
      }
    },
    [
      dbSessionId,
      currentDatabase,
      databases,
      ensureNamespacePath,
      isPathHierarchy,
      panelId,
      switchDatabase,
      updatePanel,
      sessionController,
      confirmTransactionSwitch,
      onSessionPrompt,
    ],
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
      // Level 0 is always the database/catalog root. Sync it to the panel's
      // bound `database` (for both plain and path-hierarchy drivers) so that
      // re-execution and tab restoration keep using the tab's own database.
      if (index === 0) updatePanel(panelId, { database: value });
      // Level 1 is the PG-family schema envelope only for non-path-hierarchy
      // drivers; in path-hierarchy trees it is a namespace level carried by
      // `namespacePath`, not the panel schema.
      if (!isPathHierarchy && index === 1) updatePanel(panelId, { schema: value });
      void applyContextPath([...contextPath.slice(0, index), value]);
    },
    [applyContextPath, contextPath, panelId, updatePanel, isPathHierarchy],
  );

  const handleQualifiedPath = useCallback(
    (parents: string[]) => {
      if (ensureTimer.current) clearTimeout(ensureTimer.current);
      ensureTimer.current = setTimeout(() => {
        void ensureNamespacePath(parents, dbSessionId);
      }, 120);
      const roots = new Set(namespaceRootsFrom(namespaceTree, pathAliases, databases));
      if (parents[0] && roots.has(parents[0]) && !pathsEqual(parents, contextPath)) {
        void applyContextPath(parents);
      }
    },
    [
      applyContextPath,
      contextPath,
      databases,
      dbSessionId,
      ensureNamespacePath,
      namespaceTree,
      pathAliases,
    ],
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
  const favoritesRoot = usePanelStore((s) => s.favoritesRoot);
  const updateSql = usePanelStore((s) => s.updateSql);
  const loadHistory = usePanelStore((s) => s.loadHistory);
  const refreshFavorites = usePanelStore((s) => s.refreshFavorites);
  const deleteFavorite = usePanelStore((s) => s.deleteFavorite);
  const [historySearch, setHistorySearch] = useState('');
  const [historyScopeMode, setHistoryScopeMode] = useState<'current' | 'all'>('current');

  // Since §2.6 a favorite is a file the user can put in a synced folder, so
  // the listing can change without this app writing anything. Two moments can
  // see a file we have never looked at: the panel being opened, and the window
  // coming back to the foreground after the sync client did its work. Both ask
  // for a rescan; a stale panel is worse than a redundant one.
  useEffect(() => {
    if (!favoritesVisible) return;
    void refreshFavorites(connectionId);
  }, [favoritesVisible, connectionId, refreshFavorites]);

  useEffect(() => {
    if (!favoritesVisible) return;
    const onFocus = () => void refreshFavorites(connectionId);
    window.addEventListener('focus', onFocus);
    return () => window.removeEventListener('focus', onFocus);
  }, [favoritesVisible, connectionId, refreshFavorites]);

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
        <aside
          className="w-64 shrink-0 overflow-y-auto border-l border-edge bg-surface-alt"
          data-testid="query-favorites-panel"
        >
          <div className="flex items-center justify-between border-b border-edge px-3 py-2">
            <span className="text-[11px] font-semibold uppercase tracking-wider text-fg-muted">
              {t('query.favoritesTitle')}
            </span>
            <button
              type="button"
              data-testid="favorites-refresh"
              className="p-1 text-fg-muted hover:text-fg"
              title={t('query.favoritesRefresh')}
              aria-label={t('query.favoritesRefresh')}
              onClick={() => void refreshFavorites(connectionId)}
            >
              <RefreshCw className="h-3 w-3" />
            </button>
          </div>
          {favoritesRoot && (
            // §2.6.3: the user has to be told which folder to sync, otherwise
            // the file-first format is invisible to the person who benefits.
            <div
              data-testid="favorites-root"
              className="border-b border-edge px-3 py-1.5 text-[11px] text-fg-muted"
            >
              <span className="opacity-70">{t('query.favoritesRoot')}</span>
              <span className="ml-1 break-all font-mono">{favoritesRoot}</span>
            </div>
          )}
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
        <aside
          className="w-64 shrink-0 overflow-y-auto border-l border-edge bg-surface-alt"
          data-testid="query-history-panel"
        >
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
                className={`rounded px-2 py-0.5 text-[11px] ${historyScopeMode === 'current' ? 'bg-accent text-on-accent' : 'border border-edge text-fg-muted hover:text-fg'}`}
              >
                {t('query.historyScopeCurrent')}
              </button>
              <button
                type="button"
                data-testid="history-scope-all"
                aria-pressed={historyScopeMode === 'all'}
                onClick={() => setHistoryScopeMode('all')}
                className={`rounded px-2 py-0.5 text-[11px] ${historyScopeMode === 'all' ? 'bg-accent text-on-accent' : 'border border-edge text-fg-muted hover:text-fg'}`}
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
                      data-testid="query-history-item"
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
