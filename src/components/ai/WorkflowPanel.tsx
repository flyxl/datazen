import { useCallback, useEffect, useState } from 'react';
import {
  BookOpen,
  FolderOpen,
  History,
  Loader2,
  Pencil,
  Play,
  Plus,
  RefreshCw,
  Trash2,
  Wand2,
} from 'lucide-react';
import { useI18n } from '../../hooks/useI18n';
import { useConfirmDialog } from '../../hooks/useConfirmDialog';
import { useAiStore } from '../../stores/aiStore';
import { aiCommands } from '../../commands/ai';
import { connectionCommands } from '../../commands/connection';
import { openDocsWindow } from '../../lib/windowManager';
import { Select } from '../ui/Select';
import { WorkflowForm, emptyDraft } from '../../windows/workflow/WorkflowForm';
import type { WorkflowDraft } from '../../windows/workflow/WorkflowForm';
import {
  workflowDefinitionToDraft,
  workflowDraftToDefinition,
} from '../../windows/workflow/workflowDraftConvert';
import { WorkflowExecutionResultPanel } from '../../windows/workflow/WorkflowExecutionResultPanel';
import { WorkflowHistoryTab } from '../../windows/workflow/WorkflowHistorySection';
import type { HistoryListItem, WorkflowExecutionResult, WorkflowListItem } from '../../types';

interface WorkflowPanelProps {
  dbSessionId?: string;
}

type PanelTab = 'workflows' | 'history';

export function WorkflowPanel({ dbSessionId }: WorkflowPanelProps) {
  const { t } = useI18n();
  const [confirmWf, confirmWfDialog] = useConfirmDialog();
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
  const [draft, setDraft] = useState<WorkflowDraft>(emptyDraft());
  const [feedback, setFeedback] = useState('');
  const [tab, setTab] = useState<PanelTab>('workflows');
  const [historyItems, setHistoryItems] = useState<HistoryListItem[]>([]);
  const [historyDetail, setHistoryDetail] = useState<WorkflowExecutionResult | null>(null);
  const [savedConnections, setSavedConnections] = useState<
    { id: string; name: string; databaseType: string; database?: string }[]
  >([]);

  useEffect(() => {
    void loadWorkflows();
    void aiCommands.workflowGetDir().then(setWorkflowsDir);
    void connectionCommands.getConnections().then((conns) =>
      setSavedConnections(
        conns.map((c) => ({
          id: c.id,
          name: c.name,
          databaseType: c.databaseType,
          database: c.database,
        })),
      ),
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
    await executeWorkflow({
      workflowId: selectedWorkflow.id,
      variables,
      connectionId: dbSessionId,
    });
    void loadHistory();
  };

  const handleCreate = () => {
    setDraft(emptyDraft());
    setEditingId(null);
    setShowForm(true);
  };

  const handleEdit = async (workflowId: string) => {
    try {
      const workflow = await aiCommands.workflowGet(workflowId);
      setDraft(workflowDefinitionToDraft(workflow));
      setEditingId(workflowId);
      setShowForm(true);
    } catch (e) {
      setFeedback(String(e));
    }
  };

  const handleDelete = async (workflowId: string) => {
    const ok = await confirmWf({
      title: t('workflows.delete'),
      message: t('workflows.deleteConfirm'),
      kind: 'warning',
    });
    if (!ok) return;
    await aiCommands.workflowDelete(workflowId);
    if (selectedWorkflow?.id === workflowId) {
      setSelectedWorkflow(null);
      clearWorkflowResult();
    }
    void loadWorkflows();
  };

  const handleSave = async () => {
    try {
      await aiCommands.workflowSave(workflowDraftToDefinition(draft));
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
    const ok = await confirmWf({
      title: t('workflows.history.clear'),
      message: t('workflows.history.clearConfirm'),
      kind: 'warning',
    });
    if (!ok) return;
    await aiCommands.workflowHistoryClear();
    setHistoryItems([]);
    setHistoryDetail(null);
  };

  const inputClass =
    'w-full h-7 rounded border border-edge bg-surface px-2 text-xs text-fg outline-none focus:border-accent';

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
      <div className="flex items-center justify-between">
        <h3 className="text-sm font-medium flex items-center gap-2">
          <Wand2 className="w-4 h-4" />
          {t('workflows.title')}
        </h3>
        <div className="flex items-center gap-1">
          <button
            type="button"
            onMouseDown={(e) => e.preventDefault()}
            onClick={() => setTab('workflows')}
            className={`px-2 py-0.5 text-[11px] rounded transition-colors ${tab === 'workflows' ? 'bg-accent/10 text-accent' : 'text-fg-muted hover:text-fg'}`}
          >
            {t('workflows.title')}
          </button>
          <button
            type="button"
            onMouseDown={(e) => e.preventDefault()}
            onClick={() => setTab('history')}
            className={`flex items-center gap-1 px-2 py-0.5 text-[11px] rounded transition-colors ${tab === 'history' ? 'bg-accent/10 text-accent' : 'text-fg-muted hover:text-fg'}`}
          >
            <History className="h-3 w-3" />
            {t('workflows.history.title')}
          </button>
        </div>
      </div>

      {feedback && (
        <p
          className={`text-xs ${feedback.startsWith('Error') || feedback.startsWith('error') ? 'text-red-400' : 'text-green-500'}`}
        >
          {feedback}
        </p>
      )}

      {tab === 'history' ? (
        <WorkflowHistoryTab
          items={historyItems}
          detail={historyDetail}
          onView={(id) => void handleViewHistory(id)}
          onClear={() => void handleClearHistory()}
          onCloseDetail={() => setHistoryDetail(null)}
          t={t}
        />
      ) : (
        <>
          <div className="flex items-center gap-1">
            <button
              type="button"
              onMouseDown={(e) => e.preventDefault()}
              onClick={handleCreate}
              className="flex items-center gap-1 px-2 py-1 text-[11px] text-accent hover:bg-accent/10 rounded transition-colors"
              title={t('workflows.create')}
            >
              <Plus className="h-3.5 w-3.5" />
              {t('workflows.create')}
            </button>
            <button
              type="button"
              onMouseDown={(e) => e.preventDefault()}
              onClick={() => void handleReload()}
              className="p-1 text-fg-muted hover:text-fg rounded transition-colors"
              title={t('workflows.reload')}
            >
              <RefreshCw className="h-3.5 w-3.5" />
            </button>
            <button
              type="button"
              onMouseDown={(e) => e.preventDefault()}
              onClick={() => openDocsWindow('workflows')}
              className="p-1 text-fg-muted hover:text-fg rounded transition-colors"
              title={t('docs.openWorkflowHelp')}
            >
              <BookOpen className="h-3.5 w-3.5" />
            </button>
          </div>

          {workflowsDir && (
            <div className="flex items-start gap-2 rounded-md border border-edge bg-surface-alt/50 p-2">
              <FolderOpen className="h-3.5 w-3.5 shrink-0 mt-0.5 text-fg-muted" />
              <div>
                <div className="text-[10px] text-fg-muted">{t('workflows.storageDir')}</div>
                <code className="text-[11px] text-fg-secondary break-all select-all">
                  {workflowsDir}
                </code>
                <div className="text-[10px] text-fg-muted mt-0.5">
                  {t('workflows.storageDirHint')}
                </div>
              </div>
            </div>
          )}

          {showForm && (
            <WorkflowForm
              draft={draft}
              editingId={editingId}
              connections={savedConnections}
              onDraftChange={setDraft}
              onSave={() => void handleSave()}
              onCancel={() => setShowForm(false)}
              variant="compact"
            />
          )}

          {workflows.length === 0 && !showForm ? (
            <div className="py-4 text-center text-xs text-fg-muted">{t('workflows.empty')}</div>
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
                  onMouseDown={(e) => e.preventDefault()}
                  onClick={() => handleSelect(workflow)}
                >
                  <div className="min-w-0 flex-1">
                    <div className="font-medium">{workflow.name}</div>
                    <div className="text-[11px] text-fg-muted truncate">{workflow.description}</div>
                  </div>
                  <div className="flex items-center gap-0.5 shrink-0 ml-2">
                    <button
                      type="button"
                      onMouseDown={(e) => e.preventDefault()}
                      onClick={(e) => {
                        e.stopPropagation();
                        void handleEdit(workflow.id);
                      }}
                      className="p-1 text-fg-muted hover:text-fg rounded transition-colors"
                      title={t('workflows.edit')}
                    >
                      <Pencil className="h-3 w-3" />
                    </button>
                    <button
                      type="button"
                      onMouseDown={(e) => e.preventDefault()}
                      onClick={(e) => {
                        e.stopPropagation();
                        void handleDelete(workflow.id);
                      }}
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

          {selectedWorkflow && !showForm && (
            <div className="space-y-2 border-t border-edge pt-3">
              {selectedWorkflow.variables.map((v) => (
                <div key={v.name}>
                  <label className="text-[11px] text-fg-muted block mb-0.5">
                    {v.description || v.name}
                    {v.required && <span className="text-red-400 ml-0.5">*</span>}
                    {v.type === 'connection' && (
                      <span className="ml-1 text-accent text-[10px]">
                        [{t('workflows.form.varTypeConnection')}]
                      </span>
                    )}
                  </label>
                  {v.type === 'connection' ? (
                    <Select
                      value={variables[v.name] ?? ''}
                      onChange={(val) => setVariables((prev) => ({ ...prev, [v.name]: val }))}
                      className="!h-8 !text-xs"
                      disabled={isExecuting}
                      options={[
                        { value: '', label: t('common.selectConnection') },
                        ...savedConnections.map((c) => ({ value: c.id, label: c.name })),
                      ]}
                    />
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
                type="button"
                onMouseDown={(e) => e.preventDefault()}
                className="flex items-center gap-1.5 rounded bg-query-run px-3 py-1.5 text-xs text-white transition-colors hover:bg-query-run/90 disabled:opacity-50"
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
            <div className="text-xs text-red-400 rounded bg-red-500/10 p-2">{workflowError}</div>
          )}

          {result && <WorkflowExecutionResultPanel result={result} t={t} />}
        </>
      )}
      {confirmWfDialog}
    </div>
  );
}
