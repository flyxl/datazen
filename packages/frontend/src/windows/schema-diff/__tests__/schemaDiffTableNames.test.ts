import { describe, expect, it } from 'vitest';
import type { TableInfo } from '../../../types';
import {
  enabledSourceTableNames,
  enabledTargetOnlyTableNames,
  mergeSchemaDiffTablePicks,
} from '../schemaDiffTableNames';

function table(
  name: string,
  schema?: string,
  tableType: TableInfo['tableType'] = 'table',
): TableInfo {
  return { name, schema, tableType };
}

describe('Schema Diff table picker identity', () => {
  it('merges mixed inventories by relation identity and keeps endpoint selectors', () => {
    const picks = mergeSchemaDiffTablePicks(
      [table('users', 'public'), table('source_only', 'public'), table('view', 'public', 'view')],
      [table('users', 'public'), table('target_only', 'public'), table('view', 'public', 'view')],
      'public',
      'public',
    );

    expect(picks).toEqual([
      {
        name: 'public.source_only',
        enabled: true,
        origin: 'source-only',
        sourceName: 'public.source_only',
        targetName: undefined,
      },
      {
        name: 'public.target_only',
        enabled: false,
        origin: 'target-only',
        sourceName: undefined,
        targetName: 'public.target_only',
      },
      {
        name: 'public.users',
        enabled: true,
        origin: 'both',
        sourceName: 'public.users',
        targetName: 'public.users',
      },
    ]);
  });

  it('uses relation identity across dialect qualification and filters non-tables', () => {
    const picks = mergeSchemaDiffTablePicks(
      [table('users', 'public'), table('ignored', 'public', 'view')],
      [table('users'), table('target_only')],
      'public',
      '',
    );

    expect(picks.map((pick) => [pick.name, pick.origin])).toEqual([
      ['public.users', 'both'],
      ['target_only', 'target-only'],
    ]);
    expect(picks.find((pick) => pick.origin === 'both')).toMatchObject({
      sourceName: 'public.users',
      targetName: 'users',
    });
  });

  it('does not expose empty-schema sentinels as table rows', () => {
    const picks = mergeSchemaDiffTablePicks(
      [table('', 'public', 'systemTable'), table('', 'app', 'table'), table('users', 'public')],
      [table('', 'public', 'systemTable'), table('users', 'public')],
      'public',
      'public',
    );

    expect(picks.map((pick) => pick.name)).toEqual(['public.users']);
  });

  it('separates source and target-only IPC selections and handles empty selection', () => {
    const picks = mergeSchemaDiffTablePicks([table('users')], [table('users'), table('archive')]);
    expect(enabledSourceTableNames(picks)).toEqual(['users']);
    expect(enabledTargetOnlyTableNames(picks)).toEqual([]);

    const targetOnly = picks.map((pick) =>
      pick.origin === 'target-only' ? { ...pick, enabled: true } : pick,
    );
    expect(enabledSourceTableNames(targetOnly)).toEqual(['users']);
    expect(enabledTargetOnlyTableNames(targetOnly)).toEqual(['archive']);
    expect(enabledSourceTableNames([])).toEqual([]);
    expect(enabledTargetOnlyTableNames([])).toEqual([]);
  });
});
