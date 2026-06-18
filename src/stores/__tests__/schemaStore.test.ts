import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';

vi.mock('../../commands/database', () => ({
  databaseCommands: {
    getDatabases: vi.fn().mockResolvedValue(['testdb']),
    getTables: vi.fn().mockResolvedValue([
      { name: 'users', tableType: 'TABLE', schema: 'public', rowCount: null },
      { name: 'products', tableType: 'TABLE', schema: 'public', rowCount: null },
      { name: 'orders', tableType: 'TABLE', schema: 'public', rowCount: null },
    ]),
    getColumns: vi.fn().mockResolvedValue(['id', 'name']),
  },
}));

describe('schemaStore.loadTables', () => {
  let useSchemaStore: typeof import('../../stores/schemaStore').useSchemaStore;
  let databaseCommands: typeof import('../../commands/database').databaseCommands;

  beforeEach(async () => {
    vi.resetModules();
    vi.useFakeTimers();
    const storeMod = await import('../../stores/schemaStore');
    useSchemaStore = storeMod.useSchemaStore;
    const cmdMod = await import('../../commands/database');
    databaseCommands = cmdMod.databaseCommands;
    useSchemaStore.setState({ connectionId: 'test-conn' });
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('does not call getColumns during loadTables', async () => {
    await useSchemaStore.getState().loadTables('testdb');

    expect(databaseCommands.getTables).toHaveBeenCalledOnce();
    expect(databaseCommands.getColumns).not.toHaveBeenCalled();
  });

  it('populates tables but leaves columnMap empty after loadTables', async () => {
    await useSchemaStore.getState().loadTables('testdb');

    const state = useSchemaStore.getState();
    expect(state.tables).toHaveLength(3);
    expect(Object.keys(state.columnMap)).toHaveLength(0);
  });
});

describe('schemaStore.loadColumnMap', () => {
  let useSchemaStore: typeof import('../../stores/schemaStore').useSchemaStore;
  let databaseCommands: typeof import('../../commands/database').databaseCommands;

  beforeEach(async () => {
    vi.resetModules();
    vi.clearAllMocks();
    const storeMod = await import('../../stores/schemaStore');
    useSchemaStore = storeMod.useSchemaStore;
    const cmdMod = await import('../../commands/database');
    databaseCommands = cmdMod.databaseCommands;
    useSchemaStore.setState({ connectionId: 'test-conn' });
  });

  it('loads columns for all tables sequentially when called', async () => {
    await useSchemaStore.getState().loadTables('testdb');
    expect(databaseCommands.getColumns).not.toHaveBeenCalled();

    await useSchemaStore.getState().loadColumnMap();

    const state = useSchemaStore.getState();
    expect(databaseCommands.getColumns).toHaveBeenCalledTimes(3);
    expect(state.columnMap).toEqual({
      users: ['id', 'name'],
      products: ['id', 'name'],
      orders: ['id', 'name'],
    });
  });

  it('does nothing when connectionId is null', async () => {
    useSchemaStore.setState({ connectionId: null });
    await useSchemaStore.getState().loadColumnMap();
    expect(databaseCommands.getColumns).not.toHaveBeenCalled();
  });
});
