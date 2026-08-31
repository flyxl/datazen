import { describe, expect, it, vi } from 'vitest';
import type { DatabaseType } from '../../types';

// The worktree intentionally lacks the ignored generated builtinLocales.ts file. Keep this
// domain test isolated from that unrelated generated module while exercising the metadata API.
vi.mock('../databaseTypes', () => ({
  escapeIdent: (name: string, databaseType?: string) => {
    const quote = databaseType === 'mysql' ? '`' : '"';
    return `${quote}${name.replaceAll(quote, `${quote}${quote}`)}${quote}`;
  },
}));

import {
  buildQueryOpenContext,
  buildSafeTableSqlTemplate,
  buildTableContext,
  buildTableSqlAction,
  buildTableSqlActions,
  quoteTableIdentifier,
} from '../tableSqlActions';

const tableInput = {
  connectionId: 'connection-1',
  dbSessionId: 'session-1',
  databaseType: 'mysql' as DatabaseType,
  database: 'app',
  tableSchema: 'sales',
  tableName: 'order`items',
};

describe('tableSqlActions', () => {
  it('builds a complete table context from panel-shaped input', () => {
    expect(buildTableContext(tableInput)).toEqual({
      connectionId: 'connection-1',
      dbSessionId: 'session-1',
      databaseType: 'mysql',
      database: 'app',
      schema: 'sales',
      tableName: 'order`items',
    });
  });

  it('quotes schema and table identifiers through driver metadata', () => {
    const context = buildTableContext(tableInput);

    expect(quoteTableIdentifier(context)).toBe('`sales`.`order``items`');
    expect(buildSafeTableSqlTemplate(context, 'select')).toBe(
      'SELECT * FROM `sales`.`order``items`;',
    );
  });

  it('returns the five actions in the fixed product order without executing SQL', () => {
    const actions = buildTableSqlActions(tableInput);

    expect(actions.map((action) => action.kind)).toEqual([
      'openData',
      'select',
      'insert',
      'update',
      'ddl',
    ]);
    expect(actions.every((action) => action.execution === 'draft-only')).toBe(true);
    expect(actions.map((action) => action.context.tableName)).toEqual(
      Array.from({ length: 5 }, () => 'order`items'),
    );
    expect(actions.find((action) => action.kind === 'openData')?.context.focus).toBe('result');
  });

  it('keeps connection/database/schema/table context on query-open actions', () => {
    const context = buildQueryOpenContext(tableInput, {
      kind: 'select',
      source: 'object-search',
      focus: 'editor',
    });

    expect(context).toMatchObject({
      connectionId: 'connection-1',
      dbSessionId: 'session-1',
      database: 'app',
      schema: 'sales',
      tableName: 'order`items',
      source: 'object-search',
      focus: 'editor',
    });
    expect(context.initialSql).toContain('`sales`.`order``items`');
  });

  it('does not accept credentials or invoke a command when building drafts', () => {
    const action = buildTableSqlAction(
      { ...tableInput, password: 'secret' } as typeof tableInput,
      'update',
    );

    expect(action.context).not.toHaveProperty('password');
    expect(action.sqlTemplate).toContain('driver-generated predicate');
    expect(action.requiresDriverGeneration).toBe(true);
  });

  it('rejects incomplete or non-table object contexts before building SQL', () => {
    expect(() =>
      buildTableContext({
        ...tableInput,
        objectType: 'function',
        name: 'refresh_users',
      }),
    ).toThrow(/cannot produce a table context/);
    expect(() => buildTableContext({ ...tableInput, tableName: '  ' })).toThrow(/tableName/);
  });
});
