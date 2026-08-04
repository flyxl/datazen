import { useCallback, useEffect, useMemo, useState } from 'react';
import {
  AlertTriangle,
  BarChart3,
  ChevronDown,
  ChevronRight,
  Clock,
  FolderOpen,
  History,
  Loader2,
  Pencil,
  Play,
  Plus,
  RefreshCw,
  TableProperties,
  Trash2,
  Wand2,
  X,
} from 'lucide-react';
import { useI18n } from '../../hooks/useI18n';
import { useAiStore } from '../../stores/aiStore';
import { aiCommands } from '../../commands/ai';
import { connectionCommands } from '../../commands/connection';
import { databaseCommands } from '../../commands/database';
import { DB_REGISTRY } from '../../lib/databaseTypes';
import { ChartView } from '../chart/ChartView';
import { isChartableResult } from '../../lib/chart/fieldInference';
import { cn } from '../../lib/cn';
import type { TranslationKey } from '../../locales';
import type {
  ColumnInfo,
  HistoryListItem,
  StatementResult,
  Value,
  WorkflowDefinition,
  WorkflowExecutionResult,
  WorkflowListItem,
  WorkflowStepType,
  StepExecutionResult,
} from '../../types';
import type { ChartConfig } from '../../types/chart';

type TFn = (key: TranslationKey, params?: Record<string, string | number>) => string;

interface WorkflowPanelProps {
  connectionId?: string;
}

interface WorkflowStepDraft {
  type: WorkflowStepType;
  id: string;
  sql?: string;
  prompt?: string;
  connection?: string;
  database?: string;
  // condition
  expr?: string;
  thenSteps?: WorkflowStepDraft[];
  elseSteps?: WorkflowStepDraft[];
  // foreach
  items?: string;
  asVar?: string;
  steps?: WorkflowStepDraft[];
  maxIterations?: number;
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

type PanelTab = 'workflows' | 'history';

export function WorkflowPanel({ connectionId }: WorkflowPanelProps) {
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
  const [tab, setTab] = useState<PanelTab>('workflows');
  const [historyItems, setHistoryItems] = useState<HistoryListItem[]>([]);
  const [historyDetail, setHistoryDetail] = useState<WorkflowExecutionResult | null>(null);
  const [savedConnections, setSavedConnections] = useState<{ id: string; name: string; databaseType: string }[]>([]);

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
    const defaults: Record<string, string> = {};
    for (const v of workflow.variables) {
      defaults[v.name] = v.default != null ? String(v.default) : '';
    }
    setVariables(defaults);
  };

  const handleExecute = async () => {
    if (!selectedWorkflow) return;
    await executeWorkflow({ workflowId: selectedWorkflow.id, variables, connectionId });
    void loadHistory();
  };

  const handleCreate = () => {
    setDraft(emptyDraft());
    setEditingId(null);
    setShowForm(true);
  };

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
        steps: workflow.steps.map(defStepToDraft),
      });
      setEditingId(workflowId);
      setShowForm(true);
    } catch (e) {
      setFeedback(String(e));
    }
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
      steps: draft.steps.map(draftStepToDef),
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

  const handleReload = useCallback(async () => {
    await aiCommands.workflowReload();
    void loadWorkflows();
  }, [loadWorkflows]);

  const handleViewHistory = async (historyId: string) => {
    try {
      const entry = await aiCommands.workflowHistoryGet(historyId);
      setHistoryDetail(entry.result);
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

  const inputClass =
    'w-full h-7 rounded border border-edge bg-surface px-2 text-xs text-fg outline-none focus:border-accent';
  const textareaClass =
    'w-full rounded border border-edge bg-surface px-2 py-1 text-xs font-mono text-fg outline-none focus:border-accent resize-y min-h-[60px]';

  if (workflowsLoading) {
    return (
      <div className="flex items-center justify-center py-4 text-fg-muted text-xs">
        <Loader2 className="h-4 w-4 animate-spin mr-1" />
        {t('workflows.loading')}
      </div>
    );
  }

  return (
    <div className="space-y-3">
      {/* Header */}
      <div className="flex items-center justify-between">
        <h3 className="text-sm font-medium flex items-center gap-2">
          <Wand2 className="w-4 h-4" />
          {t('workflows.title')}
        </h3>
        <div className="flex items-center gap-1">
          <button
            type="button" onMouseDown={(e) => e.preventDefault()}
            onClick={() => setTab('workflows')}
            className={`px-2 py-0.5 text-[11px] rounded transition-colors ${tab === 'workflows' ? 'bg-accent/10 text-accent' : 'text-fg-muted hover:text-fg'}`}
          >
            {t('workflows.title')}
          </button>
          <button
            type="button" onMouseDown={(e) => e.preventDefault()}
            onClick={() => setTab('history')}
            className={`flex items-center gap-1 px-2 py-0.5 text-[11px] rounded transition-colors ${tab === 'history' ? 'bg-accent/10 text-accent' : 'text-fg-muted hover:text-fg'}`}
          >
            <History className="h-3 w-3" />
            {t('workflows.history.title')}
          </button>
        </div>
      </div>

      {feedback && (
        <p className={`text-xs ${feedback.startsWith('Error') || feedback.startsWith('error') ? 'text-red-400' : 'text-green-500'}`}>{feedback}</p>
      )}

      {tab === 'history' ? (
        <HistoryTab
          items={historyItems}
          detail={historyDetail}
          onView={handleViewHistory}
          onClear={() => void handleClearHistory()}
          onCloseDetail={() => setHistoryDetail(null)}
          t={t}
        />
      ) : (
        <>
          {/* Actions bar */}
          <div className="flex items-center gap-1">
            <button
              type="button" onMouseDown={(e) => e.preventDefault()}
              onClick={handleCreate}
              className="flex items-center gap-1 px-2 py-1 text-[11px] text-accent hover:bg-accent/10 rounded transition-colors"
              title={t('workflows.create')}
            >
              <Plus className="h-3.5 w-3.5" />
              {t('workflows.create')}
            </button>
            <button
              type="button" onMouseDown={(e) => e.preventDefault()}
              onClick={() => void handleReload()}
              className="p-1 text-fg-muted hover:text-fg rounded transition-colors"
              title={t('workflows.reload')}
            >
              <RefreshCw className="h-3.5 w-3.5" />
            </button>
          </div>

          {/* Storage path */}
          {workflowsDir && (
            <div className="flex items-start gap-2 rounded-md border border-edge bg-surface-alt/50 p-2">
              <FolderOpen className="h-3.5 w-3.5 shrink-0 mt-0.5 text-fg-muted" />
              <div>
                <div className="text-[10px] text-fg-muted">{t('workflows.storageDir')}</div>
                <code className="text-[11px] text-fg-secondary break-all select-all">{workflowsDir}</code>
                <div className="text-[10px] text-fg-muted mt-0.5">{t('workflows.storageDirHint')}</div>
              </div>
            </div>
          )}

          {/* Create/Edit form */}
          {showForm && (
            <WorkflowForm
              draft={draft}
              setDraft={setDraft}
              editingId={editingId}
              savedConnections={savedConnections}
              onSave={() => void handleSave()}
              onCancel={() => setShowForm(false)}
              t={t}
              inputClass={inputClass}
              textareaClass={textareaClass}
            />
          )}

          {/* Workflows list */}
          {workflows.length === 0 && !showForm ? (
            <div className="py-4 text-center text-xs text-fg-muted">
              {t('workflows.empty')}
            </div>
          ) : (
            <div className="space-y-1">
              {workflows.map((workflow) => (
                <div
                  key={workflow.id}
                  className={`flex items-center justify-between px-3 py-2 rounded-md text-sm transition-colors cursor-pointer ${
                    selectedWorkflow?.id === workflow.id
                      ? 'bg-accent/10 text-accent'
                      : 'hover:bg-surface-raised text-fg-secondary'
                  }`}
                  onClick={() => handleSelect(workflow)}
                >
                  <div className="min-w-0 flex-1">
                    <div className="font-medium">{workflow.name}</div>
                    <div className="text-[11px] text-fg-muted truncate">{workflow.description}</div>
                  </div>
                  <div className="flex items-center gap-0.5 shrink-0 ml-2">
                    <button
                      type="button" onMouseDown={(e) => e.preventDefault()}
                      onClick={(e) => { e.stopPropagation(); void handleEdit(workflow.id); }}
                      className="p-1 text-fg-muted hover:text-fg rounded transition-colors"
                      title={t('workflows.edit')}
                    >
                      <Pencil className="h-3 w-3" />
                    </button>
                    <button
                      type="button" onMouseDown={(e) => e.preventDefault()}
                      onClick={(e) => { e.stopPropagation(); void handleDelete(workflow.id); }}
                      className="p-1 text-fg-muted hover:text-red-400 rounded transition-colors"
                      title={t('workflows.delete')}
                    >
                      <Trash2 className="h-3 w-3" />
                    </button>
                  </div>
                </div>
              ))}
            </div>
          )}

          {/* Execute selected workflow */}
          {selectedWorkflow && !showForm && (
            <div className="space-y-2 border-t border-edge pt-3">
              {selectedWorkflow.variables.map((v) => (
                <div key={v.name}>
                  <label className="text-[11px] text-fg-muted block mb-0.5">
                    {v.description || v.name}
                    {v.required && <span className="text-red-400 ml-0.5">*</span>}
                    {v.type === 'connection' && (
                      <span className="ml-1 text-accent text-[10px]">[{t('workflows.form.varTypeConnection')}]</span>
                    )}
                  </label>
                  {v.type === 'connection' ? (
                    <select
                      value={variables[v.name] ?? ''}
                      onChange={(e) =>
                        setVariables((prev) => ({ ...prev, [v.name]: e.target.value }))
                      }
                      className={inputClass}
                      disabled={isExecuting}
                    >
                      <option value="">{t('workflows.form.selectConnection')}</option>
                      {savedConnections.map((c) => (
                        <option key={c.id} value={c.id}>{c.name}</option>
                      ))}
                    </select>
                  ) : (
                    <input
                      type="text"
                      value={variables[v.name] ?? ''}
                      onChange={(e) =>
                        setVariables((prev) => ({ ...prev, [v.name]: e.target.value }))
                      }
                      className={inputClass}
                      placeholder={v.name}
                      disabled={isExecuting}
                    />
                  )}
                </div>
              ))}

              <button
                type="button" onMouseDown={(e) => e.preventDefault()}
                className="flex items-center gap-1.5 px-3 py-1.5 text-xs bg-accent text-white rounded hover:bg-accent/90 disabled:opacity-50 transition-colors"
                onClick={() => void handleExecute()}
                disabled={isExecuting}
              >
                {isExecuting ? (
                  <Loader2 className="h-3.5 w-3.5 animate-spin" />
                ) : (
                  <Play className="h-3.5 w-3.5" />
                )}
                {isExecuting ? t('workflows.running') : t('workflows.run')}
              </button>
            </div>
          )}

          {workflowError && (
            <div className="text-xs text-red-400 rounded bg-red-500/10 p-2">
              {workflowError}
            </div>
          )}

          {/* Structured execution result */}
          {result && <ExecutionResultPanel result={result} t={t} />}
        </>
      )}
    </div>
  );
}

// ─── Structured result display ──────────────────────────────────────────────

function ExecutionResultPanel({
  result,
  t,
}: {
  result: WorkflowExecutionResult;
  t: TFn;
}) {
  return (
    <div className="border border-edge rounded-md bg-surface overflow-hidden">
      <div className={`flex items-center justify-between px-3 py-2 text-xs font-medium border-b border-edge ${result.success ? 'bg-green-500/5 text-green-600 dark:text-green-400' : 'bg-red-500/5 text-red-600 dark:text-red-400'}`}>
        <span>
          {result.success ? '✓' : '✗'} {result.success ? t('workflows.result') : t('workflows.executionFailed')}
        </span>
        <span className="text-fg-muted font-normal">{result.totalTimeMs}ms</span>
      </div>

      {result.steps.map((step) => (
        <StepResultRow key={step.stepId} step={step} />
      ))}

      {result.error && (
        <div className="px-3 py-2 text-xs text-red-400 bg-red-500/5 border-t border-edge">
          {result.error}
        </div>
      )}

      {result.finalOutput && (
        <div className="px-3 py-2 border-t border-edge">
          <div className="text-[10px] text-fg-muted mb-1">{t('workflows.finalOutput')}</div>
          <pre className="text-xs text-fg-secondary whitespace-pre-wrap break-words max-h-40 overflow-auto">
            {result.finalOutput}
          </pre>
        </div>
      )}
    </div>
  );
}

function stepToStatementResult(step: StepExecutionResult): StatementResult | null {
  const r = step.result;
  if (!r?.rows) return null;
  const cols = extractColumnNames(r);
  if (cols.length === 0) return null;
  const columnInfos: ColumnInfo[] = cols.map((c) => ({
    name: c.name,
    dataType: (c as { dataType?: string }).dataType || 'text',
    nullable: true,
  }));
  const rawRows = r.rows as unknown[];
  let rows: (Value | null)[][];
  if (rawRows.length > 0 && typeof rawRows[0] === 'object' && rawRows[0] !== null && !Array.isArray(rawRows[0])) {
    rows = (rawRows as Record<string, unknown>[]).map((obj) =>
      cols.map((c) => (obj[c.name] ?? null) as Value | null),
    );
  } else {
    rows = rawRows as (Value | null)[][];
  }
  return {
    sql: step.sqlExecuted ?? '',
    columns: columnInfos,
    rows,
    executionTimeMs: (r.execution_time_ms as number) ?? step.executionTimeMs,
  };
}

function extractColumnNames(stepResult: Record<string, unknown> | undefined): { name: string }[] {
  if (!stepResult) return [];
  const cols = stepResult.columns as { name: string }[] | undefined;
  if (Array.isArray(cols) && cols.length > 0) return cols;
  const rows = stepResult.rows as Record<string, unknown>[] | undefined;
  if (Array.isArray(rows) && rows.length > 0 && typeof rows[0] === 'object' && rows[0] !== null && !Array.isArray(rows[0])) {
    return Object.keys(rows[0]).map((k) => ({ name: k }));
  }
  return [];
}

function StepResultRow({ step }: { step: StepExecutionResult }) {
  const { t } = useI18n();
  const [expanded, setExpanded] = useState(false);
  const [viewMode, setViewMode] = useState<'table' | 'chart'>('table');
  const [chartConfig, setChartConfig] = useState<ChartConfig | undefined>();
  const statusIcon = step.status === 'success' ? '✓' : step.status === 'skipped' ? '⏭' : step.status === 'timed_out' ? '⏱' : '✗';
  const statusColor = step.status === 'success' ? 'text-green-500' : step.status === 'skipped' ? 'text-yellow-500' : 'text-red-400';

  const colInfos = useMemo(() => extractColumnNames(step.result), [step.result]);

  const statementResult = useMemo(() => stepToStatementResult(step), [step]);
  const rows = statementResult?.rows;
  const rowsCount = (step.result?.rows_count as number | undefined) ?? rows?.length ?? 0;
  const chartable = useMemo(
    () => statementResult != null && isChartableResult(statementResult),
    [statementResult],
  );
  const hasData = colInfos.length > 0 && rows && rows.length > 0;

  return (
    <div className="border-b border-edge last:border-b-0">
      <button
        type="button" onMouseDown={(e) => e.preventDefault()}
        onClick={() => setExpanded(!expanded)}
        className="w-full flex items-center gap-2 px-3 py-1.5 text-xs hover:bg-surface-raised/50 transition-colors"
      >
        {expanded ? <ChevronDown className="h-3 w-3 text-fg-muted shrink-0" /> : <ChevronRight className="h-3 w-3 text-fg-muted shrink-0" />}
        <span className={`${statusColor} shrink-0`}>{statusIcon}</span>
        <span className="font-medium text-fg truncate">{step.stepId}</span>
        <span className="text-fg-muted">[{step.stepType}]</span>
        {step.connectionName && (
          <span className="text-accent text-[10px]">{step.connectionName}</span>
        )}
        <span className="ml-auto text-fg-muted shrink-0">{step.executionTimeMs}ms</span>
      </button>

      {expanded && (
        <div className="px-3 pb-2 space-y-1">
          {step.sqlExecuted && (
            <div>
              <div className="text-[10px] text-fg-muted">SQL</div>
              <pre className="text-[11px] font-mono text-fg-secondary bg-surface-alt/50 p-1.5 rounded whitespace-pre-wrap break-words max-h-24 overflow-auto">
                {step.sqlExecuted}
              </pre>
            </div>
          )}

          {step.error && (
            <div className="text-[11px] text-red-400">{step.error}</div>
          )}

          {hasData && (
            <div>
              <div className="flex items-center gap-2">
                <div className="text-[10px] text-fg-muted">{rowsCount} row(s)</div>
                {chartable && (
                  <div className="flex items-center gap-0.5 rounded-md bg-surface p-0.5 ml-auto">
                    <button
                      type="button" onMouseDown={(e) => e.preventDefault()}
                      className={cn(
                        'flex items-center gap-0.5 rounded px-1.5 py-0.5 text-[10px] transition-colors',
                        viewMode === 'table'
                          ? 'bg-accent/20 text-accent font-medium'
                          : 'text-fg-muted hover:text-fg-secondary',
                      )}
                      onClick={() => setViewMode('table')}
                    >
                      <TableProperties className="h-2.5 w-2.5" />
                      {t('chart.viewTable')}
                    </button>
                    <button
                      type="button" onMouseDown={(e) => e.preventDefault()}
                      className={cn(
                        'flex items-center gap-0.5 rounded px-1.5 py-0.5 text-[10px] transition-colors',
                        viewMode === 'chart'
                          ? 'bg-accent/20 text-accent font-medium'
                          : 'text-fg-muted hover:text-fg-secondary',
                      )}
                      onClick={() => setViewMode('chart')}
                    >
                      <BarChart3 className="h-2.5 w-2.5" />
                      {t('chart.viewChart')}
                    </button>
                  </div>
                )}
              </div>

              {viewMode === 'chart' && statementResult ? (
                <div className="mt-1 border border-edge rounded overflow-hidden" style={{ height: 260 }}>
                  {statementResult.rows.length > 1000 && (
                    <div className="flex items-center gap-1 bg-surface-alt px-2 py-0.5 text-[10px] text-yellow-400">
                      <AlertTriangle className="h-2.5 w-2.5" />
                      {t('chart.sampledWarning', { limit: '1000' })}
                    </div>
                  )}
                  <ChartView
                    result={statementResult}
                    savedConfig={chartConfig}
                    onConfigChange={setChartConfig}
                    onDataPointClick={() => setViewMode('table')}
                  />
                </div>
              ) : (
                <div className="overflow-auto max-h-40 border border-edge rounded mt-0.5">
                  <table className="w-full text-[11px]">
                    <thead className="sticky top-0">
                      <tr className="bg-surface-alt">
                        {colInfos.map((col) => (
                          <th key={col.name} className="px-2 py-1 text-left font-semibold text-fg border-b border-edge whitespace-nowrap">
                            {col.name}
                          </th>
                        ))}
                      </tr>
                    </thead>
                    <tbody>
                      {rows.slice(0, 20).map((row, i) => (
                        <tr key={i} className="border-b border-edge last:border-b-0 hover:bg-surface-raised/30">
                          {row.map((val, j) => (
                            <td key={j} className="px-2 py-0.5 text-fg-secondary whitespace-nowrap max-w-[200px] truncate">
                              {val == null ? <span className="text-fg-muted italic">null</span> : String(val)}
                            </td>
                          ))}
                        </tr>
                      ))}
                      {rows.length > 20 && (
                        <tr><td colSpan={colInfos.length} className="px-2 py-0.5 text-fg-muted text-center">... {rows.length - 20} more</td></tr>
                      )}
                    </tbody>
                  </table>
                </div>
              )}
            </div>
          )}

          {step.stepType === 'ai' && step.result?.result != null && (
            <pre className="text-[11px] text-fg-secondary whitespace-pre-wrap break-words max-h-40 overflow-auto">
              {String(step.result.result as string)}
            </pre>
          )}
        </div>
      )}
    </div>
  );
}

// ─── History Tab ────────────────────────────────────────────────────────────

function HistoryTab({
  items,
  detail,
  onView,
  onClear,
  onCloseDetail,
  t,
}: {
  items: HistoryListItem[];
  detail: WorkflowExecutionResult | null;
  onView: (id: string) => void;
  onClear: () => void;
  onCloseDetail: () => void;
  t: TFn;
}) {
  if (detail) {
    return (
      <div>
        <button
          type="button" onMouseDown={(e) => e.preventDefault()}
          onClick={onCloseDetail}
          className="flex items-center gap-1 text-xs text-accent hover:underline mb-2"
        >
          <X className="h-3 w-3" />
          {t('workflows.history.back')}
        </button>
        <ExecutionResultPanel result={detail} t={t} />
      </div>
    );
  }

  return (
    <div className="space-y-2">
      {items.length > 0 && (
        <button
          type="button" onMouseDown={(e) => e.preventDefault()}
          onClick={onClear}
          className="text-[11px] text-red-400 hover:underline"
        >
          {t('workflows.history.clear')}
        </button>
      )}

      {items.length === 0 ? (
        <div className="py-4 text-center text-xs text-fg-muted">
          {t('workflows.history.empty')}
        </div>
      ) : (
        items.map((item) => (
          <div
            key={item.id}
            onClick={() => onView(item.id)}
            className="flex items-center justify-between px-3 py-2 rounded-md hover:bg-surface-raised text-xs cursor-pointer transition-colors"
          >
            <div className="min-w-0 flex-1">
              <div className="font-medium text-fg">{item.workflowName}</div>
              <div className="flex items-center gap-2 text-[10px] text-fg-muted mt-0.5">
                <Clock className="h-3 w-3 shrink-0" />
                {new Date(item.createdAt).toLocaleString()}
                <span className="text-fg-muted">{item.totalTimeMs}ms</span>
              </div>
            </div>
            <span className={`shrink-0 ml-2 text-[11px] font-medium ${item.success ? 'text-green-500' : 'text-red-400'}`}>
              {item.success ? '✓' : '✗'}
            </span>
          </div>
        ))
      )}
    </div>
  );
}

// ─── Workflow Form ──────────────────────────────────────────────────────────

function WorkflowForm({
  draft,
  setDraft,
  editingId,
  savedConnections,
  onSave,
  onCancel,
  t,
  inputClass,
  textareaClass,
}: {
  draft: ReturnType<typeof emptyDraft>;
  setDraft: React.Dispatch<React.SetStateAction<ReturnType<typeof emptyDraft>>>;
  editingId: string | null;
  savedConnections: { id: string; name: string; databaseType: string }[];
  onSave: () => void;
  onCancel: () => void;
  t: TFn;
  inputClass: string;
  textareaClass: string;
}) {
  const addStep = (type: WorkflowStepType) => {
    const idx = draft.steps.length + 1;
    const base: WorkflowStepDraft = { type, id: `step${idx}` };
    if (type === 'query') base.sql = '';
    if (type === 'ai') base.prompt = '';
    if (type === 'condition') {
      base.expr = '';
      base.thenSteps = [];
      base.elseSteps = [];
    }
    if (type === 'foreach') {
      base.items = '';
      base.asVar = 'item';
      base.steps = [];
      base.maxIterations = 100;
    }
    setDraft((prev) => ({ ...prev, steps: [...prev.steps, base] }));
  };

  const removeStep = (idx: number) => {
    setDraft((prev) => ({ ...prev, steps: prev.steps.filter((_, i) => i !== idx) }));
  };

  const updateStep = (idx: number, field: string, value: unknown) => {
    setDraft((prev) => ({
      ...prev,
      steps: prev.steps.map((s, i) => (i === idx ? { ...s, [field]: value } : s)),
    }));
  };

  const addVariable = () => {
    setDraft((prev) => ({
      ...prev,
      variables: [...prev.variables, { name: '', varType: 'string', description: '', required: false }],
    }));
  };

  const removeVariable = (idx: number) => {
    setDraft((prev) => ({ ...prev, variables: prev.variables.filter((_, i) => i !== idx) }));
  };

  const connVarNames = draft.variables
    .filter((v) => v.varType === 'connection')
    .map((v) => v.name);

  return (
    <div className="space-y-2 border border-edge rounded-md p-3 bg-surface">
      <h4 className="text-xs font-medium text-fg">
        {editingId ? t('workflows.edit') : t('workflows.create')}
      </h4>

      <div>
        <label className="text-[10px] text-fg-muted block mb-0.5">{t('workflows.form.id')}</label>
        <input
          type="text"
          value={draft.id}
          onChange={(e) => setDraft((p) => ({ ...p, id: e.target.value }))}
          className={inputClass}
          placeholder={t('workflows.form.idPlaceholder')}
          disabled={!!editingId}
        />
      </div>

      <div>
        <label className="text-[10px] text-fg-muted block mb-0.5">{t('workflows.form.name')}</label>
        <input
          type="text"
          value={draft.name}
          onChange={(e) => setDraft((p) => ({ ...p, name: e.target.value }))}
          className={inputClass}
          placeholder={t('workflows.form.namePlaceholder')}
        />
      </div>

      <div>
        <label className="text-[10px] text-fg-muted block mb-0.5">{t('workflows.form.description')}</label>
        <input
          type="text"
          value={draft.description}
          onChange={(e) => setDraft((p) => ({ ...p, description: e.target.value }))}
          className={inputClass}
          placeholder={t('workflows.form.descriptionPlaceholder')}
        />
      </div>

      {/* Variables */}
      <div>
        <div className="flex items-center justify-between mb-1">
          <label className="text-[10px] text-fg-muted">{t('workflows.form.variables')}</label>
          <button type="button" onMouseDown={(e) => e.preventDefault()} onClick={addVariable} className="text-[10px] text-accent hover:underline">
            {t('workflows.form.addVariable')}
          </button>
        </div>
        {draft.variables.map((v, i) => (
          <div key={i} className="flex items-center gap-1 mb-1">
            <input
              type="text"
              value={v.name}
              onChange={(e) => {
                const vars = [...draft.variables];
                vars[i] = { ...vars[i], name: e.target.value };
                setDraft((p) => ({ ...p, variables: vars }));
              }}
              className={inputClass}
              placeholder={t('workflows.form.varName')}
              style={{ width: '25%' }}
            />
            <select
              value={v.varType}
              onChange={(e) => {
                const vars = [...draft.variables];
                vars[i] = { ...vars[i], varType: e.target.value };
                setDraft((p) => ({ ...p, variables: vars }));
              }}
              className={inputClass}
              style={{ width: '20%' }}
            >
              <option value="string">string</option>
              <option value="number">number</option>
              <option value="connection">{t('workflows.form.varTypeConnection')}</option>
            </select>
            <input
              type="text"
              value={v.description}
              onChange={(e) => {
                const vars = [...draft.variables];
                vars[i] = { ...vars[i], description: e.target.value };
                setDraft((p) => ({ ...p, variables: vars }));
              }}
              className={inputClass}
              placeholder={t('workflows.form.varDesc')}
              style={{ width: '35%' }}
            />
            <label className="flex items-center gap-0.5 text-[10px] text-fg-muted whitespace-nowrap">
              <input
                type="checkbox"
                checked={v.required}
                onChange={(e) => {
                  const vars = [...draft.variables];
                  vars[i] = { ...vars[i], required: e.target.checked };
                  setDraft((p) => ({ ...p, variables: vars }));
                }}
              />
              {t('workflows.form.varRequired')}
            </label>
            <button
              type="button" onMouseDown={(e) => e.preventDefault()}
              onClick={() => removeVariable(i)}
              className="p-0.5 text-fg-muted hover:text-red-400"
            >
              <Trash2 className="h-3 w-3" />
            </button>
          </div>
        ))}
      </div>

      {/* Steps */}
      <div>
        <label className="text-[10px] text-fg-muted block mb-1">{t('workflows.form.steps')}</label>
        {draft.steps.map((step, i) => (
          <StepEditor
            key={i}
            step={step}
            index={i}
            onRemove={() => removeStep(i)}
            onUpdate={(field, value) => updateStep(i, field, value)}
            connVarNames={connVarNames}
            savedConnections={savedConnections}
            t={t}
            inputClass={inputClass}
            textareaClass={textareaClass}
          />
        ))}
        <div className="flex gap-2 flex-wrap">
          <button type="button" onMouseDown={(e) => e.preventDefault()} onClick={() => addStep('query')} className="text-[10px] text-accent hover:underline">
            + {t('workflows.form.addQueryStep')}
          </button>
          <button type="button" onMouseDown={(e) => e.preventDefault()} onClick={() => addStep('ai')} className="text-[10px] text-accent hover:underline">
            + {t('workflows.form.addAiStep')}
          </button>
          <button type="button" onMouseDown={(e) => e.preventDefault()} onClick={() => addStep('condition')} className="text-[10px] text-accent hover:underline">
            + {t('workflows.form.addConditionStep')}
          </button>
          <button type="button" onMouseDown={(e) => e.preventDefault()} onClick={() => addStep('foreach')} className="text-[10px] text-accent hover:underline">
            + {t('workflows.form.addForeachStep')}
          </button>
        </div>
      </div>

      {/* Actions */}
      <div className="flex gap-2 pt-1">
        <button
          type="button" onMouseDown={(e) => e.preventDefault()}
          className="px-3 py-1 text-xs bg-accent text-white rounded hover:bg-accent/90 transition-colors disabled:opacity-50"
          onClick={onSave}
          disabled={!draft.id.trim() || !draft.name.trim() || draft.steps.length === 0}
        >
          {t('common.save')}
        </button>
        <button
          type="button" onMouseDown={(e) => e.preventDefault()}
          className="px-3 py-1 text-xs text-fg-secondary border border-edge rounded hover:bg-surface-raised transition-colors"
          onClick={onCancel}
        >
          {t('common.cancel')}
        </button>
      </div>
    </div>
  );
}

function DatabasePicker({
  step,
  savedConnections,
  onUpdate,
  t,
  inputClass,
}: {
  step: WorkflowStepDraft;
  savedConnections: { id: string; name: string; databaseType: string }[];
  onUpdate: (field: string, value: unknown) => void;
  t: TFn;
  inputClass: string;
}) {
  const [databases, setDatabases] = useState<string[]>([]);
  const [loading, setLoading] = useState(false);

  const connId = step.connection ?? '';
  const isVariable = connId.startsWith('{{');
  const savedConn = !isVariable ? savedConnections.find((c) => c.id === connId) : undefined;
  const meta = savedConn ? DB_REGISTRY[savedConn.databaseType as keyof typeof DB_REGISTRY] : undefined;
  const needsDb = !!meta?.hasMultiDatabase;

  useEffect(() => {
    if (!needsDb || !connId) {
      setDatabases([]);
      return;
    }
    let cancelled = false;
    setLoading(true);
    (async () => {
      try {
        const dbs = await databaseCommands.getDatabases(connId);
        if (!cancelled) setDatabases(dbs);
      } catch {
        if (!cancelled) setDatabases([]);
      } finally {
        if (!cancelled) setLoading(false);
      }
    })();
    return () => { cancelled = true; };
  }, [connId, needsDb]);

  if (!needsDb && !isVariable) return null;

  if (isVariable) {
    return (
      <div className="flex items-center gap-1">
        <label className="text-[10px] text-fg-muted w-12 shrink-0">{t('workflows.form.database')}</label>
        <input
          type="text"
          value={step.database ?? ''}
          onChange={(e) => onUpdate('database', e.target.value || undefined)}
          className={inputClass}
          placeholder={t('workflows.form.databasePlaceholder')}
        />
      </div>
    );
  }

  return (
    <div className="flex items-center gap-1">
      <label className="text-[10px] text-fg-muted w-12 shrink-0">{t('workflows.form.database')}</label>
      {loading ? (
        <span className="text-[10px] text-fg-muted">{t('workflows.loading')}</span>
      ) : databases.length > 0 ? (
        <select
          value={step.database ?? ''}
          onChange={(e) => onUpdate('database', e.target.value || undefined)}
          className={inputClass}
        >
          <option value="">{t('workflows.form.selectDatabase')}</option>
          {databases.map((db) => (
            <option key={db} value={db}>{db}</option>
          ))}
        </select>
      ) : (
        <input
          type="text"
          value={step.database ?? ''}
          onChange={(e) => onUpdate('database', e.target.value || undefined)}
          className={inputClass}
          placeholder={t('workflows.form.databasePlaceholder')}
        />
      )}
    </div>
  );
}

function StepEditor({
  step,
  index,
  onRemove,
  onUpdate,
  connVarNames,
  savedConnections,
  t,
  inputClass,
  textareaClass,
}: {
  step: WorkflowStepDraft;
  index: number;
  onRemove: () => void;
  onUpdate: (field: string, value: unknown) => void;
  connVarNames: string[];
  savedConnections: { id: string; name: string; databaseType: string }[];
  t: TFn;
  inputClass: string;
  textareaClass: string;
}) {
  const typeLabel =
    step.type === 'query' ? t('workflows.form.sql')
      : step.type === 'ai' ? t('workflows.form.prompt')
        : step.type === 'condition' ? t('workflows.form.condition')
          : t('workflows.form.foreach');

  return (
    <div className="mb-2 rounded border border-edge p-2 space-y-1 bg-surface-alt/30">
      <div className="flex items-center justify-between">
        <span className="text-[10px] font-medium text-fg-secondary">
          #{index + 1} {typeLabel}
        </span>
        <button type="button" onMouseDown={(e) => e.preventDefault()} onClick={onRemove} className="text-[10px] text-red-400 hover:underline">
          {t('workflows.form.removeStep')}
        </button>
      </div>

      <div className="flex items-center gap-1">
        <label className="text-[10px] text-fg-muted w-12 shrink-0">{t('workflows.form.stepId')}</label>
        <input
          type="text"
          value={step.id}
          onChange={(e) => onUpdate('id', e.target.value)}
          className={inputClass}
        />
      </div>

      {step.type === 'query' && (
        <>
          <div className="flex items-center gap-1">
            <label className="text-[10px] text-fg-muted w-12 shrink-0">{t('workflows.form.connection')}</label>
            <select
              value={step.connection ?? ''}
              onChange={(e) => onUpdate('connection', e.target.value || undefined)}
              className={inputClass}
            >
              <option value="">{t('workflows.form.defaultConnection')}</option>
              {connVarNames.map((name) => (
                <option key={`var:${name}`} value={`{{${name}}}`}>
                  {'{{' + name + '}}'}
                </option>
              ))}
              {savedConnections.map((c) => (
                <option key={c.id} value={c.id}>{c.name}</option>
              ))}
            </select>
          </div>
          <DatabasePicker
            step={step}
            savedConnections={savedConnections}
            onUpdate={onUpdate}
            t={t}
            inputClass={inputClass}
          />
          <textarea
            value={step.sql ?? ''}
            onChange={(e) => onUpdate('sql', e.target.value)}
            className={textareaClass}
            rows={3}
            placeholder="SELECT ..."
          />
        </>
      )}

      {step.type === 'ai' && (
        <textarea
          value={step.prompt ?? ''}
          onChange={(e) => onUpdate('prompt', e.target.value)}
          className={textareaClass}
          rows={3}
          placeholder={t('workflows.form.prompt')}
        />
      )}

      {step.type === 'condition' && (
        <>
          <div className="flex items-center gap-1">
            <label className="text-[10px] text-fg-muted w-12 shrink-0">if</label>
            <input
              type="text"
              value={step.expr ?? ''}
              onChange={(e) => onUpdate('expr', e.target.value)}
              className={inputClass}
              placeholder="steps.s1.rows_count > 0"
            />
          </div>
          <div className="text-[10px] text-fg-muted mt-1">then / else {t('workflows.form.conditionHint')}</div>
        </>
      )}

      {step.type === 'foreach' && (
        <>
          <div className="flex items-center gap-1">
            <label className="text-[10px] text-fg-muted w-12 shrink-0">items</label>
            <input
              type="text"
              value={step.items ?? ''}
              onChange={(e) => onUpdate('items', e.target.value)}
              className={inputClass}
              placeholder="steps.s1.rows"
            />
          </div>
          <div className="flex items-center gap-1">
            <label className="text-[10px] text-fg-muted w-12 shrink-0">as</label>
            <input
              type="text"
              value={step.asVar ?? ''}
              onChange={(e) => onUpdate('asVar', e.target.value)}
              className={inputClass}
              placeholder="item"
            />
          </div>
          <div className="flex items-center gap-1">
            <label className="text-[10px] text-fg-muted w-12 shrink-0">max</label>
            <input
              type="number"
              value={step.maxIterations ?? 100}
              onChange={(e) => onUpdate('maxIterations', parseInt(e.target.value) || 100)}
              className={inputClass}
              style={{ width: '80px' }}
            />
          </div>
        </>
      )}
    </div>
  );
}

// ─── Helpers ────────────────────────────────────────────────────────────────

function defStepToDraft(s: WorkflowDefinition['steps'][number]): WorkflowStepDraft {
  const base: WorkflowStepDraft = { type: s.type, id: s.id };
  if (s.type === 'query') {
    base.sql = s.sql;
    base.connection = s.connection;
    base.database = s.database;
  } else if (s.type === 'ai') {
    base.prompt = s.prompt;
  } else if (s.type === 'condition') {
    base.expr = s.if;
    base.thenSteps = s.thenSteps?.map(defStepToDraft);
    base.elseSteps = s.elseSteps?.map(defStepToDraft);
  } else if (s.type === 'foreach') {
    base.items = s.items;
    base.asVar = s.asVar;
    base.steps = s.steps?.map(defStepToDraft);
    base.maxIterations = s.maxIterations;
  }
  return base;
}

function draftStepToDef(s: WorkflowStepDraft): WorkflowDefinition['steps'][number] {
  if (s.type === 'query') {
    return {
      type: 'query',
      id: s.id,
      sql: s.sql ?? '',
      connection: s.connection,
      database: s.database,
    };
  }
  if (s.type === 'ai') {
    return { type: 'ai', id: s.id, prompt: s.prompt ?? '' };
  }
  if (s.type === 'condition') {
    return {
      type: 'condition',
      id: s.id,
      if: s.expr ?? '',
      thenSteps: s.thenSteps?.map(draftStepToDef) ?? [],
      elseSteps: s.elseSteps?.map(draftStepToDef),
    };
  }
  return {
    type: 'foreach',
    id: s.id,
    items: s.items ?? '',
    asVar: s.asVar ?? 'item',
    steps: s.steps?.map(draftStepToDef) ?? [],
    maxIterations: s.maxIterations,
  };
}
