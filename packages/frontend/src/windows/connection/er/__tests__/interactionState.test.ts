import { describe, it, expect } from 'vitest';
import type { Edge, Node } from '@xyflow/react';
import {
  HOVER_DIM_OPACITY,
  HOVER_EMPHASIS_WIDTH,
  applyHoverToEdges,
  applyHoverToNodes,
  applyPinnedPositions,
  fitEdgeLabel,
  hoveredNeighbourhood,
  labelFits,
} from '../interactionState';

function node(id: string, x = 0, y = 0): Node {
  return { id, type: 'tableNode', position: { x, y }, width: 260, height: 100, data: {} };
}
function edge(id: string, source: string, target: string): Edge {
  return { id, source, target, style: { stroke: 'red' } };
}

describe('applyPinnedPositions', () => {
  it('returns the nodes untouched when nothing is pinned', () => {
    const nodes = [node('a', 10, 20)];
    expect(applyPinnedPositions(nodes, new Map())).toEqual(nodes);
  });

  it('overrides only the pinned nodes', () => {
    // Dragging one table must not disturb the rest of the diagram.
    const nodes = [node('a', 10, 20), node('b', 30, 40)];
    const laid = applyPinnedPositions(nodes, new Map([['a', { x: 500, y: 600 }]]));
    expect(laid.find((n) => n.id === 'a')!.position).toEqual({ x: 500, y: 600 });
    expect(laid.find((n) => n.id === 'b')!.position).toEqual({ x: 30, y: 40 });
  });

  it('does not mutate the nodes it is given', () => {
    const nodes = [node('a', 10, 20)];
    const before = JSON.parse(JSON.stringify(nodes));
    applyPinnedPositions(nodes, new Map([['a', { x: 1, y: 2 }]]));
    expect(nodes).toEqual(before);
  });

  it('ignores a pin for a table that is no longer on the canvas', () => {
    const nodes = [node('a')];
    const laid = applyPinnedPositions(nodes, new Map([['gone', { x: 1, y: 2 }]]));
    expect(laid.map((n) => n.id)).toEqual(['a']);
  });
});

describe('hoveredNeighbourhood', () => {
  const edges = [edge('e1', 'a', 'b'), edge('e2', 'c', 'a'), edge('e3', 'd', 'e')];

  it('returns null when nothing is hovered', () => {
    expect(hoveredNeighbourhood(edges, null)).toBeNull();
  });

  it('collects the table and both directions of its relationships', () => {
    // `a` is a source in one edge and a target in another; both count.
    expect(hoveredNeighbourhood(edges, 'a')).toEqual(new Set(['a', 'b', 'c']));
  });

  it('does not pull in unrelated tables', () => {
    expect(hoveredNeighbourhood(edges, 'a')!.has('d')).toBe(false);
    expect(hoveredNeighbourhood(edges, 'a')!.has('e')).toBe(false);
  });

  it('returns just the table when it has no relationships', () => {
    expect(hoveredNeighbourhood(edges, 'lonely')).toEqual(new Set(['lonely']));
  });
});

describe('applyHoverToNodes', () => {
  const nodes = [node('a'), node('b'), node('c')];

  it('dims everything outside the neighbourhood', () => {
    const styled = applyHoverToNodes(nodes, new Set(['a', 'b']));
    expect(styled.find((n) => n.id === 'a')!.data.dimmed).toBeUndefined();
    expect(styled.find((n) => n.id === 'c')!.data.dimmed).toBe(true);
  });

  it('leaves the nodes alone when nothing is hovered', () => {
    expect(applyHoverToNodes(nodes, null)).toEqual(nodes);
  });

  it('preserves the dimming search already applied', () => {
    // Search dims too; hovering must not un-dim a node search had dimmed.
    const searched = [{ ...node('c'), data: { dimmed: true } }];
    expect(applyHoverToNodes(searched, new Set(['a']))[0]!.data.dimmed).toBe(true);
  });
});

describe('applyHoverToEdges', () => {
  const edges = [edge('e1', 'a', 'b'), edge('e2', 'c', 'd')];

  it('emphasises an incident edge', () => {
    const styled = applyHoverToEdges(edges, 'a');
    const incident = styled.find((e) => e.id === 'e1')!;
    expect(incident.style?.strokeWidth).toBe(HOVER_EMPHASIS_WIDTH);
    expect(incident.zIndex).toBeGreaterThan(0);
  });

  it('dims an unrelated edge', () => {
    const styled = applyHoverToEdges(edges, 'a');
    expect(styled.find((e) => e.id === 'e2')!.style?.opacity).toBe(HOVER_DIM_OPACITY);
  });

  it('counts an edge where the hovered table is the target', () => {
    expect(applyHoverToEdges(edges, 'b').find((e) => e.id === 'e1')!.style?.strokeWidth).toBe(
      HOVER_EMPHASIS_WIDTH,
    );
  });

  it('keeps the edge styling it already had', () => {
    expect(applyHoverToEdges(edges, 'a')[0]!.style?.stroke).toBe('red');
  });

  it('leaves the edges alone when nothing is hovered', () => {
    expect(applyHoverToEdges(edges, null)).toEqual(edges);
  });

  it('does not mutate the edges it is given', () => {
    const before = JSON.parse(JSON.stringify(edges));
    applyHoverToEdges(edges, 'a');
    expect(edges).toEqual(before);
  });
});

describe('fitEdgeLabel', () => {
  it('keeps a single-column label when it fits', () => {
    // A 110px gap holds about 16 characters — every plain foreign key.
    expect(fitEdgeLabel(['user_id'], 110)).toBe('user_id');
  });

  it('summarises a composite label that would not fit', () => {
    // `tenant_id, account_id` needs 138px and would overlap the nodes.
    expect(fitEdgeLabel(['tenant_id', 'account_id'], 110)).toBe('tenant_id +1');
  });

  it('keeps a composite label that does fit', () => {
    expect(fitEdgeLabel(['a', 'b'], 110)).toBe('a, b');
  });

  it('truncates when even the summary is too wide', () => {
    const label = fitEdgeLabel(['a_very_long_column_name', 'another'], 60);
    expect(labelFits(label, 60)).toBe(true);
    expect(label.endsWith('…')).toBe(true);
  });

  it('always produces a label that fits the gap', () => {
    const columns = ['tenant_identifier', 'account_identifier', 'region_identifier'];
    for (const gap of [20, 40, 60, 110, 200]) {
      expect(labelFits(fitEdgeLabel(columns, gap), gap)).toBe(true);
    }
  });

  it('returns nothing for a relationship with no columns', () => {
    expect(fitEdgeLabel([], 110)).toBe('');
  });
});
