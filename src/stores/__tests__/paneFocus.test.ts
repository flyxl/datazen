import { describe, expect, it } from 'vitest';
import { syncPaneFocus } from '../paneFocus';
import type { Panel } from '../panelTypes';

function panel(id: string): Panel {
  return { id, type: 'query', title: id } as unknown as Panel;
}

/**
 * The pane-focus rule, pinned where it now lives.
 *
 * The rule was moved out of `panelStore.ts` as a pure displacement — the code
 * is unchanged — so it needs its own nail: the next person who "tidies it up"
 * must find these expectations waiting.
 */
describe('syncPaneFocus', () => {
  it('keeps the focus of the tabs that are still there', () => {
    const patch = syncPaneFocus(
      { focusedPaneIdByPanel: { a: 'pane-1', b: 'pane-2' } },
      [panel('a'), panel('b')],
      'a',
    );

    expect(patch.focusedPaneIdByPanel).toEqual({ a: 'pane-1', b: 'pane-2' });
    expect(patch.focusedPaneId).toBe('pane-1');
    expect(patch.activePanelId).toBe('a');
  });

  it('drops the focus of a tab that is gone — a pane cannot outlive its panel', () => {
    const patch = syncPaneFocus(
      { focusedPaneIdByPanel: { a: 'pane-1', gone: 'pane-9' } },
      [panel('a')],
      'a',
    );

    expect(patch.focusedPaneIdByPanel).toEqual({ a: 'pane-1' });
  });

  it('applies the pane the action names and re-derives the mirror', () => {
    const patch = syncPaneFocus(
      { focusedPaneIdByPanel: { a: 'pane-1' } },
      [panel('a'), panel('b')],
      'b',
      { panelId: 'b', paneId: 'pane-2' },
    );

    expect(patch.focusedPaneIdByPanel).toEqual({ a: 'pane-1', b: 'pane-2' });
    expect(patch.focusedPaneId).toBe('pane-2');
  });

  it('removes the entry when the action names the default pane', () => {
    const patch = syncPaneFocus(
      { focusedPaneIdByPanel: { a: 'pane-1' } },
      [panel('a')],
      'a',
      { panelId: 'a', paneId: null },
    );

    expect(patch.focusedPaneIdByPanel).toEqual({});
    expect(patch.focusedPaneId).toBeNull();
  });

  it('has no mirror when no tab is active', () => {
    const patch = syncPaneFocus({ focusedPaneIdByPanel: {} }, [], null);

    expect(patch.focusedPaneId).toBeNull();
    expect(patch.activePanelId).toBeNull();
  });
});
