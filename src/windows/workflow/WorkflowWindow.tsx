import { useCallback, useEffect, useMemo, useState } from 'react';
import {
  AlertTriangle,
  BarChart3,
  Clock,
  FolderOpen,
  History,
  Loader2,
  Pencil,
  Play,
  Plus,
  RefreshCw,
  Sparkles,
  TableProperties,
  Trash2,
  Wand2,
  X,
} from 'lucide-react';
import { TitleBar } from '../../components/TitleBar';
import { StatusBar } from '../../components/StatusBar';
import { DataTable } from '../../components/DataTable/DataTable';
import type { ColumnDef } from '../../components/DataTable/TableHeader';
import { ChartView } from '../../components/chart/ChartView';
import { WorkflowChatPanel } from '../../components/ai/WorkflowChatPanel';
import { isChartableResult } from '../../lib/chart/fieldInference';
import { Button } from '../../components/ui/Button';
import { Select } from '../../components/ui/Select';
import { useResizable } from '../../hooks/useResizable';
import { useThemeListener } from '../../hooks/useThemeListener';
import { useI18n } from '../../hooks/useI18n';
import { useAiStore } from '../../stores/aiStore';
import { aiCommands } from '../../commands/ai';
import { connectionCommands } from '../../commands/connection';
import { cn } from '../../lib/cn';
import { WorkflowForm, emptyDraft } from './WorkflowForm';
import type { WorkflowDraft } from './WorkflowForm';
import type {
  ColumnInfo,
  HistoryListItem,
  StatementResult,
  StepExecutionResult,
  Value,
  WorkflowDefinition,
  WorkflowExecutionResult,
  WorkflowListItem,
  WorkflowStepType,
} from '../../types';
import type { ChartConfig } from '../../types/chart';

// ── Panel types (same pattern as ConnectionWindow) ──────────────────

interface WorkflowRunPanel {
  type: 'run';
  id: string;
  workflowId: string;
  workflowName: string;
  result: WorkflowExecutionResult | null;
  isExecuting: boolean;
}

interface HistoryDetailPanel {
  type: 'history';
  id: string;
  workflowName: string;
  result: WorkflowExecutionResult;
}

interface EditPanel {
  type: 'edit';
  id: string;
  editingId: string | null;
  draft: WorkflowDraft;
}

interface AiCreatePanel {
  type: 'ai-create';
  id: string;
}

type Panel = WorkflowRunPanel | HistoryDetailPanel | EditPanel | AiCreatePanel;

let panelCounter = 0;
function nextPanelId(prefix: string) {
  panelCounter += 1;
  return `${prefix}-${panelCounter}`;
}

// (WorkflowDraft types and emptyDraft moved to ./WorkflowForm.tsx)

// ── Main Component ──────────────────────────────────────────────────

export function WorkflowWindow() {
  useThemeListener();
  const { t } = useI18n();

  const workflows = useAiStore((s) => s.workflows);
  const workflowsLoading = useAiStore((s) => s.workflowsLoading);
  const loadWorkflows = useAiStore((s) => s.loadWorkflows);
  const loadConfig = useAiStore((s) => s.loadConfig);
  const executeWorkflow = useAiStore((s) => s.executeWorkflow);
  const workflowError = useAiStore((s) => s.workflowError);
  const clearWorkflowResult = useAiStore((s) => s.clearWorkflowResult);
  const setupAiListeners = useAiStore((s) => s.setupEventListeners);

  const [panels, setPanels] = useState<Panel[]>([]);
  const [activePanelId, setActivePanelId] = useState<string | null>(null);
  const [activeStepIndex, setActiveStepIndex] = useState<number | null>(null);

  const [workflowsDir, setWorkflowsDir] = useState('');
  const [feedback, setFeedback] = useState('');
  const [savedConnections, setSavedConnections] = useState<{ id: string; name: string; databaseType: string }[]>([]);
  const [sideTab, setSideTab] = useState<'workflows' | 'history'>('workflows');
  const [historyItems, setHistoryItems] = useState<HistoryListItem[]>([]);
  const { size: sidebarWidth, handleRef: sidebarHandleRef } = useResizable({
    direction: 'horizontal',
    initialSize: 256,
    minSize: 180,
    maxSize: 420,
    storageKey: 'workflow.sidebar',
  });

  useEffect(() => {
    void loadConfig();
    void loadWorkflows();
    void aiCommands.workflowGetDir().then(setWorkflowsDir);
    void connectionCommands.getConnections().then((conns) =>
      setSavedConnections(conns.map((c) => ({ id: c.id, name: c.name, databaseType: c.databaseType }))),
    );
    const cleanup = setupAiListeners();
    return () => { void cleanup.then((fn) => fn()); };
  }, [loadConfig, loadWorkflows, setupAiListeners]);

  const loadHistory = useCallback(async () => {
    const items = await aiCommands.workflowHistoryList();
    setHistoryItems(items);
  }, []);

  useEffect(() => {
    if (sideTab === 'history') void loadHistory();
  }, [sideTab, loadHistory]);

  const activePanel = panels.find((p) => p.id === activePanelId) ?? null;

  // ── Panel operations ──────────────────────────────────────────────

  const openRunPanel = useCallback((workflow: WorkflowListItem) => {
    const existing = panels.find((p) => p.type === 'run' && p.workflowId === workflow.id);
    if (existing) {
      setActivePanelId(existing.id);
      return;
    }
    const panel: WorkflowRunPanel = {
      type: 'run',
      id: nextPanelId('wf'),
      workflowId: workflow.id,
      workflowName: workflow.name,
      result: null,
      isExecuting: false,
    };
    setPanels((prev) => [...prev, panel]);
    setActivePanelId(panel.id);
    setActiveStepIndex(null);
    clearWorkflowResult();
  }, [panels, clearWorkflowResult]);

  const openHistoryPanel = useCallback(async (historyId: string, workflowName: string) => {
    try {
      const entry = await aiCommands.workflowHistoryGet(historyId);
      const panel: HistoryDetailPanel = {
        type: 'history',
        id: nextPanelId('hist'),
        workflowName,
        result: entry.result,
      };
      setPanels((prev) => [...prev, panel]);
      setActivePanelId(panel.id);
      setActiveStepIndex(null);
    } catch (e) {
      setFeedback(String(e));
    }
  }, []);

  const closePanel = useCallback((panelId: string) => {
    setPanels((prev) => {
      const idx = prev.findIndex((p) => p.id === panelId);
      const next = prev.filter((p) => p.id !== panelId);
      setActivePanelId((current) => {
        if (current !== panelId) return current;
        if (next.length === 0) return null;
        return next[Math.min(idx, next.length - 1)].id;
      });
      return next;
    });
  }, []);

  const updateEditDraft = useCallback((panelId: string, draft: WorkflowDraft) => {
    setPanels((prev) => prev.map((p): Panel => {
      if (p.id !== panelId || p.type !== 'edit') return p;
      return { ...p, draft };
    }));
  }, []);

  // ── Workflow CRUD ─────────────────────────────────────────────────

  const [variables, setVariables] = useState<Record<string, string>>({});

  const handleSelectWorkflow = useCallback((workflow: WorkflowListItem) => {
    openRunPanel(workflow);
    const defaults: Record<string, string> = {};
    for (const v of workflow.variables) {
      defaults[v.name] = v.default != null ? String(v.default) : '';
    }
    setVariables(defaults);
  }, [openRunPanel]);

  const handleExecute = useCallback(async () => {
    if (!activePanel || activePanel.type !== 'run') return;
    const wfPanel = activePanel as WorkflowRunPanel;
    setPanels((prev) => prev.map((p): Panel => {
      if (p.id !== wfPanel.id || p.type !== 'run') return p;
      return { ...p, isExecuting: true };
    }));
    setActiveStepIndex(null);

    try {
      await executeWorkflow({ workflowId: wfPanel.workflowId, variables });
      const result = useAiStore.getState().workflowExecutionResult;
      setPanels((prev) => prev.map((p): Panel => {
        if (p.id !== wfPanel.id || p.type !== 'run') return p;
        return { ...p, result, isExecuting: false };
      }));
    } catch {
      setPanels((prev) => prev.map((p): Panel => {
        if (p.id !== wfPanel.id || p.type !== 'run') return p;
        return { ...p, isExecuting: false };
      }));
    }
    void loadHistory();
  }, [activePanel, variables, executeWorkflow, loadHistory]);

  const handleDelete = async (workflowId: string) => {
    if (!confirm(t('workflows.deleteConfirm'))) return;
    await aiCommands.workflowDelete(workflowId);
    setPanels((prev) => prev.filter((p) => !(p.type === 'run' && p.workflowId === workflowId)));
    void loadWorkflows();
  };

  const handleReload = useCallback(async () => {
    await aiCommands.workflowReload();
    void loadWorkflows();
  }, [loadWorkflows]);

  const handleOpenDir = useCallback(async () => {
    if (!workflowsDir) return;
    try {
      const { settingsCommands } = await import('../../commands/settings');
      await settingsCommands.openPath(workflowsDir);
    } catch { /* browser mode */ }
  }, [workflowsDir]);

  const openEditPanel = useCallback((editingId: string | null, draft: WorkflowDraft) => {
    const existing = panels.find((p) => p.type === 'edit' && (p as EditPanel).editingId === editingId);
    if (existing) {
      setActivePanelId(existing.id);
      return;
    }
    const panel: EditPanel = {
      type: 'edit',
      id: nextPanelId('edit'),
      editingId,
      draft,
    };
    setPanels((prev) => [...prev, panel]);
    setActivePanelId(panel.id);
  }, [panels]);

  const handleEdit = async (workflowId: string) => {
    try {
      const workflow: WorkflowDefinition = await aiCommands.workflowGet(workflowId);
      openEditPanel(workflowId, {
        id: workflow.id,
        name: workflow.name,
        description: workflow.description,
        variables: workflow.variables.map((v) => ({
          name: v.name, varType: v.type || 'string', description: v.description, required: v.required ?? false,
        })),
        steps: workflow.steps.map((s) => ({
          type: s.type as WorkflowStepType, id: s.id, sql: s.sql, prompt: s.prompt, connection: s.connection, database: s.database,
        })),
      });
    } catch (e) { setFeedback(String(e)); }
  };

  const handleCreate = () => { openEditPanel(null, emptyDraft()); };

  const handleAiCreate = useCallback(() => {
    const existing = panels.find((p) => p.type === 'ai-create');
    if (existing) {
      setActivePanelId(existing.id);
      return;
    }
    const panel: AiCreatePanel = { type: 'ai-create', id: nextPanelId('ai') };
    setPanels((prev) => [...prev, panel]);
    setActivePanelId(panel.id);
  }, [panels]);

  const handleSave = useCallback(async (panelId: string, d: WorkflowDraft) => {
    const workflow: WorkflowDefinition = {
      id: d.id.trim(), name: d.name.trim(), description: d.description.trim(),
      variables: d.variables.map((v) => ({ name: v.name, type: v.varType, description: v.description, required: v.required })),
      steps: d.steps.map((s) => ({ type: s.type, id: s.id, sql: s.sql, prompt: s.prompt, connection: s.connection, database: s.database })),
    };
    try {
      await aiCommands.workflowSave(workflow);
      closePanel(panelId);
      setFeedback(t('workflows.saved'));
      setTimeout(() => setFeedback(''), 2000);
      void loadWorkflows();
    } catch (e) { setFeedback(String(e)); }
  }, [closePanel, t, loadWorkflows]);

  const handleClearHistory = async () => {
    if (!confirm(t('workflows.history.clearConfirm'))) return;
    await aiCommands.workflowHistoryClear();
    setHistoryItems([]);
  };

  // ── Current panel result ──────────────────────────────────────────

  const currentResult: WorkflowExecutionResult | null = useMemo(() => {
    if (!activePanel) return null;
    if (activePanel.type === 'history') return activePanel.result;
    if (activePanel.type === 'run') return activePanel.result;
    return null;
  }, [activePanel]);

  const currentStep = currentResult && activeStepIndex != null ? currentResult.steps[activeStepIndex] : null;
  const currentWorkflow = activePanel?.type === 'run'
    ? workflows.find((w) => w.id === (activePanel as WorkflowRunPanel).workflowId) ?? null
    : null;
  const isExecuting = activePanel?.type === 'run' ? (activePanel as WorkflowRunPanel).isExecuting : false;

  // ── Render ────────────────────────────────────────────────────────

  return (
    <div className="flex h-screen flex-col bg-surface text-fg">
      <TitleBar
        title={t('win.workflow')}
        leftContent={
          <div className="flex items-center gap-1 ml-2">
            <Button variant="secondary" className="h-6 w-6 !px-0" title={t('workflows.reload')} onClick={() => void handleReload()}>
              <RefreshCw className="h-3 w-3" />
            </Button>
            <Button variant="secondary" className="h-6 w-6 !px-0" title={t('workflows.create')} onClick={handleCreate}>
              <Plus className="h-3 w-3" />
            </Button>
            <Button variant="secondary" className="h-6 w-6 !px-0" title={t('workflows.aiCreate.title')} onClick={handleAiCreate}>
              <Sparkles className="h-3 w-3" />
            </Button>
            {workflowsDir && (
              <Button variant="secondary" className="h-6 w-6 !px-0" title={t('workflows.openDir')} onClick={() => void handleOpenDir()}>
                <FolderOpen className="h-3 w-3" />
              </Button>
            )}
          </div>
        }
      />

      <div className="flex min-h-0 flex-1">
        {/* Left sidebar: workflow list / history */}
        <div style={{ width: sidebarWidth }} className="flex shrink-0 flex-col border-r border-edge bg-surface-alt">
          <div className="flex items-center border-b border-edge">
            <button
              type="button"
              onClick={() => setSideTab('workflows')}
              className={cn('flex-1 px-3 py-2 text-xs transition-colors relative', sideTab === 'workflows' ? 'text-fg font-medium' : 'text-fg-muted hover:text-fg')}
            >
              Workflows
              {sideTab === 'workflows' && <span className="absolute inset-x-0 bottom-0 h-0.5 bg-blue-500" />}
            </button>
            <button
              type="button"
              onClick={() => setSideTab('history')}
              className={cn('flex-1 px-3 py-2 text-xs transition-colors relative flex items-center justify-center gap-1', sideTab === 'history' ? 'text-fg font-medium' : 'text-fg-muted hover:text-fg')}
            >
              <History className="h-3 w-3" />
              {t('workflows.history.title')}
              {sideTab === 'history' && <span className="absolute inset-x-0 bottom-0 h-0.5 bg-blue-500" />}
            </button>
          </div>

          {feedback && (
            <p className={cn('px-3 py-1 text-xs', feedback.startsWith('Error') ? 'text-red-400' : 'text-green-500')}>{feedback}</p>
          )}

          <div className="flex-1 overflow-y-auto">
            {sideTab === 'history' ? (
              <HistoryList items={historyItems} onView={(id, name) => void openHistoryPanel(id, name)} onClear={() => void handleClearHistory()} t={t} />
            ) : (
              <WorkflowSidebarList
                workflows={workflows}
                loading={workflowsLoading}
                activePanelWorkflowId={activePanel?.type === 'run' ? (activePanel as WorkflowRunPanel).workflowId : undefined}
                onSelect={handleSelectWorkflow}
                onEdit={(id) => void handleEdit(id)}
                onDelete={(id) => void handleDelete(id)}
                t={t}
              />
            )}
          </div>

        </div>

        <div
          ref={sidebarHandleRef}
          className="w-1 shrink-0 cursor-col-resize bg-transparent hover:bg-blue-500/30"
        />

        {/* Main content: tab bar + result */}
        <div className="flex min-h-0 min-w-0 flex-1 flex-col">
          {/* Tab bar */}
          {panels.length > 0 && (
            <div className="flex shrink-0 items-center border-b border-edge bg-surface-alt">
              <div className="flex min-w-0 flex-1 overflow-x-auto">
                {panels.map((panel) => {
                  const isActive = panel.id === activePanelId;
                  const icon = panel.type === 'ai-create'
                    ? <Sparkles className="h-3.5 w-3.5 shrink-0 text-accent" />
                    : panel.type === 'edit'
                      ? <Pencil className="h-3.5 w-3.5 shrink-0" />
                      : panel.type === 'run'
                        ? <Wand2 className="h-3.5 w-3.5 shrink-0" />
                        : <History className="h-3.5 w-3.5 shrink-0" />;
                  const label = panel.type === 'ai-create'
                    ? t('workflows.aiCreate.title')
                    : panel.type === 'edit'
                      ? ((panel as EditPanel).editingId ? (panel as EditPanel).draft.name || t('workflows.edit') : t('workflows.create'))
                      : panel.type === 'run'
                        ? (panel as WorkflowRunPanel).workflowName
                        : (panel as HistoryDetailPanel).workflowName;
                  const resultStatus = panel.type === 'run'
                    ? (panel as WorkflowRunPanel).result
                    : panel.type === 'history'
                      ? (panel as HistoryDetailPanel).result
                      : null;

                  return (
                    <div
                      key={panel.id}
                      className={cn(
                        'group relative flex items-center gap-1.5 border-r border-edge px-3 py-2 text-xs',
                        isActive ? 'bg-surface text-fg' : 'text-fg-secondary hover:bg-surface-raised hover:text-fg',
                      )}
                    >
                      <button type="button" className="flex items-center gap-1.5" onClick={() => { setActivePanelId(panel.id); setActiveStepIndex(null); }}>
                        {icon}
                        <span className="max-w-[140px] truncate">{label}</span>
                        {resultStatus && (
                          <span className={cn('text-[10px]', resultStatus.success ? 'text-green-500' : 'text-red-400')}>
                            {resultStatus.success ? '✓' : '✗'}
                          </span>
                        )}
                        {panel.type === 'run' && (panel as WorkflowRunPanel).isExecuting && (
                          <Loader2 className="h-3 w-3 animate-spin text-accent" />
                        )}
                      </button>
                      <button
                        type="button"
                        className="rounded p-0.5 text-fg-muted opacity-0 hover:bg-surface-raised hover:text-fg group-hover:opacity-100"
                        onClick={() => closePanel(panel.id)}
                      >
                        <X className="h-3 w-3" />
                      </button>
                      {isActive && <span className="absolute inset-x-0 bottom-0 h-0.5 bg-blue-500" />}
                    </div>
                  );
                })}
              </div>
            </div>
          )}

          {/* Steps sub-tab bar (when result exists) */}
          {currentResult && (
            <div className="flex shrink-0 items-center gap-1 border-b border-edge bg-surface-alt px-3 py-1 overflow-x-auto">
              <span className={cn('text-xs font-medium mr-2', currentResult.success ? 'text-green-500' : 'text-red-400')}>
                {currentResult.success ? '✓' : '✗'} {currentResult.totalTimeMs}ms
              </span>
              {currentResult.steps.map((step, i) => (
                <button
                  key={step.stepId}
                  type="button"
                  onClick={() => setActiveStepIndex(i)}
                  className={cn(
                    'relative flex items-center gap-1 rounded px-2 py-1 text-[11px] transition-colors whitespace-nowrap',
                    activeStepIndex === i ? 'bg-accent/10 text-accent' : 'text-fg-muted hover:text-fg hover:bg-surface-raised/50',
                  )}
                >
                  <StepStatusIcon status={step.status} />
                  {step.stepId}
                  <span className="text-fg-muted">[{step.stepType}]</span>
                </button>
              ))}
            </div>
          )}

          {/* Variables input + execute button (when run panel active) */}
          {currentWorkflow && (
            <div className="shrink-0 border-b border-edge bg-surface px-4 py-3">
              {currentWorkflow.variables.length > 0 && (
                <div className="flex flex-wrap items-end gap-3 mb-3">
                  {currentWorkflow.variables.map((v) => (
                    <div key={v.name} className="min-w-[160px] max-w-[260px] flex-1">
                      <label className="text-[11px] text-fg-muted block mb-0.5">
                        {v.name}
                        {v.required && <span className="text-red-400">*</span>}
                        {v.description && <span className="ml-1 text-fg-muted/60">({v.description})</span>}
                      </label>
                      {v.type === 'connection' ? (
                        <Select
                          value={variables[v.name] ?? ''}
                          options={savedConnections.map((c) => ({ value: c.id, label: c.name }))}
                          onChange={(val) => setVariables({ ...variables, [v.name]: val })}
                          className="!h-7 !text-xs"
                        />
                      ) : (
                        <input
                          type="text"
                          className="w-full h-7 rounded border border-edge bg-surface-alt px-2 text-xs text-fg outline-none focus:border-accent"
                          value={variables[v.name] ?? ''}
                          onChange={(e) => setVariables({ ...variables, [v.name]: e.target.value })}
                          placeholder={v.description}
                        />
                      )}
                    </div>
                  ))}
                </div>
              )}
              <Button
                variant="primary"
                className="h-8 text-xs"
                onClick={() => void handleExecute()}
                disabled={isExecuting}
              >
                {isExecuting ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <Play className="h-3.5 w-3.5" />}
                {isExecuting ? t('workflows.executing') : t('workflows.execute')}
              </Button>
            </div>
          )}

          {/* Panel content */}
          {activePanel?.type === 'ai-create' ? (
            <div className="relative flex-1 min-h-0">
              <div className="absolute inset-0">
                <WorkflowChatPanel
                  connections={savedConnections}
                  onSaved={() => void loadWorkflows()}
                  onBack={() => closePanel(activePanel.id)}
                />
              </div>
            </div>
          ) : activePanel?.type === 'edit' ? (
            <div className="flex flex-1 min-h-0 overflow-y-auto">
              <WorkflowForm
                draft={(activePanel as EditPanel).draft}
                editingId={(activePanel as EditPanel).editingId}
                connections={savedConnections}
                onDraftChange={(d) => updateEditDraft(activePanel.id, d)}
                onSave={() => void handleSave(activePanel.id, (activePanel as EditPanel).draft)}
                onCancel={() => closePanel(activePanel.id)}
              />
            </div>
          ) : currentStep ? (
            <StepDetailView step={currentStep} t={t} />
          ) : currentResult ? (
            <div className="flex flex-1 items-center justify-center text-xs text-fg-muted">
              {t('workflows.selectStep')}
            </div>
          ) : workflowError ? (
            <div className="flex flex-1 items-center justify-center p-4">
              <p className="text-xs text-red-400">{workflowError}</p>
            </div>
          ) : !currentWorkflow && !activePanel ? (
            <div className="flex flex-1 items-center justify-center text-sm text-fg-muted">
              <div className="text-center">
                <Wand2 className="h-12 w-12 mx-auto mb-3 opacity-20" />
                <p>{t('workflows.emptyHint')}</p>
              </div>
            </div>
          ) : null}
        </div>
      </div>

      <StatusBar />
    </div>
  );
}

// ── Sub-components ──────────────────────────────────────────────────

function StepStatusIcon({ status }: { status: string }) {
  if (status === 'success') return <span className="text-green-500">✓</span>;
  if (status === 'skipped') return <span className="text-yellow-500">⏭</span>;
  if (status === 'timed_out') return <span className="text-yellow-500">⏱</span>;
  return <span className="text-red-400">✗</span>;
}

function extractStepColumnNames(stepResult: Record<string, unknown> | undefined): { name: string; dataType: string }[] {
  if (!stepResult) return [];
  const cols = stepResult.columns as { name: string; dataType?: string }[] | undefined;
  if (Array.isArray(cols) && cols.length > 0) {
    return cols.map((c) => ({ name: c.name, dataType: c.dataType || 'text' }));
  }
  const data = (stepResult.data ?? stepResult.result) as Record<string, unknown>[] | undefined;
  if (Array.isArray(data) && data.length > 0 && typeof data[0] === 'object' && data[0] !== null && !Array.isArray(data[0])) {
    return Object.keys(data[0]).map((k) => ({ name: k, dataType: 'text' }));
  }
  const rows = stepResult.rows as unknown[][] | undefined;
  if (Array.isArray(rows) && rows.length > 0 && Array.isArray(rows[0])) {
    return rows[0].map((_, i) => ({ name: `col_${i + 1}`, dataType: 'text' }));
  }
  return [];
}

function stepToStatementResult(step: StepExecutionResult): StatementResult | null {
  const r = step.result;
  if (!r?.rows) return null;
  const cols = extractStepColumnNames(r);
  if (cols.length === 0) return null;
  const columnInfos: ColumnInfo[] = cols.map((c) => ({
    name: c.name,
    dataType: c.dataType,
    nullable: true,
  }));
  return {
    sql: step.sqlExecuted ?? '',
    columns: columnInfos,
    rows: r.rows as (Value | null)[][],
    executionTimeMs: (r.execution_time_ms as number) ?? step.executionTimeMs,
  };
}

function StepDetailView({ step, t }: { step: StepExecutionResult; t: ReturnType<typeof useI18n>['t'] }) {
  const tableRows = step.result?.rows as unknown[][] | undefined;
  const rowsCount = (step.result?.rows_count ?? tableRows?.length ?? 0) as number;
  const [viewMode, setViewMode] = useState<'table' | 'chart'>('table');
  const [chartConfig, setChartConfig] = useState<ChartConfig | undefined>();

  const colInfos = useMemo(() => extractStepColumnNames(step.result), [step.result]);

  const columns: ColumnDef[] = useMemo(() => {
    if (colInfos.length === 0) return [];
    return colInfos.map((c) => ({ id: c.name, name: c.name, type: c.dataType || 'text' }));
  }, [colInfos]);

  const statementResult = useMemo(() => stepToStatementResult(step), [step]);
  const chartable = useMemo(
    () => statementResult != null && isChartableResult(statementResult),
    [statementResult],
  );
  const hasData = columns.length > 0 && tableRows && tableRows.length > 0;

  return (
    <div className="flex flex-1 flex-col min-h-0">
      <div className="flex items-center gap-3 border-b border-edge bg-surface-alt px-3 py-1.5 text-xs text-fg-secondary">
        <span className="font-medium text-fg">{step.stepId}</span>
        <span className="text-fg-muted">[{step.stepType}]</span>
        <StepStatusIcon status={step.status} />
        {step.connectionName ? <span className="text-accent">{step.connectionName}</span> : null}
        <span className="text-fg-muted">{step.executionTimeMs}ms</span>
        {rowsCount > 0 && <span>{rowsCount} {t('common.rows')}</span>}

        {hasData && chartable && (
          <div className="ml-auto flex items-center gap-0.5 rounded-md bg-surface p-0.5">
            <button
              type="button"
              className={cn(
                'flex items-center gap-1 rounded px-2 py-0.5 text-xs transition-colors',
                viewMode === 'table'
                  ? 'bg-accent/20 text-accent font-medium'
                  : 'text-fg-muted hover:text-fg-secondary',
              )}
              onClick={() => setViewMode('table')}
            >
              <TableProperties className="h-3 w-3" />
              {t('chart.viewTable')}
            </button>
            <button
              type="button"
              className={cn(
                'flex items-center gap-1 rounded px-2 py-0.5 text-xs transition-colors',
                viewMode === 'chart'
                  ? 'bg-accent/20 text-accent font-medium'
                  : 'text-fg-muted hover:text-fg-secondary',
              )}
              onClick={() => setViewMode('chart')}
            >
              <BarChart3 className="h-3 w-3" />
              {t('chart.viewChart')}
            </button>
          </div>
        )}
      </div>

      {step.sqlExecuted ? (
        <div className="border-b border-edge bg-surface px-3 py-2">
          <pre className="text-[11px] font-mono text-fg-secondary whitespace-pre-wrap break-words max-h-20 overflow-auto">
            {step.sqlExecuted}
          </pre>
        </div>
      ) : null}

      {step.error ? (
        <div className="border-b border-edge bg-red-500/5 px-3 py-2 text-xs text-red-400">{step.error}</div>
      ) : null}

      {step.stepType === 'ai' && step.result?.result != null ? (
        <div className="border-b border-edge px-3 py-2">
          <pre className="text-xs text-fg-secondary whitespace-pre-wrap break-words max-h-60 overflow-auto">{String(step.result.result)}</pre>
        </div>
      ) : null}

      {viewMode === 'chart' && statementResult ? (
        <>
          {statementResult.rows.length > 1000 && (
            <div className="flex items-center gap-1 border-b border-edge bg-surface-alt px-3 py-1 text-[11px] text-yellow-400">
              <AlertTriangle className="h-3 w-3" />
              {t('chart.sampledWarning', { limit: '1000' })}
            </div>
          )}
          <div className="flex flex-1 min-h-0">
            <ChartView
              result={statementResult}
              savedConfig={chartConfig}
              onConfigChange={setChartConfig}
              onDataPointClick={() => setViewMode('table')}
            />
          </div>
        </>
      ) : hasData ? (
        <div className="flex flex-1 min-h-0">
          <DataTable columns={columns} rows={tableRows} rowHeight={32} />
        </div>
      ) : step.stepType === 'query' && !step.error ? (
        <div className="flex flex-1 items-center justify-center text-xs text-fg-muted">{t('workflows.noQueryResult')}</div>
      ) : null}
    </div>
  );
}

function WorkflowSidebarList({
  workflows, loading, activePanelWorkflowId, onSelect, onEdit, onDelete, t,
}: {
  workflows: WorkflowListItem[];
  loading: boolean;
  activePanelWorkflowId?: string;
  onSelect: (w: WorkflowListItem) => void;
  onEdit: (id: string) => void;
  onDelete: (id: string) => void;
  t: ReturnType<typeof useI18n>['t'];
}) {
  if (loading) {
    return (
      <div className="flex items-center justify-center py-8 text-fg-muted text-xs">
        <Loader2 className="h-4 w-4 animate-spin mr-1" />
        {t('workflows.loading')}
      </div>
    );
  }

  return (
    <div className="p-1.5 space-y-0.5">
      {workflows.map((w) => (
        <div
          key={w.id}
          className={cn(
            'group flex items-center gap-2 rounded-md px-3 py-2 cursor-pointer transition-colors',
            activePanelWorkflowId === w.id ? 'bg-accent/10 text-accent' : 'text-fg-secondary hover:bg-surface-raised/50 hover:text-fg',
          )}
          onClick={() => onSelect(w)}
        >
          <Wand2 className="h-3.5 w-3.5 shrink-0" />
          <div className="flex-1 min-w-0">
            <div className="text-xs font-medium truncate">{w.name}</div>
            {w.description && <div className="text-[10px] text-fg-muted truncate">{w.description}</div>}
          </div>
          <div className="hidden group-hover:flex items-center gap-0.5">
            <button type="button" onClick={(e) => { e.stopPropagation(); onEdit(w.id); }} className="p-0.5 rounded hover:bg-surface-alt" title={t('workflows.edit')}>
              <Pencil className="h-3 w-3" />
            </button>
            <button type="button" onClick={(e) => { e.stopPropagation(); onDelete(w.id); }} className="p-0.5 rounded hover:bg-surface-alt text-red-400" title={t('workflows.delete')}>
              <Trash2 className="h-3 w-3" />
            </button>
          </div>
        </div>
      ))}
      {workflows.length === 0 && (
        <div className="py-6 text-center text-xs text-fg-muted">{t('workflows.noWorkflows')}</div>
      )}
    </div>
  );
}

function HistoryList({
  items, onView, onClear, t,
}: {
  items: HistoryListItem[];
  onView: (id: string, workflowName: string) => void;
  onClear: () => void;
  t: ReturnType<typeof useI18n>['t'];
}) {
  return (
    <div className="p-1.5">
      {items.length > 0 && (
        <div className="flex justify-end mb-1">
          <button type="button" onClick={onClear} className="text-[10px] text-fg-muted hover:text-red-400">{t('workflows.history.clear')}</button>
        </div>
      )}
      {items.map((item) => (
        <button key={item.id} type="button" onClick={() => onView(item.id, item.workflowName)}
          className="flex w-full items-center gap-2 rounded-md px-3 py-2 text-left hover:bg-surface-raised/50 transition-colors">
          <Clock className="h-3.5 w-3.5 text-fg-muted shrink-0" />
          <div className="flex-1 min-w-0">
            <div className="text-xs font-medium text-fg truncate">{item.workflowName}</div>
            <div className="text-[10px] text-fg-muted">{new Date(item.createdAt).toLocaleString()}</div>
          </div>
          <span className={cn('text-[10px]', item.success ? 'text-green-500' : 'text-red-400')}>{item.success ? '✓' : '✗'}</span>
        </button>
      ))}
      {items.length === 0 && (
        <div className="py-6 text-center text-xs text-fg-muted">{t('workflows.history.empty')}</div>
      )}
    </div>
  );
}
