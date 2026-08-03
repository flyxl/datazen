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

interface WorkflowStepDraft {
  type: WorkflowStepType;
  id: string;
  sql?: string;
  prompt?: string;
  connection?: string;
  database?: string;
}

interface WorkflowVariableDraft {
  name: string;
  varType: string;
  description: string;
  required: boolean;
}

function emptyDraft() {
  return {
    id: '',
    name: '',
    description: '',
    variables: [] as WorkflowVariableDraft[],
    steps: [{ type: 'query' as WorkflowStepType, id: 'step1', sql: '' }] as WorkflowStepDraft[],
  };
}

export function WorkflowWindow() {
  useThemeListener();
  const { t } = useI18n();

  const workflows = useAiStore((s) => s.workflows);
  const workflowsLoading = useAiStore((s) => s.workflowsLoading);
  const loadWorkflows = useAiStore((s) => s.loadWorkflows);
  const executeWorkflow = useAiStore((s) => s.executeWorkflow);
  const result = useAiStore((s) => s.workflowExecutionResult);
  const isExecuting = useAiStore((s) => s.isExecutingWorkflow);
  const workflowError = useAiStore((s) => s.workflowError);
  const clearWorkflowResult = useAiStore((s) => s.clearWorkflowResult);

  const [selectedWorkflow, setSelectedWorkflow] = useState<WorkflowListItem | null>(null);
  const [variables, setVariables] = useState<Record<string, string>>({});
  const [workflowsDir, setWorkflowsDir] = useState('');
  const [showForm, setShowForm] = useState(false);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [draft, setDraft] = useState(emptyDraft());
  const [feedback, setFeedback] = useState('');
  const [savedConnections, setSavedConnections] = useState<{ id: string; name: string; databaseType: string }[]>([]);
  const [activeStepIndex, setActiveStepIndex] = useState<number | null>(null);
  const [tab, setTab] = useState<'workflows' | 'history'>('workflows');
  const [historyItems, setHistoryItems] = useState<HistoryListItem[]>([]);
  const [historyDetail, setHistoryDetail] = useState<WorkflowExecutionResult | null>(null);

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
    if (tab === 'history') void loadHistory();
  }, [tab, loadHistory]);

  const handleSelect = (workflow: WorkflowListItem) => {
    setSelectedWorkflow(workflow);
    clearWorkflowResult();
    setActiveStepIndex(null);
    const defaults: Record<string, string> = {};
    for (const v of workflow.variables) {
      defaults[v.name] = v.default != null ? String(v.default) : '';
    }
    setVariables(defaults);
  };

  const handleExecute = async () => {
    if (!selectedWorkflow) return;
    setActiveStepIndex(null);
    await executeWorkflow({ workflowId: selectedWorkflow.id, variables });
    void loadHistory();
  };

  const handleDelete = async (workflowId: string) => {
    if (!confirm(t('workflows.deleteConfirm'))) return;
    await aiCommands.workflowDelete(workflowId);
    if (selectedWorkflow?.id === workflowId) {
      setSelectedWorkflow(null);
      clearWorkflowResult();
    }
    void loadWorkflows();
  };

  const handleReload = useCallback(async () => {
    await aiCommands.workflowReload();
    void loadWorkflows();
  }, [loadWorkflows]);

  const handleOpenDir = useCallback(async () => {
    try {
      const { invoke } = await import('@tauri-apps/api/core');
      await invoke('open_in_explorer', { path: workflowsDir });
    } catch {
      /* browser mode — ignore */
    }
  }, [workflowsDir]);

  const handleEdit = async (workflowId: string) => {
    try {
      const workflow: WorkflowDefinition = await aiCommands.workflowGet(workflowId);
      setDraft({
        id: workflow.id,
        name: workflow.name,
        description: workflow.description,
        variables: workflow.variables.map((v) => ({
          name: v.name,
          varType: v.type || 'string',
          description: v.description,
          required: v.required ?? false,
        })),
        steps: workflow.steps.map((s) => ({
          type: s.type as WorkflowStepType,
          id: s.id,
          sql: s.sql,
          prompt: s.prompt,
          connection: s.connection,
          database: s.database,
        })),
      });
      setEditingId(workflowId);
      setShowForm(true);
    } catch (e) {
      setFeedback(String(e));
    }
  };

  const handleCreate = () => {
    setDraft(emptyDraft());
    setEditingId(null);
    setShowForm(true);
  };

  const handleSave = async () => {
    const workflow: WorkflowDefinition = {
      id: draft.id.trim(),
      name: draft.name.trim(),
      description: draft.description.trim(),
      variables: draft.variables.map((v) => ({
        name: v.name,
        type: v.varType,
        description: v.description,
        required: v.required,
      })),
      steps: draft.steps.map((s) => ({
        type: s.type,
        id: s.id,
        sql: s.sql,
        prompt: s.prompt,
        connection: s.connection,
        database: s.database,
      })),
    };
    try {
      await aiCommands.workflowSave(workflow);
      setShowForm(false);
      setFeedback(t('workflows.saved'));
      setTimeout(() => setFeedback(''), 2000);
      void loadWorkflows();
    } catch (e) {
      setFeedback(String(e));
    }
  };

  const handleViewHistory = async (historyId: string) => {
    try {
      const entry = await aiCommands.workflowHistoryGet(historyId);
      setHistoryDetail(entry.result);
      setActiveStepIndex(null);
    } catch (e) {
      setFeedback(String(e));
    }
  };

  const handleClearHistory = async () => {
    if (!confirm(t('workflows.history.clearConfirm'))) return;
    await aiCommands.workflowHistoryClear();
    setHistoryItems([]);
    setHistoryDetail(null);
  };

  const displayResult = historyDetail ?? result;
  const activeStep = displayResult && activeStepIndex != null ? displayResult.steps[activeStepIndex] : null;

  return (
    <div className="flex h-screen flex-col bg-surface text-fg">
      <TitleBar title={t('win.workflow')} />

      <div className="flex flex-1 min-h-0">
        {/* Left panel: workflow list + config */}
        <div className="flex w-80 shrink-0 flex-col border-r border-edge bg-surface-alt">
          {/* Tabs */}
          <div className="flex items-center gap-1 border-b border-edge px-3 py-2">
            <button
              type="button"
              onClick={() => { setTab('workflows'); setHistoryDetail(null); }}
              className={cn('px-2 py-0.5 text-xs rounded transition-colors', tab === 'workflows' ? 'bg-accent/10 text-accent' : 'text-fg-muted hover:text-fg')}
            >
              <Wand2 className="inline h-3 w-3 mr-1" />
              Workflows
            </button>
            <button
              type="button"
              onClick={() => setTab('history')}
              className={cn('flex items-center gap-1 px-2 py-0.5 text-xs rounded transition-colors', tab === 'history' ? 'bg-accent/10 text-accent' : 'text-fg-muted hover:text-fg')}
            >
              <History className="h-3 w-3" />
              {t('workflows.history.title')}
            </button>
            <div className="flex-1" />
            <button type="button" onClick={handleReload} className="text-fg-muted hover:text-fg p-1 rounded" title={t('workflows.reload')}>
              <RefreshCw className="h-3.5 w-3.5" />
            </button>
            {workflowsDir && (
              <button type="button" onClick={() => void handleOpenDir()} className="text-fg-muted hover:text-fg p-1 rounded" title={t('workflows.openDir')}>
                <FolderOpen className="h-3.5 w-3.5" />
              </button>
            )}
          </div>

          {feedback && (
            <p className={cn('px-3 py-1 text-xs', feedback.startsWith('Error') ? 'text-red-400' : 'text-green-500')}>{feedback}</p>
          )}

          <div className="flex-1 overflow-y-auto">
            {tab === 'history' ? (
              <HistoryList
                items={historyItems}
                onView={handleViewHistory}
                onClear={() => void handleClearHistory()}
                t={t}
              />
            ) : showForm ? (
              <WorkflowForm
                draft={draft}
                editingId={editingId}
                connections={savedConnections}
                onDraftChange={setDraft}
                onSave={() => void handleSave()}
                onCancel={() => setShowForm(false)}
                t={t}
              />
            ) : (
              <WorkflowList
                workflows={workflows}
                loading={workflowsLoading}
                selectedId={selectedWorkflow?.id}
                onSelect={handleSelect}
                onEdit={(id) => void handleEdit(id)}
                onDelete={(id) => void handleDelete(id)}
                onCreate={handleCreate}
                t={t}
              />
            )}
          </div>

          {/* Variables + Execute (when workflow selected) */}
          {tab === 'workflows' && !showForm && selectedWorkflow && (
            <div className="border-t border-edge p-3 space-y-2">
              <div className="text-xs font-medium text-fg-secondary">{selectedWorkflow.name}</div>
              {selectedWorkflow.description && (
                <div className="text-[11px] text-fg-muted">{selectedWorkflow.description}</div>
              )}
              {selectedWorkflow.variables.length > 0 && (
                <div className="space-y-1.5">
                  {selectedWorkflow.variables.map((v) => (
                    <div key={v.name}>
                      <label className="text-[11px] text-fg-muted">
                        {v.name} {v.required && <span className="text-red-400">*</span>}
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
                          className="w-full h-7 rounded border border-edge bg-surface px-2 text-xs text-fg outline-none focus:border-accent"
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
                onClick={() => void handleExecute()}
                disabled={isExecuting}
                className="w-full"
              >
                {isExecuting ? (
                  <Loader2 className="h-3.5 w-3.5 animate-spin mr-1" />
                ) : (
                  <Play className="h-3.5 w-3.5 mr-1" />
                )}
                {isExecuting ? t('workflows.executing') : t('workflows.execute')}
              </Button>
              {workflowError && (
                <p className="text-xs text-red-400">{workflowError}</p>
              )}
            </div>
          )}
        </div>

        {/* Right panel: result display */}
        <div className="flex flex-1 flex-col min-h-0">
          {displayResult ? (
            <>
              {/* Steps header */}
              <div className="flex items-center gap-1 border-b border-edge bg-surface-alt px-3 py-1.5 overflow-x-auto">
                <span className={cn('text-xs font-medium mr-2', displayResult.success ? 'text-green-500' : 'text-red-400')}>
                  {displayResult.success ? '✓' : '✗'} {displayResult.totalTimeMs}ms
                </span>
                {displayResult.steps.map((step, i) => (
                  <button
                    key={step.stepId}
                    type="button"
                    onClick={() => setActiveStepIndex(i)}
                    className={cn(
                      'flex items-center gap-1 rounded px-2 py-0.5 text-[11px] transition-colors whitespace-nowrap',
                      activeStepIndex === i ? 'bg-accent/10 text-accent' : 'text-fg-muted hover:text-fg hover:bg-surface-raised/50',
                    )}
                  >
                    <StepStatusIcon status={step.status} />
                    {step.stepId}
                    <span className="text-fg-muted">[{step.stepType}]</span>
                  </button>
                ))}
                {historyDetail && (
                  <button
                    type="button"
                    onClick={() => setHistoryDetail(null)}
                    className="ml-auto flex items-center gap-1 text-xs text-fg-muted hover:text-fg"
                  >
                    <X className="h-3 w-3" />
                    {t('workflows.history.back')}
                  </button>
                )}
              </div>

              {/* Step detail / DataTable */}
              {activeStep ? (
                <StepDetailView step={activeStep} t={t} />
              ) : (
                <div className="flex flex-1 items-center justify-center text-xs text-fg-muted">
                  {t('workflows.selectStep')}
                </div>
              )}
            </>
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
      {/* Step meta bar */}
      <div className="flex items-center gap-3 border-b border-edge bg-surface-alt px-3 py-1.5 text-xs text-fg-secondary">
        <span className="font-medium text-fg">{step.stepId}</span>
        <span className="text-fg-muted">[{step.stepType}]</span>
        <StepStatusIcon status={step.status} />
        {step.connectionName && <span className="text-accent">{step.connectionName}</span>}
        <span className="text-fg-muted">{step.executionTimeMs}ms</span>
        {rowsCount > 0 && (
          <span>{rowsCount} {t('common.rows')}</span>
        )}
      </div>

      {/* SQL display */}
      {step.sqlExecuted ? (
        <div className="border-b border-edge bg-surface px-3 py-2">
          <pre className="text-[11px] font-mono text-fg-secondary whitespace-pre-wrap break-words max-h-20 overflow-auto">
            {step.sqlExecuted}
          </pre>
        </div>
      ) : null}

      {/* Error display */}
      {step.error ? (
        <div className="border-b border-edge bg-red-500/5 px-3 py-2 text-xs text-red-400">
          {step.error}
        </div>
      ) : null}

      {/* AI result display */}
      {step.stepType === 'ai' && step.result?.response ? (
        <div className="border-b border-edge px-3 py-2">
          <pre className="text-xs text-fg-secondary whitespace-pre-wrap break-words max-h-60 overflow-auto">
            {String(step.result.response)}
          </pre>
        </div>
      ) : null}

      {/* DataTable for query results */}
      {columns.length > 0 ? (
        <div className="flex flex-1 min-h-0">
          <DataTable
            columns={columns}
            rows={tableRows}
            rowHeight={32}
          />
        </div>
      ) : step.stepType === 'query' && !step.error ? (
        <div className="flex flex-1 items-center justify-center text-xs text-fg-muted">
          {t('workflows.noQueryResult')}
        </div>
      ) : null}
    </div>
  );
}

function WorkflowList({
  workflows,
  loading,
  selectedId,
  onSelect,
  onEdit,
  onDelete,
  onCreate,
  t,
}: {
  workflows: WorkflowListItem[];
  loading: boolean;
  selectedId?: string;
  onSelect: (w: WorkflowListItem) => void;
  onEdit: (id: string) => void;
  onDelete: (id: string) => void;
  onCreate: () => void;
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
    <div className="p-2 space-y-0.5">
      <button
        type="button"
        onClick={onCreate}
        className="flex w-full items-center gap-2 rounded-md px-3 py-2 text-xs text-accent hover:bg-surface-raised/50 transition-colors"
      >
        <Plus className="h-3.5 w-3.5" />
        {t('workflows.create')}
      </button>
      {workflows.map((w) => (
        <div
          key={w.id}
          className={cn(
            'group flex items-center gap-2 rounded-md px-3 py-2 cursor-pointer transition-colors',
            selectedId === w.id ? 'bg-accent/10 text-accent' : 'text-fg-secondary hover:bg-surface-raised/50 hover:text-fg',
          )}
          onClick={() => onSelect(w)}
        >
          <Wand2 className="h-3.5 w-3.5 shrink-0" />
          <div className="flex-1 min-w-0">
            <div className="text-xs font-medium truncate">{w.name}</div>
            {w.description && <div className="text-[10px] text-fg-muted truncate">{w.description}</div>}
          </div>
          <div className="hidden group-hover:flex items-center gap-0.5">
            <button type="button" onClick={(e) => { e.stopPropagation(); onEdit(w.id); }} className="p-0.5 rounded hover:bg-surface-alt" title="Edit">
              <Pencil className="h-3 w-3" />
            </button>
            <button type="button" onClick={(e) => { e.stopPropagation(); onDelete(w.id); }} className="p-0.5 rounded hover:bg-surface-alt text-red-400" title="Delete">
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
  draft,
  editingId,
  connections,
  onDraftChange,
  onSave,
  onCancel,
  t,
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
        <button type="button" onClick={onCancel} className="text-fg-muted hover:text-fg">
          <X className="h-3.5 w-3.5" />
        </button>
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

      {/* Steps */}
      <div>
        <div className="flex items-center justify-between mb-1">
          <label className="text-[11px] text-fg-muted">{t('workflows.steps')}</label>
          <button
            type="button"
            onClick={() => onDraftChange({ ...draft, steps: [...draft.steps, { type: 'query', id: `step${draft.steps.length + 1}`, sql: '' }] })}
            className="text-accent text-[10px] hover:underline"
          >
            + {t('workflows.addStep')}
          </button>
        </div>
        {draft.steps.map((step, i) => (
          <div key={i} className="mb-2 rounded border border-edge p-2 space-y-1">
            <div className="flex items-center gap-1">
              <input
                className="h-6 w-20 rounded border border-edge bg-surface px-1 text-[11px] text-fg outline-none"
                value={step.id}
                onChange={(e) => {
                  const next = [...draft.steps];
                  next[i] = { ...next[i], id: e.target.value };
                  onDraftChange({ ...draft, steps: next });
                }}
                placeholder="step_id"
              />
              <Select
                value={step.type}
                options={[
                  { value: 'query', label: 'Query' },
                  { value: 'ai', label: 'AI' },
                ]}
                onChange={(v) => {
                  const next = [...draft.steps];
                  next[i] = { ...next[i], type: v as WorkflowStepType };
                  onDraftChange({ ...draft, steps: next });
                }}
                className="!h-6 !text-[11px] w-20"
              />
              {connections.length > 0 && (
                <Select
                  value={step.connection ?? ''}
                  options={[{ value: '', label: t('workflows.defaultConn') }, ...connections.map((c) => ({ value: c.id, label: c.name }))]}
                  onChange={(v) => {
                    const next = [...draft.steps];
                    next[i] = { ...next[i], connection: v || undefined };
                    onDraftChange({ ...draft, steps: next });
                  }}
                  className="!h-6 !text-[11px] flex-1"
                />
              )}
              {draft.steps.length > 1 && (
                <button
                  type="button"
                  onClick={() => onDraftChange({ ...draft, steps: draft.steps.filter((_, j) => j !== i) })}
                  className="text-red-400 hover:text-red-300 p-0.5"
                >
                  <X className="h-3 w-3" />
                </button>
              )}
            </div>
            {step.type === 'query' && (
              <textarea
                className={textareaClass}
                value={step.sql ?? ''}
                onChange={(e) => {
                  const next = [...draft.steps];
                  next[i] = { ...next[i], sql: e.target.value };
                  onDraftChange({ ...draft, steps: next });
                }}
                placeholder="SELECT ..."
                rows={3}
              />
            )}
            {step.type === 'ai' && (
              <textarea
                className={textareaClass}
                value={step.prompt ?? ''}
                onChange={(e) => {
                  const next = [...draft.steps];
                  next[i] = { ...next[i], prompt: e.target.value };
                  onDraftChange({ ...draft, steps: next });
                }}
                placeholder="AI prompt..."
                rows={3}
              />
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
  items,
  onView,
  onClear,
  t,
}: {
  items: HistoryListItem[];
  onView: (id: string) => void;
  onClear: () => void;
  t: ReturnType<typeof useI18n>['t'];
}) {
  return (
    <div className="p-2">
      {items.length > 0 && (
        <div className="flex justify-end mb-1">
          <button type="button" onClick={onClear} className="text-[10px] text-fg-muted hover:text-red-400">
            {t('workflows.history.clear')}
          </button>
        </div>
      )}
      {items.map((item) => (
        <button
          key={item.id}
          type="button"
          onClick={() => onView(item.id)}
          className="flex w-full items-center gap-2 rounded-md px-3 py-2 text-left hover:bg-surface-raised/50 transition-colors"
        >
          <Clock className="h-3.5 w-3.5 text-fg-muted shrink-0" />
          <div className="flex-1 min-w-0">
            <div className="text-xs font-medium text-fg truncate">{item.workflowName}</div>
            <div className="text-[10px] text-fg-muted">{new Date(item.createdAt).toLocaleString()}</div>
          </div>
          <span className={cn('text-[10px]', item.success ? 'text-green-500' : 'text-red-400')}>
            {item.success ? '✓' : '✗'}
          </span>
        </button>
      ))}
      {items.length === 0 && (
        <div className="py-6 text-center text-xs text-fg-muted">{t('workflows.history.empty')}</div>
      )}
    </div>
  );
}
