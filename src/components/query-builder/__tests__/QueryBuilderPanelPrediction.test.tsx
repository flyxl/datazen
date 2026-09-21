/**
 * Wiring test for smart foreign-key prediction in the Visual Query Builder.
 *
 * The engine itself is covered under `src/lib/relationPrediction`; what is
 * pinned here is the panel's decision rule — an inference reaches the canvas as
 * a *candidate* carrying `origin: 'predicted'` and never as a confirmed join,
 * while the settings switch governs inference only.
 */
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, render, waitFor } from '@testing-library/react';
import { afterEach } from 'vitest';
import { QueryBuilderPanel } from '../QueryBuilderPanel';
import { useQueryBuilderStore } from '../../../stores/queryBuilderStore';
import { useSettingsStore } from '../../../stores/settingsStore';
import { getCachedTableSchema } from '../../../lib/schemaCache';
import type { ColumnSchema, ForeignKeyInfo, TableSchema } from '../../../types';

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

vi.mock('../../../lib/schemaCache', () => ({
  getCachedTableSchema: vi.fn(),
}));

const loadSchema = vi.mocked(getCachedTableSchema);

const column = (name: string, dataType = 'integer'): ColumnSchema => ({
  name,
  dataType,
  nullable: name !== 'id',
});

function tableSchema(tableName: string, foreignKeys: ForeignKeyInfo[] = []): TableSchema {
  return {
    tableName,
    columns: tableName === 'users' ? [column('id')] : [column('id'), column('user_id')],
    primaryKeys: ['id'],
    indexes: [],
    foreignKeys,
  };
}

function seedTables(...tables: string[]) {
  const store = useQueryBuilderStore.getState();
  store.openFor('panel-predict');
  for (const table of tables) useQueryBuilderStore.getState().toggleTable(table);
  render(
    <QueryBuilderPanel
      panelId="panel-predict"
      dbSessionId="session-1"
      databaseType="postgresql"
      currentSql=""
      onCommit={vi.fn()}
      onCancel={vi.fn()}
    />,
  );
}

const candidates = () => useQueryBuilderStore.getState().autoJoins;

beforeEach(() => {
  useQueryBuilderStore.setState(useQueryBuilderStore.getInitialState());
  useSettingsStore.setState((s) => ({ settings: { ...s.settings, enableFkPrediction: true } }));
});

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe('QueryBuilderPanel — foreign key prediction', () => {
  it('offers an inference as a predicted candidate, never as a confirmed join', async () => {
    loadSchema.mockImplementation((_session, table: string) => Promise.resolve(tableSchema(table)));
    seedTables('orders', 'users');

    await waitFor(() => {
      expect(candidates().some((join) => join.origin === 'predicted')).toBe(true);
    });
    // A guess waits for the user: it must not already be in the SQL.
    expect(useQueryBuilderStore.getState().joins).toEqual([]);
  });

  it('stops offering inferences when the setting is turned off', async () => {
    loadSchema.mockImplementation((_session, table: string) => Promise.resolve(tableSchema(table)));
    seedTables('orders', 'users');
    await waitFor(() => {
      expect(candidates().some((join) => join.origin === 'predicted')).toBe(true);
    });

    useSettingsStore.setState((s) => ({ settings: { ...s.settings, enableFkPrediction: false } }));
    await waitFor(() => {
      expect(candidates()).toEqual([]);
    });
  });

  it('still offers declared constraints with prediction off', async () => {
    loadSchema.mockImplementation(async (_session, table: string) => {
      if (table === 'users') return tableSchema('users');
      return tableSchema('orders', [
        {
          name: 'fk_orders_user',
          columns: ['user_id'],
          referencedTable: 'users',
          referencedColumns: ['id'],
        },
      ]);
    });
    useSettingsStore.setState((s) => ({ settings: { ...s.settings, enableFkPrediction: false } }));
    seedTables('orders', 'users');

    await waitFor(() => {
      expect(candidates()).toHaveLength(1);
    });
    expect(candidates()[0]!.origin).toBe('declared');
    expect(candidates()[0]!.constraint).toContain('fk_orders_user');
  });
});
