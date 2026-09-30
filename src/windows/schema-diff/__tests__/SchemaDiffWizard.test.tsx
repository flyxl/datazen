import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { SchemaDiffWindow } from '../SchemaDiffWindow';
import { schemaDiffCommands, type SchemaDiffPlan } from '../../../commands/schemaDiff';
import { databaseCommands } from '../../../commands/database';
import { fileCommands } from '../../../commands/file';
import type { SchemaDiffObjectIdentity } from '../../../commands/schemaDiff';
import type { DatabaseObject } from '../../../types';

const state = vi.hoisted(() => ({
  t: (key: string) => key,
  loadSettings: vi.fn().mockResolvedValue(undefined),
  endpoints: {
    sourceId: 'src',
    targetId: 'tgt',
    sourceDatabase: 'source',
    targetDatabase: 'target',
    sourceSchema: '',
    targetSchema: '',
    sourceDatabases: ['source'],
    targetDatabases: ['target'],
    sourceSchemas: [],
    targetSchemas: [],
    connOptions: [],
    targetOptions: [],
    targetConn: { name: 'Target', databaseType: 'postgres' },
    isCrossDialect: false,
    validateEndpoints: vi.fn(() => true),
    isSameEndpoint: vi.fn(() => false),
    ensureConnected: vi.fn(async (side: string) => `${side}-session`),
    setSourceId: vi.fn(),
    setTargetId: vi.fn(),
    setSourceDatabase: vi.fn(),
    setTargetDatabase: vi.fn(),
    setSourceSchema: vi.fn(),
    setTargetSchema: vi.fn(),
  },
}));
vi.mock('../useSchemaDiffEndpoints', () => ({ useSchemaDiffEndpoints: () => state.endpoints }));
vi.mock('../../../hooks/useSettings', () => ({ useSettings: vi.fn() }));
vi.mock('../../../hooks/useI18n', () => ({ useI18n: () => ({ t: state.t, language: 'en' }) }));
vi.mock('../../../hooks/useLocaleDomains', () => ({ useLocaleDomains: () => true }));
vi.mock('../../../stores/settingsStore', () => ({
  useSettingsStore: (sel: (s: { loadSettings: typeof state.loadSettings }) => unknown) =>
    sel(state),
}));
vi.mock('../../../hooks/useThemeListener', () => ({ useThemeListener: vi.fn() }));
vi.mock('../../../lib/windowManager', () => ({ openDocsWindow: vi.fn() }));
vi.mock('../../../lib/schemaDiffLimitationsPrefs', () => ({
  isSchemaDiffLimitationsDismissed: () => true,
  setSchemaDiffLimitationsDismissed: vi.fn(),
}));
vi.mock('../../../commands/database', () => ({
  databaseCommands: { listTables: vi.fn(), getDatabaseObjects: vi.fn() },
}));
vi.mock('../../../commands/file', () => ({ fileCommands: { saveTextWithDialog: vi.fn() } }));
vi.mock('../../../commands/schemaDiff', async (original) => ({
  ...(await original<typeof import('../../../commands/schemaDiff')>()),
  schemaDiffCommands: {
    getProfiles: vi.fn().mockResolvedValue([]),
    saveProfile: vi.fn().mockResolvedValue(undefined),
    deleteProfile: vi.fn().mockResolvedValue(undefined),
    compareTableSchemas: vi.fn(),
    preparePlan: vi.fn(),
    prepareUnifiedPlan: vi.fn(),
    executeDeploy: vi.fn(),
  },
}));

function plan(overrides: Partial<SchemaDiffPlan> = {}): SchemaDiffPlan {
  return {
    planId: 'review-1',
    table: 'users',
    tables: ['users'],
    sourceDialect: 'postgres',
    targetDialect: 'postgres',
    sameDialect: true,
    statements: [
      {
        sql: 'ALTER TABLE users DROP COLUMN old',
        risk: 'destructive',
        rollbackSql: 'ALTER TABLE users ADD COLUMN old int',
        summary: 'Remove old column',
      },
    ],
    warnings: [],
    requirements: [],
    rollbackCompleteness: { complete: true, missing: [] },
    ...overrides,
  };
}
const next = () => fireEvent.click(screen.getByTestId('schema-diff-next'));
const back = () => fireEvent.click(screen.getByRole('button', { name: 'schemaDiff.back' }));
function suppressClipboardFeedbackTimeout() {
  const realSetTimeout = window.setTimeout.bind(window);
  vi.spyOn(window, 'setTimeout').mockImplementation((handler, timeout, ...args) => {
    if (timeout === 2000) {
      return 0 as unknown as NodeJS.Timeout;
    }
    return realSetTimeout(handler, timeout, ...args) as unknown as NodeJS.Timeout;
  });
}
async function reachPlan() {
  next();
  await screen.findByTestId('schema-diff-table-row');
  next();
  await screen.findByTestId('schema-diff-detail-panel');
  next();
  await waitFor(() => expect(schemaDiffCommands.preparePlan).toHaveBeenCalled());
}
async function reachDeploy() {
  await reachPlan();
  await waitFor(() => expect(screen.getByTestId('schema-diff-next')).toBeEnabled());
  next();
  await screen.findByTestId('schema-diff-deploy');
}

beforeEach(() => {
  vi.clearAllMocks();
  state.endpoints.sourceDatabase = 'source';
  state.endpoints.targetDatabase = 'target';
  state.endpoints.sourceSchema = '';
  state.endpoints.targetSchema = '';
  state.endpoints.isCrossDialect = false;
  state.endpoints.validateEndpoints.mockReturnValue(true);
  state.endpoints.ensureConnected.mockImplementation(async (side) => `${side}-session`);
  vi.mocked(databaseCommands.listTables).mockResolvedValue([{ name: 'users', tableType: 'table' }]);
  vi.mocked(databaseCommands.getDatabaseObjects).mockResolvedValue([]);
  vi.mocked(schemaDiffCommands.compareTableSchemas).mockResolvedValue({
    table: 'users',
    added: [],
    removed: [],
    changed: [],
  });
  vi.mocked(schemaDiffCommands.preparePlan).mockResolvedValue(plan());
  vi.mocked(schemaDiffCommands.prepareUnifiedPlan).mockResolvedValue(plan());
  vi.mocked(schemaDiffCommands.executeDeploy).mockResolvedValue({
    status: 'committed',
    executedCount: 1,
    statementCount: 1,
    errors: [],
    statementResults: [],
  });
  Object.defineProperty(navigator, 'clipboard', {
    configurable: true,
    value: { writeText: vi.fn().mockResolvedValue(undefined) },
  });
});
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe('complete schema migration wizard journeys', () => {
  it('selects mixed schema objects by exact identity and submits one unified plan', async () => {
    const sourceObjects = [
      { kind: 'view', schema: 'public', name: 'orders_view' },
      { kind: 'type', schema: 'public', name: 'order_state' },
      { kind: 'sequence', schema: 'public', name: 'orders_id_seq' },
      {
        kind: 'function',
        schema: 'public',
        name: 'calculate_total',
        signature: 'integer, numeric',
      },
      { kind: 'procedure', schema: 'public', name: 'refresh_orders', signature: '' },
      {
        kind: 'trigger',
        schema: 'public',
        name: 'audit_order',
        targetSchema: 'public',
        targetName: 'orders',
      },
    ] as unknown as DatabaseObject[];
    const targetObjects = [
      ...sourceObjects.map((object) => ({ ...object })),
      { kind: 'view', schema: 'public', name: 'archive_view' },
    ] as DatabaseObject[];
    vi.mocked(databaseCommands.getDatabaseObjects).mockImplementation(async (sessionId, kind) => {
      const rows = sessionId === 'source-session' ? sourceObjects : targetObjects;
      return rows.filter((object) => object.kind === kind);
    });
    vi.mocked(schemaDiffCommands.prepareUnifiedPlan).mockResolvedValue(
      plan({
        planId: 'mixed-plan-1',
        tables: ['users', 'view:public:orders_view', 'function:public:calculate_total'],
      }),
    );

    render(<SchemaDiffWindow />);
    next();
    await screen.findByTestId('schema-diff-unified-object-picker');

    expect(screen.getByTestId('schema-diff-object-row-source-view-0')).toBeInTheDocument();
    expect(screen.getByTestId('schema-diff-object-row-source-type-0')).toBeInTheDocument();
    expect(screen.getByTestId('schema-diff-object-row-source-sequence-0')).toBeInTheDocument();
    expect(screen.getByTestId('schema-diff-object-row-source-function-0')).toBeInTheDocument();
    expect(screen.getByTestId('schema-diff-object-row-source-procedure-0')).toBeInTheDocument();
    expect(screen.getByTestId('schema-diff-object-row-source-trigger-0')).toBeInTheDocument();
    const sourceTriggerGroup = screen.getByTestId('schema-diff-object-kind-source-trigger');
    const targetTriggerGroup = screen.getByTestId('schema-diff-object-kind-target-trigger');
    fireEvent.click(within(sourceTriggerGroup).getByText('schemaDiff.objectKind.trigger'));
    fireEvent.click(within(targetTriggerGroup).getByText('schemaDiff.objectKind.trigger'));
    expect(
      within(screen.getByTestId('schema-diff-object-row-source-trigger-0')).getByRole('checkbox'),
    ).not.toBeChecked();
    const objectPicker = screen.getByTestId('schema-diff-unified-object-picker');
    fireEvent.click(within(objectPicker).getByText('schemaDiff.objectSelectAllSource'));
    fireEvent.click(within(objectPicker).getByText('schemaDiff.objectSelectAllTarget'));
    expect(
      within(screen.getByTestId('schema-diff-object-row-source-trigger-0')).getByRole('checkbox'),
    ).toBeChecked();
    const targetOnlyViewCheckbox = within(
      screen.getByTestId('schema-diff-object-row-target-view-1'),
    ).getByRole('checkbox');
    expect(targetOnlyViewCheckbox).toBeChecked();
    fireEvent.click(targetOnlyViewCheckbox);
    expect(targetOnlyViewCheckbox).not.toBeChecked();
    const targetTriggerCheckbox = within(
      screen.getByTestId('schema-diff-object-row-target-trigger-0'),
    ).getByRole('checkbox');
    expect(targetTriggerCheckbox).toBeChecked();
    fireEvent.click(targetTriggerCheckbox);
    expect(targetTriggerCheckbox).not.toBeChecked();

    next();
    await screen.findByTestId('schema-diff-detail-panel');
    next();
    await waitFor(() => expect(schemaDiffCommands.prepareUnifiedPlan).toHaveBeenCalledOnce());

    const call = vi.mocked(schemaDiffCommands.prepareUnifiedPlan).mock.calls[0]?.[0];
    expect(call).toMatchObject({
      sourceDbSessionId: 'source-session',
      targetDbSessionId: 'target-session',
      tableNames: ['users'],
      allowDestructive: false,
      sourceObjects: [
        { kind: 'view', schema: 'public', name: 'orders_view' },
        { kind: 'type', schema: 'public', name: 'order_state' },
        { kind: 'sequence', schema: 'public', name: 'orders_id_seq' },
        {
          kind: 'function',
          schema: 'public',
          name: 'calculate_total',
          signature: 'integer, numeric',
        },
        { kind: 'procedure', schema: 'public', name: 'refresh_orders', signature: '' },
        {
          kind: 'trigger',
          schema: 'public',
          name: 'audit_order',
          targetSchema: 'public',
          targetName: 'orders',
        },
      ],
      targetObjects: sourceObjects.filter(
        (object) => object.kind !== 'trigger' && object.name !== 'archive_view',
      ),
    });
    expect(schemaDiffCommands.preparePlan).not.toHaveBeenCalled();
  });

  it('selects a target-only table without sending it through source comparison', async () => {
    vi.mocked(databaseCommands.listTables).mockImplementation(async (sessionId) =>
      sessionId === 'source-session'
        ? [{ name: 'users', tableType: 'table' }]
        : [
            { name: 'users', tableType: 'table' },
            { name: 'archive', tableType: 'table' },
          ],
    );
    render(<SchemaDiffWindow />);
    next();
    await screen.findByTestId('schema-diff-table-origin-archive');
    const archiveRow = screen
      .getAllByTestId('schema-diff-table-row')
      .find((row) => row.getAttribute('data-table-name') === 'archive');
    expect(archiveRow).toBeDefined();
    expect(within(archiveRow!).getByRole('checkbox')).not.toBeChecked();
    fireEvent.click(within(archiveRow!).getByRole('checkbox'));

    next();
    await screen.findByTestId('schema-diff-detail-panel');
    expect(schemaDiffCommands.compareTableSchemas).toHaveBeenCalledExactlyOnceWith(
      'source-session',
      'target-session',
      'users',
      'users',
      undefined,
      undefined,
    );
    expect(screen.getByTestId('schema-diff-target-only-detail')).toBeInTheDocument();

    next();
    await waitFor(() => expect(schemaDiffCommands.preparePlan).toHaveBeenCalled());
    expect(schemaDiffCommands.preparePlan).toHaveBeenLastCalledWith(
      expect.objectContaining({
        tableNames: ['users'],
        targetTableNames: ['users'],
        targetOnlyTableNames: ['archive'],
        allowDestructive: false,
      }),
    );
  });

  it('exports and saves target-only selections as destructive profile state', async () => {
    vi.mocked(databaseCommands.listTables).mockImplementation(async (sessionId) =>
      sessionId === 'source-session'
        ? [{ name: 'users', tableType: 'table' }]
        : [
            { name: 'users', tableType: 'table' },
            { name: 'archive', tableType: 'table' },
          ],
    );
    vi.mocked(fileCommands.saveTextWithDialog).mockResolvedValue(true);
    render(<SchemaDiffWindow />);
    next();
    await screen.findByTestId('schema-diff-table-origin-archive');
    const archiveRow = screen
      .getAllByTestId('schema-diff-table-row')
      .find((row) => row.getAttribute('data-table-name') === 'archive');
    fireEvent.click(within(archiveRow!).getByRole('checkbox'));
    next();
    await screen.findByTestId('schema-diff-detail-panel');
    next();
    await waitFor(() => expect(schemaDiffCommands.preparePlan).toHaveBeenCalled());

    suppressClipboardFeedbackTimeout();
    fireEvent.click(screen.getByTestId('schema-diff-export-config'));
    await waitFor(() => expect(fileCommands.saveTextWithDialog).toHaveBeenCalled());
    const config = JSON.parse(vi.mocked(fileCommands.saveTextWithDialog).mock.calls[0][0]);
    expect(config).toMatchObject({
      tables: ['archive', 'users'],
      targetOnlyTables: ['archive'],
    });

    fireEvent.click(screen.getByTestId('schema-diff-profile-save'));
    fireEvent.change(screen.getByTestId('schema-diff-profile-name'), {
      target: { value: 'Drop archive' },
    });
    fireEvent.click(screen.getByTestId('schema-diff-profile-save-confirm'));
    await waitFor(() =>
      expect(schemaDiffCommands.saveProfile).toHaveBeenCalledWith(
        expect.objectContaining({
          tables: ['users'],
          targetOnlyTables: ['archive'],
          allowDestructive: false,
        }),
      ),
    );
  });

  it('honors footer confirmation and rollback transitions, then prevents replay after result', async () => {
    render(<SchemaDiffWindow />);
    await reachDeploy();
    const deploy = screen.getByTestId('schema-diff-deploy');
    expect(deploy).toBeDisabled();
    fireEvent.change(screen.getByPlaceholderText('DEPLOY'), { target: { value: 'DEPL' } });
    expect(deploy).toBeDisabled();
    fireEvent.change(screen.getByPlaceholderText('DEPLOY'), { target: { value: ' DEPLOY ' } });
    expect(deploy).toBeEnabled();
    fireEvent.click(screen.getByLabelText('schemaDiff.requireRollback'));
    fireEvent.click(screen.getByLabelText('schemaDiff.useTransaction'));
    expect(deploy).toBeDisabled();
    fireEvent.click(deploy);
    expect(schemaDiffCommands.executeDeploy).not.toHaveBeenCalled();
    fireEvent.click(screen.getByLabelText('schemaDiff.useTransaction'));
    fireEvent.click(deploy);
    await screen.findByTestId('schema-diff-deploy-result');
    expect(schemaDiffCommands.executeDeploy).toHaveBeenCalledExactlyOnceWith({
      targetDbSessionId: 'target-session',
      plan: plan(),
      useTransaction: true,
      requireRollback: true,
      confirmDestructive: 'DEPLOY',
      targetDatabase: 'target',
      targetSchema: null,
      profile: undefined,
    });
    expect(screen.getByTestId('schema-diff-deploy-status')).toHaveTextContent('committed');
    expect(deploy).toBeDisabled();
    fireEvent.click(deploy);
    expect(schemaDiffCommands.executeDeploy).toHaveBeenCalledTimes(1);
  });

  it('reports prepare failure, retries by returning to comparison and rejects stale deployment', async () => {
    vi.mocked(schemaDiffCommands.preparePlan).mockRejectedValueOnce(
      new Error('prepare unavailable'),
    );
    render(<SchemaDiffWindow />);
    await reachPlan();
    await screen.findByText('prepare unavailable');
    expect(screen.getByTestId('schema-diff-next')).toBeDisabled();
    back();
    next();
    await screen.findByText('Remove old column');
    next();
    fireEvent.change(screen.getByPlaceholderText('DEPLOY'), { target: { value: 'DEPLOY' } });
    vi.mocked(schemaDiffCommands.executeDeploy).mockRejectedValueOnce(
      new Error('Target schema changed; prepare again'),
    );
    fireEvent.click(screen.getByTestId('schema-diff-deploy'));
    await screen.findByText('Target schema changed; prepare again');
    expect(screen.queryByTestId('schema-diff-deploy-result')).not.toBeInTheDocument();
    back();
    fireEvent.click(screen.getByText('schemaDiff.regeneratePlan'));
    await waitFor(() => expect(schemaDiffCommands.preparePlan).toHaveBeenCalledTimes(3));
    next();
    expect(screen.getByPlaceholderText('DEPLOY')).toHaveValue('');
    expect(screen.getByTestId('schema-diff-deploy')).toBeDisabled();
  });

  it('prevents proceeding with empty selection, recovers compare errors and copies comparison', async () => {
    render(<SchemaDiffWindow />);
    next();
    await screen.findByTestId('schema-diff-table-row');
    fireEvent.click(screen.getByText('common.deselectAll'));
    next();
    await screen.findByText('schemaDiff.selectionRequired');
    expect(schemaDiffCommands.compareTableSchemas).not.toHaveBeenCalled();
    fireEvent.click(within(screen.getByTestId('schema-diff-table-row')).getByRole('checkbox'));
    vi.mocked(schemaDiffCommands.compareTableSchemas).mockRejectedValueOnce('compare failed');
    next();
    await screen.findByText('compare failed');
    fireEvent.click(screen.getByText('common.selectAll'));
    next();
    await screen.findByTestId('schema-diff-detail-panel');
    suppressClipboardFeedbackTimeout();
    fireEvent.click(screen.getByText('schemaDiff.copySummary'));
    await waitFor(() => expect(navigator.clipboard.writeText).toHaveBeenCalled());
    expect(schemaDiffCommands.compareTableSchemas).toHaveBeenCalledTimes(2);
  });

  it('updates table-local overrides and regenerates exact options, exports SQL and config', async () => {
    vi.mocked(schemaDiffCommands.preparePlan).mockResolvedValue(
      plan({
        typeSuggestions: [
          {
            table: 'users',
            column: 'name',
            sourceType: 'text',
            suggestedType: 'varchar(255)',
            currentType: 'varchar(255)',
            reason: 'index',
            isKeyOrIndexed: true,
          },
        ],
      }),
    );
    vi.mocked(fileCommands.saveTextWithDialog).mockResolvedValue(true);
    render(<SchemaDiffWindow />);
    await reachPlan();
    const input = await screen.findByTestId('type-suggestion-input-name');
    fireEvent.change(input, { target: { value: 'varchar(64)' } });
    fireEvent.click(screen.getByTestId('schema-diff-allow-destructive'));
    fireEvent.click(screen.getByTestId('schema-diff-include-indexes'));
    fireEvent.click(screen.getByTestId('schema-diff-apply-suggestions'));
    await waitFor(() =>
      expect(schemaDiffCommands.preparePlan).toHaveBeenLastCalledWith(
        expect.objectContaining({
          allowDestructive: true,
          includeIndexes: false,
          typeOverrides: [{ table: 'users', column: 'name', targetType: 'varchar(64)' }],
        }),
      ),
    );
    suppressClipboardFeedbackTimeout();
    fireEvent.click(screen.getByTestId('schema-diff-copy-sql'));
    await waitFor(() =>
      expect(navigator.clipboard.writeText).toHaveBeenCalledWith(
        expect.stringContaining('ALTER TABLE users DROP COLUMN old;'),
      ),
    );
    fireEvent.click(screen.getByTestId('schema-diff-export-config'));
    await waitFor(() => expect(fileCommands.saveTextWithDialog).toHaveBeenCalled());
    const config = JSON.parse(vi.mocked(fileCommands.saveTextWithDialog).mock.calls[0][0]);
    expect(config).toMatchObject({
      version: 2,
      sourceConnectionId: 'src',
      targetConnectionId: 'tgt',
      tables: ['users'],
      allowDestructive: true,
      includeIndexes: false,
    });
    expect(JSON.stringify(config)).not.toContain('session');
  });

  it('validates imported config, repairs invalid text and returns to selected objects', async () => {
    render(<SchemaDiffWindow />);
    await reachPlan();
    await screen.findByTestId('schema-diff-copy-sql');
    fireEvent.click(screen.getByTestId('schema-diff-import-config'));
    const text = screen.getByTestId('schema-diff-import-config-text');
    expect(screen.getByTestId('schema-diff-import-config-confirm')).toBeDisabled();
    fireEvent.change(text, { target: { value: '{"version":1}' } });
    fireEvent.click(screen.getByTestId('schema-diff-import-config-confirm'));
    await screen.findByText('schemaDiff.invalidConfig');
    const duplicateRoutine = {
      kind: 'function',
      schema: 'public',
      name: 'calculate_total',
      signature: 'integer, numeric',
    };
    fireEvent.change(text, {
      target: {
        value: JSON.stringify({
          version: 2,
          sourceConnectionId: 'src',
          targetConnectionId: 'tgt',
          tables: [],
          sourceObjects: [duplicateRoutine, duplicateRoutine],
        }),
      },
    });
    fireEvent.click(screen.getByTestId('schema-diff-import-config-confirm'));
    await screen.findByText('schemaDiff.invalidConfig');
    fireEvent.change(text, {
      target: {
        value: JSON.stringify({
          version: 2,
          sourceConnectionId: 'src',
          targetConnectionId: 'tgt',
          tables: [],
          sourceObjects: [{ ...duplicateRoutine, signature: '   ' }],
        }),
      },
    });
    fireEvent.click(screen.getByTestId('schema-diff-import-config-confirm'));
    await screen.findByText('schemaDiff.invalidConfig');
    fireEvent.change(text, {
      target: {
        value: JSON.stringify({
          version: 2,
          sourceConnectionId: 'src',
          targetConnectionId: 'tgt',
          tables: ['users'],
          requireRollback: true,
        }),
      },
    });
    fireEvent.click(screen.getByTestId('schema-diff-import-config-confirm'));
    await screen.findByTestId('schema-diff-objects-panel');
    await waitFor(() => expect(screen.getByTestId('schema-diff-next')).toBeEnabled());
    expect(state.endpoints.setSourceId).toHaveBeenCalledWith('src');
    expect(state.endpoints.setTargetId).toHaveBeenCalledWith('tgt');
    next();
    await screen.findByTestId('schema-diff-detail-panel');
    next();
    await screen.findByTestId('schema-diff-copy-sql');
    next();
    expect(screen.getByLabelText('schemaDiff.requireRollback')).toBeChecked();
  });

  it('round-trips exact routine and trigger selections through config export and import', async () => {
    const routine: DatabaseObject = {
      kind: 'function',
      schema: 'public',
      name: 'calculate_total',
      signature: 'integer, numeric',
    };
    const trigger: DatabaseObject = {
      kind: 'trigger',
      schema: 'public',
      name: 'audit_orders',
      targetSchema: 'public',
      targetName: 'orders',
    };
    vi.mocked(databaseCommands.listTables).mockResolvedValue([]);
    vi.mocked(databaseCommands.getDatabaseObjects).mockImplementation(async (sessionId, kind) =>
      (sessionId === 'source-session' ? [routine] : [trigger]).filter(
        (object) => object.kind === kind,
      ),
    );
    vi.mocked(fileCommands.saveTextWithDialog).mockResolvedValue(true);

    render(<SchemaDiffWindow />);
    next();
    await screen.findByTestId('schema-diff-unified-object-picker');
    fireEvent.click(
      within(screen.getByTestId('schema-diff-object-row-source-function-0')).getByRole('checkbox'),
    );
    const triggerGroup = screen.getByTestId('schema-diff-object-kind-target-trigger');
    fireEvent.click(within(triggerGroup).getByText('schemaDiff.objectKind.trigger'));
    fireEvent.click(
      within(screen.getByTestId('schema-diff-object-row-target-trigger-0')).getByRole('checkbox'),
    );
    next();
    await screen.findByTestId('schema-diff-copy-sql');
    await waitFor(() => expect(schemaDiffCommands.prepareUnifiedPlan).toHaveBeenCalledOnce());
    suppressClipboardFeedbackTimeout();
    fireEvent.click(screen.getByTestId('schema-diff-export-config'));
    await waitFor(() => expect(fileCommands.saveTextWithDialog).toHaveBeenCalled());
    const exportedText = vi.mocked(fileCommands.saveTextWithDialog).mock.calls[0]?.[0];
    const exported = JSON.parse(exportedText ?? '{}');
    expect(exported).toMatchObject({
      sourceObjects: [
        {
          kind: 'function',
          schema: 'public',
          name: 'calculate_total',
          signature: 'integer, numeric',
        },
      ],
      targetObjects: [
        {
          kind: 'trigger',
          schema: 'public',
          name: 'audit_orders',
          targetSchema: 'public',
          targetName: 'orders',
        },
      ],
    });

    fireEvent.click(screen.getByTestId('schema-diff-import-config'));
    fireEvent.change(screen.getByTestId('schema-diff-import-config-text'), {
      target: { value: exportedText },
    });
    fireEvent.click(screen.getByTestId('schema-diff-import-config-confirm'));
    await waitFor(() => expect(screen.getByTestId('schema-diff-next')).toBeEnabled());
    expect(
      within(screen.getByTestId('schema-diff-object-row-source-function-0')).getByRole('checkbox'),
    ).toBeChecked();
    expect(
      within(screen.getByTestId('schema-diff-object-row-target-trigger-0')).getByRole('checkbox'),
    ).toBeChecked();
    next();
    await waitFor(() => expect(schemaDiffCommands.prepareUnifiedPlan).toHaveBeenCalledTimes(2));
    expect(schemaDiffCommands.prepareUnifiedPlan).toHaveBeenLastCalledWith(
      expect.objectContaining({
        sourceObjects: [
          {
            kind: 'function',
            schema: 'public',
            name: 'calculate_total',
            signature: 'integer, numeric',
            targetSchema: null,
            targetName: null,
          },
        ],
        targetObjects: [
          {
            kind: 'trigger',
            schema: 'public',
            name: 'audit_orders',
            signature: null,
            targetSchema: 'public',
            targetName: 'orders',
          },
        ],
      }),
    );
  });

  it('preserves cross-dialect imported object selections and lets table migration continue after clearing them', async () => {
    state.endpoints.isCrossDialect = true;
    const routine: DatabaseObject = {
      kind: 'function',
      schema: 'public',
      name: 'calculate_total',
      signature: 'integer',
    };
    const view: SchemaDiffObjectIdentity = {
      kind: 'view',
      schema: 'public',
      name: 'orders_view',
    };
    const trigger: DatabaseObject = {
      kind: 'trigger',
      schema: 'public',
      name: 'audit_orders',
      targetSchema: 'public',
      targetName: 'orders',
    };
    vi.mocked(databaseCommands.getDatabaseObjects).mockImplementation(async (sessionId, kind) => {
      const rows = sessionId === 'source-session' ? [routine, view] : [trigger];
      // Rust's catalog IPC returns a string kind; the hook normalizes it from the queried bucket.
      return rows.filter((object) => object.kind === kind) as unknown as DatabaseObject[];
    });
    render(<SchemaDiffWindow />);
    await reachPlan();

    fireEvent.click(screen.getByTestId('schema-diff-import-config'));
    fireEvent.change(screen.getByTestId('schema-diff-import-config-text'), {
      target: {
        value: JSON.stringify({
          version: 2,
          sourceConnectionId: 'src',
          targetConnectionId: 'tgt',
          tables: ['users'],
          sourceObjects: [routine],
          targetObjects: [trigger],
          allowDestructive: false,
        }),
      },
    });
    fireEvent.click(screen.getByTestId('schema-diff-import-config-confirm'));
    await waitFor(() => expect(screen.getByTestId('schema-diff-next')).toBeEnabled());
    expect(screen.getByTestId('schema-diff-cross-dialect-objects-note')).toHaveTextContent(
      'schemaDiff.crossDialectObjectNote',
    );
    expect(
      within(screen.getByTestId('schema-diff-object-row-source-function-0')).getByRole('checkbox'),
    ).toBeChecked();
    expect(
      within(screen.getByTestId('schema-diff-object-row-source-view-0')).getByRole('checkbox'),
    ).toBeDisabled();
    const triggerGroup = screen.getByTestId('schema-diff-object-kind-target-trigger');
    fireEvent.click(within(triggerGroup).getByText('schemaDiff.objectKind.trigger'));
    expect(
      within(screen.getByTestId('schema-diff-object-row-target-trigger-0')).getByRole('checkbox'),
    ).toBeChecked();

    next();
    await screen.findByText('schemaDiff.crossDialectObjectBlocked');
    expect(schemaDiffCommands.prepareUnifiedPlan).not.toHaveBeenCalled();
    fireEvent.click(screen.getByTestId('schema-diff-clear-object-selections'));
    expect(
      within(screen.getByTestId('schema-diff-object-row-source-function-0')).getByRole('checkbox'),
    ).not.toBeChecked();
    expect(
      within(screen.getByTestId('schema-diff-object-row-target-trigger-0')).getByRole('checkbox'),
    ).not.toBeChecked();
    next();
    await screen.findByTestId('schema-diff-detail-panel');
    next();
    await waitFor(() => expect(schemaDiffCommands.preparePlan).toHaveBeenCalledTimes(2));
    expect(schemaDiffCommands.prepareUnifiedPlan).not.toHaveBeenCalled();
  });

  it('[tester] reports an object catalog failure and clears it after retry', async () => {
    let failFunctionLookup = true;
    vi.mocked(databaseCommands.getDatabaseObjects).mockImplementation(async (sessionId, kind) => {
      if (sessionId === 'source-session' && kind === 'function' && failFunctionLookup) {
        throw new Error('function catalog denied');
      }
      return [];
    });
    render(<SchemaDiffWindow />);
    next();

    await screen.findByTestId('schema-diff-object-error-source-function');
    expect(screen.getByTestId('schema-diff-object-retry')).toBeEnabled();
    failFunctionLookup = false;
    fireEvent.click(screen.getByTestId('schema-diff-object-retry'));

    await waitFor(() =>
      expect(
        screen.queryByTestId('schema-diff-object-error-source-function'),
      ).not.toBeInTheDocument(),
    );
    expect(screen.queryByTestId('schema-diff-object-retry')).not.toBeInTheDocument();
  });

  it('clears reviewed artifacts when endpoint changes', async () => {
    const view = render(<SchemaDiffWindow />);
    await reachDeploy();
    state.endpoints.targetDatabase = 'other-target';
    view.rerender(<SchemaDiffWindow />);
    await waitFor(() => expect(screen.getByTestId('schema-diff-deploy')).toBeDisabled());
    expect(screen.queryByPlaceholderText('DEPLOY')).not.toBeInTheDocument();
    expect(schemaDiffCommands.executeDeploy).not.toHaveBeenCalled();
  });

  it.each([
    {
      endpoint: 'source database',
      change: () => {
        state.endpoints.sourceDatabase = 'other-source';
      },
    },
    {
      endpoint: 'target database',
      change: () => {
        state.endpoints.targetDatabase = 'other-target';
      },
    },
    {
      endpoint: 'source schema',
      change: () => {
        state.endpoints.sourceSchema = 'other-source-schema';
      },
    },
    {
      endpoint: 'target schema',
      change: () => {
        state.endpoints.targetSchema = 'other-target-schema';
      },
    },
  ])(
    '[tester] clears a reviewed plan after imported config when $endpoint changes',
    async ({ change }) => {
      const view = render(<SchemaDiffWindow />);
      await reachPlan();

      fireEvent.click(screen.getByTestId('schema-diff-import-config'));
      fireEvent.change(screen.getByTestId('schema-diff-import-config-text'), {
        target: {
          value: JSON.stringify({
            version: 2,
            sourceConnectionId: 'src',
            targetConnectionId: 'tgt',
            tables: ['users'],
            allowDestructive: false,
          }),
        },
      });
      fireEvent.click(screen.getByTestId('schema-diff-import-config-confirm'));
      await waitFor(() => expect(screen.getByTestId('schema-diff-next')).toBeEnabled());

      next();
      await screen.findByTestId('schema-diff-detail-panel');
      next();
      await screen.findByTestId('schema-diff-copy-sql');

      change();
      view.rerender(<SchemaDiffWindow />);

      await waitFor(() =>
        expect(screen.queryByTestId('schema-diff-copy-sql')).not.toBeInTheDocument(),
      );
    },
  );

  it.each([
    { targetDialect: 'mysql', rollbackCompleteness: { complete: true, missing: [] } },
    { rollbackCompleteness: { complete: false, missing: ['old column'] } },
    {
      requirements: [
        { kind: 'Unsupported' as const, table: 'users', column: '', reason: 'unsupported' },
      ],
    },
  ])('refuses footer deploy when rollback or requirements are unmet: %j', async (overrides) => {
    vi.mocked(schemaDiffCommands.preparePlan).mockResolvedValue(plan(overrides));
    render(<SchemaDiffWindow />);
    await reachDeploy();
    fireEvent.change(screen.getByPlaceholderText('DEPLOY'), { target: { value: 'DEPLOY' } });
    fireEvent.click(screen.getByLabelText('schemaDiff.requireRollback'));
    expect(screen.getByTestId('schema-diff-deploy')).toBeDisabled();
    fireEvent.click(screen.getByTestId('schema-diff-deploy'));
    expect(schemaDiffCommands.executeDeploy).not.toHaveBeenCalled();
  });
});
