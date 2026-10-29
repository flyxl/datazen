/**
 * Layered (Sugiyama-style) layout for the ER diagram, via dagre.
 *
 * ## Why not a grid
 *
 * The previous layout placed table *i* at `(i % cols) * 300, floor(i / cols) * height`
 * in the order the backend returned them — which is `ORDER BY relname`, i.e. the
 * alphabet. Nothing about that relates to the foreign keys, so:
 *
 * - Related tables landed wherever their names fell. A hub and its five children
 *   scattered across three rows.
 * - The row's `y` used the *node's own* height, so nodes in one row sat at
 *   different heights — measured 672px of spread in a 12-table diagram — and a wide
 *   table above a narrow one **overlapped it by 192px**.
 * - Edges were routed across unrelated nodes: 10 edges crossed 18 unrelated node
 *   bands in a realistic 12-table shape, and React Flow draws edges *under* nodes,
 *   so the crossings were hidden rather than merely messy.
 *
 * A layered layout fixes the cause rather than the symptoms: it ranks tables by the
 * direction of the foreign keys, so a child is placed next to the parent it points
 * at, and it minimises crossings by construction.
 *
 * ## Why `LR`
 *
 * Edges run child → parent (the child holds the foreign key). With `rankdir: 'LR'`
 * dagre puts the parent at a higher rank, i.e. to the **right**, so an edge leaves
 * the child's right side and enters the parent's left side. That matches the
 * handles `TableNode` already renders (`source` on the right, `target` on the
 * left), so the common edge is a straight horizontal run instead of the S-curve
 * that 7-in-10 edges previously drew.
 */

import dagre from '@dagrejs/dagre';
import type { Edge, Node } from '@xyflow/react';

/** Gap between nodes in the same rank. */
const NODE_SEP = 40;
/** Gap between ranks — the horizontal band each layer occupies. */
const RANK_SEP = 110;
/** Gap between edges sharing a rank span. */
const EDGE_SEP = 20;
/** Padding around the whole graph, so nothing sits on the canvas edge. */
export const ER_LAYOUT_MARGIN = 24;

/** A declared constraint should shape the diagram more than an inference does. */
const DECLARED_WEIGHT = 2;
const PREDICTED_WEIGHT = 1;

export interface ErLayoutOptions {
  /** `LR` (default) puts referenced tables to the right; `TB` puts them below. */
  direction?: 'LR' | 'TB';
}

function byId(a: { id: string }, b: { id: string }): number {
  return a.id < b.id ? -1 : a.id > b.id ? 1 : 0;
}

/** Read the layout weight of an edge from the kind the graph builder tagged it with. */
function edgeWeight(edge: Edge): number {
  const kind = (edge.data as { kind?: string } | undefined)?.kind;
  return kind === 'predicted' ? PREDICTED_WEIGHT : DECLARED_WEIGHT;
}

/** One edge per ordered pair of tables — all dagre needs to rank and order them. */
interface LayoutEdge {
  source: string;
  target: string;
  weight: number;
}

/**
 * Reduce the caller's edges to the ones that can shape the layout: one per ordered
 * pair of tables, self-loops dropped.
 *
 * The first reason is not cosmetic. dagre 3.1.1 loses a dummy node when several
 * edges share an endpoint pair *and* one of them spans more than one rank: its
 * layer matrix keys nodes by `(rank, order)`, the two dummy nodes collide, one is
 * overwritten and never receives coordinates, `normalize` then pushes
 * `{ x: undefined, y: undefined }` onto that edge's points, and
 * `assignNodeIntersects` throws "Not possible to find intersection inside of the
 * rectangle" — the whole diagram fails to render. Parallel edges are exactly what
 * prediction produces (a declared constraint plus an inferred relation on the same
 * pair, or two inferences between the same tables), so the shape is ordinary.
 *
 * The second is that collapsing them costs nothing: React Flow draws its own edge
 * paths between the node handles and ignores dagre's routing points, so a second
 * edge between the same pair only made dagre reserve room for a route nobody draws.
 * The strongest kind wins, so a declared constraint still pulls its tables together
 * harder than an inference does.
 */
function layoutEdges(nodes: readonly Node[], edges: readonly Edge[]): LayoutEdge[] {
  const known = new Set(nodes.map((n) => n.id));
  const byPair = new Map<string, LayoutEdge>();

  for (const edge of edges) {
    // An edge to a table that is not on the canvas (filtered out by focus mode)
    // would otherwise make dagre invent a node for it.
    if (!known.has(edge.source) || !known.has(edge.target)) continue;
    // A self-loop cannot move its own node, and the label space dagre reserves for
    // it is space React Flow never draws into.
    if (edge.source === edge.target) continue;

    const key = `${edge.source}\u0000${edge.target}`;
    const weight = edgeWeight(edge);
    const existing = byPair.get(key);
    if (existing) existing.weight = Math.max(existing.weight, weight);
    else byPair.set(key, { source: edge.source, target: edge.target, weight });
  }

  // dagre's result depends on insertion order, so insert sorted to make the same
  // schema always produce the same diagram.
  return [...byPair.values()].sort((a, b) =>
    a.source === b.source
      ? a.target < b.target
        ? -1
        : a.target > b.target
          ? 1
          : 0
      : a.source < b.source
        ? -1
        : 1,
  );
}

/**
 * dagre's centre for every node it placed, keyed by node id.
 *
 * Throws if dagre cannot lay the graph out; the caller degrades instead.
 */
function layeredCentres(
  nodes: readonly Node[],
  edges: readonly Edge[],
  options: ErLayoutOptions,
): Map<string, { x: number; y: number }> {
  // A plain graph, not a multigraph: `layoutEdges` has already collapsed parallel
  // edges, and a simple graph is what keeps dagre's own layer bookkeeping honest.
  const graph = new dagre.graphlib.Graph({ multigraph: false, compound: false });
  graph.setGraph({
    rankdir: options.direction ?? 'LR',
    nodesep: NODE_SEP,
    ranksep: RANK_SEP,
    edgesep: EDGE_SEP,
    marginx: ER_LAYOUT_MARGIN,
    marginy: ER_LAYOUT_MARGIN,
  });
  graph.setDefaultEdgeLabel(() => ({}));

  // dagre's result depends on insertion order, so insert sorted to make the same
  // schema always produce the same diagram. The returned array keeps the caller's
  // order — this only fixes the coordinates, not the z-order React Flow draws in.
  for (const node of [...nodes].sort(byId)) {
    graph.setNode(node.id, { width: node.width ?? 0, height: node.height ?? 0 });
  }
  for (const edge of layoutEdges(nodes, edges)) {
    graph.setEdge(edge.source, edge.target, { weight: edge.weight });
  }

  dagre.layout(graph);

  const centres = new Map<string, { x: number; y: number }>();
  for (const node of nodes) {
    // A node dagre never saw cannot be positioned; leaving it out is what lets the
    // caller keep its own position rather than emit NaN.
    const placed = graph.node(node.id) as { x: number; y: number } | undefined;
    if (placed) centres.set(node.id, placed);
  }
  return centres;
}

/**
 * A deterministic grid, used only when dagre cannot lay the graph out at all.
 *
 * Deliberately dumb: the point is that the ER view still shows every table, with
 * its edges, when the layout library fails. Rows use the tallest node so nothing
 * overlaps, which is the one thing the old grid got wrong.
 */
function gridCentres(nodes: readonly Node[]): Map<string, { x: number; y: number }> {
  let widest = 0;
  let tallest = 0;
  for (const node of nodes) {
    widest = Math.max(widest, node.width ?? 0);
    tallest = Math.max(tallest, node.height ?? 0);
  }
  const cellWidth = widest + NODE_SEP;
  const cellHeight = tallest + NODE_SEP;
  const columns = Math.max(1, Math.ceil(Math.sqrt(nodes.length)));

  return new Map(
    nodes.map((node, index) => [
      node.id,
      {
        x: ER_LAYOUT_MARGIN + (index % columns) * cellWidth + cellWidth / 2,
        y: ER_LAYOUT_MARGIN + Math.floor(index / columns) * cellHeight + cellHeight / 2,
      },
    ]),
  );
}

/**
 * Position `nodes` and `edges` with a layered layout.
 *
 * Nodes must already carry `width` and `height`; the layout cannot infer them, and
 * guessing is what the previous grid got wrong. Returns new node objects — the
 * input is not mutated.
 *
 * Disconnected tables are laid out too, in their own components, rather than being
 * dropped or stacked at the origin.
 */
export function layoutErGraph(
  nodes: readonly Node[],
  edges: readonly Edge[],
  options: ErLayoutOptions = {},
): Node[] {
  if (nodes.length === 0) return [];

  let centres: Map<string, { x: number; y: number }>;
  try {
    centres = layeredCentres(nodes, edges, options);
  } catch (error) {
    // `layoutEdges` documents the shapes that used to throw here. Whatever dagre
    // trips over next, a grid is a poor diagram but still a diagram — the ER view
    // must not go blank because a layout library hit an input it cannot order.
    console.warn('[er] layered layout failed; falling back to a grid', error);
    centres = gridCentres(nodes);
  }

  return nodes.map((node) => {
    const centre = centres.get(node.id);
    if (!centre) return node;
    // dagre reports centres; React Flow positions by the top-left corner.
    return {
      ...node,
      position: {
        x: Math.round(centre.x - (node.width ?? 0) / 2),
        y: Math.round(centre.y - (node.height ?? 0) / 2),
      },
    };
  });
}
