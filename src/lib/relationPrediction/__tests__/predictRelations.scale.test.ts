import { describe, it, expect } from 'vitest';
import { predictRelations } from '../predictRelations';
import type { PredictionTable } from '../types';

/**
 * Guards the shape of the search, not a stopwatch reading.
 *
 * The engine's naming rules are all "does this column spell out some target key?".
 * Asking that by comparing every source column against every target key is
 * quadratic — measured at ~15s for this schema, which is unusable when the ER
 * diagram runs prediction over a whole database. The reverse index brings it to
 * ~12ms. The bound is deliberately far above the real cost (150×) so ordinary CI
 * jitter cannot fail it, while a return to the quadratic scan — two orders of
 * magnitude slower — still does.
 */
function makeSchema(tableCount: number, colsPerTable: number): PredictionTable[] {
  const tables: PredictionTable[] = [];
  for (let t = 0; t < tableCount; t++) {
    const name = `entity_${t}`;
    const columns = [{ name: 'id', dataType: 'integer' }];
    for (let c = 0; c < colsPerTable; c++) {
      columns.push({ name: `field_${c}`, dataType: c % 3 === 0 ? 'varchar(64)' : 'integer' });
    }
    // A third of the tables reference an earlier one, like a real schema.
    if (t > 0 && t % 3 === 0) columns.push({ name: `entity_${t - 1}_id`, dataType: 'integer' });
    tables.push({
      id: name,
      name,
      schema: 'public',
      columns,
      primaryKey: ['id'],
      uniqueColumnSets: [],
      declaredForeignKeys: [],
    });
  }
  return tables;
}

describe('predictRelations at database scale', () => {
  it('stays well clear of a quadratic scan on a 500-table schema', () => {
    const tables = makeSchema(500, 15);
    const started = performance.now();
    const found = predictRelations(tables);
    const elapsed = performance.now() - started;

    // The relationships must still be found — a fast wrong answer is no answer.
    expect(found.length).toBeGreaterThan(150);
    expect(found.every((c) => c.columnPairs.length > 0)).toBe(true);
    expect(elapsed).toBeLessThan(2000);
  });
});
