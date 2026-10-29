import { beforeEach, describe, expect, it } from 'vitest';
import { useWorkspacePanelStateStore } from '../workspacePanelStateStore';

beforeEach(() => {
  useWorkspacePanelStateStore.getState().clearWorkspacePanelSnapshots();
});

describe('workspacePanelStateStore', () => {
  it('starts with no snapshots', () => {
    const state = useWorkspacePanelStateStore.getState();
    expect(state.workflow).toBeNull();
    expect(state.dashboard).toBeNull();
    expect(state.extension).toBeNull();
  });

  it('saves and restores the workflow snapshot', () => {
    useWorkspacePanelStateStore.getState().saveWorkflowSnapshot({
      panels: [],
      activePanelId: 'wf-1',
      activeStepIndex: 2,
      sideTab: 'history',
      variables: { q: 'SELECT 1' },
    });
    const snapshot = useWorkspacePanelStateStore.getState().workflow;
    expect(snapshot?.activePanelId).toBe('wf-1');
    expect(snapshot?.sideTab).toBe('history');
    expect(snapshot?.variables).toEqual({ q: 'SELECT 1' });
  });

  it('saves and restores the dashboard snapshot', () => {
    useWorkspacePanelStateStore.getState().saveDashboardSnapshot({
      activeDashboardId: 'dash-1',
      editorOpen: true,
      editingWidget: null,
      isNewWidget: true,
      userWorkflows: [],
      historyOpen: false,
      historyWidget: null,
      renaming: true,
      nameDraft: 'Ops',
    });
    const snapshot = useWorkspacePanelStateStore.getState().dashboard;
    expect(snapshot?.activeDashboardId).toBe('dash-1');
    expect(snapshot?.editorOpen).toBe(true);
    expect(snapshot?.nameDraft).toBe('Ops');
  });

  it('clearDashboardSnapshot only drops the dashboard snapshot', () => {
    useWorkspacePanelStateStore.getState().saveWorkflowSnapshot({
      panels: [],
      activePanelId: null,
      activeStepIndex: null,
      sideTab: 'workflows',
      variables: {},
    });
    useWorkspacePanelStateStore.getState().saveDashboardSnapshot({
      activeDashboardId: 'dash-1',
      editorOpen: false,
      editingWidget: null,
      isNewWidget: false,
      userWorkflows: [],
      historyOpen: false,
      historyWidget: null,
      renaming: false,
      nameDraft: '',
    });
    useWorkspacePanelStateStore.getState().clearDashboardSnapshot();
    expect(useWorkspacePanelStateStore.getState().dashboard).toBeNull();
    expect(useWorkspacePanelStateStore.getState().workflow).not.toBeNull();
  });

  it('saves and restores the extension snapshot', () => {
    useWorkspacePanelStateStore.getState().saveExtensionSnapshot({
      search: 'bill',
      filter: 'theme',
    });
    expect(useWorkspacePanelStateStore.getState().extension).toEqual({
      search: 'bill',
      filter: 'theme',
    });
  });
});
