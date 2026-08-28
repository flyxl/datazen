import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { render, waitFor, cleanup, screen, fireEvent } from '@testing-library/react';
import { TableStructureEditor } from '../TableStructureEditor';

// The mount effect lists `t` in its deps — a fresh function per render would
// retrigger it endlessly, so the mock must return a stable reference.
const stableT = (key: string) => key;
vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: stableT }),
}));

const {
  mockPlanTableStructureChanges,
  mockExecuteQuery,
  mockGetTableSchema,
  mockGetStructureCapabilities,
  mockGetConnectionCommands,
  mockDriverExecute,
  mockConfirmApply,
  mockExportTableStructureToFile,
} = vi.hoisted(() => ({
  mockPlanTableStructureChanges: vi.fn().mockResolvedValue({ statements: [] }),
  mockExecuteQuery: vi.fn().mockResolvedValue({}),
  mockGetTableSchema: vi.fn().mockResolvedValue(null),
  mockGetStructureCapabilities: vi.fn().mockResolvedValue({ createTable: true }),
  mockGetConnectionCommands: vi.fn().mockResolvedValue([]),
  mockDriverExecute: vi.fn().mockResolvedValue({ data: {} }),
  mockConfirmApply: vi.fn().mockResolvedValue(true),
  mockExportTableStructureToFile: vi.fn().mockResolvedValue('ok'),
}));

vi.mock('../../../commands/database', () => ({
  databaseCommands: {
    getTableSchema: (...args: unknown[]) => mockGetTableSchema(...args),
  },
}));

vi.mock('../../../commands/driver', () => ({
  driverCommands: {
    getConnectionCommands: (...args: unknown[]) => mockGetConnectionCommands(...args),
    execute: (...args: unknown[]) => mockDriverExecute(...args),
  },
}));

vi.mock('../../../commands/structure', () => ({
  structureCommands: {
    getStructureCapabilities: (...args: unknown[]) => mockGetStructureCapabilities(...args),
    planTableStructureChanges: (...args: unknown[]) => mockPlanTableStructureChanges(...args),
  },
}));

vi.mock('../../../commands/query', () => ({
  queryCommands: {
    executeQuery: (...args: unknown[]) => mockExecuteQuery(...args),
  },
}));

vi.mock('../../../hooks/useConfirmDialog', () => ({
  useConfirmDialog: () => [mockConfirmApply, null],
}));

vi.mock('../../../lib/exportTableStructure', () => ({
  exportTableStructureToFile: (...args: unknown[]) => mockExportTableStructureToFile(...args),
}));

vi.mock('../../../lib/databaseTypes', () => ({
  DB_REGISTRY: {
    postgresql: {
      supportsSQL: true,
      structureEditor: {
        enabled: true,
        defaultColumnType: 'text',
        indexMethods: ['btree'],
        columnTypes: [{ value: 'text', label: 'text' }],
        fields: {},
      },
    },
    // supportsSQL but no structure editor config → unsupported branch.
    sqlite: {
      supportsSQL: true,
    },
  },
}));

vi.mock('../structure/StructureColumnTable', () => ({
  StructureColumnTable: (props: {
    onUpdate: (id: string, patch: Record<string, unknown>) => void;
    onRemove: (id: string) => void;
    onDragStart: (idx: number) => void;
    onDragOver: (e: unknown, targetIdx: number) => void;
    onDragEnd: () => void;
  }) => (
    <div data-testid="column-table">
      <button
        type="button"
        data-testid="col-update"
        onClick={() => props.onUpdate('c1', { name: 'title' })}
      >
        col-update
      </button>
      <button type="button" data-testid="col-remove" onClick={() => props.onRemove('c1')}>
        col-remove
      </button>
      <button type="button" data-testid="col-dragstart" onClick={() => props.onDragStart(0)}>
        col-dragstart
      </button>
      <button
        type="button"
        data-testid="col-dragover"
        onClick={() => props.onDragOver({ preventDefault: () => undefined } as never, 1)}
      >
        col-dragover
      </button>
      <button type="button" data-testid="col-dragend" onClick={() => props.onDragEnd()}>
        col-dragend
      </button>
    </div>
  ),
}));

vi.mock('../structure/StructureIndexTable', () => ({
  StructureIndexTable: (props: {
    onUpdate: (id: string, patch: Record<string, unknown>) => void;
    onRemove: (id: string) => void;
  }) => (
    <div data-testid="index-table">
      <button
        type="button"
        data-testid="idx-update"
        onClick={() => props.onUpdate('i1', { name: 'idx_a' })}
      >
        idx-update
      </button>
      <button type="button" data-testid="idx-remove" onClick={() => props.onRemove('i1')}>
        idx-remove
      </button>
    </div>
  ),
  suggestedIndexName: () => 'idx_new',
}));

vi.mock('../structure/StructurePlanPreview', () => ({
  StructurePlanPreview: (props: { onClose: () => void }) => (
    <div data-testid="plan-preview">
      <button type="button" data-testid="plan-close" onClick={props.onClose}>
        close
      </button>
    </div>
  ),
}));

beforeEach(() => {
  vi.clearAllMocks();
  mockPlanTableStructureChanges.mockResolvedValue({ statements: [] });
  mockExecuteQuery.mockResolvedValue({});
  mockGetTableSchema.mockResolvedValue(null);
  mockGetStructureCapabilities.mockResolvedValue({ createTable: true });
  mockGetConnectionCommands.mockResolvedValue([]);
  mockDriverExecute.mockResolvedValue({ data: {} });
  mockConfirmApply.mockResolvedValue(true);
  mockExportTableStructureToFile.mockResolvedValue('ok');
});

afterEach(cleanup);

function baseProps(mode: 'create' | 'alter', overrides?: Record<string, unknown>) {
  return {
    dbSessionId: 'conn-1',
    databaseType: 'postgresql' as const,
    database: 'db_b',
    mode,
    tableName: mode === 'alter' ? 'users' : undefined,
    onSuccess: vi.fn(),
    onCancel: vi.fn(),
    ...overrides,
  };
}

async function mountAndLoad(props: Record<string, unknown>) {
  render(<TableStructureEditor {...baseProps('create' as const)} {...props} />);
  await waitFor(() => {
    expect(screen.getByTestId('column-table')).toBeInTheDocument();
  });
}

function fillTableName(name: string) {
  fireEvent.change(screen.getByPlaceholderText('new_table'), { target: { value: name } });
}

/** Alter-mode TableSchema fixture matching the camelCase TS shape. */
const ALTER_SCHEMA = {
  table_name: 'users',
  columns: [
    {
      name: 'id',
      dataType: 'integer',
      nullable: false,
      defaultValue: null,
      comment: null,
      isAutoIncrement: false,
    },
  ],
  primaryKeys: ['id'],
  indexes: [],
};

describe('TableStructureEditor mount (F1: no use_database IPC)', () => {
  it('renders without a session database switch when database prop is set', async () => {
    await mountAndLoad({});

    // F1 removed the useDatabase-on-mount behavior; the editor must come up
    // directly (queries pin the database explicitly instead).
    expect(screen.getByTestId('column-table')).toBeInTheDocument();
  });
});

describe('TableStructureEditor targets the panel database (F1 BUG-003)', () => {
  it('previews DDL against the target database', async () => {
    await mountAndLoad({});
    fillTableName('t_f1');

    fireEvent.click(screen.getByRole('button', { name: 'structEditor.previewSQL' }));

    await waitFor(() => {
      expect(mockPlanTableStructureChanges).toHaveBeenCalledTimes(1);
    });
    expect(mockPlanTableStructureChanges).toHaveBeenCalledWith(
      'conn-1',
      expect.objectContaining({ table: 't_f1' }),
      'db_b',
    );
  });

  it('executes DDL against the target database and succeeds', async () => {
    mockPlanTableStructureChanges.mockResolvedValue({
      statements: [
        {
          sql: 'CREATE TABLE "t_f1" ("id" integer NOT NULL)',
          summary: 'create table',
          risk: 'additive',
        },
      ],
    });
    const onSuccess = vi.fn();
    await mountAndLoad({ onSuccess });
    fillTableName('t_f1');

    fireEvent.click(screen.getByRole('button', { name: 'structEditor.createTable' }));

    await waitFor(() => {
      expect(onSuccess).toHaveBeenCalled();
    });
    expect(mockPlanTableStructureChanges).toHaveBeenCalledWith(
      'conn-1',
      expect.objectContaining({ table: 't_f1' }),
      'db_b',
    );
    // Statement execution runs unqualified on the session that the plan call
    // just pinned to db_b.
    expect(mockExecuteQuery).toHaveBeenCalledWith(
      'conn-1',
      'CREATE TABLE "t_f1" ("id" integer NOT NULL)',
      undefined,
      'db_b',
      null,
    );
  });

  it('passes null when no target database is set', async () => {
    await mountAndLoad({ database: null });
    fillTableName('t_f1');

    fireEvent.click(screen.getByRole('button', { name: 'structEditor.previewSQL' }));

    await waitFor(() => {
      expect(mockPlanTableStructureChanges).toHaveBeenCalledTimes(1);
    });
    expect(mockPlanTableStructureChanges).toHaveBeenCalledWith(
      'conn-1',
      expect.objectContaining({ table: 't_f1' }),
      null,
    );
  });
});

describe('TableStructureEditor branches', () => {
  it('shows the unsupported view when the driver has no SQL support', async () => {
    render(<TableStructureEditor {...baseProps('create')} databaseType={'mysql' as never} />);
    await waitFor(() => {
      expect(screen.getByText('structEditor.notSupported')).toBeInTheDocument();
    });
  });

  it('shows the unsupported view when structure editing is not configured', async () => {
    render(<TableStructureEditor {...baseProps('create')} databaseType="sqlite" />);
    await waitFor(() => {
      expect(screen.getByText('structEditor.notSupported')).toBeInTheDocument();
    });
  });

  it('loads an alter draft from the current table schema', async () => {
    mockGetTableSchema.mockResolvedValue(ALTER_SCHEMA);
    await mountAndLoad({ mode: 'alter', tableName: 'users' });

    expect(mockGetTableSchema).toHaveBeenCalledWith('conn-1', 'users');
    expect(screen.getByTestId('column-table')).toBeInTheDocument();
    expect(mockGetStructureCapabilities).toHaveBeenCalledWith('conn-1');
  });

  it('surfaces schema-load failures in alter mode', async () => {
    mockGetTableSchema.mockRejectedValue(new Error('relation missing'));
    render(<TableStructureEditor {...baseProps('alter')} />);
    await waitFor(() => {
      expect(screen.getByText(/relation missing/)).toBeInTheDocument();
    });
  });

  it('reports preview failures', async () => {
    mockPlanTableStructureChanges.mockRejectedValue(new Error('planner exploded'));
    await mountAndLoad({});
    fillTableName('t_f1');

    fireEvent.click(screen.getByRole('button', { name: 'structEditor.previewSQL' }));

    await waitFor(() => {
      expect(screen.getByText(/planner exploded/)).toBeInTheDocument();
    });
  });

  it('shows and closes the plan preview dialog', async () => {
    mockPlanTableStructureChanges.mockResolvedValue({
      statements: [{ sql: 'CREATE TABLE t ()', summary: 's', risk: 'additive' }],
    });
    await mountAndLoad({});
    fillTableName('t_f1');
    fireEvent.click(screen.getByRole('button', { name: 'structEditor.previewSQL' }));

    await waitFor(() => {
      expect(screen.getByTestId('plan-preview')).toBeInTheDocument();
    });
    fireEvent.click(screen.getByTestId('plan-close'));
    await waitFor(() => {
      expect(screen.queryByTestId('plan-preview')).not.toBeInTheDocument();
    });
  });

  it('reports "no changes" when the plan has no statements', async () => {
    const onSuccess = vi.fn();
    await mountAndLoad({ onSuccess });
    fillTableName('t_f1');

    fireEvent.click(screen.getByRole('button', { name: 'structEditor.createTable' }));

    await waitFor(() => {
      expect(screen.getByText('structEditor.noChanges')).toBeInTheDocument();
    });
    expect(onSuccess).not.toHaveBeenCalled();
  });

  it('reports partially executed DDL when a later statement fails', async () => {
    mockPlanTableStructureChanges.mockResolvedValue({
      statements: [
        { sql: 'ALTER TABLE t ADD c1 int', summary: 'add', risk: 'additive' },
        { sql: 'ALTER TABLE t ADD c2 int', summary: 'add', risk: 'additive' },
      ],
    });
    // First statement applies, second fails → partial-failure message.
    mockExecuteQuery.mockResolvedValueOnce({});
    mockExecuteQuery.mockRejectedValueOnce(new Error('boom'));
    const onSuccess = vi.fn();
    await mountAndLoad({ onSuccess });
    fillTableName('t_f1');

    fireEvent.click(screen.getByRole('button', { name: 'structEditor.createTable' }));

    await waitFor(() => {
      expect(screen.getByText(/structEditor.executePartial/)).toBeInTheDocument();
    });
    expect(onSuccess).not.toHaveBeenCalled();
  });

  it('surfaces execution failures from the outer catch', async () => {
    mockPlanTableStructureChanges.mockRejectedValue(new Error('ddl refused'));
    await mountAndLoad({});
    fillTableName('t_f1');

    fireEvent.click(screen.getByRole('button', { name: 'structEditor.createTable' }));

    await waitFor(() => {
      expect(screen.getByText(/ddl refused/)).toBeInTheDocument();
    });
  });

  it('estimates rows and confirms before applying risky alters', async () => {
    mockGetTableSchema.mockResolvedValue(ALTER_SCHEMA);
    mockGetConnectionCommands.mockResolvedValue([{ id: 'estimate_table_rows' }]);
    mockDriverExecute.mockResolvedValue({ data: { estimatedRows: 12345 } });
    mockPlanTableStructureChanges.mockResolvedValue({
      statements: [{ sql: 'DROP COLUMN legacy', summary: 'drop', risk: 'destructive' }],
    });
    const onSuccess = vi.fn();
    await mountAndLoad({ mode: 'alter', tableName: 'users', onSuccess });

    fireEvent.click(screen.getByRole('button', { name: 'structEditor.saveChanges' }));

    await waitFor(() => {
      expect(onSuccess).toHaveBeenCalled();
    });
    expect(mockDriverExecute).toHaveBeenCalledWith(
      expect.objectContaining({
        dbSessionId: 'conn-1',
        command: 'estimate_table_rows',
        input: expect.objectContaining({ table: 'users' }),
        database: 'db_b',
        schema: null,
      }),
    );
    expect(mockConfirmApply).toHaveBeenCalled();
    expect(mockExecuteQuery).toHaveBeenCalledWith(
      'conn-1',
      'DROP COLUMN legacy',
      undefined,
      'db_b',
      null,
    );
  });

  it('aborts risky alters when the user declines the confirmation', async () => {
    mockGetConnectionCommands.mockResolvedValue([]);
    mockPlanTableStructureChanges.mockResolvedValue({
      statements: [{ sql: 'DROP COLUMN legacy', summary: 'drop', risk: 'destructive' }],
    });
    mockConfirmApply.mockResolvedValue(false);
    const onSuccess = vi.fn();
    await mountAndLoad({ mode: 'alter', tableName: 'users', onSuccess });

    fireEvent.click(screen.getByRole('button', { name: 'structEditor.saveChanges' }));

    await waitFor(() => {
      expect(mockConfirmApply).toHaveBeenCalled();
    });
    expect(mockExecuteQuery).not.toHaveBeenCalledWith('conn-1', 'DROP COLUMN legacy');
    expect(onSuccess).not.toHaveBeenCalled();
  });

  it('handles column, index and drag interactions without crashing', async () => {
    mockGetStructureCapabilities.mockResolvedValue({
      createTable: true,
      createIndex: true,
      reorderColumn: true,
      indexMethods: ['btree'],
    });
    mockPlanTableStructureChanges.mockResolvedValue({
      statements: [{ sql: 'CREATE TABLE t ()', summary: 's', risk: 'additive' }],
    });
    await mountAndLoad({});
    fillTableName('t_f1');

    // Open the preview so later edits clear it (setPreviewPlan(null) paths).
    fireEvent.click(screen.getByRole('button', { name: 'structEditor.previewSQL' }));
    await waitFor(() => {
      expect(screen.getByTestId('plan-preview')).toBeInTheDocument();
    });

    fireEvent.click(screen.getByTestId('col-update'));
    fireEvent.click(screen.getByTestId('col-remove'));
    fireEvent.click(screen.getByRole('button', { name: 'structEditor.addColumn' }));
    fireEvent.click(screen.getByRole('button', { name: 'structEditor.addIndex' }));
    fireEvent.click(screen.getByTestId('idx-update'));
    fireEvent.click(screen.getByTestId('idx-remove'));
    fireEvent.click(screen.getByTestId('col-dragstart'));
    fireEvent.click(screen.getByTestId('col-dragover'));
    fireEvent.click(screen.getByTestId('col-dragend'));

    // Editing a column resets the stale preview plan.
    await waitFor(() => {
      expect(screen.queryByTestId('plan-preview')).not.toBeInTheDocument();
    });
  });

  it('exports the table structure and reports unsupported drivers', async () => {
    mockExportTableStructureToFile.mockResolvedValue('unsupported');
    await mountAndLoad({ mode: 'alter', tableName: 'users' });

    fireEvent.click(screen.getByTestId('struct-editor-export-structure'));

    await waitFor(() => {
      expect(screen.getByText('structEditor.exportUnsupported')).toBeInTheDocument();
    });
    expect(mockExportTableStructureToFile).toHaveBeenCalledWith(
      expect.objectContaining({ dbSessionId: 'conn-1', tableName: 'users' }),
    );
  });

  it('exports the table structure silently on success', async () => {
    mockExportTableStructureToFile.mockResolvedValue('ok');
    await mountAndLoad({ mode: 'alter', tableName: 'users' });

    fireEvent.click(screen.getByTestId('struct-editor-export-structure'));

    await waitFor(() => {
      expect(mockExportTableStructureToFile).toHaveBeenCalledTimes(1);
    });
    expect(screen.queryByText('structEditor.exportUnsupported')).not.toBeInTheDocument();
  });

  it('shows the back button when requested', async () => {
    await mountAndLoad({ showBackButton: true });
    expect(screen.getAllByText('common.back').length).toBeGreaterThan(0);
  });
});
