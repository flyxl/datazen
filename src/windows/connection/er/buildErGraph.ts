import type { Node, Edge } from '@xyflow/react';
import { MarkerType } from '@xyflow/react';
import type { TableSchema } from '../../../types';

const NODE_BASE_HEIGHT = 40;
const COL_HEIGHT = 24;
const GAP_X = 300;
const GAP_Y = 60;

/** How an edge between two tables was established. */
export type ErRelationKind = 'declared' | 'predicted';

/**
 * A relationship the engine inferred rather than read from a constraint.
 *
 * Structurally a subset of the engine's `RelationCandidate`, redeclared here so
 * the graph builder does not depend on the prediction module for its input shape.
 */
export interface ErPredictedRelation {
  /** Stable id from the engine, so a dismissal could survive re-prediction. */
  id: string;
  fromTable: string;
  toTable: string;
  columnPairs: readonly { left: string; right: string }[];
  score: number;
}

const DECLARED_COLOR = 'var(--c-accent, #3b82f6)';
const PREDICTED_COLOR = 'var(--c-warning, #e39a27)';

/**
 * Build the ER diagram's nodes and edges.
 *
 * @param schemas    Every table in the database.
 * @param focusTable When set, only the focused table and its relations are shown.
 * @param predicted  Inferred relationships to draw in addition to the declared
 *   ones. They are dashed and amber so an inference is never mistaken for a
 *   constraint the database enforces.
 */
export function buildErGraph(
  schemas: TableSchema[],
  focusTable?: string,
  predicted: readonly ErPredictedRelation[] = [],
): { nodes: Node[]; edges: Edge[] } {
  // Both kinds mark their columns as foreign keys — a predicted one is a foreign
  // key in everything but the constraint.
  const fkColumns = new Set<string>();
  for (const schema of schemas) {
    for (const fk of schema.foreignKeys) {
      for (const col of fk.columns) fkColumns.add(`${schema.tableName}.${col}`);
    }
  }
  for (const relation of predicted) {
    for (const pair of relation.columnPairs) {
      fkColumns.add(`${relation.fromTable}.${pair.left}`);
    }
  }

  /** Tables related to `tableName`, by constraint or by inference. */
  const relationsOf = (tableName: string): string[] => {
    const related = new Set<string>();
    const schema = schemas.find((s) => s.tableName === tableName);
    for (const fk of schema?.foreignKeys ?? []) related.add(fk.referencedTable);
    for (const relation of predicted) {
      if (relation.fromTable === tableName) related.add(relation.toTable);
      else if (relation.toTable === tableName) related.add(relation.fromTable);
    }
    return [...related];
  };

  let visibleSchemas = schemas;
  if (focusTable) {
    const focusSchema = schemas.find((s) => s.tableName === focusTable);
    if (focusSchema) {
      const related = new Set<string>([focusTable, ...relationsOf(focusTable)]);
      for (const schema of schemas) {
        if (relationsOf(schema.tableName).includes(focusTable)) related.add(schema.tableName);
      }
      visibleSchemas = schemas.filter((s) => related.has(s.tableName));
    }
  }

  const visibleNames = new Set(visibleSchemas.map((s) => s.tableName));
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
      if (!visibleNames.has(fk.referencedTable)) continue;
      edges.push({
        id: `${schema.tableName}-${fk.name}`,
        source: schema.tableName,
        target: fk.referencedTable,
        label: fk.columns.join(', '),
        type: 'smoothstep',
        animated: true,
        style: { stroke: DECLARED_COLOR },
        markerEnd: { type: MarkerType.ArrowClosed, color: DECLARED_COLOR },
        labelStyle: { fontSize: 10, fill: 'var(--color-fg-muted, #888)' },
        data: { kind: 'declared' satisfies ErRelationKind },
      });
    }
  }

  for (const relation of predicted) {
    if (!visibleNames.has(relation.fromTable) || !visibleNames.has(relation.toTable)) continue;
    edges.push({
      // The engine's id already encodes both tables and every column pair, so it
      // is stable across re-prediction and unique among sibling edges.
      id: relation.id,
      source: relation.fromTable,
      target: relation.toTable,
      label: relation.columnPairs.map((pair) => pair.left).join(', '),
      type: 'smoothstep',
      // Not animated and dashed: this relationship is inferred, and the diagram
      // must not present it with the same certainty as a constraint.
      animated: false,
      style: { stroke: PREDICTED_COLOR, strokeDasharray: '4 3' },
      markerEnd: { type: MarkerType.ArrowClosed, color: PREDICTED_COLOR },
      labelStyle: { fontSize: 10, fill: PREDICTED_COLOR },
      data: { kind: 'predicted' satisfies ErRelationKind, score: relation.score },
    });
  }

  return { nodes, edges };
}
