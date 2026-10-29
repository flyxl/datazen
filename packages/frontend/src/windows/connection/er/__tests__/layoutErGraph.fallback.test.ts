import { describe, it, expect, vi, afterEach } from 'vitest';
import dagre from '@dagrejs/dagre';
import type { Edge, Node } from '@xyflow/react';
import { layoutErGraph } from '../layoutErGraph';

/**
 * The safety net: when the layout library throws, the ER view must still draw
 * every table rather than go blank. `layoutErGraph` catches and falls back to a
 * deterministic grid.
 */

function node(id: string, height = 110): Node {
  return { id, type: 'tableNode', position: { x: 0, y: 0 }, width: 260, height, data: {} };
}

function edge(id: string, source: string, target: string): Edge {
  return { id, source, target, data: { kind: 'declared' } };
}

afterEach(() => {
  vi.restoreAllMocks();
});

describe('layoutErGraph fallback', () => {
  it('falls back to a grid when dagre throws', () => {
    vi.spyOn(dagre, 'layout').mockImplementation(() => {
      throw new Error('Not possible to find intersection inside of the rectangle');
    });
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});

    const nodes = [node('a'), node('b', 200), node('c'), node('d'), node('e')];
    const laid = layoutErGraph(nodes, [edge('e1', 'a', 'b')]);

    expect(laid).toHaveLength(nodes.length);
    const seen = new Set<string>();
    for (const placed of laid) {
      expect(Number.isFinite(placed.position.x)).toBe(true);
      expect(Number.isFinite(placed.position.y)).toBe(true);
      expect(placed.position.x).toBeGreaterThanOrEqual(0);
      expect(placed.position.y).toBeGreaterThanOrEqual(0);
      seen.add(`${placed.position.x},${placed.position.y}`);
    }
    // One grid cell each: no two tables stacked on the same spot.
    expect(seen.size).toBe(nodes.length);
    // Loud enough to be diagnosable, and exactly once per layout.
    expect(warn).toHaveBeenCalledTimes(1);
  });

  it('keeps the caller’s node order when it falls back', () => {
    vi.spyOn(dagre, 'layout').mockImplementation(() => {
      throw new Error('boom');
    });
    vi.spyOn(console, 'warn').mockImplementation(() => {});

    const nodes = [node('c'), node('a'), node('b')];
    expect(layoutErGraph(nodes, []).map((n) => n.id)).toEqual(['c', 'a', 'b']);
  });

  it('does not mutate the nodes it is given when it falls back', () => {
    vi.spyOn(dagre, 'layout').mockImplementation(() => {
      throw new Error('boom');
    });
    vi.spyOn(console, 'warn').mockImplementation(() => {});

    const nodes = [node('a'), node('b')];
    const before = JSON.parse(JSON.stringify(nodes));
    layoutErGraph(nodes, [edge('e1', 'a', 'b')]);
    expect(nodes).toEqual(before);
  });
});
