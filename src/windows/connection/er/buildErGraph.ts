import type { Node, Edge } from '@xyflow/react';
import { MarkerType } from '@xyflow/react';
import type { TableSchema } from '../../../types';

const NODE_BASE_HEIGHT = 40;
const COL_HEIGHT = 24;
const GAP_X = 300;
const GAP_Y = 60;

export function buildErGraph(
  schemas: TableSchema[],
  focusTable?: string,
): { nodes: Node[]; edges: Edge[] } {
  const fkColumns = new Set<string>();
  for (const schema of schemas) {
    for (const fk of schema.foreignKeys) {
      for (const col of fk.columns) {
        fkColumns.add(`${schema.tableName}.${col}`);
      }
    }
  }

  let visibleSchemas = schemas;
  if (focusTable) {
    const focusSchema = schemas.find((s) => s.tableName === focusTable);
    if (focusSchema) {
      const related = new Set<string>([focusTable]);
      for (const fk of focusSchema.foreignKeys) {
        related.add(fk.referencedTable);
      }
      for (const schema of schemas) {
        for (const fk of schema.foreignKeys) {
          if (fk.referencedTable === focusTable) {
            related.add(schema.tableName);
          }
        }
      }
      visibleSchemas = schemas.filter((s) => related.has(s.tableName));
    }
  }

  const cols = Math.max(1, Math.ceil(Math.sqrt(visibleSchemas.length)));

  const nodes: Node[] = visibleSchemas.map((schema, i) => {
    const row = Math.floor(i / cols);
    const col = i % cols;
    const nodeHeight = NODE_BASE_HEIGHT + schema.columns.length * COL_HEIGHT;

    return {
      id: schema.tableName,
      type: 'tableNode',
      position: { x: col * GAP_X, y: row * (nodeHeight + GAP_Y) },
      data: {
        tableName: schema.tableName,
        columns: schema.columns.map((c) => ({
          name: c.name,
          type: c.dataType,
          isPk: c.isPrimaryKey ?? schema.primaryKeys.includes(c.name),
          isFk: fkColumns.has(`${schema.tableName}.${c.name}`),
        })),
        highlighted: schema.tableName === focusTable,
      },
    };
  });

  const edges: Edge[] = [];
  for (const schema of visibleSchemas) {
    for (const fk of schema.foreignKeys) {
      if (visibleSchemas.some((s) => s.tableName === fk.referencedTable)) {
        edges.push({
          id: `${schema.tableName}-${fk.name}`,
          source: schema.tableName,
          target: fk.referencedTable,
          label: fk.columns.join(', '),
          type: 'smoothstep',
          animated: true,
          style: { stroke: 'var(--color-accent, #3b82f6)' },
          markerEnd: {
            type: MarkerType.ArrowClosed,
            color: 'var(--color-accent, #3b82f6)',
          },
          labelStyle: {
            fontSize: 10,
            fill: 'var(--color-fg-muted, #888)',
          },
        });
      }
    }
  }

  return { nodes, edges };
}
