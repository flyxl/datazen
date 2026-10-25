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
  buildNoPrimaryKeyEditTemplate,
  buildQueryOpenContext,
  buildSafeTableSqlTemplate,
  buildTableContext,
  buildTableSqlAction,
  buildTableSqlActions,
  noPrimaryKeyEditSpec,
  quoteTableIdentifier,
} from '../tableSqlActions';

const tableInput = {
  connectionId: 'connection-1',
  dbSessionId: 'session-1',
  databaseType: 'mysql' as DatabaseType,
  database: 'app',
  // `schema` / `viewSchema` are required on `TableContextInput`; they stay null
  // here so `buildTableContext` still resolves the schema through its
  // `tableSchema` legacy fallback, which is what this fixture exercises.
  schema: null,
  viewSchema: null,
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

  describe('no-primary-key escape hatch (DB-16)', () => {
    it('builds a read-first SELECT … WHERE draft for a table without a primary key', () => {
      const template = buildNoPrimaryKeyEditTemplate(buildTableContext(tableInput));

      // A quoted target, an explicit WHERE placeholder, and a leading comment
      // pair — the draft tells the user *why* the grid refused to edit before
      // handing over the predicate they must supply.
      expect(template).toContain('SELECT * FROM `sales`.`order``items` WHERE');
      expect(template).toContain('primary key');
      expect(template).toMatch(/^-- .*no primary key.*\n-- .*UPDATE\/DELETE/s);
      expect(template.endsWith(';')).toBe(true);
      // Read-first contract: no UPDATE/DELETE/INSERT keyword outside comments.
      expect(template.replace(/^--.*$/gm, '')).not.toMatch(/\b(UPDATE|DELETE|INSERT)\b/);
    });

    it('keeps the escape hatch a select draft focused on the editor', () => {
      const spec = noPrimaryKeyEditSpec(tableInput);

      expect(spec.kind).toBe('select');
      expect(spec.source).toBe('table-action');
      expect(spec.focus).toBe('editor');
      expect(spec.initialSql).toBe(buildNoPrimaryKeyEditTemplate(buildTableContext(tableInput)));
    });

    it('buildQueryOpenContext keeps the supplied draft instead of the generic SELECT', () => {
      const context = buildQueryOpenContext(tableInput, noPrimaryKeyEditSpec(tableInput));

      expect(context.action).toBe('select');
      expect(context.initialSql).toContain('WHERE');
      // The generic SELECT template has no WHERE clause; the escape hatch must
      // not silently degrade to it.
      expect(context.initialSql).not.toBe(buildSafeTableSqlTemplate(context, 'select'));
      expect(context.focus).toBe('editor');
    });
  });
});
