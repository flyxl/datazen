import { beforeEach, describe, expect, it } from 'vitest';
import { useWorkspaceTabsStore, workspaceTabKey, type WorkspaceTab } from '../workspaceTabsStore';

function tab(
  pluginId: string,
  pageId: string,
  overrides: Partial<WorkspaceTab> = {},
): WorkspaceTab {
  return {
    key: workspaceTabKey(pluginId, pageId),
    pluginId,
    pageId,
    title: `${pluginId}/${pageId}`,
    version: '1.0.0',
    ...overrides,
  };
}

const A = () => tab('acme.demo', 'main', { title: 'A' });
const B = () => tab('acme.demo', 'stats', { title: 'B' });
const C = () => tab('acme.other', 'main', { title: 'C' });
const D = () => tab('acme.other', 'extra', { title: 'D' });

describe('workspaceTabsStore', () => {
  beforeEach(() => {
    useWorkspaceTabsStore.setState({ tabs: [], activeKey: null });
  });

  function seed(tabs: WorkspaceTab[], activeKey: string | null): void {
    useWorkspaceTabsStore.setState({ tabs, activeKey });
  }

  it('open appends a tab and activates it', () => {
    const store = useWorkspaceTabsStore.getState();
    store.open(A());
    expect(useWorkspaceTabsStore.getState().tabs).toHaveLength(1);
    expect(useWorkspaceTabsStore.getState().activeKey).toBe(A().key);

    useWorkspaceTabsStore.getState().open(C());
    expect(useWorkspaceTabsStore.getState().tabs.map((t) => t.title)).toEqual(['A', 'C']);
    expect(useWorkspaceTabsStore.getState().activeKey).toBe(C().key);
  });

  it('open is idempotent on key conflict (no duplicate, metadata refreshed)', () => {
    seed([A(), C()], A().key);

    useWorkspaceTabsStore.getState().open(tab('acme.demo', 'main', { version: '2.0.0' }));

    const state = useWorkspaceTabsStore.getState();
    expect(state.tabs).toHaveLength(2);
    expect(state.tabs[0].version).toBe('2.0.0');
    expect(state.activeKey).toBe(A().key);
  });

  it('activate only switches to existing tabs', () => {
    seed([A(), C()], A().key);

    useWorkspaceTabsStore.getState().activate(C().key);
    expect(useWorkspaceTabsStore.getState().activeKey).toBe(C().key);

    useWorkspaceTabsStore.getState().activate('ghost:main');
    expect(useWorkspaceTabsStore.getState().activeKey).toBe(C().key);
  });

  it('close matrix: active middle → right neighbor preferred', () => {
    seed([A(), B(), C()], B().key);
    useWorkspaceTabsStore.getState().close(B().key);

    const state = useWorkspaceTabsStore.getState();
    expect(state.tabs.map((t) => t.title)).toEqual(['A', 'C']);
    expect(state.activeKey).toBe(C().key);
  });

  it('close matrix: active last → left neighbor', () => {
    seed([A(), B(), C()], C().key);
    useWorkspaceTabsStore.getState().close(C().key);

    const state = useWorkspaceTabsStore.getState();
    expect(state.activeKey).toBe(B().key);
  });

  it('close of a non-active tab keeps the current selection', () => {
    seed([A(), B()], B().key);
    useWorkspaceTabsStore.getState().close(A().key);

    const state = useWorkspaceTabsStore.getState();
    expect(state.tabs.map((t) => t.title)).toEqual(['B']);
    expect(state.activeKey).toBe(B().key);
  });

  it('close of the last remaining tab clears activeKey', () => {
    seed([A()], A().key);
    useWorkspaceTabsStore.getState().close(A().key);

    const state = useWorkspaceTabsStore.getState();
    expect(state.tabs).toEqual([]);
    expect(state.activeKey).toBeNull();
  });

  it('close ignores unknown keys', () => {
    seed([A()], A().key);
    useWorkspaceTabsStore.getState().close('missing:key');
    expect(useWorkspaceTabsStore.getState()).toMatchObject({
      tabs: [expect.objectContaining({ title: 'A' })],
      activeKey: A().key,
    });
  });

  it('closeByPlugin removes all plugin tabs and falls back to a neighbor', () => {
    // acme.demo owns the active tab and its neighbor; fallback comes from acme.other.
    seed([A(), B(), C(), D()], A().key);
    useWorkspaceTabsStore.getState().closeByPlugin('acme.demo');

    const state = useWorkspaceTabsStore.getState();
    expect(state.tabs.map((t) => t.pluginId)).toEqual(['acme.other', 'acme.other']);
    expect(state.activeKey).toBe(C().key);
  });

  it('closeByPlugin keeps activeKey when another plugin is active', () => {
    seed([A(), B(), C()], C().key);
    useWorkspaceTabsStore.getState().closeByPlugin('acme.demo');

    const state = useWorkspaceTabsStore.getState();
    expect(state.tabs.map((t) => t.title)).toEqual(['C']);
    expect(state.activeKey).toBe(C().key);
  });

  it('closeByPlugin clearing everything sets activeKey to null', () => {
    seed([C(), D()], D().key);
    useWorkspaceTabsStore.getState().closeByPlugin('acme.other');

    const state = useWorkspaceTabsStore.getState();
    expect(state.tabs).toEqual([]);
    expect(state.activeKey).toBeNull();
  });

  it('closeByPlugin is a no-op for unknown plugins', () => {
    seed([A(), C()], C().key);
    useWorkspaceTabsStore.getState().closeByPlugin('ghost.pkg');
    expect(useWorkspaceTabsStore.getState().tabs).toHaveLength(2);
    expect(useWorkspaceTabsStore.getState().activeKey).toBe(C().key);
  });
});
