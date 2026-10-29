import { describe, it, expect } from 'vitest';
import type { Edge, Node } from '@xyflow/react';
import { ER_LAYOUT_MARGIN, layoutErGraph } from '../layoutErGraph';

function node(id: string, height = 100): Node {
  return { id, type: 'tableNode', position: { x: 0, y: 0 }, width: 260, height, data: {} };
}
function edge(source: string, target: string, kind?: 'declared' | 'predicted'): Edge {
  return {
    id: `${source}->${target}`,
    source,
    target,
    data: kind ? { kind } : undefined,
  };
}
const at = (nodes: Node[], id: string) => nodes.find((n) => n.id === id)!.position;

describe('layoutErGraph', () => {
  it('returns nothing for an empty graph', () => {
    expect(layoutErGraph([], [])).toEqual([]);
  });

  it('keeps the caller’s node order', () => {
    // React Flow draws in array order, so the layout must not reorder.
    const nodes = [node('c'), node('a'), node('b')];
    expect(layoutErGraph(nodes, []).map((n) => n.id)).toEqual(['c', 'a', 'b']);
  });

  it('does not mutate the nodes it is given', () => {
    const nodes = [node('a'), node('b')];
    const before = JSON.parse(JSON.stringify(nodes));
    layoutErGraph(nodes, [edge('a', 'b')]);
    expect(nodes).toEqual(before);
  });

  it('puts a referenced table to the right of the one referencing it', () => {
    const laid = layoutErGraph([node('child'), node('parent')], [edge('child', 'parent')]);
    expect(at(laid, 'parent').x).toBeGreaterThan(at(laid, 'child').x);
  });

  it('runs a chain of references in one direction', () => {
    const laid = layoutErGraph([node('a'), node('b'), node('c')], [edge('a', 'b'), edge('b', 'c')]);
    expect(at(laid, 'b').x).toBeGreaterThan(at(laid, 'a').x);
    expect(at(laid, 'c').x).toBeGreaterThan(at(laid, 'b').x);
  });

  it('pads the graph so no node sits on the canvas edge', () => {
    const laid = layoutErGraph([node('a')], []);
    expect(at(laid, 'a')).toEqual({ x: ER_LAYOUT_MARGIN, y: ER_LAYOUT_MARGIN });
  });

  it('never overlaps two nodes', () => {
    const nodes = [node('a', 338), node('b', 62), node('c', 146)];
    const laid = layoutErGraph(nodes, [edge('a', 'b'), edge('b', 'c')]);
    for (let i = 0; i < laid.length; i++) {
      for (let j = i + 1; j < laid.length; j++) {
        const p = laid[i]!;
        const q = laid[j]!;
        const overlapX =
          p.position.x < q.position.x + (q.width ?? 0) &&
          q.position.x < p.position.x + (p.width ?? 0);
        const overlapY =
          p.position.y < q.position.y + (q.height ?? 0) &&
          q.position.y < p.position.y + (p.height ?? 0);
        expect(overlapX && overlapY).toBe(false);
      }
    }
  });

  it('is deterministic', () => {
    const nodes = [node('a'), node('b'), node('c'), node('d')];
    const edges = [edge('a', 'b'), edge('c', 'd')];
    const first = layoutErGraph(nodes, edges);
    const second = layoutErGraph(nodes, edges);
    expect(second.map((n) => n.position)).toEqual(first.map((n) => n.position));
  });

  it('does not depend on the order the nodes arrive in', () => {
    const edges = [edge('a', 'b'), edge('b', 'c')];
    const forward = layoutErGraph([node('a'), node('b'), node('c')], edges);
    const shuffled = layoutErGraph([node('c'), node('a'), node('b')], edges);
    const byId = (laid: Node[]) =>
      Object.fromEntries(
        [...laid].sort((x, y) => (x.id < y.id ? -1 : 1)).map((n) => [n.id, n.position]),
      );
    expect(byId(shuffled)).toEqual(byId(forward));
  });

  it('lays out disconnected tables rather than stacking them', () => {
    const laid = layoutErGraph([node('a'), node('b')], []);
    expect(at(laid, 'a')).not.toEqual(at(laid, 'b'));
  });

  it('ignores an edge whose table is not on the canvas', () => {
    // Focus mode filters tables out; a dangling edge must not make dagre invent a
    // node for a table that is not drawn.
    const laid = layoutErGraph([node('a')], [edge('a', 'ghost')]);
    expect(laid.map((n) => n.id)).toEqual(['a']);
    expect(Number.isFinite(at(laid, 'a').x)).toBe(true);
  });

  it('ranks a pair once however many relationships they share', () => {
    // Two relationships between the same tables are collapsed before dagre sees
    // them (see `layoutEdges`); React Flow still draws both, because it draws the
    // caller's edge list and its own paths, not dagre's routing points.
    const edges: Edge[] = [
      { id: 'e1', source: 'orders', target: 'users', data: { kind: 'declared' } },
      { id: 'e2', source: 'orders', target: 'users', data: { kind: 'declared' } },
    ];
    const laid = layoutErGraph([node('orders'), node('users')], edges);
    expect(laid).toHaveLength(2);
    expect(at(laid, 'users').x).toBeGreaterThan(at(laid, 'orders').x);
  });

  it('survives a self-referencing table', () => {
    const laid = layoutErGraph([node('employee')], [edge('employee', 'employee')]);
    expect(Number.isFinite(at(laid, 'employee').x)).toBe(true);
  });

  it('survives a cycle', () => {
    const laid = layoutErGraph([node('a'), node('b')], [edge('a', 'b'), edge('b', 'a')]);
    for (const n of laid) {
      expect(Number.isFinite(n.position.x)).toBe(true);
      expect(Number.isFinite(n.position.y)).toBe(true);
    }
  });

  it('rounds positions to whole pixels', () => {
    const laid = layoutErGraph([node('a'), node('b')], [edge('a', 'b')]);
    for (const n of laid) {
      expect(Number.isInteger(n.position.x)).toBe(true);
      expect(Number.isInteger(n.position.y)).toBe(true);
    }
  });

  it('supports a top-to-bottom direction', () => {
    const laid = layoutErGraph([node('child'), node('parent')], [edge('child', 'parent')], {
      direction: 'TB',
    });
    expect(at(laid, 'parent').y).toBeGreaterThan(at(laid, 'child').y);
  });
});
