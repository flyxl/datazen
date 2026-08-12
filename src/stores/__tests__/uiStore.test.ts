import { describe, expect, it, beforeEach } from 'vitest';

describe('uiStore', () => {
  let useUiStore: typeof import('../uiStore').useUiStore;

  beforeEach(async () => {
    const mod = await import('../uiStore');
    useUiStore = mod.useUiStore;
    useUiStore.setState({
      mainSidebarWidth: 220,
      connectionSidebarWidth: 280,
      editorHeight: 320,
      resultHeight: 360,
      connectionsViewMode: 'grid',
      activeDialog: null,
      isFullscreen: false,
    });
  });

  it('has sensible defaults', () => {
    const s = useUiStore.getState();
    expect(s.mainSidebarWidth).toBe(220);
    expect(s.connectionsViewMode).toBe('grid');
    expect(s.activeDialog).toBeNull();
    expect(s.isFullscreen).toBe(false);
  });

  it('updates layout dimensions', () => {
    useUiStore.getState().setMainSidebarWidth(300);
    useUiStore.getState().setConnectionSidebarWidth(400);
    useUiStore.getState().setEditorHeight(500);
    useUiStore.getState().setResultHeight(600);
    const s = useUiStore.getState();
    expect(s.mainSidebarWidth).toBe(300);
    expect(s.connectionSidebarWidth).toBe(400);
    expect(s.editorHeight).toBe(500);
    expect(s.resultHeight).toBe(600);
  });

  it('opens and closes dialogs', () => {
    useUiStore.getState().openDialog('new-connection');
    expect(useUiStore.getState().activeDialog).toBe('new-connection');
    useUiStore.getState().closeDialog();
    expect(useUiStore.getState().activeDialog).toBeNull();
  });

  it('sets connections view mode', () => {
    useUiStore.getState().setConnectionsViewMode('list');
    expect(useUiStore.getState().connectionsViewMode).toBe('list');
  });

  it('toggles fullscreen', () => {
    useUiStore.getState().setFullscreen(true);
    expect(useUiStore.getState().isFullscreen).toBe(true);
  });
});
