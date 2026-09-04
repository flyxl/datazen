import { describe, expect, it } from 'vitest';
import {
  isAiCreatePanel,
  isEditPanel,
  isHistoryDetailPanel,
  isWorkflowRunPanel,
  panelExecutionResult,
  panelTabLabel,
  type WorkflowPanel,
} from '../workflowPanelGuards';

const labels = { aiCreate: 'AI Create', edit: 'Edit', create: 'Create' };

describe('[tester] workflowPanelGuards', () => {
  it('type guards discriminate panel variants', () => {
    const runPanel: WorkflowPanel = {
      type: 'run',
      id: 'r1',
      workflowId: 'wf-1',
      workflowName: 'Demo',
      startedAt: '2026-01-01T10:00:00.000Z',
      result: null,
      isExecuting: false,
    };
    const editPanel: WorkflowPanel = {
      type: 'edit',
      id: 'e1',
      editingId: 'wf-1',
      draft: { name: 'Draft', description: '', variables: [], steps: [] },
      editorMode: 'visual',
      yamlText: '',
    };

    expect(isWorkflowRunPanel(runPanel)).toBe(true);
    expect(isEditPanel(editPanel)).toBe(true);
    expect(isAiCreatePanel({ type: 'ai-create', id: 'a1' })).toBe(true);
    expect(isHistoryDetailPanel({
      type: 'history',
      id: 'h1',
      historyId: 'hist-1',
      workflowName: 'Old',
      createdAt: '2026-01-01T09:00:00.000Z',
      result: { status: 'success', steps: [], durationMs: 1 },
    })).toBe(true);
  });

  it('panelExecutionResult returns result for run/history panels only', () => {
    const result = { status: 'success' as const, steps: [], durationMs: 1 };
    expect(
      panelExecutionResult({
        type: 'run',
        id: 'r1',
        workflowId: 'wf-1',
        workflowName: 'Demo',
        startedAt: '2026-01-01T10:00:00.000Z',
        result,
        isExecuting: false,
      }),
    ).toEqual(result);
    expect(
      panelExecutionResult({ type: 'ai-create', id: 'a1' }),
    ).toBeNull();
  });

  it('panelTabLabel formats labels per panel type', () => {
    expect(panelTabLabel({ type: 'ai-create', id: 'a1' }, labels)).toBe('AI Create');
    expect(
      panelTabLabel(
        {
          type: 'edit',
          id: 'e1',
          editingId: null,
          draft: { name: '', description: '', variables: [], steps: [] },
          editorMode: 'yaml',
          yamlText: '',
        },
        labels,
      ),
    ).toBe('Create');
    expect(
      panelTabLabel(
        {
          type: 'edit',
          id: 'e2',
          editingId: 'wf-1',
          draft: { name: 'My WF', description: '', variables: [], steps: [] },
          editorMode: 'visual',
          yamlText: '',
        },
        labels,
      ),
    ).toBe('My WF');
    expect(
      panelTabLabel(
        {
          type: 'run',
          id: 'r1',
          workflowId: 'wf-1',
          workflowName: 'Demo',
          startedAt: '2026-01-01T10:00:00.000Z',
          result: null,
          isExecuting: false,
        },
        labels,
      ),
    ).toContain('Demo');
  });
});
