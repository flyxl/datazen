import { useCallback, useEffect, useMemo, useState } from 'react';
import {
  Clock,
  FolderOpen,
  History,
  Loader2,
  Pencil,
  Play,
  Plus,
  RefreshCw,
  Trash2,
  Wand2,
  X,
} from 'lucide-react';
import { TitleBar } from '../../components/TitleBar';
import { ThemeToggle } from '../../components/ThemeToggle';
import { StatusBar } from '../../components/StatusBar';
import { DataTable } from '../../components/DataTable/DataTable';
import type { ColumnDef } from '../../components/DataTable/TableHeader';
import { Button } from '../../components/ui/Button';
import { Select } from '../../components/ui/Select';
import { useThemeListener } from '../../hooks/useThemeListener';
import { useI18n } from '../../hooks/useI18n';
import { useAiStore } from '../../stores/aiStore';
import { aiCommands } from '../../commands/ai';
import { connectionCommands } from '../../commands/connection';
import { cn } from '../../lib/cn';
import type {
  HistoryListItem,
  StepExecutionResult,
  WorkflowDefinition,
  WorkflowExecutionResult,
  WorkflowListItem,
  WorkflowStepType,
} from '../../types';

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

type Panel = WorkflowRunPanel | HistoryDetailPanel;

let panelCounter = 0;
function nextPanelId(prefix: string) {
  panelCounter += 1;
  return `${prefix}-${panelCounter}`;
}

// ── Draft types ─────────────────────────────────────────────────────

interface WorkflowStepDraft {
  type: WorkflowStepType;
  id: string;
  sql?: string;
  prompt?: string;
  connection?: string;
  database?: string;
}

function emptyDraft() {
  return {
    id: '',
    name: '',
    description: '',
    variables: [] as { name: string; varType: string; description: string; required: boolean }[],
    steps: [{ type: 'query' as WorkflowStepType, id: 'step1', sql: '' }] as WorkflowStepDraft[],
  };
}

// ── Main Component ──────────────────────────────────────────────────

export function WorkflowWindow() {
  useThemeListener();
  const { t } = useI18n();

  const workflows = useAiStore((s) => s.workflows);
  const workflowsLoading = useAiStore((s) => s.workflowsLoading);
  const loadWorkflows = useAiStore((s) => s.loadWorkflows);
  const executeWorkflow = useAiStore((s) => s.executeWorkflow);
  const workflowError = useAiStore((s) => s.workflowError);
  const clearWorkflowResult = useAiStore((s) => s.clearWorkflowResult);

  const [panels, setPanels] = useState<Panel[]>([]);
  const [activePanelId, setActivePanelId] = useState<string | null>(null);
  const [activeStepIndex, setActiveStepIndex] = useState<number | null>(null);

  const [workflowsDir, setWorkflowsDir] = useState('');
  const [showForm, setShowForm] = useState(false);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [draft, setDraft] = useState(emptyDraft());
  const [feedback, setFeedback] = useState('');
  const [savedConnections, setSavedConnections] = useState<{ id: string; name: string; databaseType: string }[]>([]);
  const [sideTab, setSideTab] = useState<'workflows' | 'history'>('workflows');
  const [historyItems, setHistoryItems] = useState<HistoryListItem[]>([]);

  useEffect(() => {
    void loadWorkflows();
    void aiCommands.workflowGetDir().then(setWorkflowsDir);
    void connectionCommands.getConnections().then((conns) =>
      setSavedConnections(conns.map((c) => ({ id: c.id, name: c.name, databaseType: c.databaseType }))),
    );
  }, [loadWorkflows]);

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
    try {
      const { open } = await import('@tauri-apps/plugin-shell');
      await open(workflowsDir);
    } catch { /* browser mode */ }
  }, [workflowsDir]);

  const handleEdit = async (workflowId: string) => {
    try {
      const workflow: WorkflowDefinition = await aiCommands.workflowGet(workflowId);
      setDraft({
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
      setEditingId(workflowId);
      setShowForm(true);
    } catch (e) { setFeedback(String(e)); }
  };

  const handleCreate = () => { setDraft(emptyDraft()); setEditingId(null); setShowForm(true); };

  const handleSave = async () => {
    const workflow: WorkflowDefinition = {
      id: draft.id.trim(), name: draft.name.trim(), description: draft.description.trim(),
      variables: draft.variables.map((v) => ({ name: v.name, type: v.varType, description: v.description, required: v.required })),
      steps: draft.steps.map((s) => ({ type: s.type, id: s.id, sql: s.sql, prompt: s.prompt, connection: s.connection, database: s.database })),
    };
    try {
      await aiCommands.workflowSave(workflow);
      setShowForm(false);
      setFeedback(t('workflows.saved'));
      setTimeout(() => setFeedback(''), 2000);
      void loadWorkflows();
    } catch (e) { setFeedback(String(e)); }
  };

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
      <TitleBar title={t('win.workflow')} rightContent={<ThemeToggle />} />

      {/* Toolbar */}
      <div className="flex h-10 shrink-0 items-center gap-2 border-b border-edge bg-surface-alt px-4">
        <Button variant="secondary" className="h-7 w-7 !px-0" title={t('workflows.reload')} onClick={() => void handleReload()}>
          <RefreshCw className="h-3.5 w-3.5" />
        </Button>
        <Button variant="primary" className="h-7 text-xs" onClick={handleCreate}>
          <Plus className="h-3.5 w-3.5" />
          {t('workflows.create')}
        </Button>
        {workflowsDir && (
          <Button variant="secondary" className="h-7 w-7 !px-0" title={t('workflows.openDir')} onClick={() => void handleOpenDir()}>
            <FolderOpen className="h-3.5 w-3.5" />
          </Button>
        )}

        <div className="flex-1" />

        {/* Execute button (when run panel active) */}
        {currentWorkflow && (
          <Button
            variant="primary"
            className="h-7 text-xs"
            onClick={() => void handleExecute()}
            disabled={isExecuting}
          >
            {isExecuting ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <Play className="h-3.5 w-3.5" />}
            {isExecuting ? t('workflows.executing') : t('workflows.execute')}
          </Button>
        )}
      </div>

      <div className="flex min-h-0 flex-1">
        {/* Left sidebar: workflow list / history */}
        <div className="flex w-64 shrink-0 flex-col border-r border-edge bg-surface-alt">
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
            ) : showForm ? (
              <WorkflowForm draft={draft} editingId={editingId} connections={savedConnections} onDraftChange={setDraft} onSave={() => void handleSave()} onCancel={() => setShowForm(false)} t={t} />
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

          {/* Variables input (when run panel active) */}
          {currentWorkflow && currentWorkflow.variables.length > 0 && (
            <div className="border-t border-edge p-3 space-y-1.5">
              <div className="text-[11px] text-fg-muted font-medium">{t('workflows.form.variables')}</div>
              {currentWorkflow.variables.map((v) => (
                <div key={v.name}>
                  <label className="text-[10px] text-fg-muted">{v.name}{v.required && <span className="text-red-400">*</span>}</label>
                  {v.type === 'connection' ? (
                    <Select
                      value={variables[v.name] ?? ''}
                      options={savedConnections.map((c) => ({ value: c.id, label: c.name }))}
                      onChange={(val) => setVariables({ ...variables, [v.name]: val })}
                      className="!h-6 !text-[11px]"
                    />
                  ) : (
                    <input
                      type="text"
                      className="w-full h-6 rounded border border-edge bg-surface px-2 text-[11px] text-fg outline-none focus:border-accent"
                      value={variables[v.name] ?? ''}
                      onChange={(e) => setVariables({ ...variables, [v.name]: e.target.value })}
                      placeholder={v.description}
                    />
                  )}
                </div>
              ))}
            </div>
          )}
        </div>

        {/* Main content: tab bar + result */}
        <div className="flex min-h-0 min-w-0 flex-1 flex-col">
          {/* Tab bar */}
          {panels.length > 0 && (
            <div className="flex shrink-0 items-center border-b border-edge bg-surface-alt">
              <div className="flex min-w-0 flex-1 overflow-x-auto">
                {panels.map((panel) => {
                  const isActive = panel.id === activePanelId;
                  const icon = panel.type === 'run'
                    ? <Wand2 className="h-3.5 w-3.5 shrink-0" />
                    : <History className="h-3.5 w-3.5 shrink-0" />;
                  const label = panel.type === 'run'
                    ? (panel as WorkflowRunPanel).workflowName
                    : `${(panel as HistoryDetailPanel).workflowName}`;
                  const resultStatus = panel.type === 'run'
                    ? (panel as WorkflowRunPanel).result
                    : (panel as HistoryDetailPanel).result;

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

          {/* Panel content */}
          {currentStep ? (
            <StepDetailView step={currentStep} t={t} />
          ) : currentResult ? (
            <div className="flex flex-1 items-center justify-center text-xs text-fg-muted">
              {t('workflows.selectStep')}
            </div>
          ) : workflowError ? (
            <div className="flex flex-1 items-center justify-center p-4">
              <p className="text-xs text-red-400">{workflowError}</p>
            </div>
          ) : (
            <div className="flex flex-1 items-center justify-center text-sm text-fg-muted">
              <div className="text-center">
                <Wand2 className="h-12 w-12 mx-auto mb-3 opacity-20" />
                <p>{t('workflows.emptyHint')}</p>
              </div>
            </div>
          )}
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

function StepDetailView({ step, t }: { step: StepExecutionResult; t: ReturnType<typeof useI18n>['t'] }) {
  const rows = step.result?.rows as Record<string, unknown>[] | undefined;
  const rowsCount = (step.result?.rows_count ?? rows?.length ?? 0) as number;

  const { columns, tableRows } = useMemo(() => {
    if (!rows || rows.length === 0) return { columns: [] as ColumnDef[], tableRows: [] as unknown[][] };
    const keys = Object.keys(rows[0]);
    const cols: ColumnDef[] = keys.map((k) => ({ id: k, name: k, type: 'text' }));
    const tRows = rows.map((r) => keys.map((k) => r[k] ?? null));
    return { columns: cols, tableRows: tRows };
  }, [rows]);

  return (
    <div className="flex flex-1 flex-col min-h-0">
      <div className="flex items-center gap-3 border-b border-edge bg-surface-alt px-3 py-1.5 text-xs text-fg-secondary">
        <span className="font-medium text-fg">{step.stepId}</span>
        <span className="text-fg-muted">[{step.stepType}]</span>
        <StepStatusIcon status={step.status} />
        {step.connectionName ? <span className="text-accent">{step.connectionName}</span> : null}
        <span className="text-fg-muted">{step.executionTimeMs}ms</span>
        {rowsCount > 0 && <span>{rowsCount} {t('common.rows')}</span>}
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

      {step.stepType === 'ai' && step.result?.response ? (
        <div className="border-b border-edge px-3 py-2">
          <pre className="text-xs text-fg-secondary whitespace-pre-wrap break-words max-h-60 overflow-auto">{String(step.result.response)}</pre>
        </div>
      ) : null}

      {columns.length > 0 ? (
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

function WorkflowForm({
  draft, editingId, connections, onDraftChange, onSave, onCancel, t,
}: {
  draft: ReturnType<typeof emptyDraft>;
  editingId: string | null;
  connections: { id: string; name: string; databaseType: string }[];
  onDraftChange: (d: ReturnType<typeof emptyDraft>) => void;
  onSave: () => void;
  onCancel: () => void;
  t: ReturnType<typeof useI18n>['t'];
}) {
  const inputClass = 'w-full h-7 rounded border border-edge bg-surface px-2 text-xs text-fg outline-none focus:border-accent';
  const textareaClass = 'w-full rounded border border-edge bg-surface px-2 py-1 text-xs font-mono text-fg outline-none focus:border-accent resize-y min-h-[60px]';

  return (
    <div className="p-3 space-y-3">
      <div className="flex items-center justify-between">
        <span className="text-xs font-medium text-fg">{editingId ? t('workflows.edit') : t('workflows.create')}</span>
        <button type="button" onClick={onCancel} className="text-fg-muted hover:text-fg"><X className="h-3.5 w-3.5" /></button>
      </div>
      <div>
        <label className="text-[11px] text-fg-muted">ID</label>
        <input className={inputClass} value={draft.id} onChange={(e) => onDraftChange({ ...draft, id: e.target.value })} disabled={!!editingId} />
      </div>
      <div>
        <label className="text-[11px] text-fg-muted">{t('workflows.name')}</label>
        <input className={inputClass} value={draft.name} onChange={(e) => onDraftChange({ ...draft, name: e.target.value })} />
      </div>
      <div>
        <label className="text-[11px] text-fg-muted">{t('workflows.description')}</label>
        <input className={inputClass} value={draft.description} onChange={(e) => onDraftChange({ ...draft, description: e.target.value })} />
      </div>
      <div>
        <div className="flex items-center justify-between mb-1">
          <label className="text-[11px] text-fg-muted">{t('workflows.steps')}</label>
          <button type="button" onClick={() => onDraftChange({ ...draft, steps: [...draft.steps, { type: 'query', id: `step${draft.steps.length + 1}`, sql: '' }] })} className="text-accent text-[10px] hover:underline">
            + {t('workflows.addStep')}
          </button>
        </div>
        {draft.steps.map((step, i) => (
          <div key={i} className="mb-2 rounded border border-edge p-2 space-y-1">
            <div className="flex items-center gap-1">
              <input className="h-6 w-20 rounded border border-edge bg-surface px-1 text-[11px] text-fg outline-none" value={step.id}
                onChange={(e) => { const s = [...draft.steps]; s[i] = { ...s[i], id: e.target.value }; onDraftChange({ ...draft, steps: s }); }} placeholder="step_id" />
              <Select value={step.type} options={[{ value: 'query', label: 'Query' }, { value: 'ai', label: 'AI' }]}
                onChange={(v) => { const s = [...draft.steps]; s[i] = { ...s[i], type: v as WorkflowStepType }; onDraftChange({ ...draft, steps: s }); }} className="!h-6 !text-[11px] w-20" />
              {connections.length > 0 && (
                <Select value={step.connection ?? ''} options={[{ value: '', label: t('workflows.defaultConn') }, ...connections.map((c) => ({ value: c.id, label: c.name }))]}
                  onChange={(v) => { const s = [...draft.steps]; s[i] = { ...s[i], connection: v || undefined }; onDraftChange({ ...draft, steps: s }); }} className="!h-6 !text-[11px] flex-1" />
              )}
              {draft.steps.length > 1 && (
                <button type="button" onClick={() => onDraftChange({ ...draft, steps: draft.steps.filter((_, j) => j !== i) })} className="text-red-400 hover:text-red-300 p-0.5">
                  <X className="h-3 w-3" />
                </button>
              )}
            </div>
            {step.type === 'query' && (
              <textarea className={textareaClass} value={step.sql ?? ''} onChange={(e) => { const s = [...draft.steps]; s[i] = { ...s[i], sql: e.target.value }; onDraftChange({ ...draft, steps: s }); }} placeholder="SELECT ..." rows={3} />
            )}
            {step.type === 'ai' && (
              <textarea className={textareaClass} value={step.prompt ?? ''} onChange={(e) => { const s = [...draft.steps]; s[i] = { ...s[i], prompt: e.target.value }; onDraftChange({ ...draft, steps: s }); }} placeholder="AI prompt..." rows={3} />
            )}
          </div>
        ))}
      </div>
      <div className="flex gap-2">
        <Button onClick={onSave} className="flex-1">{t('common.save')}</Button>
        <Button variant="secondary" onClick={onCancel} className="flex-1">{t('common.cancel')}</Button>
      </div>
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
