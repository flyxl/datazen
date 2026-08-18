import { describe, expect, it, beforeEach, vi } from 'vitest';

vi.mock('../../locales/t', () => ({
  t: (key: string) => key,
}));

describe('panelStore', () => {
  let usePanelStore: typeof import('../panelStore').usePanelStore;
  let nextPanelId: typeof import('../panelStore').nextPanelId;
  type Panel = import('../panelStore').Panel;
  type TablePanel = import('../panelStore').TablePanel;

  const base = {
    configId: 'cfg-1',
    connectionId: 'conn-1',
    connectionName: 'TestDB',
    databaseType: 'postgresql' as const,
  };

  function makeTable(name: string): TablePanel {
    return {
      ...base,
      type: 'table',
      id: nextPanelId('tbl'),
      tableName: name,
      subTab: 'data',
    };
  }

  beforeEach(async () => {
    vi.resetModules();
    const mod = await import('../panelStore');
    usePanelStore = mod.usePanelStore;
    nextPanelId = mod.nextPanelId;
    usePanelStore.setState({ panels: [], activePanelId: null });
  });

  // ── addPanel ─────────────────────────────────────────────────

  it('addPanel appends panel and activates it by default', () => {
    const panel = makeTable('users');
    usePanelStore.getState().addPanel(panel);

    const state = usePanelStore.getState();
    expect(state.panels).toHaveLength(1);
    expect(state.panels[0].id).toBe(panel.id);
    expect(state.activePanelId).toBe(panel.id);
  });

  it('addPanel with activate=false does not change activePanelId', () => {
    const p1 = makeTable('users');
    const p2 = makeTable('orders');
    usePanelStore.getState().addPanel(p1);
    usePanelStore.getState().addPanel(p2, false);

    const state = usePanelStore.getState();
    expect(state.panels).toHaveLength(2);
    expect(state.activePanelId).toBe(p1.id);
  });

  // ── removePanel ──────────────────────────────────────────────

  it('removePanel removes panel and adjusts active', () => {
    const p1 = makeTable('users');
    const p2 = makeTable('orders');
    usePanelStore.getState().addPanel(p1);
    usePanelStore.getState().addPanel(p2);

    usePanelStore.getState().removePanel(p2.id);

    const state = usePanelStore.getState();
    expect(state.panels).toHaveLength(1);
    expect(state.activePanelId).toBe(p1.id);
  });

  it('removePanel selects next panel when removing active', () => {
    const p1 = makeTable('users');
    const p2 = makeTable('orders');
    const p3 = makeTable('products');
    usePanelStore.getState().addPanel(p1);
    usePanelStore.getState().addPanel(p2, false);
    usePanelStore.getState().addPanel(p3, false);
    usePanelStore.getState().setActivePanel(p2.id);

    usePanelStore.getState().removePanel(p2.id);

    const state = usePanelStore.getState();
    expect(state.panels).toHaveLength(2);
    expect(state.activePanelId).toBe(p3.id);
  });

  it('removePanel selects previous panel when removing last', () => {
    const p1 = makeTable('users');
    const p2 = makeTable('orders');
    usePanelStore.getState().addPanel(p1);
    usePanelStore.getState().addPanel(p2);

    usePanelStore.getState().removePanel(p2.id);

    expect(usePanelStore.getState().activePanelId).toBe(p1.id);
  });

  it('removePanel sets active to null when last panel removed', () => {
    const p1 = makeTable('users');
    usePanelStore.getState().addPanel(p1);
    usePanelStore.getState().removePanel(p1.id);

    const state = usePanelStore.getState();
    expect(state.panels).toHaveLength(0);
    expect(state.activePanelId).toBeNull();
  });

  // ── setActivePanel ───────────────────────────────────────────

  it('setActivePanel updates activePanelId', () => {
    const p1 = makeTable('users');
    const p2 = makeTable('orders');
    usePanelStore.getState().addPanel(p1);
    usePanelStore.getState().addPanel(p2, false);

    usePanelStore.getState().setActivePanel(p2.id);
    expect(usePanelStore.getState().activePanelId).toBe(p2.id);
  });

  // ── updatePanel ──────────────────────────────────────────────

  it('updatePanel merges partial data into panel', () => {
    const p1 = makeTable('users');
    usePanelStore.getState().addPanel(p1);

    usePanelStore.getState().updatePanel(p1.id, { subTab: 'structure' });

    const updated = usePanelStore.getState().panels[0] as TablePanel;
    expect(updated.subTab).toBe('structure');
    expect(updated.tableName).toBe('users');
  });

  it('updatePanel with structureEditing', () => {
    const p1 = makeTable('users');
    usePanelStore.getState().addPanel(p1);

    usePanelStore.getState().updatePanel(p1.id, {
      subTab: 'structure',
      structureEditing: true,
    } as Partial<Panel>);

    const updated = usePanelStore.getState().panels[0] as TablePanel;
    expect(updated.structureEditing).toBe(true);
  });

  // ── removeAllForConnection ───────────────────────────────────

  it('removeAllForConnection removes all panels for a configId', () => {
    const p1 = makeTable('users');
    const p2: Panel = {
      ...base,
      configId: 'cfg-2',
      connectionId: 'conn-2',
      type: 'table',
      id: nextPanelId('tbl'),
      tableName: 'other',
      subTab: 'data',
    };
    usePanelStore.getState().addPanel(p1);
    usePanelStore.getState().addPanel(p2, false);

    usePanelStore.getState().removeAllForConnection('cfg-1');

    const state = usePanelStore.getState();
    expect(state.panels).toHaveLength(1);
    expect(state.panels[0].configId).toBe('cfg-2');
    expect(state.activePanelId).toBe(p2.id);
  });

  it('removeAllForConnection sets active to last remaining', () => {
    const p1 = makeTable('users');
    const p2 = makeTable('orders');
    usePanelStore.getState().addPanel(p1);
    usePanelStore.getState().addPanel(p2);

    usePanelStore.getState().removeAllForConnection('cfg-1');

    const state = usePanelStore.getState();
    expect(state.panels).toHaveLength(0);
    expect(state.activePanelId).toBeNull();
  });

  // ── closeOtherPanels ─────────────────────────────────────────

  it('closeOtherPanels keeps only the specified panel', () => {
    const p1 = makeTable('users');
    const p2 = makeTable('orders');
    const p3 = makeTable('products');
    usePanelStore.getState().addPanel(p1);
    usePanelStore.getState().addPanel(p2, false);
    usePanelStore.getState().addPanel(p3, false);

    usePanelStore.getState().closeOtherPanels(p2.id);

    const state = usePanelStore.getState();
    expect(state.panels).toHaveLength(1);
    expect(state.panels[0].id).toBe(p2.id);
    expect(state.activePanelId).toBe(p2.id);
  });

  // ── closeAllPanels ───────────────────────────────────────────

  it('closeAllPanels removes all panels', () => {
    usePanelStore.getState().addPanel(makeTable('users'));
    usePanelStore.getState().addPanel(makeTable('orders'), false);

    usePanelStore.getState().closeAllPanels();

    const state = usePanelStore.getState();
    expect(state.panels).toHaveLength(0);
    expect(state.activePanelId).toBeNull();
  });

  // ── closePanelsToTheRight ────────────────────────────────────

  it('closePanelsToTheRight removes panels after the specified one', () => {
    const p1 = makeTable('users');
    const p2 = makeTable('orders');
    const p3 = makeTable('products');
    usePanelStore.getState().addPanel(p1);
    usePanelStore.getState().addPanel(p2, false);
    usePanelStore.getState().addPanel(p3, false);

    usePanelStore.getState().closePanelsToTheRight(p1.id);

    const state = usePanelStore.getState();
    expect(state.panels).toHaveLength(1);
    expect(state.panels[0].id).toBe(p1.id);
  });

  it('closePanelsToTheRight adjusts active when active is removed', () => {
    const p1 = makeTable('users');
    const p2 = makeTable('orders');
    const p3 = makeTable('products');
    usePanelStore.getState().addPanel(p1);
    usePanelStore.getState().addPanel(p2, false);
    usePanelStore.getState().addPanel(p3);

    usePanelStore.getState().closePanelsToTheRight(p1.id);

    expect(usePanelStore.getState().activePanelId).toBe(p1.id);
  });

  // ── closePanelsToTheLeft ─────────────────────────────────────

  it('closePanelsToTheLeft removes panels before the specified one', () => {
    const p1 = makeTable('users');
    const p2 = makeTable('orders');
    const p3 = makeTable('products');
    usePanelStore.getState().addPanel(p1);
    usePanelStore.getState().addPanel(p2, false);
    usePanelStore.getState().addPanel(p3, false);

    usePanelStore.getState().closePanelsToTheLeft(p3.id);

    const state = usePanelStore.getState();
    expect(state.panels).toHaveLength(1);
    expect(state.panels[0].id).toBe(p3.id);
  });

  it('closePanelsToTheLeft adjusts active when active is removed', () => {
    const p1 = makeTable('users');
    const p2 = makeTable('orders');
    const p3 = makeTable('products');
    usePanelStore.getState().addPanel(p1);
    usePanelStore.getState().addPanel(p2, false);
    usePanelStore.getState().addPanel(p3, false);

    usePanelStore.getState().closePanelsToTheLeft(p3.id);

    expect(usePanelStore.getState().activePanelId).toBe(p3.id);
  });

  // ── nextPanelId ──────────────────────────────────────────────

  it('nextPanelId generates unique IDs with prefix', () => {
    const id1 = nextPanelId('tbl');
    const id2 = nextPanelId('tbl');
    const id3 = nextPanelId('query');

    expect(id1).toMatch(/^panel-tbl-/);
    expect(id2).toMatch(/^panel-tbl-/);
    expect(id3).toMatch(/^panel-query-/);
    expect(id1).not.toBe(id2);
  });

  // ── Cross-connection scenarios ───────────────────────────────

  it('panels from different connections coexist', () => {
    const p1 = makeTable('users');
    const p2: Panel = {
      configId: 'cfg-2',
      connectionId: 'conn-2',
      connectionName: 'OtherDB',
      databaseType: 'mysql' as any,
      type: 'table',
      id: nextPanelId('tbl'),
      tableName: 'other',
      subTab: 'data',
    };
    usePanelStore.getState().addPanel(p1);
    usePanelStore.getState().addPanel(p2);

    const state = usePanelStore.getState();
    expect(state.panels).toHaveLength(2);
    expect(state.panels[0].configId).toBe('cfg-1');
    expect(state.panels[1].configId).toBe('cfg-2');
    expect(state.activePanelId).toBe(p2.id);
  });

  it('removeAllForConnection preserves other connections panels', () => {
    const p1 = makeTable('users');
    const p2: Panel = {
      configId: 'cfg-2',
      connectionId: 'conn-2',
      connectionName: 'OtherDB',
      databaseType: 'mysql' as any,
      type: 'table',
      id: nextPanelId('tbl'),
      tableName: 'other',
      subTab: 'data',
    };
    usePanelStore.getState().addPanel(p1);
    usePanelStore.getState().addPanel(p2, false);

    usePanelStore.getState().removeAllForConnection('cfg-1');

    const state = usePanelStore.getState();
    expect(state.panels).toHaveLength(1);
    expect(state.panels[0].configId).toBe('cfg-2');
  });

  // ── Redis panel ──────────────────────────────────────────────

  it('supports redis-db panel type', () => {
    const redisPanel: Panel = {
      configId: 'cfg-redis',
      connectionId: 'conn-redis',
      connectionName: 'Redis',
      databaseType: 'redis' as any,
      type: 'redis-db',
      id: nextPanelId('redis'),
      dbName: 'db0',
    };
    usePanelStore.getState().addPanel(redisPanel);

    const state = usePanelStore.getState();
    expect(state.panels).toHaveLength(1);
    expect(state.panels[0].type).toBe('redis-db');
    expect(state.activePanelId).toBe(redisPanel.id);
  });
});
