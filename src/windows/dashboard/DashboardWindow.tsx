import { useCallback, useEffect, useMemo, useState } from 'react';
import {
  BookOpen,
  Download,
  Gauge,
  Loader2,
  Pause,
  Play,
  Plus,
  RefreshCw,
  Trash2,
  Upload,
} from 'lucide-react';
import { TitleBar } from '../../components/TitleBar';
import { StatusBar } from '../../components/StatusBar';
import { Button } from '../../components/ui/Button';
import { Input } from '../../components/ui/Input';
import { useSettings } from '../../hooks/useSettings';
import { useI18n } from '../../hooks/useI18n';
import { getUrlParam } from '../../lib/windowKind';
import { cn } from '../../lib/cn';
import { openDashboardWindow, openDocsWindow, openWorkflowWindow } from '../../lib/windowManager';
import { dashboardCommands } from '../../commands/dashboard';
import { aiCommands } from '../../commands/ai';
import { useDashboardStore } from '../../stores/dashboardStore';
import { DEFAULT_CHART_CONFIG } from '../../types/chart';
import type { Dashboard, DashboardWidget, ViewMode } from '../../types/dashboard';
import type { WorkflowListItem } from '../../types';
import { DEFAULT_REFRESH } from '../../types/dashboard';
import { ChartWidgetTile } from './ChartWidgetTile';
import { RunHistoryDrawer } from './RunHistoryDrawer';
import { WidgetEditorDrawer } from './WidgetEditorDrawer';

function createEmptyDashboard(name: string): Dashboard {
  const now = new Date().toISOString();
  return {
    id: crypto.randomUUID(),
    name,
    createdAt: now,
    updatedAt: now,
    layout: { cols: 12, rowHeight: 80 },
    widgets: [],
    enabled: true,
  };
}

function nextWidgetLayout(widgets: DashboardWidget[]): DashboardWidget['layout'] {
  const maxY = widgets.reduce((m, w) => Math.max(m, w.layout.y + w.layout.h), 0);
  return { x: 0, y: maxY, w: 6, h: 4 };
}

export function DashboardWindow() {
  useSettings();
  const { t } = useI18n();
  const urlDashboardId = getUrlParam('dashboardId') ?? '';
  const [activeDashboardId, setActiveDashboardId] = useState(urlDashboardId);
  const dashboardId = activeDashboardId;

  const entry = useDashboardStore((s) => s.dashboardsById[dashboardId]);
  const current = entry?.dashboard ?? null;
  const runs = entry?.runs ?? {};
  const busyWidgets = entry?.busyWidgets ?? {};
  const loading = entry?.loading ?? false;
  const error = entry?.error ?? null;
  const list = useDashboardStore((s) => s.list);
  const listLoading = useDashboardStore((s) => s.listLoading);
  const mountDashboard = useDashboardStore((s) => s.mountDashboard);
  const loadDashboard = useDashboardStore((s) => s.loadDashboard);
  const saveDashboard = useDashboardStore((s) => s.saveDashboard);
  const fetchDashboards = useDashboardStore((s) => s.fetchDashboards);
  const deleteDashboard = useDashboardStore((s) => s.deleteDashboard);
  const refreshWidget = useDashboardStore((s) => s.refreshWidget);
  const refreshAllWidgets = useDashboardStore((s) => s.refreshAllWidgets);
  const releaseDashboard = useDashboardStore((s) => s.releaseDashboard);

  const [editingWidget, setEditingWidget] = useState<DashboardWidget | null>(null);
  const [editorOpen, setEditorOpen] = useState(false);
  const [historyWidget, setHistoryWidget] = useState<DashboardWidget | null>(null);
  const [historyOpen, setHistoryOpen] = useState(false);
  const [isNewWidget, setIsNewWidget] = useState(false);
  const [refreshingAll, setRefreshingAll] = useState(false);
  const [monitorPaused, setMonitorPaused] = useState(false);
  const [renaming, setRenaming] = useState(false);
  const [nameDraft, setNameDraft] = useState('');
  const [bootstrapping, setBootstrapping] = useState(!urlDashboardId);
  const [editorHiddenSql, setEditorHiddenSql] = useState<
    { configId: string; sql: string } | undefined
  >(undefined);
  const [userWorkflows, setUserWorkflows] = useState<WorkflowListItem[]>([]);

  useEffect(() => {
    void fetchDashboards();
    if (urlDashboardId) {
      setActiveDashboardId(urlDashboardId);
      setBootstrapping(false);
      return;
    }
    let cancelled = false;
    void (async () => {
      setBootstrapping(true);
      await fetchDashboards();
      if (cancelled) return;
      const boards = useDashboardStore.getState().list;
      if (boards.length > 0) {
        setActiveDashboardId(boards[0]!.id);
      }
      setBootstrapping(false);
    })();
    return () => {
      cancelled = true;
    };
  }, [urlDashboardId, fetchDashboards]);

  useEffect(() => {
    if (!dashboardId) return;
    mountDashboard(dashboardId);
    void loadDashboard(dashboardId);
    return () => {
      releaseDashboard(dashboardId);
    };
  }, [dashboardId, mountDashboard, loadDashboard, releaseDashboard]);

  const handleCreatePanel = useCallback(async () => {
    const board = createEmptyDashboard(t('dashboard.newPanel'));
    await saveDashboard(board);
    await fetchDashboards();
    setActiveDashboardId(board.id);
  }, [fetchDashboards, saveDashboard, t]);

  const handleDeletePanel = useCallback(async () => {
    if (!current) return;
    if (!confirm(t('dashboard.deletePanelConfirm'))) return;
    const deletedId = current.id;
    await deleteDashboard(deletedId);
    const remaining = useDashboardStore.getState().list;
    if (remaining.length === 0) {
      setActiveDashboardId('');
    } else {
      setActiveDashboardId(remaining[0]!.id);
    }
  }, [current, deleteDashboard, t]);

  const handleCreateFirstBoard = useCallback(async () => {
    const board = createEmptyDashboard(t('dashboard.defaultName'));
    await saveDashboard(board);
    await fetchDashboards();
    setActiveDashboardId(board.id);
  }, [fetchDashboards, saveDashboard, t]);

  useEffect(() => {
    if (current) setNameDraft(current.name);
  }, [current]);

  useEffect(() => {
    if (!current) return;
    setMonitorPaused(!!current.refreshPaused);
  }, [current]);

  useEffect(() => {
    if (!current) return;
    for (const widget of current.widgets) {
      if (widget.enabled && widget.refresh.mode === 'onOpen' && !runs[widget.id]) {
        void refreshWidget(current.id, widget.id);
      }
    }
  }, [current, runs, refreshWidget]);

  const handleRefreshAll = useCallback(async () => {
    if (!current) return;
    setRefreshingAll(true);
    try {
      await refreshAllWidgets(current.id);
    } finally {
      setRefreshingAll(false);
    }
  }, [current, refreshAllWidgets]);

  const openWidgetEditor = useCallback(async (widget: DashboardWidget, asNew: boolean) => {
    setEditingWidget(widget);
    setIsNewWidget(asNew);
    setEditorHiddenSql(undefined);
    setUserWorkflows([]);
    if (!asNew && widget.workflowId) {
      try {
        const wf = await aiCommands.workflowGet(widget.workflowId);
        if (wf.visibility === 'dashboardHidden') {
          const queryStep = wf.steps.find((s) => s.type === 'query');
          setEditorHiddenSql({
            configId: wf.connection ?? queryStep?.connection ?? '',
            sql: queryStep?.sql ?? 'SELECT 1 AS v',
          });
        } else {
          const workflows = await aiCommands.workflowList();
          setUserWorkflows(workflows);
        }
      } catch {
        /* keep read-only workflow id */
      }
    } else if (asNew) {
      setEditorHiddenSql({ configId: '', sql: 'SELECT 1 AS v' });
    }
    setEditorOpen(true);
  }, []);

  const handleAddWidget = useCallback(() => {
    if (!current) return;
    const draft: DashboardWidget = {
      id: crypto.randomUUID(),
      title: t('dashboard.newWidget'),
      workflowId: '',
      viewMode: 'chart',
      chartConfig: { ...DEFAULT_CHART_CONFIG, yAxes: ['v'] },
      layout: nextWidgetLayout(current.widgets),
      refresh: { ...DEFAULT_REFRESH },
      enabled: true,
    };
    void openWidgetEditor(draft, true);
  }, [current, openWidgetEditor, t]);

  const handleSaveWidget = useCallback(
    async (widget: DashboardWidget, hiddenSql?: { configId: string; sql: string }) => {
      if (!current) return;
      let savedWidget = widget;
      if (isNewWidget && hiddenSql) {
        const created = await dashboardCommands.createWidgetFromSql({
          dashboardId: current.id,
          configId: hiddenSql.configId,
          sql: hiddenSql.sql,
          title: widget.title,
          viewMode: widget.viewMode,
          chartConfig: widget.chartConfig,
        });
        savedWidget = created.widget;
        await loadDashboard(current.id);
      } else {
        if (!isNewWidget && hiddenSql) {
          await dashboardCommands.updateHiddenWidgetSql({
            workflowId: widget.workflowId,
            configId: hiddenSql.configId,
            sql: hiddenSql.sql,
          });
        }
        const widgets = isNewWidget
          ? [...current.widgets, widget]
          : current.widgets.map((w) => (w.id === widget.id ? widget : w));
        await saveDashboard({ ...current, widgets });
      }
      setEditorOpen(false);
      setEditingWidget(null);
      if (isNewWidget && !hiddenSql) {
        void refreshWidget(current.id, savedWidget.id);
      } else if (!isNewWidget) {
        void refreshWidget(current.id, savedWidget.id);
      }
      setIsNewWidget(false);
    },
    [current, isNewWidget, saveDashboard, refreshWidget, loadDashboard],
  );

  const handleViewModeChange = useCallback(
    async (widgetId: string, viewMode: ViewMode) => {
      if (!current) return;
      const widgets = current.widgets.map((w) => (w.id === widgetId ? { ...w, viewMode } : w));
      await saveDashboard({ ...current, widgets });
    },
    [current, saveDashboard],
  );

  const handleDeleteWidget = useCallback(
    async (widgetId: string) => {
      if (!current) return;
      if (!confirm(t('dashboard.deleteWidgetConfirm'))) return;
      const widgets = current.widgets.filter((w) => w.id !== widgetId);
      await saveDashboard({ ...current, widgets });
    },
    [current, saveDashboard, t],
  );

  const handleRename = useCallback(async () => {
    if (!current || !nameDraft.trim()) return;
    await saveDashboard({ ...current, name: nameDraft.trim() });
    setRenaming(false);
  }, [current, nameDraft, saveDashboard]);

  const handleExport = useCallback(async () => {
    if (!current) return;
    const safeName = current.name.replace(/[^\w.-]+/g, '_') || 'dashboard';
    await dashboardCommands.exportWithDialog(current.id, `${safeName}.json`);
  }, [current]);

  const handleImport = useCallback(async () => {
    const imported = await dashboardCommands.importWithDialog();
    if (!imported) return;
    if (imported.id === dashboardId) {
      await loadDashboard(dashboardId);
    } else {
      openDashboardWindow(imported.id, imported.name);
    }
  }, [dashboardId, loadDashboard]);

  const handleToggleMonitorPause = useCallback(async () => {
    if (!current) return;
    const next = !monitorPaused;
    await dashboardCommands.setDashboardRefreshPaused(current.id, next);
    setMonitorPaused(next);
    await loadDashboard(current.id);
  }, [current, monitorPaused, loadDashboard]);

  const titleContent = useMemo(() => {
    if (!current) return t('win.dashboard');
    if (renaming) {
      return (
        <Input
          value={nameDraft}
          onChange={(e) => setNameDraft(e.target.value)}
          onBlur={() => void handleRename()}
          onKeyDown={(e) => {
            if (e.key === 'Enter') void handleRename();
            if (e.key === 'Escape') setRenaming(false);
          }}
          className="h-7 max-w-xs text-sm"
          autoFocus
          data-no-drag
        />
      );
    }
    return (
      <button
        type="button"
        className="truncate text-sm font-medium hover:underline"
        onClick={() => setRenaming(true)}
        data-no-drag
      >
        {current.name}
      </button>
    );
  }, [current, renaming, nameDraft, handleRename, t]);

  if (bootstrapping || (listLoading && !dashboardId && list.length === 0)) {
    return (
      <div className="flex h-screen flex-col bg-surface text-fg" data-testid="dashboard-window">
        <TitleBar title={t('win.dashboard')} />
        <div
          className="flex flex-1 items-center justify-center text-sm text-fg-muted"
          data-testid="dashboard-bootstrapping"
        >
          <Loader2 className="mr-2 h-4 w-4 animate-spin" />
          {t('common.loading')}
        </div>
      </div>
    );
  }

  if (!dashboardId && list.length === 0) {
    return (
      <div className="flex h-screen flex-col bg-surface text-fg" data-testid="dashboard-window">
        <TitleBar title={t('win.dashboard')} />
        <div
          className="flex flex-1 flex-col items-center justify-center gap-3 text-sm text-fg-muted"
          data-testid="dashboard-empty-boards"
        >
          <Gauge className="h-10 w-10 opacity-40" />
          <p>{t('dashboard.emptyBoards')}</p>
          <Button
            className="h-7 gap-1 px-2 text-xs"
            data-testid="dashboard-create-first"
            onClick={() => void handleCreateFirstBoard()}
          >
            <Plus className="mr-1 h-3.5 w-3.5" />
            {t('dashboard.createFirstBoard')}
          </Button>
        </div>
      </div>
    );
  }

  if (!dashboardId) {
    return (
      <div className="flex h-screen flex-col bg-surface text-fg" data-testid="dashboard-window">
        <TitleBar title={t('win.dashboard')} />
        <div className="flex flex-1 items-center justify-center text-sm text-fg-muted">
          {t('dashboard.missingId')}
        </div>
      </div>
    );
  }

  return (
    <div
      className="flex h-screen min-h-0 flex-col bg-surface text-fg"
      data-testid="dashboard-window"
    >
      <TitleBar
        title={titleContent}
        leftContent={<Gauge className="h-4 w-4 text-fg-muted" />}
        rightContent={
          <div className="flex items-center gap-1" data-no-drag>
            <Button
              variant="ghost"
              className="h-7 gap-1 px-2 text-xs"
              data-testid="dashboard-pause-toggle"
              onClick={() => void handleToggleMonitorPause()}
              title={
                monitorPaused ? t('dashboard.resumeMonitoring') : t('dashboard.pauseMonitoring')
              }
            >
              {monitorPaused ? <Play className="h-3.5 w-3.5" /> : <Pause className="h-3.5 w-3.5" />}
            </Button>
            <Button
              variant="ghost"
              className="h-7 gap-1 px-2 text-xs"
              data-testid="dashboard-import"
              onClick={() => void handleImport()}
              disabled={loading}
              title={t('dashboard.import')}
            >
              <Upload className="h-3.5 w-3.5" />
            </Button>
            <Button
              variant="ghost"
              className="h-7 gap-1 px-2 text-xs"
              data-testid="dashboard-export"
              onClick={() => void handleExport()}
              disabled={!current || loading}
              title={t('dashboard.export')}
            >
              <Download className="h-3.5 w-3.5" />
            </Button>
            <Button
              variant="ghost"
              className="h-7 gap-1 px-2 text-xs text-red-400 hover:text-red-300"
              data-testid="dashboard-delete-panel"
              onClick={() => void handleDeletePanel()}
              disabled={!current || loading}
              title={t('dashboard.deletePanel')}
            >
              <Trash2 className="h-3.5 w-3.5" />
            </Button>
            <Button
              variant="ghost"
              className="h-7 gap-1 px-2 text-xs"
              data-testid="dashboard-add-widget"
              onClick={handleAddWidget}
              disabled={!current || loading}
            >
              <Plus className="h-3.5 w-3.5" />
              {t('dashboard.addWidget')}
            </Button>
            <Button
              variant="ghost"
              className="h-7 gap-1 px-2 text-xs"
              data-testid="dashboard-refresh-all"
              onClick={() => void handleRefreshAll()}
              disabled={!current || refreshingAll}
            >
              {refreshingAll ? (
                <Loader2 className="h-3.5 w-3.5 animate-spin" />
              ) : (
                <RefreshCw className="h-3.5 w-3.5" />
              )}
              {t('dashboard.refreshAll')}
            </Button>
            <Button
              variant="ghost"
              className="h-7 w-7 !px-0"
              data-testid="dashboard-docs-help"
              title={t('docs.openDashboardHelp')}
              onClick={() => openDocsWindow('opsDashboard')}
            >
              <BookOpen className="h-3.5 w-3.5" />
            </Button>
          </div>
        }
      />

      {!bootstrapping && list.length > 0 && (
        <div
          className="flex shrink-0 items-center gap-1 overflow-x-auto border-b border-edge px-3 py-1.5"
          data-no-drag
        >
          {list.map((board) => (
            <button
              key={board.id}
              type="button"
              data-testid="dashboard-tab"
              data-dashboard-id={board.id}
              className={cn(
                'shrink-0 rounded-md px-2.5 py-1 text-xs transition-colors',
                board.id === dashboardId
                  ? 'bg-accent/15 font-medium text-accent'
                  : 'text-fg-muted hover:bg-surface-raised hover:text-fg',
              )}
              onClick={() => setActiveDashboardId(board.id)}
            >
              {board.name}
            </button>
          ))}
          <button
            type="button"
            data-testid="dashboard-tab-add"
            className="shrink-0 rounded-md px-2 py-1 text-xs text-fg-muted hover:bg-surface-raised hover:text-fg"
            title={t('dashboard.newPanel')}
            onClick={() => void handleCreatePanel()}
          >
            <Plus className="h-3.5 w-3.5" />
          </button>
        </div>
      )}

      <main className="min-h-0 flex-1 overflow-auto p-4" data-testid="dashboard-main">
        {(bootstrapping || listLoading) && !dashboardId && (
          <div
            className="flex h-full items-center justify-center text-sm text-fg-muted"
            data-testid="dashboard-bootstrapping"
          >
            <Loader2 className="mr-2 h-4 w-4 animate-spin" />
            {t('common.loading')}
          </div>
        )}
        {!bootstrapping && !dashboardId && list.length === 0 && (
          <div
            className="flex h-full flex-col items-center justify-center gap-3 text-sm text-fg-muted"
            data-testid="dashboard-empty-boards"
          >
            <Gauge className="h-10 w-10 opacity-40" />
            <p>{t('dashboard.emptyBoards')}</p>
            <Button
              className="h-7 gap-1 px-2 text-xs"
              data-testid="dashboard-create-first"
              onClick={() => void handleCreateFirstBoard()}
            >
              <Plus className="mr-1 h-3.5 w-3.5" />
              {t('dashboard.createFirstBoard')}
            </Button>
          </div>
        )}
        {loading && !current && !!dashboardId && (
          <div className="flex h-full items-center justify-center text-sm text-fg-muted">
            <Loader2 className="mr-2 h-4 w-4 animate-spin" />
            {t('common.loading')}
          </div>
        )}
        {error && (
          <div className="mb-3 rounded-md border border-red-500/30 bg-red-500/10 px-3 py-2 text-sm text-red-400">
            {error}
          </div>
        )}
        {current && current.widgets.length === 0 && (
          <div
            className="flex h-full flex-col items-center justify-center gap-3 text-sm text-fg-muted"
            data-testid="dashboard-empty"
          >
            <Gauge className="h-10 w-10 opacity-40" />
            <p>{t('dashboard.empty')}</p>
            <Button className="h-7 gap-1 px-2 text-xs" onClick={handleAddWidget}>
              <Plus className="mr-1 h-3.5 w-3.5" />
              {t('dashboard.addWidget')}
            </Button>
          </div>
        )}
        {current && current.widgets.length > 0 && (
          <div
            className="grid gap-3"
            data-testid="dashboard-grid"
            style={{
              gridTemplateColumns: `repeat(${current.layout.cols}, 1fr)`,
              gridAutoRows: `${current.layout.rowHeight}px`,
            }}
          >
            {current.widgets.map((widget) => (
              <ChartWidgetTile
                key={widget.id}
                widget={widget}
                run={runs[widget.id] ?? null}
                busy={!!busyWidgets[widget.id]}
                onEdit={() => void openWidgetEditor(widget, false)}
                onDelete={() => void handleDeleteWidget(widget.id)}
                onHistory={() => {
                  setHistoryWidget(widget);
                  setHistoryOpen(true);
                }}
                onRefresh={() => void refreshWidget(current.id, widget.id)}
                onViewModeChange={(mode) => void handleViewModeChange(widget.id, mode)}
              />
            ))}
          </div>
        )}
      </main>

      <StatusBar
        left={current ? t('dashboard.widgetCount', { count: current.widgets.length }) : ''}
      />

      <WidgetEditorDrawer
        open={editorOpen}
        widget={editingWidget}
        isNew={isNewWidget}
        hiddenSql={editorHiddenSql}
        userWorkflows={userWorkflows}
        onOpenWorkflowEditor={() => openWorkflowWindow()}
        onClose={() => {
          setEditorOpen(false);
          setEditingWidget(null);
          setIsNewWidget(false);
          setEditorHiddenSql(undefined);
          setUserWorkflows([]);
        }}
        onSave={(w, hiddenSql) => void handleSaveWidget(w, hiddenSql)}
      />

      <RunHistoryDrawer
        open={historyOpen}
        dashboardId={dashboardId}
        widget={historyWidget}
        onClose={() => {
          setHistoryOpen(false);
          setHistoryWidget(null);
        }}
      />
    </div>
  );
}

export { createEmptyDashboard };
