import { describe, expect, it, beforeEach } from 'vitest';
import { nextPanelId, panelSchema, resetPanelIdCounter, resolveNextActive } from '../panelTypes';
import type { QueryPanel } from '../panelTypes';

describe('[tester] panelTypes', () => {
  beforeEach(() => {
    resetPanelIdCounter();
  });

  it('nextPanelId generates unique prefixed ids', () => {
    expect(nextPanelId('tbl')).toMatch(/^panel-tbl-\d+$/);
    expect(nextPanelId('tbl')).not.toBe(nextPanelId('tbl'));
  });

  it('resolveNextActive keeps current when removing unrelated panel', () => {
    const panels: QueryPanel[] = [
      {
        id: 'a',
        type: 'query',
        title: 'A',
        connectionId: 'c',
        dbSessionId: 's',
        connectionName: 'n',
        databaseType: 'postgresql',
      },
      {
        id: 'b',
        type: 'query',
        title: 'B',
        connectionId: 'c',
        dbSessionId: 's',
        connectionName: 'n',
        databaseType: 'postgresql',
      },
    ];
    expect(resolveNextActive(panels, 'b', 'a')).toBe('a');
  });

  it('resolveNextActive selects neighbor when active panel is removed', () => {
    const panels: QueryPanel[] = [
      {
        id: 'a',
        type: 'query',
        title: 'A',
        connectionId: 'c',
        dbSessionId: 's',
        connectionName: 'n',
        databaseType: 'postgresql',
      },
      {
        id: 'b',
        type: 'query',
        title: 'B',
        connectionId: 'c',
        dbSessionId: 's',
        connectionName: 'n',
        databaseType: 'postgresql',
      },
    ];
    expect(resolveNextActive(panels, 'b', 'b')).toBe('a');
    expect(resolveNextActive([panels[0]], 'a', 'a')).toBeNull();
  });
});

describe('[tester] panelSchema', () => {
  const base = {
    id: 'p1',
    connectionId: 'c',
    dbSessionId: 's',
    connectionName: 'n',
    databaseType: 'postgresql',
  } as const;

  it('reads the schema each panel kind is scoped to', () => {
    expect(
      panelSchema({
        ...base,
        type: 'table',
        tableName: 't',
        database: 'd',
        tableSchema: 'public',
        subTab: 'data',
      }),
    ).toBe('public');
    expect(
      panelSchema({
        ...base,
        type: 'view',
        viewName: 'v',
        database: 'd',
        viewSchema: 'reporting',
        subTab: 'data',
      }),
    ).toBe('reporting');
    expect(
      panelSchema({ ...base, type: 'query', title: 'q', database: 'd', schema: 'sales' }),
    ).toBe('sales');
    expect(
      panelSchema({ ...base, type: 'create-table', database: 'd', tableSchema: 'sales' }),
    ).toBe('sales');
    expect(
      panelSchema({
        ...base,
        type: 'db-object',
        objectKind: 'function',
        objectName: 'f',
        objectSchema: 'audit',
      }),
    ).toBe('audit');
    expect(panelSchema({ ...base, type: 'er-diagram', schema: 'reporting' })).toBe('reporting');
  });

  it('returns null — never the database — when a panel carries no schema', () => {
    // A schema is a namespace inside a database; substituting the database name
    // would make a schema-aware driver resolve the wrong namespace and a
    // schema-less driver reject the argument outright.
    expect(panelSchema({ ...base, type: 'objects' })).toBeNull();
    expect(panelSchema({ ...base, type: 'privileges' })).toBeNull();
    expect(
      panelSchema({ ...base, type: 'query', title: 'q', database: 'd', schema: null }),
    ).toBeNull();
    expect(panelSchema(null)).toBeNull();
    expect(panelSchema(undefined)).toBeNull();
  });
});
