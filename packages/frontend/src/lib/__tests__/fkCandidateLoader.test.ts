/**
 * The foreign-key branch of the candidate loader (DB-20).
 *
 * A single-column foreign-key column loads its candidates from the **target**
 * table, through the same resolution the jump uses — in browse mode, with no
 * key values. These cases pin the wire shape and the one refusal the loader
 * raises on its own: a target whose referenced columns are not unique would
 * make a pick an ambiguous write, so it is refused with a reason (07 §7
 * rule 12).
 */
import { describe, expect, it, vi, beforeEach } from 'vitest';

const resolveForeignKeyTargets = vi.hoisted(() => vi.fn());
const executeQuery = vi.hoisted(() => vi.fn());

vi.mock('../../commands/database', () => ({
  databaseCommands: {
    resolveForeignKeyTargets,
  },
}));

vi.mock('../../commands/query', () => ({
  queryCommands: {
    executeQuery,
  },
}));

import { createCandidateValuesLoader } from '../../windows/connection/tableEditing';
import type { FkCandidateSource } from '../../lib/fkSelector';

const SOURCE: FkCandidateSource = {
  localColumn: 'customer_id',
  constraintName: 'orders_customer_fk',
};

function args(fkSources?: ReadonlyMap<string, FkCandidateSource>) {
  return {
    dbSessionId: 'session-1',
    table: 'orders',
    schema: null,
    database: 'app',
    dialect: undefined,
    fkSources,
  };
}

beforeEach(() => {
  resolveForeignKeyTargets.mockReset();
});

describe('the FK branch of createCandidateValuesLoader', () => {
  it('browses the target table with no key values and the selector cap', async () => {
    resolveForeignKeyTargets.mockResolvedValue({
      target: { unique: true, columns: ['id'] },
      exists: true,
      candidates: [[1], [2]],
      truncated: false,
    });

    const loader = createCandidateValuesLoader(args(new Map([['customer_id', SOURCE]])));
    const values = await loader('customer_id');

    expect(values).toEqual(['1', '2']);
    expect(resolveForeignKeyTargets).toHaveBeenCalledWith(
      expect.objectContaining({
        dbSessionId: 'session-1',
        database: 'app',
        table: 'orders',
        columns: ['customer_id'],
        constraintName: 'orders_customer_fk',
        values: [],
        browse: true,
      }),
    );
  });

  it('refuses a target whose referenced columns are not unique', async () => {
    // Picking a non-unique referenced row would silently write an ambiguous
    // reference — the refusal is the feature (07 §7 rule 12).
    resolveForeignKeyTargets.mockResolvedValue({
      target: { unique: false, columns: ['id'] },
      exists: true,
      candidates: [[1], [1]],
      truncated: false,
    });

    const loader = createCandidateValuesLoader(args(new Map([['customer_id', SOURCE]])));
    await expect(loader('customer_id')).rejects.toThrow(/not unique/i);
  });

  it('returns the empty list the target offered, without inventing rows', async () => {
    resolveForeignKeyTargets.mockResolvedValue({
      target: { unique: true, columns: ['id'] },
      exists: false,
      candidates: [],
      truncated: false,
    });

    const loader = createCandidateValuesLoader(args(new Map([['customer_id', SOURCE]])));
    await expect(loader('customer_id')).resolves.toEqual([]);
  });

  it('lets an unknown constraint travel as the resolver’s classified rejection', async () => {
    // The resolver rejects with a `grid.fk.*` message; the loader is a pipe,
    // not a classifier.
    resolveForeignKeyTargets.mockRejectedValue(new Error('grid.fk.unknownConstraint: gone'));

    const loader = createCandidateValuesLoader(args(new Map([['customer_id', SOURCE]])));
    await expect(loader('customer_id')).rejects.toThrow('grid.fk.unknownConstraint');
  });

  it('ignores the FK branch for a column without a source — the enum path is untouched', async () => {
    const loader = createCandidateValuesLoader(args());
    // Without `fkSources` the loader falls through to the enum branch; the
    // FK command must not be reached, whatever the enum branch does.
    await loader('customer_id').catch(() => undefined);
    expect(resolveForeignKeyTargets).not.toHaveBeenCalled();
  });
});
