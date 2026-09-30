import { beforeEach, describe, expect, it, vi } from 'vitest';
import { schemaClient } from '../src/ipc/schemaClient';
import { relationKey } from '../src/types/schemaMetadata';

const invoke = vi.hoisted(() => vi.fn());
vi.mock('@tauri-apps/api/core', () => ({ invoke }));

describe('schema metadata transport contract', () => {
  beforeEach(() => {
    invoke.mockReset();
  });

  it('keeps the complete target in input and unwraps the command result', async () => {
    const ref = { database: 'app', schema: 'archive', name: 'users' };
    const output = { results: [{ status: 'ok', value: { ref, columns: [], primaryKeys: [] } }] };
    invoke.mockResolvedValue({ data: output });
    expect(await schemaClient.readColumns('session', [ref])).toEqual(output);
    expect(invoke).toHaveBeenCalledWith('execute_driver_command', {
      request: {
        dbSessionId: 'session',
        command: 'read_relation_columns',
        input: { relations: [ref] },
      },
    });
  });

  it('preserves null schema and exact refresh scope', async () => {
    const relation = { database: 'app', schema: null, name: 'users' };
    invoke.mockResolvedValue({ data: { revision: 2 } });
    await schemaClient.refresh('session', { kind: 'relation', relation });
    expect(invoke).toHaveBeenCalledWith('execute_driver_command', {
      request: {
        dbSessionId: 'session',
        command: 'refresh_schema_metadata',
        input: { scope: { kind: 'relation', relation } },
      },
    });
  });

  it('does not collide on delimiter-bearing identity components', () => {
    const base = { dbSessionId: 'session', database: 'app', schema: null, name: 'users' };
    expect(relationKey({ ...base, database: 'app:public', name: 'users' })).not.toEqual(
      relationKey({ ...base, database: 'app', name: 'public:users' }),
    );
    expect(relationKey(base)).not.toEqual(relationKey({ ...base, schema: 'null' }));
  });
});
