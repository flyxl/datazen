import { describe, it, expect } from 'vitest';
import { buildErGraph } from '../buildErGraph';
import type { TableSchema } from '../../../../types';

/**
 * Guards the cost of a full ER layout, not a stopwatch reading.
 *
 * A database can hold hundreds of tables and the diagram lays out all of them, so
 * the layout runs on every schema load. The bound is far above the real cost
 * (~32ms at 500 tables) so CI jitter cannot fail it, while a layout that became
 * accidentally quadratic still would.
 */
function makeSchema(tableCount: number): TableSchema[] {
  return Array.from({ length: tableCount }, (_, t) => ({
    tableName: `entity_${t}`,
    columns: [
      { name: 'id', dataType: 'integer', nullable: false, isPrimaryKey: true },
      ...Array.from({ length: 14 }, (_, c) => ({
        name: `field_${c}`,
        dataType: 'integer',
        nullable: true,
      })),
    ],
    primaryKeys: ['id'],
    indexes: [],
    foreignKeys:
      t > 0 && t % 3 === 0
        ? [
            {
              name: `fk_${t}`,
              columns: ['parent_id'],
              referencedTable: `entity_${t - 1}`,
              referencedColumns: ['id'],
            },
          ]
        : [],
  }));
}

describe('ER layout at database scale', () => {
  it('lays out a 500-table schema well inside an interactive budget', () => {
    const started = performance.now();
    const { nodes } = buildErGraph(makeSchema(500));
    const elapsed = performance.now() - started;

    expect(nodes).toHaveLength(500);
    // A fast wrong answer is no answer: every node must have a real position.
    expect(nodes.every((n) => Number.isFinite(n.position.x) && Number.isFinite(n.position.y))).toBe(
      true,
    );
    expect(elapsed).toBeLessThan(3000);
  });
});
