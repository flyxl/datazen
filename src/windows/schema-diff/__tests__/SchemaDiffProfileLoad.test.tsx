import { describe, expect, it, vi, beforeAll, beforeEach, afterEach } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { SchemaDiffWindow } from '../SchemaDiffWindow';
import type { SchemaDiffObjectIdentity } from '../../../commands/schemaDiff';
import type { DatabaseObject } from '../../../types';
// Fixture, not production wiring: `useI18n` (key-echoing, so assertions may
// target key strings) and `useLocaleDomains` (returns true) are mocked, so the
// graph never registers the host dictionaries and @datazen/ui's own components
// warn about perfectly registered keys. SchemaDiffWindow.test.tsx carries the
// full root-cause note; this is the same test-only registration route
// locales.test.ts uses.
import '../../../locales';
import { ensureAllLazyDomains } from '../../../locales/lazyPacks';

const { endpointState, profile, schemaDiffCommands, databaseCommands } = vi.hoisted(() => {
  const state = {
    sourceId: 'initial-source',
    targetId: 'initial-target',
    sourceDatabase: 'initial-db',
    targetDatabase: 'initial-db',
    sourceSchema: '',
    targetSchema: '',
  };
  const savedProfile = {
    version: 1 as const,
    id: 'schema-profile-1',
    name: 'Production schema',
    sourceConnectionId: 'profile-source',
    targetConnectionId: 'profile-target',
    sourceDatabase: 'profile-db',
    targetDatabase: 'profile-db',
    sourceSchema: 'public',
    targetSchema: 'public',
    targetOnlyTables: [] as string[],
    tables: ['public.users'],
    sourceObjects: [] as SchemaDiffObjectIdentity[],
    targetObjects: [] as SchemaDiffObjectIdentity[],
    allowDestructive: true,
    includeIndexes: false,
    requireRollback: true,
    typeOverrides: [{ table: 'public.users', column: 'name', targetType: 'VARCHAR(64)' }],
    createdAt: '2026-09-21T00:00:00.000Z',
    updatedAt: '2026-09-21T00:00:00.000Z',
  };
  return {
    endpointState: state,
    profile: savedProfile,
    schemaDiffCommands: {
      getProfiles: vi.fn().mockResolvedValue([savedProfile]),
      saveProfile: vi.fn().mockResolvedValue(undefined),
      deleteProfile: vi.fn().mockResolvedValue(undefined),
      compareTableSchemas: vi.fn().mockResolvedValue({
        table: 'public.users',
        missingOnTarget: [],
        extraOnTarget: [],
        changed: [],
        added: [],
        removed: [],
      }),
      preparePlan: vi.fn().mockResolvedValue({
        plan: {
          table: 'public.users',
          tables: ['public.users'],
          sourceDialect: 'postgresql',
          targetDialect: 'postgresql',
          sameDialect: true,
          statements: [],
          warnings: [],
          requirements: [],
          rollbackCompleteness: { complete: true, missing: [] },
          typeSuggestions: [],
        },
        planId: 'plan-1',
        selectionRevision: 1,
        planVersion: 1,
        handlerVersion: 1,
        checkpointVersion: 1,
        expiresAt: '2026-01-01T00:00:00Z',
        recoveryPolicy: 'readOnlyVerify',
      }),
      prepareUnifiedPlan: vi.fn().mockResolvedValue({
        plan: {
          table: 'schema objects',
          tables: [],
          sourceDialect: 'postgresql',
          targetDialect: 'postgresql',
          sameDialect: true,
          statements: [],
          warnings: [],
          requirements: [],
          rollbackCompleteness: { complete: true, missing: [] },
          typeSuggestions: [],
        },
        planId: 'plan-1',
        selectionRevision: 1,
        planVersion: 1,
        handlerVersion: 1,
        checkpointVersion: 1,
        expiresAt: '2026-01-01T00:00:00Z',
        recoveryPolicy: 'readOnlyVerify',
      }),
      executeDeploy: vi.fn(),
      listJobs: vi.fn().mockResolvedValue([]),
      getJobDetails: vi.fn(),
      cancelDeploy: vi.fn().mockResolvedValue(true),
      verifyRecovery: vi.fn(),
    },
    databaseCommands: {
      listTables: vi
        .fn()
        .mockResolvedValue([{ name: 'users', schema: 'public', tableType: 'table' }]),
      getDatabaseObjects: vi.fn().mockResolvedValue([]),
    },
  };
});

vi.mock('../useSchemaDiffEndpoints', () => ({
  useSchemaDiffEndpoints: () => ({
    ...endpointState,
    sourceDatabases: ['initial-db', 'profile-db'],
    targetDatabases: ['initial-db', 'profile-db'],
    sourceSchemas: ['public'],
    targetSchemas: ['public'],
    sourceSession: null,
    targetSession: null,
    sourceConn: {
      id: endpointState.sourceId,
      name: endpointState.sourceId,
      databaseType: 'postgres',
    },
    targetConn: {
      id: endpointState.targetId,
      name: endpointState.targetId,
      databaseType: 'postgres',
    },
    connOptions: [],
    targetOptions: [],
    isCrossDialect: false,
    setSourceId: (value: string) => {
      endpointState.sourceId = value;
    },
    setTargetId: (value: string) => {
      endpointState.targetId = value;
    },
    setSourceDatabase: (value: string) => {
      endpointState.sourceDatabase = value;
    },
    setTargetDatabase: (value: string) => {
      endpointState.targetDatabase = value;
    },
    setSourceSchema: (value: string) => {
      endpointState.sourceSchema = value;
    },
    setTargetSchema: (value: string) => {
      endpointState.targetSchema = value;
    },
    ensureConnected: vi
      .fn()
      .mockImplementation(async (side: 'source' | 'target') =>
        side === 'source' ? 'source-session' : 'target-session',
      ),
    refreshEndpointSessions: vi.fn().mockResolvedValue({ source: null, target: null }),
    validateEndpoints: vi.fn().mockReturnValue(true),
    isSameEndpoint: vi.fn().mockReturnValue(false),
    handleSwap: vi.fn(),
    connections: [],
  }),
}));

vi.mock('../../../commands/database', () => ({ databaseCommands }));
vi.mock('../../../commands/file', () => ({
  fileCommands: {
    saveTextWithDialog: vi.fn(),
    openTextWithDialog: vi.fn(),
  },
}));
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn().mockResolvedValue([]) }));
vi.mock('../../../hooks/useThemeListener', () => ({ useThemeListener: vi.fn() }));
vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key, language: 'en' }),
}));
vi.mock('../../../hooks/useLocaleDomains', () => ({ useLocaleDomains: () => true }));
vi.mock('../../../stores/settingsStore', () => ({
  useSettingsStore: (selector: (state: { loadSettings: () => Promise<void> }) => unknown) =>
    selector({ loadSettings: vi.fn().mockResolvedValue(undefined) }),
}));
vi.mock('../../../hooks/useSettings', () => ({ useSettings: vi.fn() }));
vi.mock('../../../lib/windowManager', () => ({ openDocsWindow: vi.fn() }));
vi.mock('../../../lib/crossWindowBus', () => ({
  listenCrossWindow: vi.fn().mockResolvedValue(() => {}),
}));
vi.mock('../../../lib/schemaDiffLimitationsPrefs', () => ({
  isSchemaDiffLimitationsDismissed: vi.fn().mockReturnValue(true),
  setSchemaDiffLimitationsDismissed: vi.fn(),
}));
vi.mock('../../../commands/schemaDiff', () => ({
  dialectSupportsTransactionalDdl: vi.fn().mockReturnValue(true),
  exportPlanSql: vi.fn().mockReturnValue(''),
  planHasDestructive: vi.fn().mockReturnValue(false),
  subscribeSchemaDiffJobUpdates: vi.fn().mockReturnValue(() => {}),
  schemaDiffCommands,
}));

describe('SchemaDiffWindow profile loading', () => {
  beforeAll(async () => {
    // Eager packs register at import; the lazy `sync` pack needs an explicit
    // await because the mocked `useLocaleDomains` never requests it.
    await ensureAllLazyDomains('en');
  });

  beforeEach(() => {
    endpointState.sourceId = 'initial-source';
    endpointState.targetId = 'initial-target';
    endpointState.sourceDatabase = 'initial-db';
    endpointState.targetDatabase = 'initial-db';
    endpointState.sourceSchema = '';
    endpointState.targetSchema = '';
    profile.tables = ['public.users'];
    profile.targetOnlyTables = [];
    profile.sourceObjects = [];
    profile.targetObjects = [];
    vi.clearAllMocks();
    schemaDiffCommands.getProfiles.mockResolvedValue([profile]);
    schemaDiffCommands.compareTableSchemas.mockResolvedValue({
      table: 'public.users',
      missingOnTarget: [],
      extraOnTarget: [],
      changed: [],
      added: [],
      removed: [],
    });
    schemaDiffCommands.preparePlan.mockResolvedValue({
      plan: {
        table: 'public.users',
        tables: ['public.users'],
        sourceDialect: 'postgresql',
        targetDialect: 'postgresql',
        sameDialect: true,
        statements: [],
        warnings: [],
        requirements: [],
        rollbackCompleteness: { complete: true, missing: [] },
        typeSuggestions: [],
      },
      planId: 'plan-1',
      selectionRevision: 1,
      planVersion: 1,
      handlerVersion: 1,
      checkpointVersion: 1,
      expiresAt: '2026-01-01T00:00:00Z',
      recoveryPolicy: 'readOnlyVerify',
    });
    schemaDiffCommands.prepareUnifiedPlan.mockResolvedValue({
      plan: {
        table: 'schema objects',
        tables: [],
        sourceDialect: 'postgresql',
        targetDialect: 'postgresql',
        sameDialect: true,
        statements: [],
        warnings: [],
        requirements: [],
        rollbackCompleteness: { complete: true, missing: [] },
        typeSuggestions: [],
      },
      planId: 'plan-1',
      selectionRevision: 1,
      planVersion: 1,
      handlerVersion: 1,
      checkpointVersion: 1,
      expiresAt: '2026-01-01T00:00:00Z',
      recoveryPolicy: 'readOnlyVerify',
    });
    databaseCommands.listTables.mockResolvedValue([
      { name: 'users', schema: 'public', tableType: 'table' },
    ]);
    databaseCommands.getDatabaseObjects.mockResolvedValue([]);
  });

  afterEach(() => cleanup());

  function chooseProfile(profileName: string) {
    fireEvent.click(screen.getByTestId('schema-diff-profile-select'));
    fireEvent.mouseDown(screen.getByRole('option', { name: profileName }));
  }

  it('[tester] reloads a profile through fresh inspection and preserves options and type overrides', async () => {
    render(<SchemaDiffWindow />);

    await waitFor(() => expect(screen.getByTestId('schema-diff-next')).toBeInTheDocument());
    fireEvent.click(screen.getByTestId('schema-diff-next'));
    await waitFor(() =>
      expect(screen.getByTestId('schema-diff-objects-panel')).toBeInTheDocument(),
    );
    fireEvent.click(screen.getByTestId('schema-diff-next'));
    await waitFor(() => expect(screen.getByTestId('schema-diff-step-compare')).toBeInTheDocument());
    fireEvent.click(screen.getByTestId('schema-diff-next'));
    await waitFor(() => expect(screen.getByTestId('schema-diff-step-plan')).toBeInTheDocument());
    await waitFor(() => expect(schemaDiffCommands.preparePlan).toHaveBeenCalled());

    chooseProfile(profile.name);
    fireEvent.click(screen.getByTestId('schema-diff-profile-load'));

    await waitFor(() =>
      expect(screen.getByTestId('schema-diff-objects-panel')).toBeInTheDocument(),
    );
    expect(databaseCommands.listTables).toHaveBeenCalledWith('source-session', 'profile-db');
    expect(screen.getByTestId('schema-diff-table-row')).toHaveAttribute(
      'data-table-name',
      'public.users',
    );

    fireEvent.click(screen.getByTestId('schema-diff-next'));
    await waitFor(() => expect(screen.getByTestId('schema-diff-step-compare')).toBeInTheDocument());
    fireEvent.click(screen.getByTestId('schema-diff-next'));
    await waitFor(() => expect(screen.getByTestId('schema-diff-step-plan')).toBeInTheDocument());
    await waitFor(() => {
      expect(schemaDiffCommands.preparePlan).toHaveBeenLastCalledWith(
        expect.objectContaining({
          tableNames: ['public.users'],
          allowDestructive: true,
          includeIndexes: false,
          typeOverrides: profile.typeOverrides,
        }),
      );
    });
  });

  it('[tester] restores a target-only profile row and forwards separate selectors', async () => {
    profile.targetOnlyTables = ['public.archive'];
    databaseCommands.listTables.mockImplementation(async (sessionId: string) =>
      sessionId === 'source-session'
        ? [{ name: 'users', schema: 'public', tableType: 'table' }]
        : [
            { name: 'users', schema: 'public', tableType: 'table' },
            { name: 'archive', schema: 'public', tableType: 'table' },
          ],
    );
    render(<SchemaDiffWindow />);

    fireEvent.click(screen.getByTestId('schema-diff-next'));
    await waitFor(() =>
      expect(screen.getByTestId('schema-diff-objects-panel')).toBeInTheDocument(),
    );
    fireEvent.click(screen.getByTestId('schema-diff-next'));
    await waitFor(() => expect(screen.getByTestId('schema-diff-step-compare')).toBeInTheDocument());
    fireEvent.click(screen.getByTestId('schema-diff-next'));
    await waitFor(() => expect(screen.getByTestId('schema-diff-step-plan')).toBeInTheDocument());

    chooseProfile(profile.name);
    fireEvent.click(screen.getByTestId('schema-diff-profile-load'));
    await waitFor(() =>
      expect(screen.getByTestId('schema-diff-objects-panel')).toBeInTheDocument(),
    );

    const archive = screen
      .getAllByTestId('schema-diff-table-row')
      .find((row) => row.getAttribute('data-table-name') === 'public.archive');
    expect(archive).toBeDefined();
    expect(archive).toHaveAttribute('data-table-origin', 'target-only');
    expect(within(archive!).getByRole('checkbox')).toBeChecked();

    fireEvent.click(screen.getByTestId('schema-diff-next'));
    await waitFor(() => expect(screen.getByTestId('schema-diff-step-compare')).toBeInTheDocument());
    expect(schemaDiffCommands.compareTableSchemas).toHaveBeenCalledWith(
      'source-session',
      'target-session',
      'public.users',
      'public.users',
      'public',
      'public',
    );
    expect(schemaDiffCommands.compareTableSchemas).not.toHaveBeenCalledWith(
      'source-session',
      'target-session',
      'public.archive',
    );
    fireEvent.click(screen.getByTestId('schema-diff-next'));
    await waitFor(() =>
      expect(schemaDiffCommands.preparePlan).toHaveBeenLastCalledWith(
        expect.objectContaining({
          tableNames: ['public.users'],
          targetOnlyTableNames: ['public.archive'],
        }),
      ),
    );
  });

  it('saves an object-only profile with exact routine and trigger identities', async () => {
    const sourceRoutine: DatabaseObject = {
      kind: 'function',
      schema: 'public',
      name: 'calculate_total',
      signature: 'integer, numeric',
    };
    const targetTrigger: DatabaseObject = {
      kind: 'trigger',
      schema: 'public',
      name: 'audit_orders',
      targetSchema: 'public',
      targetName: 'orders',
    };
    databaseCommands.listTables.mockResolvedValue([]);
    databaseCommands.getDatabaseObjects.mockImplementation(
      async (sessionId: string, kind: string) =>
        (sessionId === 'source-session' ? [sourceRoutine] : [targetTrigger]).filter(
          (object) => object.kind === kind,
        ),
    );

    render(<SchemaDiffWindow />);
    fireEvent.click(screen.getByTestId('schema-diff-next'));
    await screen.findByTestId('schema-diff-unified-object-picker');
    fireEvent.click(
      within(screen.getByTestId('schema-diff-object-row-source-function-0')).getByRole('checkbox'),
    );
    const targetTriggerGroup = screen.getByTestId('schema-diff-object-kind-target-trigger');
    fireEvent.click(within(targetTriggerGroup).getByText('schemaDiff.objectKind.trigger'));
    fireEvent.click(
      within(screen.getByTestId('schema-diff-object-row-target-trigger-0')).getByRole('checkbox'),
    );
    fireEvent.click(screen.getByTestId('schema-diff-next'));
    await waitFor(() => expect(schemaDiffCommands.prepareUnifiedPlan).toHaveBeenCalled());

    fireEvent.click(screen.getByTestId('schema-diff-profile-save'));
    fireEvent.change(screen.getByTestId('schema-diff-profile-name'), {
      target: { value: 'Objects only' },
    });
    fireEvent.click(screen.getByTestId('schema-diff-profile-save-confirm'));

    await waitFor(() => expect(schemaDiffCommands.saveProfile).toHaveBeenCalled());
    expect(schemaDiffCommands.saveProfile).toHaveBeenCalledWith(
      expect.objectContaining({
        tables: [],
        targetOnlyTables: [],
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

  it('restores only exact saved identities after profile catalogs finish loading', async () => {
    const desiredSource: SchemaDiffObjectIdentity = {
      kind: 'function',
      schema: 'public',
      name: 'calculate_total',
      signature: 'integer, numeric',
      targetSchema: null,
      targetName: null,
    };
    const otherOverload: DatabaseObject = {
      kind: 'function',
      schema: 'public',
      name: 'calculate_total',
      signature: 'text',
    };
    const desiredTarget: SchemaDiffObjectIdentity = {
      kind: 'trigger',
      schema: 'public',
      name: 'audit_orders',
      signature: null,
      targetSchema: 'public',
      targetName: 'orders',
    };
    const otherTrigger: DatabaseObject = {
      kind: 'trigger',
      schema: 'public',
      name: 'audit_orders',
      targetSchema: 'public',
      targetName: 'invoices',
    };
    profile.tables = [];
    profile.sourceObjects = [desiredSource];
    profile.targetObjects = [desiredTarget];
    databaseCommands.getDatabaseObjects.mockImplementation(
      async (sessionId: string, kind: string) =>
        (sessionId === 'source-session'
          ? [otherOverload, { ...desiredSource }]
          : [otherTrigger, { ...desiredTarget }]
        ).filter((object) => object.kind === kind),
    );
    databaseCommands.listTables.mockResolvedValue([]);

    render(<SchemaDiffWindow />);
    fireEvent.click(screen.getByTestId('schema-diff-next'));
    await screen.findByTestId('schema-diff-unified-object-picker');
    fireEvent.click(
      within(screen.getByTestId('schema-diff-object-row-source-function-0')).getByRole('checkbox'),
    );
    fireEvent.click(screen.getByTestId('schema-diff-next'));
    await waitFor(() => expect(schemaDiffCommands.prepareUnifiedPlan).toHaveBeenCalled());
    chooseProfile(profile.name);
    fireEvent.click(screen.getByTestId('schema-diff-profile-load'));

    await waitFor(() =>
      expect(
        within(screen.getByTestId('schema-diff-object-row-source-function-1')).getByRole(
          'checkbox',
        ),
      ).toBeChecked(),
    );
    expect(
      within(screen.getByTestId('schema-diff-object-row-source-function-0')).getByRole('checkbox'),
    ).not.toBeChecked();
    const targetTriggerGroup = screen.getByTestId('schema-diff-object-kind-target-trigger');
    fireEvent.click(within(targetTriggerGroup).getByText('schemaDiff.objectKind.trigger'));
    expect(
      within(screen.getByTestId('schema-diff-object-row-target-trigger-1')).getByRole('checkbox'),
    ).toBeChecked();
    expect(
      within(screen.getByTestId('schema-diff-object-row-target-trigger-0')).getByRole('checkbox'),
    ).not.toBeChecked();
  });

  it('[tester] reports saved object identities missing from the current catalogs', async () => {
    profile.sourceObjects = [
      {
        kind: 'function',
        schema: 'public',
        name: 'calculate_total',
        signature: 'integer',
        targetSchema: null,
        targetName: null,
      },
    ];
    profile.targetObjects = [];
    render(<SchemaDiffWindow />);

    fireEvent.click(screen.getByTestId('schema-diff-next'));
    await screen.findByTestId('schema-diff-objects-panel');
    fireEvent.click(screen.getByTestId('schema-diff-next'));
    await screen.findByTestId('schema-diff-detail-panel');
    fireEvent.click(screen.getByTestId('schema-diff-next'));
    await screen.findByTestId('schema-diff-copy-sql');
    chooseProfile(profile.name);
    fireEvent.click(screen.getByTestId('schema-diff-profile-load'));

    await screen.findByText('schemaDiff.savedObjectsMissing');
    expect(
      screen.queryByTestId('schema-diff-object-row-source-function-0'),
    ).not.toBeInTheDocument();
  });

  it('[tester] closes import and profile dialogs through their cancel actions', async () => {
    render(<SchemaDiffWindow />);
    fireEvent.click(screen.getByTestId('schema-diff-next'));
    await screen.findByTestId('schema-diff-objects-panel');
    fireEvent.click(screen.getByTestId('schema-diff-next'));
    await screen.findByTestId('schema-diff-detail-panel');
    fireEvent.click(screen.getByTestId('schema-diff-next'));
    await screen.findByTestId('schema-diff-copy-sql');

    fireEvent.click(screen.getByTestId('schema-diff-import-config'));
    fireEvent.click(screen.getByRole('button', { name: 'common.cancel' }));
    expect(screen.queryByTestId('schema-diff-import-config-dialog')).not.toBeInTheDocument();

    fireEvent.click(screen.getByTestId('schema-diff-profile-save'));
    fireEvent.click(screen.getByRole('button', { name: 'common.cancel' }));
    expect(screen.queryByTestId('schema-diff-profile-dialog')).not.toBeInTheDocument();
  });
});
