import { afterEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import {
  extensionRegistry,
  sqlEditorEnhancedEP,
  type QueryBuilderContribution,
  type SqlEditorEnhancedFeatures,
} from '@datazen/extension-points';
import { QueryEditorSection, type QueryEditorSectionProps } from '../QueryEditorSection';

vi.mock('../../../../components/SqlEditor', async () => {
  const { forwardRef } = await import('react');
  return { SqlEditor: forwardRef(() => <div data-testid="mock-sql-editor" />) };
});

vi.mock('../QueryBuilderHostAdapter', () => ({
  QueryBuilderHostAdapter: () => <div data-testid="mock-query-builder-panel" />,
}));

vi.mock('../../../../components/query/QueryContextSelectors', () => ({
  QueryContextSelectors: () => <div data-testid="mock-query-context-selectors" />,
}));

vi.mock('../../../../components/ui/ToolbarShell', () => ({
  ToolbarShell: ({ children }: { children: React.ReactNode }) => <div>{children}</div>,
}));

vi.mock('../../../../components/ui/ToolbarButton', () => ({
  ToolbarButton: ({
    label,
    onClick,
    'data-testid': testId,
  }: {
    label: string;
    onClick?: () => void;
    'data-testid'?: string;
  }) => (
    <button type="button" aria-label={label} data-testid={testId} onClick={onClick}>
      {label}
    </button>
  ),
}));

vi.mock('../../../../components/query/QueryExecutionStatus', () => ({
  QueryExecutionStatus: () => null,
}));

vi.mock('../../../../components/ai/Nl2SqlPanel', () => ({ Nl2SqlPanel: () => null }));

vi.mock('../../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

vi.mock('../../../../hooks/usePlatform', () => ({ usePlatform: () => 'linux' }));

vi.mock('../../../../hooks/useSettingsStore', () => ({
  useSettingsStore: (selector: (state: { settings: Record<string, unknown> }) => unknown) =>
    selector({ settings: {} }),
}));

vi.mock('../../../../stores/settingsStore', () => ({
  useSettingsStore: (selector: (state: { settings: Record<string, unknown> }) => unknown) =>
    selector({ settings: {} }),
}));

vi.mock('../../../../components/sql-editor/metadata/metadataCache', () => ({
  metadataCache: { invalidateSession: vi.fn() },
}));

vi.mock('../../../../lib/schemaCache', () => ({ invalidateSchemaCache: vi.fn() }));

const emptyExecutionViewModel = {} as QueryEditorSectionProps['executionViewModel'];
let unregister: (() => void) | undefined;

afterEach(() => {
  cleanup();
  unregister?.();
  unregister = undefined;
  vi.clearAllMocks();
});

function createContribution() {
  let openPanelId: string | null = null;
  const listeners = new Set<() => void>();
  const contribution: QueryBuilderContribution = {
    getOpenPanelId: () => openPanelId,
    subscribe: (listener) => {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    openFor: vi.fn((panelId) => {
      openPanelId = panelId;
      for (const listener of listeners) listener();
    }),
    hideFor: vi.fn(() => {
      openPanelId = null;
      for (const listener of listeners) listener();
    }),
    closeFor: vi.fn(() => {
      openPanelId = null;
      for (const listener of listeners) listener();
    }),
    destroyFor: vi.fn(),
    Panel: () => null,
    dispose: vi.fn(),
  };
  return contribution;
}

function props(overrides: Partial<QueryEditorSectionProps> = {}): QueryEditorSectionProps {
  const noop = () => {};
  return {
    panelId: 'query-panel-1',
    dbSessionId: 'session-1',
    connectionId: 'connection-1',
    databaseType: 'postgresql',
    editorRef: { current: null },
    toolbarRef: { current: null },
    compactToolbar: false,
    sql: 'SELECT 1',
    running: false,
    executionTimeMs: null,
    executionViewModel: emptyExecutionViewModel,
    sqlParams: [],
    paramValues: {},
    onParamChange: noop,
    editorHeight: 280,
    editorResizeRef: { current: null },
    editorSchema: {} as QueryEditorSectionProps['editorSchema'],
    editorDefaultSchema: undefined,
    editorDefaultTable: undefined,
    namespaceLoading: false,
    supportsExplain: false,
    safeMode: false,
    inTransaction: false,
    txBusy: false,
    isMultiDb: true,
    isPathHierarchy: false,
    hasContextSelectors: true,
    databases: ['app', 'analytics'],
    selectedDatabase: 'app',
    selectedSchema: 'public',
    namespaceTree: {} as QueryEditorSectionProps['namespaceTree'],
    pathAliases: {},
    contextPath: [],
    nl2sqlVisible: false,
    onToggleNl2sql: noop,
    historyVisible: false,
    favoritesVisible: false,
    onToggleHistory: noop,
    onToggleFavorites: noop,
    onUpdateSql: noop,
    onExecute: noop,
    onExecuteSelection: noop,
    onCancel: noop,
    onFormat: noop,
    onCompletionRefreshed: noop,
    onExplain: noop,
    onBeginTx: noop,
    onCommitTx: noop,
    onRollbackTx: noop,
    onApplyAiSql: noop,
    onOpenAddFavoriteDialog: noop,
    onQualifiedPath: noop,
    onSelectContextLevel: noop,
    onDropTable: noop,
    ...overrides,
  };
}

describe('QueryEditorSection Query Builder context', () => {
  it('rebinds the open builder when its query tab changes database or schema', () => {
    const contribution = createContribution();
    unregister = extensionRegistry.register(sqlEditorEnhancedEP, {
      queryBuilder: contribution,
    } as SqlEditorEnhancedFeatures);

    const view = render(<QueryEditorSection {...props()} />);
    // `tid()` is stripped outside E2E builds, so the icon-only button is
    // addressed through its accessible name.
    act(() => fireEvent.click(screen.getByRole('button', { name: 'query.visualBuilder.title' })));
    expect(contribution.openFor).toHaveBeenCalledTimes(1);

    view.rerender(
      <QueryEditorSection
        {...props({ selectedDatabase: 'analytics', selectedSchema: 'private' })}
      />,
    );

    expect(contribution.openFor).toHaveBeenLastCalledWith(
      'query-panel-1',
      JSON.stringify(['connection-1', 'session-1', 'analytics', 'private']),
    );
  });
});
