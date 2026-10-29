import { describe, expect, it } from 'vitest';
import { EditorState } from '@codemirror/state';
import { CompletionContext } from '@codemirror/autocomplete';
import type { SQLNamespace } from '@codemirror/lang-sql';
import type { CompletionQuotePolicy } from '../../contracts';
import type { EditorMetadataSnapshot, EditorRelationMetadata } from '../../metadata/types';
import { buildSemanticModel, getDialectAdapter } from '@datazen/extension-points';
import { produceSchemaCompletions, type SchemaCompletionOptions } from '../schemaCompletion';

// Host contract only: standard SQL identifiers and double quotes, no driver fixtures.
const adapter = getDialectAdapter('standard');
const columnNames = ['name', 'order', 'display name', 'a"b'];
const users: EditorRelationMetadata = {
  key: 'users',
  identity: { namespacePath: [], name: { name: 'users', quoted: false } },
  kind: 'table',
  columns: columnNames.map((name) => ({
    name,
    dataType: 'varchar',
    nullable: true,
    isPrimaryKey: false,
    comment: 'Customer attribute',
  })),
  primaryKey: [],
  indexes: [],
  foreignKeys: [],
  loadedAt: 0,
};
function snapshot(relations: EditorRelationMetadata[]): EditorMetadataSnapshot {
  return {
    dbSessionId: 'prefix-tests',
    database: 'test',
    epoch: 1,
    relations: new Map(relations.map((relation) => [relation.key, relation])),
  };
}
const populatedSnapshot = snapshot([users]);
const emptySnapshot = snapshot([]);
const flatSchema: SQLNamespace = { users: columnNames };
const nestedSchema: SQLNamespace = { catalog: { app: { users: columnNames } } };
const preferences = [
  { title: 'default (omitted)', options: {}, enabled: true },
  { title: 'true', options: { includeTablePrefix: true }, enabled: true },
  { title: 'false', options: { includeTablePrefix: false }, enabled: false },
] as const;
const policies: CompletionQuotePolicy[] = ['unquoted', 'always', 'both'];
type Options = Omit<SchemaCompletionOptions, 'model' | 'adapter'>;

/** A | marker places the cursor before a suffix that replacement must preserve. */
function complete(markedSql: string, options: Options) {
  const marker = markedSql.indexOf('|');
  const sql = markedSql.replace('|', '');
  const pos = marker < 0 ? sql.length : marker;
  const model = buildSemanticModel(sql, pos, { dialectId: 'standard' });
  const state = EditorState.create({ doc: sql });
  const context = new CompletionContext(state, pos, true);
  // Like editorExtensions, anchor replacement at the current identifier, not
  // the semantic intent's cursor-only range; a preceding dot is never replaced.
  const from = context.matchBefore(/[\w$"']+/)?.from ?? pos;
  const items = produceSchemaCompletions({ ...options, model, adapter });
  return {
    model,
    items,
    accept(label: string) {
      const item = items.find((candidate) => candidate.label === label);
      expect(item, `Completion ${label} in ${markedSql}`).toBeDefined();
      const insert = item?.apply ?? item?.label;
      if (typeof insert !== 'string') throw new Error('Expected string completion insertion');
      // Apply the completion edit and assert resulting SQL, not only menu text.
      return state.update({ changes: { from, to: pos, insert } }).state.doc.toString();
    },
  };
}

const contexts = [
  {
    title: 'visible table',
    sql: 'SELECT na| FROM users',
    prefix: 'users.',
    suffix: ' FROM users',
    snapshot: populatedSnapshot,
  },
  {
    title: 'visible alias',
    sql: 'SELECT na| FROM users u',
    prefix: 'u.',
    suffix: ' FROM users u',
    snapshot: populatedSnapshot,
  },
  {
    title: 'snapshot fallback without FROM',
    sql: 'SELECT na',
    prefix: '"users".',
    suffix: '',
    snapshot: populatedSnapshot,
    // Snapshot columns must win over unrelated schema fallback columns.
    schema: { other: ['schema_only'] },
  },
  {
    title: 'flat editor schema fallback',
    sql: 'SELECT na',
    prefix: '"users".',
    suffix: '',
    snapshot: emptySnapshot,
    schema: flatSchema,
  },
  {
    title: 'nested editor schema fallback with zero-column snapshot',
    sql: 'SELECT na',
    prefix: '"users".',
    suffix: '',
    snapshot: snapshot([{ ...users, columns: [] }]),
    schema: nestedSchema,
  },
];

describe.each(contexts)('includeTablePrefix: $title', (context) => {
  describe.each(preferences)('preference $title', (preference) => {
    it.each(policies)(
      'preserves %s quote policy and applies the selected replacement',
      (quotePolicy) => {
        const result = complete(context.sql, {
          snapshot: context.snapshot,
          schema: context.schema,
          ...preference.options,
          quotePolicy,
        });
        expect(result.model.cursorIntent.kind).toBe('projection');
        const columns = result.items.filter((item) => item.type === 'property');
        const safeLabels =
          quotePolicy === 'both'
            ? ['name', '"name"']
            : [quotePolicy === 'always' ? '"name"' : 'name'];
        const labels = [...safeLabels, '"order"', '"display name"', '"a""b"'];
        expect(columns.map((item) => item.label)).toEqual(labels);
        const prefix = preference.enabled ? context.prefix : '';
        for (const label of labels) {
          const item = columns.find((column) => column.label === label)!;
          expect(item.apply).toBe(`${prefix}${label}`);
          expect(item.filterText).toBe(adapter.unquoteIdentifier(label) ?? label);
          expect(result.accept(label)).toBe(`SELECT ${prefix}${label}${context.suffix}`);
        }
        // Toggling insertion policy must not remove aliases or change menu metadata/ranking.
        const enabled = complete(context.sql, {
          snapshot: context.snapshot,
          schema: context.schema,
          includeTablePrefix: true,
          quotePolicy,
        });
        const withoutApply = (items: typeof result.items) =>
          items.map(({ apply: _apply, ...rest }) => rest);
        expect(withoutApply(result.items)).toEqual(withoutApply(enabled.items));
        if (context.title === 'visible alias') {
          expect(result.items).toContainEqual(
            expect.objectContaining({ label: 'u', type: 'variable' }),
          );
        }
      },
    );
  });
});

describe.each(preferences)('explicit qualification with preference $title', (preference) => {
  it.each(policies)('keeps typed table/alias dots under %s quoting', (quotePolicy) => {
    for (const qualifier of ['users', 'u', '"users"']) {
      const from = qualifier === 'u' ? 'users u' : 'users';
      for (const fragment of ['', 'n', 'na']) {
        const result = complete(`SELECT ${qualifier}.${fragment}| FROM ${from}`, {
          snapshot: populatedSnapshot,
          ...preference.options,
          quotePolicy,
        });
        expect(result.model.cursorIntent.kind).toBe('qualified_column');
        expect(result.model.cursorIntent.qualifierParts).toEqual([qualifier.replaceAll('"', '')]);
        const label = quotePolicy === 'always' ? '"name"' : 'name';
        expect(result.items.find((item) => item.label === label)?.apply).toBe(label);
        expect(result.accept(label)).toBe(`SELECT ${qualifier}.${label} FROM ${from}`);
        expect(result.accept('"order"')).toBe(`SELECT ${qualifier}."order" FROM ${from}`);
      }
    }
  });

  it('keeps explicit alias dots when columns come from the schema tree', () => {
    const result = complete('SELECT u.na| FROM users u', {
      snapshot: emptySnapshot,
      schema: flatSchema,
      ...preference.options,
    });
    expect(result.model.cursorIntent.kind).toBe('qualified_column');
    expect(result.accept('name')).toBe('SELECT u.name FROM users u');
  });

  it('does not change schema-dot table completion or duplicate the namespace', () => {
    const result = complete('SELECT * FROM app.us', {
      snapshot: emptySnapshot,
      schema: { app: flatSchema },
      ...preference.options,
    });
    expect(result.model.cursorIntent.kind).toBe('qualified_column');
    expect(result.accept('"users"')).toBe('SELECT * FROM app."users"');
  });
});

describe.each(preferences)('continuous SQL typing with preference $title', (preference) => {
  it('enters and exits projection, FROM, explicit dot, and predicate states after acceptance', () => {
    const options: Options = { snapshot: populatedSnapshot, ...preference.options };
    let sql = 'SELECT ';
    for (const key of ['n', 'a']) {
      sql += key;
      const result = complete(sql, options);
      expect(result.model.cursorIntent.kind).toBe('projection');
      expect(result.items.find((item) => item.label === 'name')?.apply).toBe(
        preference.enabled ? '"users".name' : 'name',
      );
    }
    sql = complete(sql, options).accept('name');
    const projection = preference.enabled ? '"users".name' : 'name';
    expect(sql).toBe(`SELECT ${projection}`);

    sql += ' FROM ';
    expect(complete(sql, options).model.cursorIntent.kind).toBe('relation');
    for (const key of ['u', 's']) {
      sql += key;
      const result = complete(sql, options);
      expect(result.model.cursorIntent.kind).toBe('relation');
      expect(result.items.every((item) => item.type === 'type')).toBe(true);
    }
    sql = complete(sql, options).accept('"users"');
    sql += ' ';
    expect(complete(sql, options).model.cursorIntent.kind).not.toBe('relation');
    sql += 'WHERE ';
    expect(complete(sql, options).model.cursorIntent.kind).toBe('projection');
    sql += 'users';
    expect(complete(sql, options).model.cursorIntent.kind).toBe('projection');
    sql += '.';
    for (const key of ['', 'n', 'a']) {
      sql += key;
      const result = complete(sql, options);
      expect(result.model.cursorIntent.kind).toBe('qualified_column');
      expect(result.model.cursorIntent.qualifierParts).toEqual(['users']);
      expect(result.items.find((item) => item.label === 'name')?.apply).toBe('name');
    }
    sql = complete(sql, options).accept('name');
    expect(sql).toBe(`SELECT ${projection} FROM "users" WHERE users.name`);
    sql += " = 'Ada' AND ";
    const predicateStart = sql;
    for (const key of ['n', 'a']) {
      sql += key;
      const result = complete(sql, options);
      expect(result.model.cursorIntent.kind).toBe('projection');
      expect(result.model.cursorIntent.qualifierParts).toEqual([]);
    }
    sql = complete(sql, options).accept('name');
    expect(sql).toBe(`${predicateStart}${preference.enabled ? 'users.name' : 'name'}`);
    // Deletion back through a fragment and the dot must release explicit qualification.
    const bare = `${predicateStart}users`;
    expect(complete(`${bare}.n`, options).model.cursorIntent.kind).toBe('qualified_column');
    expect(complete(`${bare}.`, options).model.cursorIntent.kind).toBe('qualified_column');
    expect(complete(bare, options).model.cursorIntent.kind).toBe('projection');
    expect(complete(predicateStart, options).model.cursorIntent.qualifierParts).toEqual([]);
  });
});
