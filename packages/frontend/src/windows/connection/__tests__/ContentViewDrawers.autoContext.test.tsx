import { cleanup, render } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { ContentViewDrawers } from '../ContentViewDrawers';
import { EMPTY_QUERY_EXEC, usePanelStore } from '../../../stores/panelStore';
import { usePanelResultEdit } from '../../../stores/panelResultEdit';
import { useEditorSelectionStore } from '../../../stores/editorSelectionStore';
import type { Panel } from '../../../stores/panelStore';
import type { AiChatContextSnapshot } from '../../../types';
import type { StatementResult } from '../../../types';

vi.mock('../../../hooks/useResizable', () => ({
  useResizable: () => ({ size: 320, handleRef: { current: null }, startResize: vi.fn() }),
}));

vi.mock('../../../components/DataTable/DetailPanel', () => ({
  DetailPanel: () => null,
}));

vi.mock('../../../components/ai/AiChatPanel', () => ({
  AiChatPanel: ({ autoContext }: { autoContext?: AiChatContextSnapshot }) => (
    <div
      data-testid="mock-ai-chat-panel"
      data-auto-sql={autoContext?.editorSql ?? ''}
      data-auto-selection={autoContext?.selection ?? ''}
      data-auto-error={autoContext?.lastError ?? ''}
      data-auto-rows={String(autoContext?.resultPreview?.rows.length ?? 0)}
      data-auto-row-count={String(autoContext?.resultPreview?.rowCount ?? 0)}
      data-auto-elapsed={String(autoContext?.resultPreview?.elapsedMs ?? 0)}
    />
  ),
}));

vi.mock('../../../commands/query', () => ({
  queryCommands: { resolveResultOrigin: vi.fn().mockResolvedValue(null) },
}));

const PANEL = {
  id: 'panel-query-auto-context',
  type: 'query',
  connectionId: 'cfg-1',
  dbSessionId: 'sess-1',
  connectionName: 'App',
  databaseType: 'postgres',
  title: 'Query',
  database: 'app',
  schema: 'public',
} as unknown as Panel;

const RESULT = {
  sql: 'SELECT id FROM orders',
  columns: [{ name: 'id', dataType: 'INTEGER', nullable: false }],
  rows: [[1], [2], [3], [4], [5], [6], [7]],
  executionTimeMs: 9,
} as unknown as StatementResult;

function renderDrawers() {
  return render(
    <ContentViewDrawers
      activePanel={PANEL}
      detailOpen={false}
      aiChatOpen
      detailPanelApplicable
      dbSessionId="sess-1"
      connectionName="App"
      currentDatabase="app"
      databaseType="postgresql"
      onCloseDetail={() => undefined}
      pendingDraftRequest={null}
      onDraftConsumed={() => undefined}
    />,
  );
}

beforeEach(() => {
  usePanelResultEdit.setState({ verdicts: {}, resolving: {}, errors: {} });
  useEditorSelectionStore.setState({
    byPanel: { [PANEL.id]: '  WHERE id = 1  ' },
  });
  usePanelStore.setState({
    panels: [PANEL],
    activePanelId: PANEL.id,
    focusedPaneId: null,
    queryExec: new Map([
      [
        PANEL.id,
        {
          ...EMPTY_QUERY_EXEC,
          sql: RESULT.sql,
          results: [RESULT],
          activeResultIdx: 0,
          error: 'syntax error near FROM',
          executionTimeMs: 9,
        },
      ],
    ]),
  });
});

afterEach(() => {
  cleanup();
  usePanelStore.setState({ panels: [], activePanelId: null, queryExec: new Map() });
  useEditorSelectionStore.setState({ byPanel: {} });
});

describe('ContentViewDrawers automatic chat context', () => {
  it('passes the active SQL, last error, and a row-capped result preview', () => {
    renderDrawers();
    const panel = document.querySelector('[data-testid="mock-ai-chat-panel"]')!;

    expect(panel.getAttribute('data-auto-sql')).toBe('SELECT id FROM orders');
    expect(panel.getAttribute('data-auto-selection')).toBe('WHERE id = 1');
    expect(panel.getAttribute('data-auto-error')).toBe('syntax error near FROM');
    expect(panel.getAttribute('data-auto-rows')).toBe('5');
    expect(panel.getAttribute('data-auto-row-count')).toBe('7');
    expect(panel.getAttribute('data-auto-elapsed')).toBe('9');
  });
});
