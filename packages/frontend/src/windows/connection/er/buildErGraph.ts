import type { Node, Edge } from '@xyflow/react';
import { MarkerType } from '@xyflow/react';
import type { TableSchema } from '../../../types';
import { fitEdgeLabel } from './interactionState';
import { layoutErGraph } from './layoutErGraph';
import { ER_AUTO_COLLAPSE_COLUMNS, ER_NODE_WIDTH, erNodeHeight } from './nodeMetrics';
import { ER_DECLARED_COLOR, ER_PREDICTED_COLOR, ER_PREDICTED_DASH } from './relationStyle';

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
  /**
   * Another table matched these columns just as well. The engine will not pick one
   * automatically, and the diagram must not present its guess as a certain one.
   */
  ambiguous?: boolean;
}

/**
 * The tables a diagram starts collapsed on.
 *
 * The column list has no internal scroll, so a table of sixty columns is a
 * 1478px node that would space its whole rank that far apart. Seeding the
 * collapsed set — rather than forcing collapse in the graph builder — keeps the
 * chevron working in both directions: the user can expand such a table, and it
 * stays expanded.
 */
export function defaultCollapsedTables(schemas: readonly TableSchema[]): Set<string> {
  return new Set(
    schemas
      .filter((schema) => schema.columns.length > ER_AUTO_COLLAPSE_COLUMNS)
      .map((schema) => schema.tableName),
  );
}

/** Shared empty set, so the default argument does not allocate per call. */
const EMPTY_COLLAPSED: ReadonlySet<string> = new Set<string>();

/** How quiet an ambiguous inference sits: still visible, never as loud as a constraint. */
const AMBIGUOUS_OPACITY = 0.3;
/**
 * A label that lands on top of another line has to knock that line out behind it.
 * React Flow's default chip is white, which disappears into the light theme's own
 * whiteness and glares in the dark one, so it is taken from the surface token.
 */
const LABEL_BG_STYLE = { fill: 'var(--c-surface, #ffffff)' };
const LABEL_BG_PADDING: [number, number] = [4, 2];
const LABEL_BG_RADIUS = 3;
/**
 * An arrowhead is painted by a `<marker>`, which the line's own opacity does not
 * reach: leaving it fully saturated would put a bright chevron on a faint line.
 * `color-mix` is already a baseline here (globals.css).
 */
const AMBIGUOUS_MARKER_COLOR = `color-mix(in oklab, ${ER_PREDICTED_COLOR} ${AMBIGUOUS_OPACITY * 100}%, transparent)`;

/**
 * The column a line attaches to. Connection points live on rendered rows, so a
 * name the table does not report would drop the edge entirely.
 */
function handleColumn(schema: TableSchema | undefined, wanted: string | undefined): string {
  const columns = schema?.columns ?? [];
  if (columns.length === 0) return '';
  const needle = wanted?.toLowerCase();
  const match = needle ? columns.find((c) => c.name.toLowerCase() === needle) : undefined;
  return (match ?? columns[0]).name;
}

/**
 * Collapse a relationship the engine guessed in both directions into one edge.
 *
 * `a.user_id → b.user_id` and `b.user_id → a.user_id` are the same unknown link
 * seen twice; drawing both doubles the lines crossing the canvas without adding
 * information. The stronger-scoring direction wins.
 *
 * Exported because the stats readout must count the lines that are actually drawn.
 */
export function dedupeSymmetricPredictions(
  relations: readonly ErPredictedRelation[],
): ErPredictedRelation[] {
  const best = new Map<string, ErPredictedRelation>();
  for (const relation of relations) {
    const tables = [relation.fromTable, relation.toTable].sort();
    const columns = relation.columnPairs
      .flatMap((pair) => [pair.left, pair.right])
      .map((name) => name.toLowerCase())
      .sort()
      .join(',');
    const key = `${tables[0]}\u0000${tables[1]}\u0000${columns}`;
    const existing = best.get(key);
    if (!existing || relation.score > existing.score) best.set(key, relation);
  }
  return [...best.values()];
}

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
  collapsedTables: ReadonlySet<string> = EMPTY_COLLAPSED,
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
  const schemaByName = new Map(visibleSchemas.map((s) => [s.tableName, s]));
  // The diagram draws the same guess only once even when the engine saw it from
  // both ends, and it never shows a table that the focus filter has hidden.
  const predictions = dedupeSymmetricPredictions(
    predicted.filter((r) => visibleNames.has(r.fromTable) && visibleNames.has(r.toTable)),
  );

  // Sizes are declared up front, exactly as `TableNode` renders them, and the
  // positions come from the layout below — never from the node's index.
  const nodes: Node[] = visibleSchemas.map((schema) => {
    // Collapse is read from the caller's set and nothing else. Folding a
    // "too wide to show" rule in here would make the chevron one-way: the user
    // could never expand such a table, because the rule would re-collapse it on
    // every relayout. The initial set is seeded by `defaultCollapsedTables`.
    const collapsed = collapsedTables.has(schema.tableName);
    return {
      id: schema.tableName,
      type: 'tableNode',
      position: { x: 0, y: 0 },
      width: ER_NODE_WIDTH,
      height: erNodeHeight(schema.columns.length, collapsed),
      data: {
        tableName: schema.tableName,
        columns: schema.columns.map((c) => ({
          name: c.name,
          type: c.dataType,
          isPk: c.isPrimaryKey ?? schema.primaryKeys.includes(c.name),
          isFk: fkColumns.has(`${schema.tableName}.${c.name}`),
        })),
        highlighted: schema.tableName === focusTable,
        collapsed,
      },
    };
  });

  const edges: Edge[] = [];

  for (const schema of visibleSchemas) {
    for (const fk of schema.foreignKeys) {
      if (!visibleNames.has(fk.referencedTable)) continue;
      // A composite key still draws as one line, anchored on its first column and
      // labelled with all of them.
      const fromColumn = handleColumn(schema, fk.columns[0]);
      const toColumn = handleColumn(schemaByName.get(fk.referencedTable), fk.referencedColumns[0]);
      edges.push({
        id: `${schema.tableName}-${fk.name}`,
        source: schema.tableName,
        target: fk.referencedTable,
        type: 'smoothstep',
        animated: true,
        style: { stroke: ER_DECLARED_COLOR },
        markerEnd: { type: MarkerType.ArrowClosed, color: ER_DECLARED_COLOR },
        labelStyle: { fontSize: 10, fill: 'var(--c-fg-muted, #888)' },
        labelBgStyle: LABEL_BG_STYLE,
        labelBgPadding: LABEL_BG_PADDING,
        labelBgBorderRadius: LABEL_BG_RADIUS,
        // A composite foreign key spans several rows; the edge meets the first
        // column the table actually reports (never a name it does not have),
        // while the label still lists them all.
        data: {
          kind: 'declared' satisfies ErRelationKind,
          sourceColumn: fromColumn || undefined,
          targetColumn: toColumn || undefined,
          columns: [...fk.columns],
        },
      });
    }
  }

  for (const relation of predictions) {
    const ambiguous = relation.ambiguous === true;
    const fromColumn = handleColumn(
      schemaByName.get(relation.fromTable),
      relation.columnPairs[0]?.left,
    );
    const toColumn = handleColumn(
      schemaByName.get(relation.toTable),
      relation.columnPairs[0]?.right,
    );
    edges.push({
      // The engine's id already encodes both tables and every column pair, so it
      // is stable across re-prediction and unique among sibling edges.
      id: relation.id,
      source: relation.fromTable,
      target: relation.toTable,
      type: 'smoothstep',
      // Not animated and dashed: this relationship is inferred, and the diagram
      // must not present it with the same certainty as a constraint.
      animated: false,
      style: {
        stroke: ER_PREDICTED_COLOR,
        strokeDasharray: ambiguous ? '2 5' : ER_PREDICTED_DASH,
        opacity: ambiguous ? AMBIGUOUS_OPACITY : 1,
      },
      markerEnd: {
        type: MarkerType.ArrowClosed,
        color: ambiguous ? AMBIGUOUS_MARKER_COLOR : ER_PREDICTED_COLOR,
      },
      labelStyle: { fontSize: 10, fill: ER_PREDICTED_COLOR },
      labelBgStyle: LABEL_BG_STYLE,
      labelBgPadding: LABEL_BG_PADDING,
      labelBgBorderRadius: LABEL_BG_RADIUS,
      data: {
        kind: 'predicted' satisfies ErRelationKind,
        score: relation.score,
        ambiguous,
        sourceColumn: fromColumn || undefined,
        targetColumn: toColumn || undefined,
        columns: relation.columnPairs.map((pair) => pair.left),
      },
    });
  }

  // A layered layout keyed on the relationships, not on the order the backend
  // happened to return the tables in.
  const laidOut = layoutErGraph(nodes, edges);

  return { nodes: withHandleColumns(laidOut, edges), edges: withHandles(edges, laidOut) };
}

/**
 * Record, per node, which columns an edge actually touches.
 *
 * `TableNode` renders connection points only for these columns rather than for
 * every column: a wide table would otherwise carry four handles per row, and a
 * schema of a few hundred tables would carry tens of thousands of them.
 */
function withHandleColumns(nodes: readonly Node[], edges: readonly Edge[]): Node[] {
  const touched = new Map<string, Set<string>>();
  const touch = (table: string, column: unknown) => {
    if (typeof column !== 'string' || column.length === 0) return;
    const set = touched.get(table);
    if (set) set.add(column);
    else touched.set(table, new Set([column]));
  };

  for (const edge of edges) {
    const data = edge.data as { sourceColumn?: string; targetColumn?: string } | undefined;
    touch(edge.source, data?.sourceColumn);
    touch(edge.target, data?.targetColumn);
  }

  return nodes.map((node) => ({
    ...node,
    data: {
      ...node.data,
      handleColumns: [...(touched.get(node.id) ?? [])],
    },
  }));
}

/** Handle id for a column, or the node-level fallback when there is no column. */
function handleId(column: unknown, role: 's' | 't', side: 'l' | 'r', collapsed: boolean): string {
  if (collapsed || typeof column !== 'string' || column.length === 0) return `node:${role}-${side}`;
  return `${column}:${role}-${side}`;
}

/**
 * Point each edge at the rows it actually joins.
 *
 * Every edge after a layered `LR` layout runs between two different ranks, so it
 * is horizontal — measured across acyclic, chained and cyclic shapes, none was
 * vertical. Which side it leaves and enters therefore depends only on which node
 * is further right, and a reversed edge (a cycle) simply mirrors the pair.
 */
function withHandles(edges: readonly Edge[], nodes: readonly Node[]): Edge[] {
  const byId = new Map(nodes.map((node) => [node.id, node]));
  const centreOf = (id: string) => {
    const node = byId.get(id);
    if (!node) return undefined;
    return {
      x: node.position.x + (node.width ?? 0) / 2,
      y: node.position.y + (node.height ?? 0) / 2,
      collapsed: node.data?.collapsed === true,
    };
  };

  return edges.map((edge) => {
    const source = centreOf(edge.source);
    const target = centreOf(edge.target);
    const data = edge.data as { sourceColumn?: string; targetColumn?: string } | undefined;
    if (!source || !target) return edge;

    // A self-reference has no left/right to speak of; leave it on the node-level
    // handles and let React Flow draw the loop.
    if (edge.source === edge.target) {
      return {
        ...edge,
        sourceHandle: handleId(data?.sourceColumn, 's', 'r', true),
        targetHandle: handleId(data?.targetColumn, 't', 'l', true),
      };
    }

    const forward = target.x >= source.x;
    const typed = data as { columns?: string[]; ambiguous?: boolean } | undefined;
    const columns = typed?.columns ?? [];
    // The label is placed between the two nodes, so it can only be as wide as the
    // gap between them.
    const gap = forward
      ? (byId.get(edge.target)?.position.x ?? 0) -
        ((byId.get(edge.source)?.position.x ?? 0) + (byId.get(edge.source)?.width ?? 0))
      : (byId.get(edge.source)?.position.x ?? 0) -
        ((byId.get(edge.target)?.position.x ?? 0) + (byId.get(edge.target)?.width ?? 0));

    return {
      ...edge,
      // An ambiguous guess carries no label: a clique of identical labels is
      // precisely what made this diagram unreadable.
      label: typed?.ambiguous ? undefined : fitEdgeLabel(columns, Math.max(gap, 0)),
      sourceHandle: handleId(data?.sourceColumn, 's', forward ? 'r' : 'l', source.collapsed),
      targetHandle: handleId(data?.targetColumn, 't', forward ? 'l' : 'r', target.collapsed),
    };
  });
}
