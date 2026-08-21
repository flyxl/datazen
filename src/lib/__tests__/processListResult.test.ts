import { describe, expect, it } from 'vitest';
import { commandResultColumns, commandResultRows } from '../processListResult';

describe('processListResult normalization', () => {
  it('places the client column directly after database and before state', () => {
    const cols = commandResultColumns();
    const names = cols.map((c) => c.id);
    const dbIdx = names.indexOf('database');
    const clientIdx = names.indexOf('client');
    const stateIdx = names.indexOf('state');
    expect(clientIdx).toBeGreaterThan(dbIdx);
    expect(clientIdx).toBeLessThan(stateIdx);
  });

  it('maps the client column to the 客户端 header', () => {
    const t = (k: string) =>
      ({
        'processList.colClient': '客户端',
      })[k] ?? k;
    const cols = commandResultColumns([{ name: 'client', dataType: 'string', nullable: true }], t);
    expect(cols.find((c) => c.id === 'client')?.name).toBe('客户端');
  });

  it('maps a `processes` record preserving the client value', () => {
    const result = commandResultRows({
      processes: [
        {
          pid: 1,
          user: 'postgres',
          database: 'app',
          client: '127.0.0.1',
          state: 'active',
          query: 'SELECT 1',
          durationMs: 12,
        },
      ],
    });
    const colNames = result.columns.map((c) => c.name);
    const clientIdx = colNames.indexOf('client');
    const dbIdx = colNames.indexOf('database');
    const stateIdx = colNames.indexOf('state');
    expect(clientIdx).toBeGreaterThan(dbIdx);
    expect(clientIdx).toBeLessThan(stateIdx);
    // 行按 DEFAULT_PROCESS_COLUMNS 顺序映射；client 值正确落在该列。
    expect(result.rows[0]?.[clientIdx]).toBe('127.0.0.1');
  });
});
