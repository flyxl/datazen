import { describe, it, expect, vi, afterEach } from 'vitest';
import type { Edge, Node } from '@xyflow/react';
import { layoutErGraph } from '../layoutErGraph';

/**
 * Regression cover for the shape that used to blank the whole ER diagram.
 *
 * dagre 3.1.1 loses a dummy node when several edges share an endpoint pair *and*
 * one of them spans more than one rank: its layer matrix keys nodes by
 * `(rank, order)`, the two dummy nodes collide, one is overwritten and never gets
 * coordinates, `normalize` pushes `{ x: undefined, y: undefined }` onto that edge,
 * and `assignNodeIntersects` throws "Not possible to find intersection inside of
 * the rectangle". Prediction is what produces those parallel edges in practice —
 * a declared constraint plus an inferred relation on the same pair, or two
 * inferences between the same tables — so the diagram went blank exactly when the
 * user turned prediction on.
 *
 * `layoutErGraph` collapses parallel edges before handing the graph to dagre
 * (React Flow draws its own edge paths and ignores dagre's points), so these
 * shapes must lay out on the layered path — the assertions below also fail if the
 * grid fallback is what saved us.
 */

function node(id: string, height = 110): Node {
  return { id, type: 'tableNode', position: { x: 0, y: 0 }, width: 260, height, data: {} };
}

function edge(id: string, source: string, target: string): Edge {
  return { id, source, target, data: { kind: 'predicted' } };
}

/** Every node placed, finite, and not on top of another node. */
function expectSaneLayout(laid: Node[]): void {
  const seen = new Set<string>();
  for (const placed of laid) {
    expect(Number.isFinite(placed.position.x), `${placed.id} x`).toBe(true);
    expect(Number.isFinite(placed.position.y), `${placed.id} y`).toBe(true);
    const key = `${placed.position.x},${placed.position.y}`;
    expect(seen.has(key), `${placed.id} overlaps ${key}`).toBe(false);
    seen.add(key);
  }
}

afterEach(() => {
  vi.restoreAllMocks();
});

describe('layoutErGraph with parallel edges', () => {
  it('lays out the shape that used to throw', () => {
    // Three edges t2→t0 (rank 2 → 4, so each needs a dummy chain) plus a cycle
    // t1↔t2. This is the minimal reproduction of the dagre failure.
    const nodes = [node('t0'), node('t1'), node('t2')];
    const edges = [
      edge('p0', 't2', 't0'),
      edge('p1', 't2', 't0'),
      edge('p2', 't1', 't2'),
      edge('p3', 't1', 't0'),
      edge('p4', 't2', 't0'),
      edge('p5', 't2', 't1'),
    ];
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});

    const laid = layoutErGraph(nodes, edges);

    expectSaneLayout(laid);
    // The layered layout handled it; the grid is not what saved us.
    expect(warn).not.toHaveBeenCalled();
  });

  it('keeps a declared constraint pulling harder than an inference', () => {
    // Collapsing parallel edges keeps the strongest weight, so this is the same
    // diagram whether the second relationship is declared or inferred.
    const nodes = [node('orders'), node('users')];
    const both = [
      { ...edge('e1', 'orders', 'users'), data: { kind: 'declared' } },
      { ...edge('e2', 'orders', 'users'), data: { kind: 'predicted' } },
    ];
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
    const laid = layoutErGraph(nodes, both);
    expectSaneLayout(laid);
    expect(warn).not.toHaveBeenCalled();
    expect(laid.find((n) => n.id === 'users')!.position.x).toBeGreaterThan(
      laid.find((n) => n.id === 'orders')!.position.x,
    );
  });

  it('survives parallel edges with a self-loop and a cycle together', () => {
    const nodes = [node('a'), node('b'), node('c')];
    const edges = [
      edge('e1', 'a', 'b'),
      edge('e2', 'a', 'b'),
      edge('e3', 'b', 'a'),
      edge('e4', 'b', 'b'),
      edge('e5', 'a', 'c'),
      edge('e6', 'c', 'a'),
    ];
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});

    const laid = layoutErGraph(nodes, edges);

    expectSaneLayout(laid);
    expect(warn).not.toHaveBeenCalled();
  });

  it('lays out several hundred randomly shaped graphs', () => {
    // The failure needed a specific combination, so a handful of hand-written
    // shapes is not enough cover. The generator is seeded, so a regression is
    // reproducible from the seed printed below.
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});

    for (let seed = 1; seed <= 400; seed += 1) {
      let state = seed;
      const random = () => {
        state = (state + 0x6d2b79f5) | 0;
        let t = Math.imul(state ^ (state >>> 15), 1 | state);
        t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
        return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
      };

      const tableCount = 2 + Math.floor(random() * 8);
      const nodes = Array.from({ length: tableCount }, (_, index) =>
        node(`t${index}`, 60 + Math.floor(random() * 8) * 40),
      );
      const edges = Array.from({ length: Math.floor(random() * 12) }, (_, index) =>
        edge(
          `e${index}`,
          `t${Math.floor(random() * tableCount)}`,
          `t${Math.floor(random() * tableCount)}`,
        ),
      );

      const laid = layoutErGraph(nodes, edges);
      expect(laid, `seed ${seed}`).toHaveLength(tableCount);
      expectSaneLayout(laid);
    }

    expect(warn).not.toHaveBeenCalled();
  });
});
