/**
 * [tester] `useQueryExec` / `useQueryExecField` across the pane dimension.
 *
 * The pane wave gave both hooks a trailing `paneId`. `useQueryExecField` had no
 * test at all (the file sat at 50% line coverage), so the pane dimension of the
 * *field* selector was never exercised — a reader subscripted by pane is the
 * exact thing the split-pane wave will lean on.
 */

import { describe, expect, it, beforeEach, afterEach, vi } from 'vitest';
import { act, render, renderHook } from '@testing-library/react';

import { usePanelStore, EMPTY_QUERY_EXEC } from '../../stores/panelStore';
import type { QueryExecState } from '../../stores/queryExecActions';
import { useQueryExec, useQueryExecField } from '../useQueryExec';

const PANEL_ID = 'panel-q-1';
const PANE_2 = 'p2';
const KEY_2 = `${PANEL_ID}::${PANE_2}`;

function execState(overrides: Partial<QueryExecState>): QueryExecState {
  return { ...EMPTY_QUERY_EXEC, ...overrides };
}

describe('[tester] useQueryExec pane selectors', () => {
  beforeEach(() => {
    usePanelStore.setState({
      panels: [],
      activePanelId: null,
      queryExec: new Map([
        [PANEL_ID, execState({ sql: 'SELECT main', running: true, error: 'boom' })],
        [KEY_2, execState({ sql: 'SELECT second', running: false, error: null })],
      ]),
      focusedPaneId: null,
    });
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('reads the default pane when no pane id is given', () => {
    const { result } = renderHook(() => useQueryExec(PANEL_ID));
    expect(result.current.sql).toBe('SELECT main');
    expect(result.current.running).toBe(true);
  });

  it('reads a secondary pane when one is named', () => {
    const { result } = renderHook(() => useQueryExec(PANEL_ID, PANE_2));
    expect(result.current.sql).toBe('SELECT second');
    expect(result.current.running).toBe(false);
  });

  it('falls back to the empty state for a pane that was never opened', () => {
    const { result } = renderHook(() => useQueryExec(PANEL_ID, 'never-opened'));
    expect(result.current).toBe(EMPTY_QUERY_EXEC);
  });

  it('re-renders when the *other* pane changes, without adopting its value', () => {
    const { result } = renderHook(() => useQueryExec(PANEL_ID, PANE_2));
    expect(result.current.sql).toBe('SELECT second');

    act(() => {
      usePanelStore.setState((s) => ({
        queryExec: new Map(s.queryExec).set(PANEL_ID, execState({ sql: 'SELECT changed' })),
      }));
    });

    // The focused pane's own value is unchanged.
    expect(result.current.sql).toBe('SELECT second');
  });

  describe('useQueryExecField', () => {
    it('subscripts a field on the default pane', () => {
      const { result } = renderHook(() => useQueryExecField(PANEL_ID, 'sql'));
      expect(result.current).toBe('SELECT main');
    });

    it('subscripts a field on a named pane, independent of its sibling', () => {
      const { result } = renderHook(() => useQueryExecField(PANEL_ID, 'sql', PANE_2));
      expect(result.current).toBe('SELECT second');
    });

    it('subscripts a falsy field on a named pane without falling back to the sibling', () => {
      const { result } = renderHook(() => useQueryExecField(PANEL_ID, 'error', PANE_2));
      // The default pane has an error; the second pane must not inherit it.
      expect(result.current).toBeNull();
      expect(usePanelStore.getState().queryExec.get(PANEL_ID)?.error).toBe('boom');
    });

    it('subscripts a non-string field on a named pane', () => {
      const { result } = renderHook(() => useQueryExecField(PANEL_ID, 'running', PANE_2));
      expect(result.current).toBe(false);
    });

    it('re-renders when the pane it watches changes', () => {
      const { result } = renderHook(() => useQueryExecField(PANEL_ID, 'sql', PANE_2));
      expect(result.current).toBe('SELECT second');

      act(() => {
        usePanelStore.setState((s) => ({
          queryExec: new Map(s.queryExec).set(KEY_2, execState({ sql: 'SELECT edited' })),
        }));
      });

      expect(result.current).toBe('SELECT edited');
    });

    it('yields the empty state for a pane that was never opened', () => {
      const { result } = renderHook(() => useQueryExecField(PANEL_ID, 'sql', 'never-opened'));
      expect(result.current).toBe('');
    });
  });

  it('keeps two panes of one tab independent through the hook', () => {
    const view = render(
      <div>
        <Probe panelId={PANEL_ID} />
        <Probe panelId={PANEL_ID} paneId={PANE_2} />
      </div>,
    );
    const [main, second] = view.container.querySelectorAll('[data-probe]');
    expect(main.textContent).toBe('SELECT main');
    expect(second.textContent).toBe('SELECT second');
  });
});

function Probe({ panelId, paneId }: { panelId: string; paneId?: string }) {
  const sql = useQueryExecField(panelId, 'sql', paneId);
  return <span data-probe={paneId ?? 'main'}>{sql}</span>;
}
