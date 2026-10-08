import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { ConnectionConfig } from '../../../types';
import type { TransferProfile, TransferTableResult } from '../../../commands/transfer';
import { transferCommands } from '../../../commands/transfer';
import type {
  TransferApplyJobView,
  TransferBackendScope,
  TransferPrepareJobRequest,
  TransferPrepareJobView,
} from '../../../commands/transferJobs';
import { clearTransferLimitationsDismissed } from '../../../lib/transferLimitationsPrefs';

/**
 * P5 cutover: the window no longer calls `transferCommands.preview` /
 * `.execute`. It calls `prepare_data_transfer_job` then `apply_data_transfer_job`
 * through `src/commands/transferJobs.ts`, and the Job views — not a
 * `TransferExecutionResult` — are what the result panel renders.
 *
 * `prepareTransferJobMock` is therefore keyed on the *raw Job* the window built
 * (the adapter below unwraps `request.job`), so every assertion about what the
 * mapping step sent still reads exactly as it did before the cutover.
 */
const {
  invokeMock,
  inspectTransferMock,
  inspectSqlFileTransferMock,
  prepareTransferJobMock,
  prepareKeys,
  applyTransferJobMock,
  cancelTransferJobMock,
  getDatabasesMock,
  stableT,
  urlParamMock,
  crossWindowHandlers,
} = vi.hoisted(() => {
  const stableT = (key: string, params?: Record<string, string | number>) =>
    params ? `${key}:${JSON.stringify(params)}` : key;
  return {
    invokeMock: vi.fn(),
    inspectTransferMock: vi.fn(),
    inspectSqlFileTransferMock: vi.fn(),
    prepareTransferJobMock: vi.fn(),
    prepareKeys: [] as (string | undefined)[],
    applyTransferJobMock: vi.fn(),
    cancelTransferJobMock: vi.fn(),
    getDatabasesMock: vi.fn(),
    stableT,
    urlParamMock: vi.fn<(name: string) => string | null>(),
    crossWindowHandlers: new Map<string, (payload?: unknown) => void>(),
  };
});

vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}));

vi.mock('../../../hooks/useSettings', () => ({
  useSettings: () => undefined,
}));

vi.mock('../../../stores/settingsStore', () => ({
  useSettingsStore: (sel: (s: { loadSettings: () => void }) => unknown) =>
    sel({ loadSettings: vi.fn() }),
}));

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: stableT }),
}));

vi.mock('../../../hooks/useLocaleDomains', () => ({
  useLocaleDomains: () => true,
}));

vi.mock('../../../lib/windowKind', () => ({
  getUrlParam: (name: string) => urlParamMock(name),
}));

vi.mock('../../../lib/crossWindowBus', () => ({
  listenCrossWindow: vi.fn(async (event: string, handler: (payload?: unknown) => void) => {
    crossWindowHandlers.set(event, handler);
    return () => crossWindowHandlers.delete(event);
  }),
}));

vi.mock('../../../commands/database', () => ({
  databaseCommands: {
    getDatabases: (...args: unknown[]) => getDatabasesMock(...args),
  },
}));

vi.mock('../../../commands/transfer', () => ({
  DEFAULT_TRANSFER_OPTIONS: {
    batchSize: 500,
    stopOnError: true,
    confirmedDestructive: false,
    useTargetDefaultCollation: false,
  },
  transferCommands: {
    getProfiles: vi.fn().mockResolvedValue([]),
    saveProfile: vi.fn().mockResolvedValue(undefined),
    deleteProfile: vi.fn().mockResolvedValue(undefined),
    pickSqlFile: vi.fn().mockResolvedValue({ fileToken: 'sql-file-token' }),
    inspect: (...args: unknown[]) => inspectTransferMock(...args),
    inspectSqlFile: (...args: unknown[]) => inspectSqlFileTransferMock(...args),
    classifyPair: vi.fn(),
  },
}));

vi.mock('../../../commands/transferJobs', () => ({
  localBackendScope: (): TransferBackendScope => ({
    sourceBackendScope: 'local-desktop-backend',
    targetBackendScope: 'local-desktop-backend',
    profileBackendScopes: [],
  }),
  transferJobCommands: {
    // The window sends `{ job, backendScope, idempotencyKey? }`; the mock is
    // keyed on the job itself so job-payload assertions stay readable, while
    // the key is captured separately for the receipt-safety assertions.
    prepare: async (request: TransferPrepareJobRequest) => {
      prepareKeys.push(request.idempotencyKey);
      return toPrepareView(await prepareTransferJobMock(request.job));
    },
    apply: (...args: unknown[]) => applyTransferJobMock(...args),
    cancel: (...args: unknown[]) => cancelTransferJobMock(...args),
  },
}));

vi.mock('../../../components/TitleBar', () => ({
  TitleBar: ({ title }: { title?: unknown }) => <div>{String(title ?? '')}</div>,
}));

vi.mock('../../../components/StatusBar', () => ({
  StatusBar: () => <div data-testid="status-bar" />,
}));

vi.mock('../../../components/SqlCodeBlock', () => ({
  SqlCodeBlock: ({ code, onChange }: { code: string; onChange?: (value: string) => void }) => (
    <>
      <pre data-testid="sql-code-block">{code}</pre>
      <button data-testid="sql-code-change" onClick={() => onChange?.('CREATE TABLE edited')}>
        edit
      </button>
    </>
  ),
}));

const pgSrc: ConnectionConfig = {
  id: 'pg-src',
  name: 'PG Src',
  databaseType: 'postgresql',
  host: '127.0.0.1',
  port: 5432,
  database: 'src',
  username: 'postgres',
  password: '',
  sslMode: 'disable',
};

const pgTgt: ConnectionConfig = {
  id: 'pg-tgt',
  name: 'PG Tgt',
  databaseType: 'postgresql',
  host: '127.0.0.1',
  port: 5432,
  database: 'tgt',
  username: 'postgres',
  password: '',
  sslMode: 'disable',
};

const unsupportedTgt: ConnectionConfig = {
  ...pgTgt,
  id: 'unsupported-tgt',
  name: 'Unsupported Tgt',
  databaseType: 'kiwi' as unknown as ConnectionConfig['databaseType'],
};

const inspectRows: TransferTableResult[] = [
  {
    sourceTable: 'users',
    targetTable: 'users_v2',
    status: 'MATCHED',
    createNew: false,
    enabled: true,
    sourceColumns: ['id', 'name', 'extra'],
    sourcePrimaryKeys: ['id'],
    sourceColumnTypes: { id: 'INTEGER', name: 'TEXT', extra: 'TEXT' },
    targetColumns: ['id', 'name', 'email'],
    columnMappings: [
      { sourceColumn: 'id', targetColumn: 'id', skip: false },
      { sourceColumn: 'name', targetColumn: 'name', skip: false },
      { sourceColumn: 'extra', targetColumn: '', skip: true },
    ],
  },
];

const sqlInspectRows: TransferTableResult[] = inspectRows.map((row) => ({
  ...row,
  status: 'CREATE_NEW',
  createNew: true,
  targetColumns: [],
}));

/**
 * The verdict inspect actually returns for a source table that has no
 * counterpart at the target, in Structure / StructureAndData, on a first run
 * with nothing saved.
 *
 * This is a transcript, not a sketch. It was produced by running the real
 * `datazen_data_transfer::mapping::inspect_tables` — `cargo test -p
 * datazen-data-transfer --lib mapping::` — with one assertion added per run,
 * both runs `EXIT=0`, `5 passed; 0 failed`:
 *
 *   assert_eq!(results[0].target_table, "new_table");
 *   assert_eq!(results[0].target_table, results[0].source_table);
 *
 * Both held. `effective_table_mappings` (packages/data-transfer/src/mapping.rs)
 * built the row with `target_table: t.name.clone(), create_new: true`, and the
 * `!mapping.enabled` branch propagated both verbatim, so the source name came
 * back as the target name. That broke the rule that a create-new row must be
 * named by the user, and — because the wire format cannot mark a name as
 * suggested rather than confirmed — it silently satisfied the mapping gate, so
 * an unnamed create-new row prepared as `targetTable: <source name>` without
 * the user typing anything.
 *
 * Fixed: the auto-build now returns `String::new()`, matching what Data mode
 * always returned for this same case, and `inspect_tables` propagates the empty
 * name verbatim to the row below. The Rust side pins that in
 * `structure_mode_marks_missing_target_as_create_new` and
 * `disabled_create_new_rows_keep_all_source_columns_for_explicit_selection`;
 * this fixture is the frontend half of the same contract.
 */
const structureCreateNewRow: TransferTableResult = {
  sourceTable: 'new_table',
  targetTable: '',
  status: 'DISABLED',
  createNew: true,
  enabled: false,
  sourceColumns: ['id', 'name'],
  sourcePrimaryKeys: ['id'],
  sourceColumnTypes: { id: 'INTEGER', name: 'TEXT' },
  targetColumns: [],
  columnMappings: [
    { sourceColumn: 'id', targetColumn: 'id', skip: false },
    { sourceColumn: 'name', targetColumn: 'name', skip: false },
  ],
};

const previewSuccess = {
  planId: 'plan-test-1',
  canExecute: true,
  ddl: [],
  writePlans: [
    {
      sourceTable: 'users',
      targetTable: 'users',
      writeMode: 'truncateInsert',
      mappedColumns: [],
      preamble: [],
      estimatedRows: 3,
    },
  ],
  warnings: [],
  pairingPath: 'direct',
  mode: 'data',
  writeMode: 'truncateInsert',
};

type ReviewShape = typeof previewSuccess & Record<string, unknown>;

/**
 * Wrap a review payload in the FrozenPlan envelope `prepare_data_transfer_job`
 * returns. The review keeps the `planId`/`canExecute`/`writePlans` the preview
 * UI has always consumed; the envelope carries the `planId` the *apply* call
 * spends.
 */
function toPrepareView(
  review: ReviewShape,
  overrides: Partial<TransferPrepareJobView> = {},
): TransferPrepareJobView {
  return {
    jobId: 'transfer-data-prepare-1',
    kind: 'data',
    state: 'prepared',
    effectOutcome: 'notStarted',
    planId: 'plan-job-1',
    planDigest: 'digest-job-1',
    planVersion: 1,
    handlerVersion: 1,
    checkpointVersion: 0,
    selectionRevision: 1,
    expiresAt: null,
    canExecute: review.canExecute,
    blockReason: null,
    review,
    ...overrides,
  } as TransferPrepareJobView;
}

/** Terminal success Job: everything committed, every boundary verified. */
/**
 * A fully certified terminal run: the run recorded one boundary, it carries
 * `EVIDENCE_*` marker, and every row is accounted for. This is the *only*
 * shape that may render as `migration.verdict.ok` — the reason each other test
 * below deliberately degrades one of those three facts.
 */
const applySuccess: TransferApplyJobView = {
  jobId: 'transfer-data-apply-1',
  kind: 'data',
  state: 'succeeded',
  effectOutcome: 'completed',
  progress: {
    read: 3,
    converted: 3,
    attempted: 3,
    committed: 3,
    unknown: 0,
  },
  planId: 'plan-job-1',
  planDigest: 'digest-job-1',
  selectionRevision: 1,
  commitBoundaries: [
    {
      stageId: 'users',
      stableTargetFingerprint: 'fp-users',
      committedAt: 1_700_000_000_000,
      operationId: 'op-1',
      batchId: 'batch-1',
      payloadDigest: 'pd-1',
      evidence: ['EVIDENCE_ROWS_COMMITTED'],
      verifiedAt: 1_700_000_001_000,
    },
  ],
  artifactIds: [],
  cancelled: false,
  partial: false,
  replayed: false,
  error: null,
  recoveryVerdict: null,
  recoveryResumeThrough: null,
  recoveryReason: null,
  createdAt: 1_700_000_000_000,
  updatedAt: 1_700_000_001_000,
} as unknown as TransferApplyJobView;

async function advanceToObjectsStep(emptyTables = false) {
  const { DataTransferWindow } = await import('../DataTransferWindow');
  render(<DataTransferWindow />);

  await waitFor(() => expect(invokeMock).toHaveBeenCalledWith('get_connections'));
  await dismissLimitationsDialog();

  await pickSelect('data-transfer-source', 'PG Src (postgresql)');
  await pickSelect('data-transfer-target', 'PG Tgt (postgresql)');
  await waitFor(() => expect(getDatabasesMock).toHaveBeenCalled());

  fireEvent.click(screen.getByTestId('data-transfer-next'));
  await waitFor(() => expect(screen.getByTestId('data-transfer-mode-data')).toBeTruthy());
  fireEvent.click(screen.getByTestId('data-transfer-mode-data'));

  inspectTransferMock.mockResolvedValue(emptyTables ? [] : inspectRows);
  fireEvent.click(screen.getByTestId('data-transfer-next'));
  await waitFor(() => expect(inspectTransferMock).toHaveBeenCalled());

  if (emptyTables) {
    await waitFor(() => expect(screen.getByTestId('data-transfer-objects-empty')).toBeTruthy());
  } else {
    await waitFor(() => expect(screen.getByTestId('data-transfer-table-row')).toBeTruthy());
  }
}

async function advanceToSetupStep() {
  const { DataTransferWindow } = await import('../DataTransferWindow');
  render(<DataTransferWindow />);
  await waitFor(() => expect(invokeMock).toHaveBeenCalledWith('get_connections'));
  await dismissLimitationsDialog();
  await pickSelect('data-transfer-source', 'PG Src (postgresql)');
  await pickSelect('data-transfer-target', 'PG Tgt (postgresql)');
  await waitFor(() => expect(getDatabasesMock).toHaveBeenCalled());
  fireEvent.click(screen.getByTestId('data-transfer-next'));
  await waitFor(() => expect(screen.getByTestId('data-transfer-mode-data')).toBeTruthy());
}

async function pickSelect(testId: string, optionLabel: string) {
  const wrap = screen.getByTestId(testId);
  const trigger = wrap.matches('button') ? wrap : within(wrap).getAllByRole('button')[0];
  fireEvent.click(trigger);
  const list = await waitFor(() => {
    return screen.getByRole('listbox');
  });
  const option = Array.from(list.children).find((el) =>
    (el.textContent || '').includes(optionLabel),
  );
  expect(option, `option ${optionLabel}`).toBeTruthy();
  fireEvent.mouseDown(option!);
}

async function advanceToSqlFilePreview(
  mode: 'data' | 'structure' = 'data',
  stopAt: 'objects' | 'mapping' | 'preview' = 'preview',
  inspectedRows: TransferTableResult[] = sqlInspectRows,
) {
  const { DataTransferWindow } = await import('../DataTransferWindow');
  render(<DataTransferWindow />);
  await waitFor(() => expect(invokeMock).toHaveBeenCalledWith('get_connections'));
  await dismissLimitationsDialog();

  fireEvent.click(screen.getByTestId('data-transfer-destination-sql-file'));
  await waitFor(() =>
    expect(screen.getByTestId('data-transfer-destination-sql-file')).toHaveTextContent(
      'transfer.destination.sqlFileSelected',
    ),
  );
  await pickSelect('data-transfer-source', 'PG Src (postgresql)');
  await waitFor(() => expect(getDatabasesMock).toHaveBeenCalled());
  await pickSelect('data-transfer-source-database', 'src');

  fireEvent.click(screen.getByTestId('data-transfer-next'));
  await waitFor(() => expect(screen.getByTestId('data-transfer-mode-data')).toBeTruthy());
  if (mode === 'structure') {
    fireEvent.click(screen.getByTestId('data-transfer-mode-structure'));
  }
  inspectSqlFileTransferMock.mockResolvedValue(inspectedRows);
  fireEvent.click(screen.getByTestId('data-transfer-next'));
  await waitFor(() => expect(inspectSqlFileTransferMock).toHaveBeenCalled());
  await waitFor(() =>
    expect(screen.getAllByTestId('data-transfer-table-row').length).toBeGreaterThan(0),
  );
  await waitFor(() => expect(screen.getByTestId('data-transfer-next')).not.toBeDisabled());
  if (stopAt === 'objects') return;
  fireEvent.click(screen.getByTestId('data-transfer-next'));
  await waitFor(() => expect(screen.getByTestId('data-transfer-mapping-step')).toBeTruthy());
  if (stopAt === 'mapping') return;
  fireEvent.click(screen.getByTestId('data-transfer-next'));
  await waitFor(() => expect(screen.getByTestId('data-transfer-preview')).toBeTruthy());
}

async function dismissLimitationsDialog() {
  await waitFor(() => {
    expect(screen.getByTestId('data-transfer-limitations')).toBeTruthy();
  });
  fireEvent.click(screen.getByTestId('data-transfer-limitations-close'));
  await waitFor(() => {
    expect(screen.queryByTestId('data-transfer-limitations')).toBeNull();
  });
}

async function advanceToMappingStep(
  inspectedRows = inspectRows,
  stopAt: 'objects' | 'mapping' = 'mapping',
) {
  const { DataTransferWindow } = await import('../DataTransferWindow');
  render(<DataTransferWindow />);

  await waitFor(() => expect(invokeMock).toHaveBeenCalledWith('get_connections'));

  await dismissLimitationsDialog();

  await pickSelect('data-transfer-source', 'PG Src (postgresql)');
  await pickSelect('data-transfer-target', 'PG Tgt (postgresql)');

  await waitFor(() => expect(getDatabasesMock).toHaveBeenCalled());

  // endpoints → setup
  fireEvent.click(screen.getByTestId('data-transfer-next'));
  await waitFor(() => expect(screen.getByTestId('data-transfer-mode-data')).toBeTruthy());

  fireEvent.click(screen.getByTestId('data-transfer-mode-data'));

  inspectTransferMock.mockResolvedValue(inspectedRows);
  // setup → objects
  fireEvent.click(screen.getByTestId('data-transfer-next'));

  await waitFor(() => expect(inspectTransferMock).toHaveBeenCalled());
  // inspect may list any number of tables, so the objects gate is checked for
  // existence rather than uniqueness.
  await waitFor(() =>
    expect(screen.getAllByTestId('data-transfer-table-row').length).toBeGreaterThan(0),
  );
  if (stopAt === 'objects') return;

  // objects → mapping
  fireEvent.click(screen.getByTestId('data-transfer-next'));

  await waitFor(() => expect(screen.getByTestId('data-transfer-mapping-step')).toBeTruthy());
}

async function advanceToPreviewStep(writeMode: 'insert' | 'truncateInsert' = 'truncateInsert') {
  const { DataTransferWindow } = await import('../DataTransferWindow');
  render(<DataTransferWindow />);

  await waitFor(() => expect(invokeMock).toHaveBeenCalledWith('get_connections'));
  await dismissLimitationsDialog();

  await pickSelect('data-transfer-source', 'PG Src (postgresql)');
  await pickSelect('data-transfer-target', 'PG Tgt (postgresql)');
  await waitFor(() => expect(getDatabasesMock).toHaveBeenCalled());

  fireEvent.click(screen.getByTestId('data-transfer-next'));
  await waitFor(() => expect(screen.getByTestId('data-transfer-mode-data')).toBeTruthy());
  fireEvent.click(screen.getByTestId('data-transfer-mode-data'));

  if (writeMode !== 'insert') {
    await waitFor(() => expect(screen.getByTestId('data-transfer-write-mode')).toBeTruthy());
    await pickSelect('data-transfer-write-mode', 'transfer.writeMode.truncateInsert');
    fireEvent.click(screen.getByTestId('data-transfer-destructive-confirm'));
  }

  inspectTransferMock.mockResolvedValue(inspectRows);
  fireEvent.click(screen.getByTestId('data-transfer-next'));
  await waitFor(() => expect(screen.getByTestId('data-transfer-table-row')).toBeTruthy());
  fireEvent.click(screen.getByTestId('data-transfer-next'));
  await waitFor(() => expect(screen.getByTestId('data-transfer-mapping-step')).toBeTruthy());
  fireEvent.click(screen.getByTestId('data-transfer-next'));
  await waitFor(() => expect(screen.getByTestId('data-transfer-preview')).toBeTruthy());
}

describe('DataTransferWindow', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(transferCommands.getProfiles).mockResolvedValue([]);
    crossWindowHandlers.clear();
    clearTransferLimitationsDismissed();
    urlParamMock.mockReset();
    urlParamMock.mockReturnValue(null);
    getDatabasesMock.mockResolvedValue(['src', 'tgt']);
    prepareTransferJobMock.mockReset();
    prepareTransferJobMock.mockResolvedValue(previewSuccess);
    prepareKeys.length = 0;
    applyTransferJobMock.mockReset();
    applyTransferJobMock.mockResolvedValue(applySuccess);
    cancelTransferJobMock.mockReset();
    cancelTransferJobMock.mockResolvedValue(true);
    Object.defineProperty(navigator, 'clipboard', {
      configurable: true,
      value: { writeText: vi.fn().mockResolvedValue(undefined) },
    });
    invokeMock.mockImplementation(async (cmd: string, args?: Record<string, unknown>) => {
      if (cmd === 'get_connections') return [pgSrc, pgTgt];
      if (cmd === 'get_available_drivers') return ['postgresql', 'mysql', 'redis'];
      if (cmd === 'connect_dedicated') {
        const conn = args?.connectionId as string;
        const db = (args?.database as string | null | undefined) ?? 'default';
        return `dedicated-${conn}-${db}`;
      }
      if (cmd === 'release_connection') return false;
      if (cmd === 'connect') return `live-${args?.connectionId as string}`;
      return null;
    });
  });

  afterEach(() => {
    cleanup();
  });

  it('[tester] prefills connection endpoints from URL params', async () => {
    urlParamMock.mockImplementation((name) => {
      const params: Record<string, string> = {
        sourceId: 'pg-src',
        targetId: 'pg-tgt',
      };
      return params[name] ?? null;
    });
    const { DataTransferWindow } = await import('../DataTransferWindow');
    render(<DataTransferWindow />);
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith('get_connections'));
    await dismissLimitationsDialog();
    await waitFor(() =>
      expect(screen.getByTestId('data-transfer-source')).toHaveTextContent('PG Src'),
    );
    await waitFor(() =>
      expect(screen.getByTestId('data-transfer-target')).toHaveTextContent('PG Tgt'),
    );
  });

  it('[tester] reports missing endpoints when saving a named profile', async () => {
    const { DataTransferWindow } = await import('../DataTransferWindow');
    render(<DataTransferWindow />);
    await dismissLimitationsDialog();

    fireEvent.change(screen.getByTestId('data-transfer-profile-name'), {
      target: { value: 'Needs endpoints' },
    });
    fireEvent.click(screen.getByTestId('data-transfer-profile-save'));

    await waitFor(() =>
      expect(screen.getByTestId('data-transfer-error')).toHaveTextContent(
        'transfer.profile.missingFields',
      ),
    );
    expect(transferCommands.saveProfile).not.toHaveBeenCalled();
  });

  it('[tester] marks unsupported target families and explains the pairing', async () => {
    urlParamMock.mockImplementation((name) => {
      const params: Record<string, string> = {
        sourceId: 'pg-src',
        targetId: 'unsupported-tgt',
      };
      return params[name] ?? null;
    });
    invokeMock.mockImplementation(async (cmd: string, args?: Record<string, unknown>) => {
      if (cmd === 'get_connections') return [pgSrc, unsupportedTgt];
      if (cmd === 'connect_dedicated') {
        return `dedicated-${args?.connectionId as string}-${String(args?.database ?? 'default')}`;
      }
      if (cmd === 'release_connection') return false;
      return null;
    });

    const { DataTransferWindow } = await import('../DataTransferWindow');
    render(<DataTransferWindow />);
    await dismissLimitationsDialog();
    await waitFor(() => expect(screen.getByTestId('data-transfer-path')).toBeTruthy());

    fireEvent.click(within(screen.getByTestId('data-transfer-target')).getAllByRole('button')[0]);
    const options = await waitFor(() => screen.getAllByTestId('select-option'));
    expect(
      options.some(
        (option) =>
          option.getAttribute('aria-disabled') === 'true' && Boolean(option.getAttribute('title')),
      ),
    ).toBe(true);
    expect(screen.getByTestId('data-transfer-next')).toBeDisabled();
  });

  it('disables Redis to Redis before opening the transfer setup', async () => {
    const redisSrc: ConnectionConfig = {
      ...pgSrc,
      id: 'redis-src',
      name: 'Redis Src',
      databaseType: 'redis',
    };
    const redisTgt: ConnectionConfig = {
      ...pgTgt,
      id: 'redis-tgt',
      name: 'Redis Tgt',
      databaseType: 'redis',
    };
    urlParamMock.mockImplementation((name) => {
      const params: Record<string, string> = { sourceId: 'redis-src', targetId: 'redis-tgt' };
      return params[name] ?? null;
    });
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === 'get_connections') return [redisSrc, redisTgt];
      return null;
    });
    const { DataTransferWindow } = await import('../DataTransferWindow');
    render(<DataTransferWindow />);
    await dismissLimitationsDialog();
    await waitFor(() => expect(screen.getByTestId('data-transfer-path')).toBeTruthy());

    expect(screen.getByTestId('data-transfer-next')).toBeDisabled();
    expect(screen.getByTestId('data-transfer-path')).toHaveTextContent('adapters');
    fireEvent.click(within(screen.getByTestId('data-transfer-target')).getAllByRole('button')[0]);
    const options = await waitFor(() => screen.getAllByTestId('select-option'));
    expect(options.some((option) => option.getAttribute('aria-disabled') === 'true')).toBe(true);
  });

  it('[tester] drops only the dedicated side reported closed by the event bus', async () => {
    await advanceToSetupStep();
    const handler = crossWindowHandlers.get('datazen:connection-closed');
    expect(handler).toBeTruthy();

    handler?.();
    handler?.({ dbSessionId: 'dedicated-pg-src-src' });
    handler?.({ dbSessionId: 'dedicated-pg-tgt-tgt' });

    fireEvent.click(screen.getByTestId('data-transfer-mode-data'));
    expect(screen.getByTestId('data-transfer-mode-data')).toBeChecked();
  });

  it('renders wizard shell', async () => {
    const { DataTransferWindow } = await import('../DataTransferWindow');
    render(<DataTransferWindow />);
    expect(screen.getByTestId('data-transfer-window')).toBeTruthy();
    expect(screen.getByTestId('data-transfer-step-endpoints')).toBeTruthy();
    expect(screen.getByTestId('data-transfer-step-setup')).toBeTruthy();
    expect(screen.getByTestId('data-transfer-source')).toBeTruthy();
    expect(screen.getByTestId('data-transfer-target')).toBeTruthy();
  });

  it('loads a saved profile without restoring a runtime SQL-file token', async () => {
    const profile: TransferProfile = {
      version: 1,
      id: 'profile-1',
      name: 'Nightly export',
      sourceConnectionId: 'pg-src',
      targetConnectionId: null,
      sourceDatabase: 'src',
      targetDatabase: null,
      sourceSchema: null,
      targetSchema: null,
      destinationMode: 'sqlFile',
      sqlFileDialect: 'mysql',
      sqlFileEncoding: 'utf8Bom',
      sqlFileCompression: 'gzip',
      sqlFileDatabase: 'analytics',
      sqlFileSchema: null,
      mode: 'data',
      writeMode: 'insert',
      tables: [],
      options: {
        batchSize: 500,
        stopOnError: true,
        confirmedDestructive: false,
        useTargetDefaultCollation: false,
      },
      createdAt: '2026-09-21T00:00:00.000Z',
      updatedAt: '2026-09-21T00:00:00.000Z',
    };
    vi.mocked(transferCommands.getProfiles).mockResolvedValue([profile]);
    const { DataTransferWindow } = await import('../DataTransferWindow');
    render(<DataTransferWindow />);
    await waitFor(() => expect(screen.getByTestId('data-transfer-profile-select')).toBeTruthy());
    await pickSelect('data-transfer-profile-select', 'Nightly export');
    fireEvent.click(screen.getByTestId('data-transfer-profile-load'));
    await waitFor(() =>
      expect(screen.getByTestId('data-transfer-sql-file-dialect')).toHaveTextContent(/mysql/i),
    );
    expect(screen.getByTestId('data-transfer-sql-file-compression')).toHaveTextContent(
      'transfer.destination.sqlCompressionGzip',
    );
    expect(screen.getByText('transfer.profile.chooseFile')).toBeTruthy();
  });

  it('[tester] updates an existing saved profile without replacing its identity', async () => {
    const profile: TransferProfile = {
      version: 1,
      id: 'profile-update',
      name: 'Nightly export',
      sourceConnectionId: 'pg-src',
      targetConnectionId: null,
      sourceDatabase: 'src',
      targetDatabase: null,
      sourceSchema: null,
      targetSchema: null,
      destinationMode: 'sqlFile',
      sqlFileDialect: 'mysql',
      sqlFileEncoding: 'utf8Bom',
      sqlFileCompression: 'gzip',
      sqlFileDatabase: 'analytics',
      sqlFileSchema: null,
      mode: 'data',
      writeMode: 'insert',
      tables: [],
      options: {
        batchSize: 500,
        stopOnError: true,
        confirmedDestructive: false,
        useTargetDefaultCollation: false,
      },
      createdAt: '2026-09-21T00:00:00.000Z',
      updatedAt: '2026-09-21T00:00:00.000Z',
    };
    vi.mocked(transferCommands.getProfiles).mockResolvedValue([profile]);
    const { DataTransferWindow } = await import('../DataTransferWindow');
    render(<DataTransferWindow />);
    await dismissLimitationsDialog();
    await waitFor(() => expect(screen.getByTestId('data-transfer-profile-select')).toBeTruthy());
    await pickSelect('data-transfer-profile-select', 'Nightly export');
    fireEvent.click(screen.getByTestId('data-transfer-profile-load'));
    await waitFor(() => expect(screen.getByText('transfer.profile.chooseFile')).toBeTruthy());
    fireEvent.click(screen.getByRole('button', { name: 'common.ok' }));
    await waitFor(() => expect(screen.queryByTestId('data-transfer-error')).toBeNull());
    fireEvent.click(screen.getByTestId('data-transfer-destination-sql-file'));
    await waitFor(() =>
      expect(screen.getByTestId('data-transfer-destination-sql-file')).toHaveTextContent(
        'transfer.destination.sqlFileSelected',
      ),
    );
    fireEvent.click(screen.getByTestId('data-transfer-profile-save'));

    await waitFor(() => expect(transferCommands.saveProfile).toHaveBeenCalledTimes(1));
    expect(transferCommands.saveProfile).toHaveBeenCalledWith(
      expect.objectContaining({
        id: 'profile-update',
        name: 'Nightly export',
        createdAt: '2026-09-21T00:00:00.000Z',
        destinationMode: 'sqlFile',
        sqlFileDatabase: 'analytics',
        sqlFileSchema: null,
      }),
    );
  });

  it('opens limitations dialog on first visit', async () => {
    const { DataTransferWindow } = await import('../DataTransferWindow');
    render(<DataTransferWindow />);

    await waitFor(() => {
      expect(screen.getByTestId('data-transfer-limitations')).toBeTruthy();
      expect(screen.getByTestId('data-transfer-limitations-close')).toBeTruthy();
    });
  });

  it('does not reopen limitations dialog after dontShowAgain is checked', async () => {
    const { DataTransferWindow } = await import('../DataTransferWindow');
    const { unmount } = render(<DataTransferWindow />);

    await waitFor(() => {
      expect(screen.getByTestId('data-transfer-limitations')).toBeTruthy();
    });

    fireEvent.click(screen.getByTestId('data-transfer-limitations-dismiss'));
    fireEvent.click(screen.getByTestId('data-transfer-limitations-close'));

    await waitFor(() => {
      expect(screen.queryByTestId('data-transfer-limitations')).toBeNull();
    });

    unmount();
    cleanup();

    render(<DataTransferWindow />);
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith('get_connections'));

    expect(screen.queryByTestId('data-transfer-limitations')).toBeNull();
  });

  it('allows editing column mappings on mapping step', async () => {
    await advanceToMappingStep();

    expect(screen.getByTestId('data-transfer-column-editor')).toBeTruthy();
    expect(screen.getAllByTestId('data-transfer-column-row')).toHaveLength(3);

    fireEvent.click(screen.getByTestId('data-transfer-auto-match'));
    expect(screen.getByTestId('data-transfer-skip-extra')).toBeChecked();
    expect(screen.getByTestId('data-transfer-unmapped-target-warning')).toBeTruthy();

    const emailRow = screen
      .getAllByTestId('data-transfer-column-row')
      .find((row) => within(row).queryByText('extra'));
    expect(emailRow).toBeTruthy();

    const selectWrap = within(emailRow!).getByTestId('data-transfer-target-select-extra');
    const trigger = within(selectWrap).getAllByRole('button')[0];
    fireEvent.click(trigger);
    const list = await waitFor(() => screen.getByRole('listbox'));
    const emailOption = Array.from(list.children).find((el) =>
      (el.textContent || '').includes('email'),
    );
    fireEvent.mouseDown(emailOption!);

    expect(screen.getByTestId('data-transfer-skip-extra')).not.toBeChecked();
    expect(screen.queryByTestId('data-transfer-unmapped-target-warning')).toBeNull();
  });

  it('keeps recordset editing parameterized and invalidates the old preview before re-preview', async () => {
    await advanceToMappingStep();

    fireEvent.click(screen.getByTestId('data-transfer-recordset-enable'));
    expect(screen.queryByTestId('data-transfer-recordset-order-error')).toBeNull();
    fireEvent.change(screen.getByTestId('data-transfer-recordset-start'), {
      target: { value: '10' },
    });
    fireEvent.change(screen.getByTestId('data-transfer-recordset-end'), {
      target: { value: '20' },
    });
    fireEvent.change(screen.getByTestId('data-transfer-recordset-limit'), {
      target: { value: '5' },
    });

    prepareTransferJobMock.mockResolvedValueOnce({
      ...previewSuccess,
      writePlans: [
        {
          ...previewSuccess.writePlans[0],
          recordsetPreview: 'ORDER BY "id" ASC LIMIT $3',
        },
      ],
    });
    fireEvent.click(screen.getByTestId('data-transfer-next'));
    await waitFor(() => expect(screen.getByTestId('data-transfer-preview')).toBeTruthy());
    expect(prepareTransferJobMock).toHaveBeenCalledWith(
      expect.objectContaining({
        tables: [
          expect.objectContaining({
            recordset: {
              orderBy: 'id',
              start: { value: '10', inclusive: true },
              end: { value: '20', inclusive: true },
              limit: 5,
            },
          }),
        ],
      }),
    );
    expect(screen.getByText('ORDER BY "id" ASC LIMIT $3')).toBeTruthy();

    fireEvent.click(screen.getByRole('button', { name: /transfer.back/i }));
    await waitFor(() => expect(screen.getByTestId('data-transfer-mapping-step')).toBeTruthy());
    fireEvent.change(screen.getByTestId('data-transfer-recordset-start'), {
      target: { value: '11' },
    });
    fireEvent.click(screen.getByTestId('data-transfer-next'));
    await waitFor(() => expect(screen.getByTestId('data-transfer-preview')).toBeTruthy());
    expect(prepareTransferJobMock).toHaveBeenCalledTimes(2);
  });

  it('sends complete composite primary-key tuple bounds in declared order', async () => {
    const compositeRows: TransferTableResult[] = [
      {
        ...inspectRows[0],
        sourceColumns: ['tenant', 'sequence', 'payload'],
        sourcePrimaryKeys: ['tenant', 'sequence'],
        sourceColumnTypes: { tenant: 'TEXT', sequence: 'BIGINT', payload: 'TEXT' },
        columnMappings: [
          { sourceColumn: 'tenant', targetColumn: 'tenant', skip: false },
          { sourceColumn: 'sequence', targetColumn: 'sequence', skip: false },
          { sourceColumn: 'payload', targetColumn: 'payload', skip: false },
        ],
      },
    ];
    await advanceToMappingStep(compositeRows);

    fireEvent.click(screen.getByTestId('data-transfer-recordset-enable'));
    expect(screen.getByTestId('data-transfer-recordset-tuple-editor')).toBeTruthy();
    expect(screen.getByTestId('data-transfer-recordset-tuple-collation-hint')).toBeTruthy();
    fireEvent.change(screen.getByTestId('data-transfer-recordset-tuple-start-0'), {
      target: { value: 'a-雪' },
    });
    fireEvent.change(screen.getByTestId('data-transfer-recordset-tuple-start-1'), {
      target: { value: '-3' },
    });
    fireEvent.click(screen.getByTestId('data-transfer-recordset-tuple-start-inclusive'));
    fireEvent.change(screen.getByTestId('data-transfer-recordset-tuple-end-0'), {
      target: { value: 'a-雪' },
    });
    fireEvent.change(screen.getByTestId('data-transfer-recordset-tuple-end-1'), {
      target: { value: '9223372036854775806' },
    });

    fireEvent.click(screen.getByTestId('data-transfer-next'));
    await waitFor(() => expect(screen.getByTestId('data-transfer-preview')).toBeTruthy());
    expect(prepareTransferJobMock).toHaveBeenCalledWith(
      expect.objectContaining({
        tables: [
          expect.objectContaining({
            recordset: {
              tupleRange: {
                columns: ['tenant', 'sequence'],
                start: { values: ['a-雪', '-3'], inclusive: false },
                end: { values: ['a-雪', '9223372036854775806'], inclusive: true },
              },
            },
          }),
        ],
      }),
    );
  });

  it('[tester] clears an empty tuple bound while preserving the opposite endpoint', async () => {
    const compositeRows: TransferTableResult[] = [
      {
        ...inspectRows[0],
        sourceColumns: ['tenant', 'sequence', 'payload'],
        sourcePrimaryKeys: ['tenant', 'sequence'],
        sourceColumnTypes: { tenant: 'TEXT', sequence: 'BIGINT', payload: 'TEXT' },
        columnMappings: [
          { sourceColumn: 'tenant', targetColumn: 'tenant', skip: false },
          { sourceColumn: 'sequence', targetColumn: 'sequence', skip: false },
          { sourceColumn: 'payload', targetColumn: 'payload', skip: false },
        ],
      },
    ];
    await advanceToMappingStep(compositeRows);

    fireEvent.click(screen.getByTestId('data-transfer-recordset-enable'));
    fireEvent.change(screen.getByTestId('data-transfer-recordset-tuple-start-0'), {
      target: { value: 'a-雪' },
    });
    fireEvent.change(screen.getByTestId('data-transfer-recordset-tuple-start-1'), {
      target: { value: '10' },
    });
    fireEvent.change(screen.getByTestId('data-transfer-recordset-tuple-end-0'), {
      target: { value: 'z-雪' },
    });
    fireEvent.change(screen.getByTestId('data-transfer-recordset-tuple-end-1'), {
      target: { value: '20' },
    });
    fireEvent.click(screen.getByTestId('data-transfer-next'));
    await waitFor(() => expect(screen.getByTestId('data-transfer-preview')).toBeTruthy());

    fireEvent.click(screen.getByRole('button', { name: /transfer.back/i }));
    await waitFor(() => expect(screen.getByTestId('data-transfer-mapping-step')).toBeTruthy());
    fireEvent.change(screen.getByTestId('data-transfer-recordset-tuple-start-0'), {
      target: { value: '' },
    });
    fireEvent.change(screen.getByTestId('data-transfer-recordset-tuple-start-1'), {
      target: { value: '' },
    });
    fireEvent.click(screen.getByTestId('data-transfer-next'));
    await waitFor(() => expect(screen.getByTestId('data-transfer-preview')).toBeTruthy());

    expect(prepareTransferJobMock).toHaveBeenLastCalledWith(
      expect.objectContaining({
        tables: [
          expect.objectContaining({
            recordset: {
              tupleRange: {
                columns: ['tenant', 'sequence'],
                end: { values: ['z-雪', '20'], inclusive: true },
              },
            },
          }),
        ],
      }),
    );
  });

  it('sends the selected registered SQL file dialect into preview', async () => {
    const { DataTransferWindow } = await import('../DataTransferWindow');
    render(<DataTransferWindow />);
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith('get_connections'));
    await dismissLimitationsDialog();

    fireEvent.click(screen.getByTestId('data-transfer-destination-sql-file'));
    await waitFor(() =>
      expect(screen.getByTestId('data-transfer-destination-sql-file')).toHaveTextContent(
        'transfer.destination.sqlFileSelected',
      ),
    );
    await pickSelect('data-transfer-source', 'PG Src (postgresql)');
    await waitFor(() => expect(getDatabasesMock).toHaveBeenCalled());
    await pickSelect('data-transfer-source-database', 'src');
    await pickSelect('data-transfer-sql-file-dialect', 'MySQL');
    await pickSelect('data-transfer-sql-file-encoding', 'transfer.destination.sqlEncodingUtf8Bom');
    fireEvent.change(screen.getByTestId('data-transfer-sql-file-target-database'), {
      target: { value: 'analytics' },
    });

    fireEvent.click(screen.getByTestId('data-transfer-next'));
    await waitFor(() => expect(screen.getByTestId('data-transfer-mode-data')).toBeTruthy());
    inspectSqlFileTransferMock.mockResolvedValue(sqlInspectRows);
    fireEvent.click(screen.getByTestId('data-transfer-next'));
    await waitFor(() => expect(screen.getByTestId('data-transfer-table-row')).toBeTruthy());
    fireEvent.click(screen.getByTestId('data-transfer-next'));
    await waitFor(() => expect(screen.getByTestId('data-transfer-mapping-step')).toBeTruthy());
    fireEvent.click(screen.getByTestId('data-transfer-next'));
    await waitFor(() => expect(screen.getByTestId('data-transfer-preview')).toBeTruthy());
    expect(prepareTransferJobMock).toHaveBeenCalledWith(
      expect.objectContaining({
        sqlFileTarget: {
          fileToken: 'sql-file-token',
          databaseType: 'mysql',
          encoding: 'utf8Bom',
          database: 'analytics',
        },
      }),
    );
  });

  it('sends UTF-16 and gzip SQL-file settings into preview and clears stale preview', async () => {
    const { DataTransferWindow } = await import('../DataTransferWindow');
    render(<DataTransferWindow />);
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith('get_connections'));
    await dismissLimitationsDialog();

    fireEvent.click(screen.getByTestId('data-transfer-destination-sql-file'));
    await waitFor(() =>
      expect(screen.getByTestId('data-transfer-destination-sql-file')).toHaveTextContent(
        'transfer.destination.sqlFileSelected',
      ),
    );
    await pickSelect('data-transfer-source', 'PG Src (postgresql)');
    await waitFor(() => expect(getDatabasesMock).toHaveBeenCalled());
    await pickSelect('data-transfer-source-database', 'src');
    await pickSelect('data-transfer-sql-file-encoding', 'transfer.destination.sqlEncodingUtf16Le');
    await pickSelect(
      'data-transfer-sql-file-compression',
      'transfer.destination.sqlCompressionGzip',
    );

    fireEvent.click(screen.getByTestId('data-transfer-next'));
    await waitFor(() => expect(screen.getByTestId('data-transfer-mode-data')).toBeTruthy());
    inspectSqlFileTransferMock.mockResolvedValue(sqlInspectRows);
    fireEvent.click(screen.getByTestId('data-transfer-next'));
    await waitFor(() => expect(screen.getByTestId('data-transfer-table-row')).toBeTruthy());
    fireEvent.click(screen.getByTestId('data-transfer-next'));
    await waitFor(() => expect(screen.getByTestId('data-transfer-mapping-step')).toBeTruthy());
    fireEvent.click(screen.getByTestId('data-transfer-next'));
    await waitFor(() => expect(screen.getByTestId('data-transfer-preview')).toBeTruthy());
    expect(prepareTransferJobMock).toHaveBeenCalledWith(
      expect.objectContaining({
        sqlFileTarget: expect.objectContaining({
          encoding: 'utf16Le',
          compression: 'gzip',
        }),
      }),
    );
  });

  it('[tester] clears empty bounds, toggles endpoint inclusivity, and disables the recordset', async () => {
    await advanceToMappingStep();

    fireEvent.click(screen.getByTestId('data-transfer-recordset-enable'));
    expect(screen.getByTestId('data-transfer-recordset-editor')).toBeTruthy();
    expect(screen.queryByTestId('data-transfer-recordset-order-error')).toBeNull();

    fireEvent.change(screen.getByTestId('data-transfer-recordset-start'), {
      target: { value: '10' },
    });
    fireEvent.click(screen.getByTestId('data-transfer-recordset-start-inclusive'));
    expect(screen.getByTestId('data-transfer-recordset-start-inclusive')).not.toBeChecked();
    fireEvent.change(screen.getByTestId('data-transfer-recordset-start'), {
      target: { value: ' ' },
    });
    expect(screen.queryByTestId('data-transfer-recordset-start-inclusive')).toBeNull();

    fireEvent.change(screen.getByTestId('data-transfer-recordset-end'), {
      target: { value: '20' },
    });
    fireEvent.click(screen.getByTestId('data-transfer-recordset-end-inclusive'));
    expect(screen.getByTestId('data-transfer-recordset-end-inclusive')).not.toBeChecked();
    fireEvent.change(screen.getByTestId('data-transfer-recordset-end'), {
      target: { value: '' },
    });
    expect(screen.queryByTestId('data-transfer-recordset-end-inclusive')).toBeNull();

    fireEvent.change(screen.getByTestId('data-transfer-recordset-limit'), {
      target: { value: '8' },
    });
    fireEvent.change(screen.getByTestId('data-transfer-recordset-limit'), {
      target: { value: '' },
    });
    fireEvent.click(screen.getByTestId('data-transfer-recordset-enable'));
    expect(screen.queryByTestId('data-transfer-recordset-editor')).toBeNull();
    expect(screen.getByText('transfer.mapping.noRecordset')).toBeTruthy();
  });

  it('[tester] requires an explicit order column when the source has no primary key', async () => {
    const previousPrimaryKeys = inspectRows[0].sourcePrimaryKeys;
    inspectRows[0].sourcePrimaryKeys = [];
    try {
      await advanceToMappingStep();
      fireEvent.click(screen.getByTestId('data-transfer-recordset-enable'));
      expect(screen.getByTestId('data-transfer-recordset-order-error')).toBeTruthy();
    } finally {
      inspectRows[0].sourcePrimaryKeys = previousPrimaryKeys;
    }
  });

  it('shows execute confirm dialog for destructive write mode before running', async () => {
    await advanceToPreviewStep('truncateInsert');

    fireEvent.click(screen.getByTestId('data-transfer-execute'));
    await waitFor(() => {
      expect(screen.getByTestId('data-transfer-execute-confirm')).toBeTruthy();
      expect(screen.getByTestId('data-transfer-execute-confirm-table-users')).toBeTruthy();
    });
    expect(applyTransferJobMock).not.toHaveBeenCalled();

    fireEvent.click(screen.getByTestId('data-transfer-execute-confirm-proceed'));
    await waitFor(() => expect(applyTransferJobMock).toHaveBeenCalled());
    // The apply spends exactly the FrozenPlan the prepare admitted.
    expect(applyTransferJobMock).toHaveBeenCalledWith(
      expect.objectContaining({
        planId: 'plan-job-1',
        planDigest: 'digest-job-1',
        selectionRevision: 1,
        selection: { sourceTables: ['users'] },
        confirmedDestructive: true,
      }),
    );
  });

  it('runs execute immediately for insert write mode without confirm dialog', async () => {
    await advanceToPreviewStep('insert');

    fireEvent.click(screen.getByTestId('data-transfer-execute'));
    await waitFor(() => expect(applyTransferJobMock).toHaveBeenCalled());
    expect(applyTransferJobMock).toHaveBeenCalledWith(
      expect.objectContaining({ confirmedDestructive: false }),
    );
    expect(screen.queryByTestId('data-transfer-execute-confirm')).toBeNull();
  });

  it('[tester] lets a SQL-file preview export the server-selected multi-table scope', async () => {
    prepareTransferJobMock.mockResolvedValueOnce({
      ...previewSuccess,
      writePlans: [
        previewSuccess.writePlans[0],
        { ...previewSuccess.writePlans[0], sourceTable: 'orders', targetTable: 'orders' },
      ],
    });
    await advanceToSqlFilePreview();

    fireEvent.click(screen.getByTestId('data-transfer-execute'));
    await waitFor(() => expect(applyTransferJobMock).toHaveBeenCalled());
    expect(applyTransferJobMock).toHaveBeenCalledWith(
      expect.objectContaining({
        planId: 'plan-job-1',
        selection: { sourceTables: ['users'] },
      }),
    );
  });

  // A SQL-file run has no target rows, so the content-addressed artifact id the
  // backend minted is the only proof the script was produced. It has to reach
  // the result surface or the run looks like it did nothing.
  it('[tester] shows the SQL-file artifact ids the apply minted', async () => {
    applyTransferJobMock.mockResolvedValueOnce({
      ...applySuccess,
      progress: { read: 0, converted: 0, attempted: 0, committed: 0, unknown: 0 },
      commitBoundaries: [],
      artifactIds: ['transfer-sql-9f2c1a', 'transfer-sql-7be40d'],
    } as unknown as TransferApplyJobView);

    await advanceToSqlFilePreview();
    fireEvent.click(screen.getByTestId('data-transfer-execute'));

    await waitFor(() => expect(screen.getByTestId('data-transfer-result')).toBeTruthy());
    const artifacts = screen.getAllByTestId('data-transfer-job-artifact');
    expect(artifacts.map((el) => el.getAttribute('data-artifact-id'))).toEqual([
      'transfer-sql-9f2c1a',
      'transfer-sql-7be40d',
    ]);
    expect(artifacts[0]).toHaveTextContent('transfer-sql-9f2c1a');
    // Nothing was written to a target, so no boundary may be claimed.
    expect(screen.getByTestId('data-transfer-job-verdict')).toHaveAttribute(
      'data-verified-boundaries',
      '0',
    );
  });

  // The 8 MiB PipelineBudget cap is an intentional fail-closed rejection. It must
  // never be dressed up as a failed or unknown run.
  it('[tester] reports an over-budget run as a deliberate refusal, not a failure', async () => {
    applyTransferJobMock.mockRejectedValueOnce(
      new Error('pipeline budget exceeded for stage users: 10485760 of 8388608 bytes'),
    );

    await advanceToPreviewStep('insert');
    fireEvent.click(screen.getByTestId('data-transfer-execute'));

    const notice = await screen.findByTestId('data-transfer-job-failure');
    expect(notice).toHaveAttribute('data-failure-kind', 'pipelineBudget');
    expect(notice).toHaveAttribute('data-fail-closed', 'true');
    expect(screen.getByTestId('data-transfer-job-failure-title')).toHaveTextContent(
      'migration.failure.pipelineBudget.title',
    );
    expect(screen.getByTestId('data-transfer-job-failure-body')).toHaveTextContent(
      'migration.failure.pipelineBudget.body',
    );
    // A budget cap is not re-reviewable and not retryable: the only correct
    // answer is a narrower selection.
    expect(screen.queryByTestId('data-transfer-job-re-review')).toBeNull();
    // No Job ran, so no verdict may be drawn — an "unknown outcome" banner
    // would be the exact misreading the fail-closed refusal forbids.
    expect(screen.queryByTestId('data-transfer-result')).toBeNull();
    expect(screen.queryByTestId('data-transfer-job-verdict')).toBeNull();
    // The review the user already holds stays on screen — there is nothing to redo.
    expect(screen.getByTestId('data-transfer-preview')).toBeTruthy();
  });

  // A scope that cannot be proven local is refused for the same fail-closed
  // reason, and it is the one refusal that offers no re-review shortcut of its
  // own beyond the shared button.
  it('[tester] refuses a run whose endpoints cannot prove the local backend scope', async () => {
    applyTransferJobMock.mockRejectedValueOnce(
      new Error('backend scope mismatch: expected local-desktop-backend'),
    );

    await advanceToPreviewStep('insert');
    fireEvent.click(screen.getByTestId('data-transfer-execute'));

    const notice = await screen.findByTestId('data-transfer-job-failure');
    expect(notice).toHaveAttribute('data-failure-kind', 'backendScope');
    expect(notice).toHaveAttribute('data-fail-closed', 'true');
    expect(screen.getByTestId('data-transfer-job-failure-body')).toHaveTextContent(
      'migration.failure.backendScope.body',
    );
    expect(screen.queryByTestId('data-transfer-result')).toBeNull();
  });

  // A spent plan cannot be applied twice. The backend refuses the second apply;
  // the window must offer a fresh review, never a second apply.
  it('[tester] rejects reuse of a spent plan and offers a fresh review instead', async () => {
    applyTransferJobMock.mockRejectedValueOnce(
      new Error('plan plan-job-1 is already consumed; re-review the migration to mint a new plan'),
    );

    await advanceToPreviewStep('insert');
    fireEvent.click(screen.getByTestId('data-transfer-execute'));

    const notice = await screen.findByTestId('data-transfer-job-failure');
    expect(notice).toHaveAttribute('data-failure-kind', 'planConsumed');
    expect(notice).toHaveAttribute('data-fail-closed', 'false');

    const reReview = screen.getByTestId('data-transfer-job-re-review');
    fireEvent.click(reReview);
    // Re-reviewing must never re-apply the spent plan: it drops back to mapping
    // with a fresh preview required.
    await waitFor(() => expect(screen.getByTestId('data-transfer-mapping-step')).toBeTruthy());
    expect(applyTransferJobMock).toHaveBeenCalledTimes(1);
  });

  it('[tester] returns SQL-file preview back to the mapping step', async () => {
    await advanceToSqlFilePreview();

    fireEvent.click(screen.getByRole('button', { name: /transfer.back/i }));
    await waitFor(() => expect(screen.getByTestId('data-transfer-mapping-step')).toBeTruthy());
  });

  it('[tester] keeps SQL-file DDL preview read-only', async () => {
    prepareTransferJobMock.mockResolvedValueOnce({
      ...previewSuccess,
      ddl: [
        {
          sourceTable: 'users',
          targetTable: 'users',
          ddl: 'CREATE TABLE users (id integer)',
        },
      ],
    });
    await advanceToSqlFilePreview('structure');

    expect(screen.getByTestId('data-transfer-ddl-preview-users')).toHaveTextContent(
      'CREATE TABLE users (id integer)',
    );
    expect(screen.queryByTestId('data-transfer-ddl-editor-users')).toBeNull();
    expect(screen.queryByTestId('sql-code-change')).toBeNull();
  });

  it('keeps SQL-file table and column selection in the reviewed preview job', async () => {
    const orders = {
      ...sqlInspectRows[0],
      sourceTable: 'orders',
      targetTable: 'orders',
      sourceColumns: ['id', 'total'],
      sourcePrimaryKeys: ['id'],
      sourceColumnTypes: { id: 'INTEGER', total: 'NUMERIC' },
      columnMappings: [
        { sourceColumn: 'id', targetColumn: 'id', skip: false },
        { sourceColumn: 'total', targetColumn: 'total', skip: false },
      ],
    } satisfies TransferTableResult;
    await advanceToSqlFilePreview('data', 'objects', [...sqlInspectRows, orders]);

    const ordersRow = screen
      .getAllByTestId('data-transfer-table-row')
      .find((row) => within(row).queryByText('orders'));
    expect(ordersRow).toBeTruthy();
    fireEvent.click(within(ordersRow!).getByRole('checkbox'));
    fireEvent.click(screen.getByTestId('data-transfer-next'));
    await waitFor(() => expect(screen.getByTestId('data-transfer-mapping-step')).toBeTruthy());

    fireEvent.change(screen.getByTestId('data-transfer-target-table-input'), {
      target: { value: 'users_copy' },
    });
    expect(screen.getByTestId('data-transfer-skip-extra')).toBeChecked();
    fireEvent.click(screen.getByTestId('data-transfer-next'));
    await waitFor(() => expect(screen.getByTestId('data-transfer-preview')).toBeTruthy());

    expect(prepareTransferJobMock).toHaveBeenCalledWith(
      expect.objectContaining({
        tables: [
          expect.objectContaining({
            sourceTable: 'users',
            targetTable: 'users_copy',
            createNew: true,
            columnMappings: expect.arrayContaining([
              expect.objectContaining({ sourceColumn: 'extra', skip: true }),
            ]),
          }),
        ],
      }),
    );
  });

  it('disables next and shows empty guidance when no tables are detected', async () => {
    await advanceToObjectsStep(true);

    expect(screen.getByText('transfer.objects.noTablesFound')).toBeTruthy();
    expect(screen.getByText('transfer.objects.noTablesHint')).toBeTruthy();
    expect(screen.getByTestId('data-transfer-next')).toBeDisabled();

    inspectTransferMock.mockClear();
    inspectTransferMock.mockResolvedValue(inspectRows);
    fireEvent.click(screen.getByTestId('data-transfer-reinspect'));

    await waitFor(() => expect(inspectTransferMock).toHaveBeenCalled());
    await waitFor(() => expect(screen.getByTestId('data-transfer-table-row')).toBeTruthy());
    expect(screen.getByTestId('data-transfer-next')).not.toBeDisabled();
  });

  it('[tester] applies setup controls and table selection before mapping', async () => {
    await advanceToSetupStep();
    fireEvent.click(screen.getByTestId('data-transfer-mode-structure'));
    expect(screen.getByTestId('data-transfer-mode-structure')).toBeChecked();
    fireEvent.click(screen.getByTestId('data-transfer-mode-data'));

    const batchInput = screen.getByRole('spinbutton');
    fireEvent.change(batchInput, { target: { value: '0' } });
    expect(batchInput).toHaveValue(0);
    expect(screen.getByTestId('data-transfer-batch-size-error')).toBeTruthy();
    expect(screen.getByTestId('data-transfer-next')).toBeDisabled();
    fireEvent.change(batchInput, { target: { value: '25' } });
    expect(batchInput).toHaveValue(25);
    expect(screen.queryByTestId('data-transfer-batch-size-error')).toBeNull();
    fireEvent.change(batchInput, { target: { value: '501' } });
    expect(batchInput).toHaveValue(501);
    expect(screen.getByTestId('data-transfer-batch-size-error')).toBeTruthy();
    expect(screen.getByTestId('data-transfer-next')).toBeDisabled();
    fireEvent.change(batchInput, { target: { value: '1.5' } });
    expect(batchInput).toHaveValue(1.5);
    expect(screen.getByTestId('data-transfer-batch-size-error')).toBeTruthy();
    fireEvent.change(batchInput, { target: { value: '500' } });
    expect(batchInput).toHaveValue(500);
    expect(screen.queryByTestId('data-transfer-batch-size-error')).toBeNull();
    const stopOnError = within(
      screen.getByText('transfer.stopOnError').closest('label')!,
    ).getByRole('checkbox');
    fireEvent.click(stopOnError);
    expect(stopOnError).not.toBeChecked();

    inspectTransferMock.mockResolvedValue([{ ...inspectRows[0], targetTable: '' }]);
    fireEvent.click(screen.getByTestId('data-transfer-next'));
    await waitFor(() => expect(screen.getByTestId('data-transfer-table-row')).toBeTruthy());

    const tableRow = screen.getByTestId('data-transfer-table-row');
    expect(within(tableRow).getByText('→ —')).toBeTruthy();
    const tableToggle = within(tableRow).getByRole('checkbox');
    fireEvent.click(tableToggle);
    expect(screen.getByTestId('data-transfer-next')).toBeDisabled();
    fireEvent.click(tableToggle);
    expect(screen.getByTestId('data-transfer-next')).not.toBeDisabled();

    fireEvent.click(screen.getByTestId('data-transfer-next'));
    await waitFor(() => expect(screen.getByTestId('data-transfer-mapping-step')).toBeTruthy());
  });

  it('[tester] reports a non-Error inspect rejection without hiding object recovery', async () => {
    await advanceToSetupStep();
    inspectTransferMock.mockRejectedValueOnce('inspect failed');
    fireEvent.click(screen.getByTestId('data-transfer-next'));

    await waitFor(() =>
      expect(screen.getByTestId('data-transfer-error')).toHaveTextContent('inspect failed'),
    );
    expect(screen.getByTestId('data-transfer-objects-empty')).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: 'common.ok' }));
    await waitFor(() => expect(screen.queryByTestId('data-transfer-error')).toBeNull());
  });

  it('shows preview error state with retry and back actions when preview fails', async () => {
    await advanceToMappingStep();

    prepareTransferJobMock.mockRejectedValueOnce(new Error('preview boom'));
    fireEvent.click(screen.getByTestId('data-transfer-next'));

    const errorPanel = await waitFor(() => screen.getByTestId('data-transfer-preview-error'));
    expect(within(errorPanel).getByText('preview boom')).toBeTruthy();
    expect(screen.queryByTestId('data-transfer-preview')).toBeNull();

    prepareTransferJobMock.mockResolvedValueOnce(previewSuccess);
    fireEvent.click(screen.getByTestId('data-transfer-preview-retry'));
    await waitFor(() => expect(screen.getByTestId('data-transfer-preview')).toBeTruthy());
  });

  it('returns to mapping from preview error state', async () => {
    await advanceToMappingStep();

    prepareTransferJobMock.mockRejectedValueOnce(new Error('preview boom'));
    fireEvent.click(screen.getByTestId('data-transfer-next'));
    await waitFor(() => expect(screen.getByTestId('data-transfer-preview-error')).toBeTruthy());

    fireEvent.click(screen.getByTestId('data-transfer-preview-back-mapping'));
    await waitFor(() => expect(screen.getByTestId('data-transfer-mapping-step')).toBeTruthy());
  });

  it('updates preview error message when retry fails', async () => {
    await advanceToMappingStep();

    prepareTransferJobMock.mockRejectedValueOnce(new Error('preview boom'));
    fireEvent.click(screen.getByTestId('data-transfer-next'));
    await waitFor(() => expect(screen.getByTestId('data-transfer-preview-error')).toBeTruthy());

    prepareTransferJobMock.mockRejectedValueOnce('retry failed');
    fireEvent.click(screen.getByTestId('data-transfer-preview-retry'));
    await waitFor(() => {
      const errorPanel = screen.getByTestId('data-transfer-preview-error');
      expect(within(errorPanel).getByText('retry failed')).toBeTruthy();
    });
  });

  it('mints a fresh prepare idempotency key for every admission', async () => {
    await advanceToMappingStep();

    prepareTransferJobMock.mockRejectedValueOnce(new Error('preview boom'));
    fireEvent.click(screen.getByTestId('data-transfer-next'));
    await waitFor(() => expect(screen.getByTestId('data-transfer-preview-error')).toBeTruthy());
    expect(prepareKeys).toEqual(['data-transfer/prepare/1']);

    // A retry is a new admission, not a replay: reusing the key would let the
    // backend answer it from an older prepare Job's receipt, which is exactly
    // what the apply key is derived from the planId to avoid.
    prepareTransferJobMock.mockResolvedValueOnce(previewSuccess);
    fireEvent.click(screen.getByTestId('data-transfer-preview-retry'));
    await waitFor(() => expect(screen.getByTestId('data-transfer-preview')).toBeTruthy());
    expect(prepareKeys).toEqual(['data-transfer/prepare/1', 'data-transfer/prepare/2']);
  });

  it('advances to preview error state instead of blank when preview fails from mapping', async () => {
    await advanceToMappingStep();

    prepareTransferJobMock.mockRejectedValueOnce(new Error('mapping preview failed'));
    fireEvent.click(screen.getByTestId('data-transfer-next'));

    const errorPanel = await waitFor(() => screen.getByTestId('data-transfer-preview-error'));
    expect(within(errorPanel).getByText('mapping preview failed')).toBeTruthy();
    expect(screen.queryByTestId('data-transfer-preview')).toBeNull();
  });

  it('holds the mapping editor inert while the prepare is in flight', async () => {
    await advanceToMappingStep();

    let releasePrepare!: (view: typeof previewSuccess) => void;
    prepareTransferJobMock.mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          releasePrepare = resolve;
        }),
    );

    fireEvent.click(screen.getByTestId('data-transfer-next'));

    // The editor is already closed for the round trip. The bypass this blocks
    // is precisely "rename the target to the source name while the backend is
    // busy": the plan under review is then not the one that gets admitted.
    const targetTable = await waitFor(() => {
      const input = screen.getByTestId('data-transfer-target-table-input');
      expect(input).toBeDisabled();
      return input as HTMLInputElement;
    });
    expect(screen.getByTestId('data-transfer-auto-match')).toBeDisabled();
    expect(screen.getByTestId('data-transfer-clear-unmapped')).toBeDisabled();

    // jsdom will happily assign .value to a disabled input and dispatch, so the
    // meaningful assertion is what the attempt leaves behind: nothing.
    fireEvent.change(targetTable, { target: { value: 'users' } });

    releasePrepare(previewSuccess);
    await waitFor(() => expect(screen.getByTestId('data-transfer-preview')).toBeTruthy());

    fireEvent.click(screen.getByText('transfer.back'));
    const restored = (await waitFor(() =>
      screen.getByTestId('data-transfer-target-table-input'),
    )) as HTMLInputElement;
    expect(restored.value).toBe('users_v2');
    expect(restored).not.toBeDisabled();
  });

  it('re-decides the mapping gate against the rows that exist after prepare', async () => {
    await advanceToMappingStep();

    // The target-table blur refresh is the one thing already in flight when
    // Next is pressed: its inspect comes back saying the table has no source
    // columns any more, so the mapping the click was judged against is gone by
    // the time prepare is admitted.
    let resolveInspect!: (rows: TransferTableResult[]) => void;
    inspectTransferMock.mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          resolveInspect = resolve;
        }),
    );
    fireEvent.focusOut(screen.getByTestId('data-transfer-target-table-input'));
    await waitFor(() => expect(inspectTransferMock).toHaveBeenCalledTimes(2));

    let releasePrepare!: (view: typeof previewSuccess) => void;
    prepareTransferJobMock.mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          releasePrepare = resolve;
        }),
    );
    fireEvent.click(screen.getByTestId('data-transfer-next'));
    await waitFor(() =>
      expect(screen.getByTestId('data-transfer-target-table-input')).toBeDisabled(),
    );

    resolveInspect(inspectRows.map((row) => ({ ...row, sourceColumns: [] })));
    releasePrepare(previewSuccess);

    const alert = await waitFor(() => screen.getByTestId('data-transfer-mapping-gate-error'));
    expect(alert.getAttribute('role')).toBe('alert');
    // Still on the mapping step, with nothing admitted: advancing here in
    // silence is the mapping-gate defect — the preview would review rows that
    // are not the ones on screen.
    expect(screen.getByTestId('data-transfer-mapping-step')).toBeTruthy();
    expect(screen.queryByTestId('data-transfer-preview')).toBeNull();
  });

  it('creating a new table asks for a name instead of assuming the source', async () => {
    await advanceToMappingStep([
      { ...inspectRows[0], status: 'CREATE_NEW', targetTable: '', targetColumns: [] },
    ]);

    const name = () => screen.getByTestId('data-transfer-target-table-input') as HTMLInputElement;

    fireEvent.click(screen.getByTestId('data-transfer-create-new-toggle'));

    // The name of a table that does not exist yet is the one thing the backend
    // cannot look up for the user, so it is the one thing that must be typed.
    // Filling the field in with the source name shows a name nobody chose.
    expect(name().value).toBe('');
    expect(name().value).not.toBe('users');

    fireEvent.change(name(), { target: { value: 'users_archive' } });
    expect(name().value).toBe('users_archive');

    // The choice-style field therefore reflects user intent all the way down:
    // the plan that is prepared is named by the user, not by the source table.
    prepareTransferJobMock.mockResolvedValueOnce(previewSuccess);
    fireEvent.click(screen.getByTestId('data-transfer-next'));
    await waitFor(() => expect(screen.getByTestId('data-transfer-preview')).toBeTruthy());
    expect(prepareTransferJobMock).toHaveBeenCalledWith(
      expect.objectContaining({
        tables: [
          expect.objectContaining({ sourceTable: 'users', targetTable: 'users_archive' }),
        ],
      }),
    );
  });

  it('an unnamed create-new row cannot reach a plan via the enable checkbox', async () => {
    await advanceToMappingStep([structureCreateNewRow], 'objects');

    // It arrives disabled and unnamed, so it cannot hold the gate on its own and
    // the step cannot be left past it without the user saying the table should
    // go. Ticking it on the objects step is ordinary intent — and it is the one
    // route to a create-new row that does NOT go through the create-new toggle,
    // so it does not hit the clear-on-toggle path either.
    const row = screen.getByTestId('data-transfer-table-row');
    const enable = within(row).getByRole('checkbox');
    expect(enable).not.toBeChecked();

    // This is the enable-checkbox route. The backend used to pre-fill
    // `target_table` with the source name, so from here the row was named, the
    // mapping gate was satisfied by a value the user never chose, and the plan
    // carried `targetTable: 'new_table'` for a table that did not exist.
    //
    // The objects step gates on "did the user pick any row" (canNext: 'objects'
    // is `tables.some(tbl => tbl.enabled)`), so it hands an enabled-but-unnamed
    // row forward by design; the strict gate is `mappingGateAllowsAdvance` on
    // the mapping step. That is where the name has to stop being free.
    fireEvent.click(enable);
    await waitFor(() => expect(screen.getByTestId('data-transfer-next')).not.toBeDisabled());
    fireEvent.click(screen.getByTestId('data-transfer-next'));
    await waitFor(() => expect(screen.getByTestId('data-transfer-mapping-step')).toBeTruthy());

    // Nothing pre-filled the name on the way in: the backend sends it empty, and
    // neither the UI nor the create-new toggle path supplies one.
    const name = screen.getByTestId('data-transfer-target-table-input') as HTMLInputElement;
    expect(name.value).toBe('');
    expect(name.value).not.toBe('new_table');

    // So the row is enabled and still unnamed, and that stops the step.
    expect(screen.getByTestId('data-transfer-next')).toBeDisabled();
    expect(screen.getByTestId('data-transfer-mapping-gate-error')).toHaveTextContent(
      'transfer.mapping.targetNameRequired',
    );
    prepareTransferJobMock.mockResolvedValueOnce(previewSuccess);
    expect(prepareTransferJobMock).not.toHaveBeenCalled();

    // And the name that does reach a plan is the one the user typed, not one
    // the UI or the backend supplied.
    fireEvent.change(name, { target: { value: 'new_table_v2' } });
    await waitFor(() => expect(screen.getByTestId('data-transfer-next')).not.toBeDisabled());
    fireEvent.click(screen.getByTestId('data-transfer-next'));
    await waitFor(() => expect(screen.getByTestId('data-transfer-preview')).toBeTruthy());
    expect(prepareTransferJobMock).toHaveBeenCalledWith(
      expect.objectContaining({
        tables: [
          expect.objectContaining({ sourceTable: 'new_table', targetTable: 'new_table_v2' }),
        ],
      }),
    );
  });

  it('an enabled create-new row with no name holds the step and says which field is the reason', async () => {
    await advanceToMappingStep([
      { ...structureCreateNewRow, enabled: true, status: 'CREATE_NEW', targetTable: '' },
    ]);

    const name = screen.getByTestId('data-transfer-target-table-input') as HTMLInputElement;
    expect(name.value).toBe('');
    expect(name.getAttribute('aria-invalid')).toBe('true');

    // A disabled Next with no explanation is the state this is about.
    expect(screen.getByTestId('data-transfer-next')).toBeDisabled();
    const alert = screen.getByTestId('data-transfer-mapping-gate-error');
    expect(alert.getAttribute('role')).toBe('alert');
    expect(alert).toHaveTextContent('transfer.mapping.targetNameRequired');
    expect(screen.queryByTestId('data-transfer-preview')).toBeNull();

    // Whitespace is what an input the user only tabbed through reads as, and
    // the backend would create the table under exactly those bytes.
    fireEvent.change(name, { target: { value: '   ' } });
    expect(screen.getByTestId('data-transfer-next')).toBeDisabled();
    expect(screen.getByTestId('data-transfer-mapping-gate-error')).toBeTruthy();

    // The same predicate that disarms Next is what clears the banner, so the
    // two can never disagree about why the step is stuck.
    fireEvent.change(name, { target: { value: 'users_v2' } });
    await waitFor(() =>
      expect(screen.queryByTestId('data-transfer-mapping-gate-error')).toBeNull(),
    );
    expect(screen.getByTestId('data-transfer-next')).not.toBeDisabled();

    prepareTransferJobMock.mockResolvedValueOnce(previewSuccess);
    fireEvent.click(screen.getByTestId('data-transfer-next'));
    await waitFor(() => expect(screen.getByTestId('data-transfer-preview')).toBeTruthy());
  });

  it('a source row filter cannot be rewritten while the prepare is in flight', async () => {
    // Two conditions so the logic selector is on screen — it changes no
    // condition and is the exit most likely to miss a guard put in `update`.
    //
    // What this test actually proves, established by mutation rather than by
    // reading the handler: deleting the `disabled` guard from SourceFilterEditor's
    // `apply` leaves this test GREEN (mutant N3). The guard that actually holds
    // this chain is one level up — ColumnMappingEditor:189 wires this editor's
    // onChange to `commit`, and `commit` already opens with `if (disabled) return`.
    // Deleting *that* guard turns this test red (mutant N4). So the effective last
    // line is `commit`, which pre-dates this track; the local `apply` guard is
    // defence in depth against a rewiring, not a fix for a reachable defect.
    await advanceToMappingStep([
      {
        ...inspectRows[0],
        sourceFilter: {
          logic: 'and',
          filters: [
            { column: 'id', operator: 'eq', value: '1' },
            { column: 'name', operator: 'eq', value: 'ada' },
          ],
        },
      },
    ]);

    let releasePrepare!: (view: typeof previewSuccess) => void;
    prepareTransferJobMock.mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          releasePrepare = resolve;
        }),
    );
    fireEvent.click(screen.getByTestId('data-transfer-next'));
    // Every editor on the step is held inert for the duration of the round
    // trip — the same `disabled` the DOM is drawn from.
    await waitFor(() =>
      expect(screen.getByTestId('data-transfer-target-table-input')).toBeDisabled(),
    );

    const filter = screen.getByTestId('data-transfer-source-filter');
    const value = within(filter).getAllByRole('textbox')[0] as HTMLInputElement;
    expect(value).toBeDisabled();
    // jsdom will happily assign .value to a disabled input and dispatch, so the
    // meaningful assertion is what the attempt leaves behind: nothing. A
    // handler that answered here would rewrite the filter for a plan already
    // sent without it.
    fireEvent.change(value, { target: { value: '999' } });
    // The logic selector is deliberately NOT driven here. Its trigger is a
    // native `<button disabled>` and React suppresses onClick on disabled form
    // elements, so the click cannot open the listbox at all — unlike onChange,
    // which React does not suppress, and which is why the textbox above is the
    // reachable half of the guard. What matters is that it is disabled, so the
    // guarded exit behind it is defence in depth rather than a hole.
    expect(screen.getByTestId('data-transfer-source-filter-logic')).toBeDisabled();

    releasePrepare(previewSuccess);
    await waitFor(() => expect(screen.getByTestId('data-transfer-preview')).toBeTruthy());

    fireEvent.click(screen.getByText('transfer.back'));
    const restored = await waitFor(() => screen.getByTestId('data-transfer-source-filter'));
    const restoredValues = within(restored).getAllByRole('textbox') as HTMLInputElement[];
    expect(restoredValues.map((input) => input.value)).toEqual(['1', 'ada']);
  });

  it('[tester] renders, edits, and copies the exact DDL preview with warnings', async () => {
    prepareTransferJobMock.mockResolvedValueOnce({
      ...previewSuccess,
      ddl: [
        {
          sourceTable: 'users',
          targetTable: 'users_copy',
          ddl: 'CREATE TABLE users_copy (id bigint)',
        },
      ],
      warnings: ['DDL commits independently'],
      blockReason: 'Target is blocked for this dry run',
      canExecute: false,
      writePlans: [
        {
          ...previewSuccess.writePlans[0],
          estimatedRows: null,
        },
      ],
    });
    await advanceToPreviewStep('insert');

    expect(screen.getByTestId('sql-code-block')).toHaveTextContent(
      'CREATE TABLE users_copy (id bigint)',
    );
    expect(screen.getByText('DDL commits independently')).toBeTruthy();
    expect(screen.getByText('Target is blocked for this dry run')).toBeTruthy();
    expect(screen.getByText(/transfer.estimatedRows.*—/)).toBeTruthy();

    fireEvent.click(screen.getByTestId('sql-code-change'));
    expect(screen.getByTestId('sql-code-block')).toHaveTextContent('CREATE TABLE edited');
    fireEvent.click(screen.getByTestId('data-transfer-copy-ddl-users'));
    await waitFor(() =>
      expect(navigator.clipboard.writeText).toHaveBeenCalledWith('CREATE TABLE edited'),
    );
  });

it('[tester] exposes cancellable execution progress and finishes as success', async () => {
    let finishApply!: (view: TransferApplyJobView) => void;
    applyTransferJobMock.mockImplementationOnce(
      () =>
        new Promise<TransferApplyJobView>((resolve) => {
          finishApply = resolve;
        }),
    );

    await advanceToPreviewStep('insert');
    fireEvent.click(screen.getByTestId('data-transfer-execute'));

    await waitFor(() => expect(screen.getByTestId('data-transfer-executing-overlay')).toBeTruthy());
    expect(screen.getAllByText(/transfer.executingProgress/)).toHaveLength(2);

    // `apply_data_transfer_job` mints the Job id server-side and only returns
    // it with the terminal view, so while the apply is in flight the window
    // holds no id to address a cancel to. The control stays visible for the
    // duration, reports that it is unaddressable, and refuses the click instead
    // of pretending a cancel landed.
    const cancelButton = screen.getByTestId('data-transfer-cancel');
    expect(cancelButton).toHaveAttribute('data-cancel-addressable', 'false');
    expect(cancelButton).toBeDisabled();
    expect(screen.getByTestId('data-transfer-cancel-pending-id')).toHaveTextContent(
      'migration.cancel.unknownJob',
    );
    fireEvent.click(cancelButton);
    expect(cancelTransferJobMock).not.toHaveBeenCalled();

    finishApply(applySuccess);

    await waitFor(() => expect(screen.getByTestId('data-transfer-result')).toBeTruthy());
    const verdict = screen.getByTestId('data-transfer-job-verdict');
    expect(verdict).toHaveAttribute('data-severity', 'ok');
    expect(verdict).toHaveAttribute('data-uncertainty', 'none');
    expect(verdict).toHaveAttribute('data-verified-boundaries', '1');
    expect(verdict).toHaveAttribute('data-unverified-boundaries', '0');
    expect(screen.getByTestId('data-transfer-job-verdict-status')).toHaveTextContent(
      'migration.verdict.ok',
    );
    expect(screen.getByTestId('data-transfer-result')).toHaveAttribute('data-completed', 'true');
    expect(screen.queryByTestId('data-transfer-job-reconcile')).toBeNull();
    expect(screen.queryByTestId('data-transfer-job-uncertainty')).toBeNull();
    expect(screen.queryByTestId('data-transfer-job-error')).toBeNull();

    // The commit boundary, not a boolean, is what makes it a success.
    const boundary = screen.getByTestId('data-transfer-job-boundary-users');
    expect(boundary).toHaveAttribute('data-boundary-verified', 'true');
    expect(boundary).toHaveTextContent('migration.boundary.verified');
    expect(applyTransferJobMock).toHaveBeenCalledWith(
      expect.objectContaining({
        planId: 'plan-job-1',
        planDigest: 'digest-job-1',
        selectionRevision: 1,
        selection: { sourceTables: ['users'] },
        confirmedDestructive: false,
      }),
    );
  });

  it('[tester] cancels an admitted plan that was never applied and re-reviews instead of resuming', async () => {
    await advanceToPreviewStep('insert');

    // Before anything is applied the window *does* hold the Job id, so the
    // cancel is legal and can be honoured exactly.
    const cancelButton = screen.getByTestId('data-transfer-cancel');
    expect(cancelButton).toHaveAttribute('data-cancel-addressable', 'true');
    expect(cancelButton).not.toBeDisabled();
    fireEvent.click(cancelButton);

    await waitFor(() => expect(cancelTransferJobMock).toHaveBeenCalledTimes(1));
    expect(cancelTransferJobMock).toHaveBeenCalledWith('transfer-data-prepare-1');

    // A disposed plan is gone: back at mapping, no apply was ever issued, and no
    // result panel may claim something happened.
    await waitFor(() => expect(screen.getByTestId('data-transfer-mapping-step')).toBeTruthy());
    expect(applyTransferJobMock).not.toHaveBeenCalled();
    expect(screen.queryByTestId('data-transfer-result')).toBeNull();
    expect(screen.queryByTestId('data-transfer-preview')).toBeNull();
  });

  it('refuses to treat an unaddressable cancel as a cancellation and leaves the review in place', async () => {
    cancelTransferJobMock.mockResolvedValueOnce(false);
    await advanceToPreviewStep('insert');

    const cancelButton = screen.getByTestId('data-transfer-cancel');
    fireEvent.click(cancelButton);

    // `cancel_data_transfer` answered `false` — it has no such P5 Job. That is
    // "nothing was cancelled", not "cancelled": the review must survive.
    await waitFor(() => expect(cancelTransferJobMock).toHaveBeenCalledTimes(1));
    expect(screen.getByTestId('data-transfer-preview')).toBeTruthy();
    expect(screen.queryByTestId('data-transfer-result')).toBeNull();
    expect(applyTransferJobMock).not.toHaveBeenCalled();
  });

  it('[tester] renders partial execution as incomplete with recovery guidance', async () => {
    applyTransferJobMock.mockResolvedValueOnce({
      ...applySuccess,
      state: 'failed',
      effectOutcome: 'partiallyApplied',
      progress: { read: 4, converted: 4, attempted: 4, committed: 4, unknown: 0 },
      // The data-transfer handler records no commit boundary, so the four
      // committed rows have nothing behind them and the run cannot be certified.
      // The panel must say exactly that.
      commitBoundaries: [],
      partial: true,
      error: 'stage orders failed after the users stage committed',
      recoveryVerdict: 'reject',
      recoveryReason: 'no checkpoint was recorded',
      recoveryResumeThrough: null,
    } as unknown as TransferApplyJobView);

    await advanceToPreviewStep('insert');
    fireEvent.click(screen.getByTestId('data-transfer-execute'));

    await waitFor(() => expect(screen.getByTestId('data-transfer-result')).toBeTruthy());
    const verdict = screen.getByTestId('data-transfer-job-verdict');
    expect(verdict).toHaveAttribute('data-severity', 'uncertain');
    // A recovery verdict of `reject` is the dominant uncertainty: whatever else
    // is unknown, the backend has already refused to certify this run.
    expect(verdict).toHaveAttribute('data-uncertainty', 'recoveryRejected');
    expect(screen.getByTestId('data-transfer-job-verdict-status')).not.toHaveTextContent(
      'migration.verdict.ok',
    );
    expect(screen.getByTestId('data-transfer-result')).toHaveAttribute('data-completed', 'false');
    expect(screen.getByTestId('data-transfer-job-verdict-status')).toHaveTextContent(
      'migration.verdict.uncertain',
    );
    expect(screen.getByTestId('data-transfer-job-uncertainty')).toHaveTextContent(
      'migration.uncertainty.recoveryRejected',
    );

    // No boundary came back, so there is nothing to verify — and that is stated
    // rather than smoothed over. With `reject` the stated uncertainty is the
    // refusal, not the missing evidence (see the evidence-gap test below).
    expect(screen.getByTestId('data-transfer-job-boundaries-empty')).toHaveAttribute(
      'data-evidence-gap',
      'false',
    );
    expect(screen.getByTestId('data-transfer-job-recovery-verdict')).toHaveAttribute(
      'data-verdict',
      'reject',
    );
    expect(screen.getByTestId('data-transfer-job-recovery-reason')).toHaveTextContent(
      'no checkpoint was recorded',
    );
    expect(screen.queryByTestId('data-transfer-job-recovery-resume-through')).toBeNull();
    expect(screen.getByTestId('data-transfer-job-reconcile')).toBeTruthy();
    expect(screen.getByTestId('data-transfer-job-error')).toHaveTextContent(
      'stage orders failed after the users stage committed',
    );
    expect(screen.getByTestId('data-transfer-job-rows')).toHaveTextContent(
      'migration.verdict.committedRows: 4',
    );
    expect(screen.queryByTestId('data-transfer-job-boundary-users')).toBeNull();

    // A rejected plan cannot be resumed, only re-reviewed for a new planId — but
    // a new planId is also a second write over a range whose first run is
    // unreconciled, so the affordance itself is closed here.
    expect(screen.queryByTestId('data-transfer-resume')).toBeNull();
    expect(screen.getByTestId('data-transfer-rereview')).toHaveAttribute(
      'data-blocked',
      'reconcile-pending',
    );
    expect(screen.getByTestId('data-transfer-rereview')).toBeDisabled();
    expect(screen.getByTestId('data-transfer-rereview-blocked')).toHaveTextContent(
      'migration.verdict.rereviewBlocked',
    );
  });

  it('flags committed rows with no commit boundary as a missing-evidence gap', async () => {
    // Rows were written but the backend returned no commit boundary for
    // them: the UI cannot claim the work was verified, so it must say so. No
    // recovery verdict is involved here — the gap alone drives the uncertainty.
    applyTransferJobMock.mockResolvedValueOnce({
      ...applySuccess,
      state: 'failed',
      effectOutcome: 'partiallyApplied',
      progress: { read: 5, converted: 5, attempted: 5, committed: 5, unknown: 0 },
      commitBoundaries: [],
      partial: true,
      error: 'the transfer ended without reporting a commit boundary',
      recoveryVerdict: null,
      recoveryReason: null,
      recoveryResumeThrough: null,
    } as unknown as TransferApplyJobView);

    await advanceToPreviewStep('insert');
    fireEvent.click(screen.getByTestId('data-transfer-execute'));

    await waitFor(() => expect(screen.getByTestId('data-transfer-result')).toBeTruthy());
    const verdict = screen.getByTestId('data-transfer-job-verdict');
    expect(verdict).toHaveAttribute('data-severity', 'uncertain');
    expect(verdict).toHaveAttribute('data-uncertainty', 'missingEvidence');
    expect(screen.getByTestId('data-transfer-job-uncertainty')).toHaveTextContent(
      'migration.uncertainty.missingEvidence',
    );
    // The gap is explained where it happened, not swallowed.
    expect(screen.getByTestId('data-transfer-job-boundaries-empty')).toHaveAttribute(
      'data-evidence-gap',
      'true',
    );
    expect(screen.getByTestId('data-transfer-job-reconcile')).toBeTruthy();
    // A missing boundary is not a resume offer: there is nothing to resume from.
    expect(screen.queryByTestId('data-transfer-resume')).toBeNull();
  });

  it('renders an unknown commit outcome distinctly and does not offer resume', async () => {
    // The acknowledgement was lost, so the backend cannot say what landed.
    // "Unknown" must never be smoothed into a failure or a success.
    applyTransferJobMock.mockResolvedValueOnce({
      ...applySuccess,
      state: 'failed',
      effectOutcome: 'unknown',
      progress: { read: 3, converted: 3, attempted: 3, committed: 1, unknown: 2 },
      commitBoundaries: [],
      error: 'commit acknowledgement lost',
      recoveryVerdict: 'requireManualReview',
      recoveryReason: null,
      recoveryResumeThrough: null,
    } as unknown as TransferApplyJobView);

    await advanceToPreviewStep('insert');
    fireEvent.click(screen.getByTestId('data-transfer-execute'));

    await waitFor(() => expect(screen.getByTestId('data-transfer-result')).toBeTruthy());
    expect(screen.getByTestId('data-transfer-result')).toHaveAttribute(
      'data-verdict-severity',
      'uncertain',
    );
    expect(screen.getByTestId('data-transfer-result')).toHaveAttribute('data-requires-reconcile', 'true');
    expect(screen.getByTestId('data-transfer-job-verdict')).toHaveAttribute(
      'data-uncertainty',
      'effectOutcomeUnknown',
    );
    expect(screen.getByTestId('data-transfer-job-verdict-status')).toHaveTextContent(
      'migration.verdict.uncertain',
    );
    expect(screen.getByTestId('data-transfer-job-uncertainty')).toHaveTextContent(
      'migration.uncertainty.effectOutcomeUnknown',
    );
    expect(screen.getByTestId('data-transfer-job-recovery-verdict')).toHaveAttribute(
      'data-verdict',
      'requireManualReview',
    );
    expect(screen.getByTestId('data-transfer-job-reconcile')).toBeTruthy();
    // committed + unknown both unaccounted for: 1 + 2.
    expect(screen.getByTestId('data-transfer-job-rows')).toHaveTextContent(
      'migration.verdict.committedRows: 3',
    );
    expect(screen.queryByTestId('data-transfer-resume')).toBeNull();
    // The read-only verification of an unknown operation comes first.
    // Re-review would mint a fresh planId over the same range, so it is offered
    // but closed, and the reason is stated where the button is.
    const reReview = screen.getByTestId('data-transfer-rereview');
    expect(reReview).toHaveAttribute('data-blocked', 'reconcile-pending');
    expect(reReview).toBeDisabled();
    fireEvent.click(reReview);
    // Still the settled verdict — a blocked affordance must not navigate.
    expect(screen.queryByTestId('data-transfer-result')).toBeTruthy();
    expect(screen.getByTestId('data-transfer-job-verdict')).toHaveAttribute(
      'data-uncertainty',
      'effectOutcomeUnknown',
    );
    expect(screen.getByTestId('data-transfer-rereview-blocked')).toHaveTextContent(
      'migration.verdict.rereviewBlocked',
    );
  });

  it('keeps the re-review affordance open once a run is reconciled', async () => {
    // The closed affordance above must not become a dead end: a fully completed,
    // fully evidenced run has nothing left to reconcile, so the next legal move
    // (a fresh review with a new planId) is available again.
    await advanceToPreviewStep('insert');
    fireEvent.click(screen.getByTestId('data-transfer-execute'));

    await waitFor(() => expect(screen.getByTestId('data-transfer-result')).toBeTruthy());
    const reReview = screen.getByTestId('data-transfer-rereview');
    expect(reReview).toHaveAttribute('data-blocked', 'false');
    expect(reReview).not.toBeDisabled();
    expect(screen.queryByTestId('data-transfer-rereview-blocked')).toBeNull();

    fireEvent.click(reReview);
    // A fresh review starts from the selection, not from the settled verdict.
    await waitFor(() => expect(screen.queryByTestId('data-transfer-result')).toBeNull());
    expect(screen.getByTestId('data-transfer-mapping-step')).toBeTruthy();
  });

  it('renders a confirmed destructive preamble with rolled-back rows as nothing applied', async () => {
    applyTransferJobMock.mockResolvedValueOnce({
      ...applySuccess,
      state: 'cancelled',
      effectOutcome: 'rolledBack',
      progress: { read: 3, converted: 3, attempted: 3, committed: 0, unknown: 0 },
      commitBoundaries: [],
      cancelled: true,
      error: 'destructive preamble rolled the batch back',
    } as unknown as TransferApplyJobView);

    await advanceToPreviewStep('truncateInsert');
    fireEvent.click(screen.getByTestId('data-transfer-execute'));
    // A destructive write mode asks once more before anything is written; the
    // apply only happens after that second, explicit confirmation.
    await waitFor(() => expect(screen.getByTestId('data-transfer-execute-confirm')).toBeTruthy());
    expect(applyTransferJobMock).not.toHaveBeenCalled();
    fireEvent.click(screen.getByTestId('data-transfer-execute-confirm-proceed'));

    await waitFor(() => expect(screen.getByTestId('data-transfer-result')).toBeTruthy());
    // Rolled back is a settlement, not a success: the verdict panel must not
    // read `ok`, and nothing may be counted as committed. It reads `failed`
    // rather than `partial` because a rolled-back run reported an error and
    // applied nothing — dressing that up as partial progress would be a lie.
    expect(screen.getByTestId('data-transfer-result')).toHaveAttribute(
      'data-verdict-severity',
      'failed',
    );
    expect(screen.getByTestId('data-transfer-result')).toHaveAttribute('data-completed', 'false');
    expect(screen.getByTestId('data-transfer-job-verdict')).toHaveAttribute('data-cancel-disposition', 'none');
    expect(screen.getByTestId('data-transfer-job-verdict-status')).toHaveTextContent(
      'migration.verdict.failed',
    );
    expect(screen.getByTestId('data-transfer-job-rows')).toHaveTextContent(
      'migration.verdict.committedRows: 0',
    );
    expect(screen.getByTestId('data-transfer-job-boundaries-empty')).toBeTruthy();
    expect(screen.getByTestId('data-transfer-job-error')).toHaveTextContent(
      'destructive preamble rolled the batch back',
    );
    // The preamble was confirmed, so the destructive call went out with it.
    expect(applyTransferJobMock).toHaveBeenCalledWith(
      expect.objectContaining({ confirmedDestructive: true }),
    );
  });

  it('[tester] renders a cancelled execution distinctly from success', async () => {
    // Cancelled before any row moved: "not started" is not a failure and is
    // certainly not a success, so it must read as its own state.
    applyTransferJobMock.mockResolvedValueOnce({
      ...applySuccess,
      state: 'cancelled',
      effectOutcome: 'notStarted',
      progress: { read: 0, converted: 0, attempted: 0, committed: 0, unknown: 0 },
      commitBoundaries: [],
      cancelled: true,
      error: null,
    } as unknown as TransferApplyJobView);

    await advanceToPreviewStep('insert');
    fireEvent.click(screen.getByTestId('data-transfer-execute'));

    await waitFor(() => expect(screen.getByTestId('data-transfer-result')).toBeTruthy());
    expect(screen.getByTestId('data-transfer-result')).toHaveAttribute(
      'data-verdict-severity',
      'partial',
    );
    expect(screen.getByTestId('data-transfer-result')).toHaveAttribute('data-completed', 'false');
    expect(screen.getByTestId('data-transfer-job-verdict-status')).toHaveTextContent(
      'migration.verdict.partial',
    );
    expect(screen.getByTestId('data-transfer-job-verdict-status')).not.toHaveTextContent(
      'migration.verdict.ok',
    );
    expect(screen.getByTestId('data-transfer-job-rows')).toHaveTextContent(
      'migration.verdict.committedRows: 0',
    );
    expect(screen.queryByTestId('data-transfer-job-error')).toBeNull();
    expect(screen.getByTestId('data-transfer-rereview')).toBeTruthy();
    expect(screen.queryByTestId('data-transfer-resume')).toBeNull();
  });

  it('retries a lost commit receipt under the same idempotency key instead of a fresh write', async () => {
    await advanceToPreviewStep('insert');

    // The apply may well have committed; only the receipt was lost.
    // The window must not present that as a settlement, and the retry has to
    // repeat the *same* key so the backend's receipt map can answer with the
    // recorded receipt rather than committing a second time.
    applyTransferJobMock.mockRejectedValueOnce(new Error('commit ack lost: transport closed'));
    fireEvent.click(screen.getByTestId('data-transfer-execute'));

    await waitFor(() => expect(screen.getByTestId('data-transfer-preview')).toBeTruthy());
    expect(screen.queryByTestId('data-transfer-result')).toBeNull();
    expect(screen.getByTestId('data-transfer-job-failure')).toHaveAttribute(
      'data-failure-kind',
      'other',
    );
    expect(screen.getByTestId('data-transfer-job-failure-detail')).toHaveTextContent(
      'commit ack lost: transport closed',
    );

    applyTransferJobMock.mockResolvedValueOnce({ ...applySuccess, replayed: true });
    fireEvent.click(screen.getByTestId('data-transfer-execute'));

    await waitFor(() => expect(screen.getByTestId('data-transfer-result')).toBeTruthy());
    expect(screen.getByTestId('data-transfer-job-replayed')).toHaveTextContent(
      'migration.verdict.replayedReceipt',
    );
    const keys = applyTransferJobMock.mock.calls.map(
      (call) => (call[0] as { idempotencyKey?: string }).idempotencyKey,
    );
    expect(keys).toEqual(['data-transfer/apply/plan-job-1', 'data-transfer/apply/plan-job-1']);
  });
});
