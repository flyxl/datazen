export interface WorkflowRunPanel {
  type: 'run';
  id: string;
  workflowId: string;
  workflowName: string;
  startedAt: string;
  result: import('../../types').WorkflowExecutionResult | null;
  isExecuting: boolean;
}

export interface HistoryDetailPanel {
  type: 'history';
  id: string;
  historyId: string;
  workflowName: string;
  createdAt: string;
  result: import('../../types').WorkflowExecutionResult;
}

export interface EditPanel {
  type: 'edit';
  id: string;
  editingId: string | null;
  draft: import('./WorkflowForm').WorkflowDraft;
  editorMode: 'visual' | 'yaml';
  yamlText: string;
}

export interface AiCreatePanel {
  type: 'ai-create';
  id: string;
}

export type WorkflowPanel = WorkflowRunPanel | HistoryDetailPanel | EditPanel | AiCreatePanel;

export function isWorkflowRunPanel(panel: WorkflowPanel): panel is WorkflowRunPanel {
  return panel.type === 'run';
}

export function isHistoryDetailPanel(panel: WorkflowPanel): panel is HistoryDetailPanel {
  return panel.type === 'history';
}

export function isEditPanel(panel: WorkflowPanel): panel is EditPanel {
  return panel.type === 'edit';
}

export function isAiCreatePanel(panel: WorkflowPanel): panel is AiCreatePanel {
  return panel.type === 'ai-create';
}

export function panelExecutionResult(
  panel: WorkflowPanel,
): import('../../types').WorkflowExecutionResult | null {
  if (isWorkflowRunPanel(panel)) return panel.result;
  if (isHistoryDetailPanel(panel)) return panel.result;
  return null;
}

export function panelTabLabel(
  panel: WorkflowPanel,
  labels: { aiCreate: string; edit: string; create: string },
): string {
  if (isAiCreatePanel(panel)) return labels.aiCreate;
  if (isEditPanel(panel)) {
    return panel.editingId ? panel.draft.name || labels.edit : labels.create;
  }
  if (isWorkflowRunPanel(panel)) {
    return `${panel.workflowName} ${new Date(panel.startedAt).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}`;
  }
  if (isHistoryDetailPanel(panel)) {
    return `${panel.workflowName} ${new Date(panel.createdAt).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}`;
  }
  return '';
}
