import { act, renderHook } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { databaseCommands } from '../../../commands/database';
import type { DatabaseObject, DatabaseObjectKind } from '../../../types';
import { useSchemaDiffUnifiedObjects } from '../useSchemaDiffUnifiedObjects';

vi.mock('../../../commands/database', () => ({
  databaseCommands: {
    getDatabaseObjects: vi.fn(),
  },
}));

describe('useSchemaDiffUnifiedObjects', () => {
  beforeEach(() => {
    vi.mocked(databaseCommands.getDatabaseObjects).mockImplementation(
      async (_dbSessionId, kind, database) => [
        {
          kind: kind as DatabaseObjectKind,
          name: `${database}-${kind}`,
          schema: 'public',
        } satisfies DatabaseObject,
      ],
    );
  });

  it('loads object metadata from each selected database', async () => {
    const { result } = renderHook(() => useSchemaDiffUnifiedObjects());

    await act(async () => {
      await result.current.load(
        'source-session',
        'target-session',
        'source_db',
        'target_db',
        'public',
        'public',
      );
    });

    expect(databaseCommands.getDatabaseObjects).toHaveBeenCalledWith(
      'source-session',
      'view',
      'source_db',
    );
    expect(databaseCommands.getDatabaseObjects).toHaveBeenCalledWith(
      'target-session',
      'view',
      'target_db',
    );
    expect(result.current.sourceObjects.map((object) => object.name)).toContain('source_db-view');
    expect(result.current.targetObjects.map((object) => object.name)).toContain('target_db-view');
  });
});
