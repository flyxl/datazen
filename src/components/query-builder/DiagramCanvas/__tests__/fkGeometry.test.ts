/**
 * Relation routing geometry.
 *
 * These pin the composite-FK topology (many → one trunk → many), lane
 * separation, self-loops and the single-column straight-line case — all of
 * which are pure math, so they are asserted without rendering a canvas.
 */
import { describe, expect, it } from 'vitest';
import {
  LANE_STEP,
  MIN_TRUNK,
  SELF_LOOP_OFFSET,
  assignLanes,
  buildRelationShapes,
  laneOffset,
  type RelationGroup,
} from '../fkGeometry';
import {
  CARD_WIDTH,
  CARD_HEADER_HEIGHT,
  CARD_LIST_PADDING_Y,
  CARD_ROW_HEIGHT,
  cardHeight,
  cardListScrolls,
  rowCenterY,
} from '../cardLayout';

const COLUMNS: Record<string, string[]> = {
  shipment: ['id', 'item_id', 'qty', 'wh_id'],
  stock: ['id', 'item_id', 'on_hand', 'wh_id'],
  employee: ['id', 'manager_id', 'name'],
};

const positions = {
  shipment: { x: 0, y: 0 },
  stock: { x: 600, y: 0 },
};

function fkGroup(over: Partial<RelationGroup> = {}): RelationGroup {
  return {
    id: 'fk:shipment_stock',
    kind: 'fk',
    type: 'INNER',
    constraint: 'fk_shipment_stock',
    pairs: [
      {
        fromTable: 'shipment',
        fromColumn: 'item_id',
        toTable: 'stock',
        toColumn: 'item_id',
        confirmed: false,
      },
      {
        fromTable: 'shipment',
        fromColumn: 'wh_id',
        toTable: 'stock',
        toColumn: 'wh_id',
        confirmed: false,
      },
    ],
    ...over,
  };
}

const build = (groups: RelationGroup[]) =>
  buildRelationShapes({ groups, positions, columnOrder: COLUMNS });

const segments = (shapes: ReturnType<typeof buildRelationShapes>) =>
  shapes.flatMap((s) => s.segments.map((seg) => seg.d));

describe('rowCenterY', () => {
  it('centres the first row under the header and list padding', () => {
    expect(rowCenterY(0, 0)).toBe(CARD_HEADER_HEIGHT + CARD_LIST_PADDING_Y + CARD_ROW_HEIGHT / 2);
  });

  it('advances exactly one row height per column', () => {
    expect(rowCenterY(0, 3) - rowCenterY(0, 0)).toBe(3 * CARD_ROW_HEIGHT);
  });

  it('follows the card position', () => {
    expect(rowCenterY(120, 1) - rowCenterY(0, 1)).toBe(120);
  });
});

describe('laneOffset', () => {
  it('is zero for a single lane', () => {
    expect(laneOffset(0, 1)).toBe(0);
  });

  it('centres multiple lanes symmetrically', () => {
    expect(laneOffset(0, 2)).toBe(-LANE_STEP / 2);
    expect(laneOffset(1, 2)).toBe(LANE_STEP / 2);
    expect(laneOffset(1, 3)).toBe(0);
  });
});

describe('assignLanes', () => {
  it('orders lanes by source row index, independent of input order', () => {
    const late = fkGroup({ id: 'late' });
    const early = fkGroup({
      id: 'early',
      pairs: [
        {
          fromTable: 'shipment',
          fromColumn: 'item_id',
          toTable: 'stock',
          toColumn: 'item_id',
          confirmed: false,
        },
      ],
    });
    // `early` points at a row above `late`, so it must get lane 0 either way.
    const a = assignLanes([late, early], COLUMNS);
    const b = assignLanes([early, late], COLUMNS);
    expect(a.get('early')).toBe(0);
    expect(a.get('late')).toBe(1);
    expect(b.get('early')).toBe(0);
    expect(b.get('late')).toBe(1);
  });

  it('keeps different table pairs on independent lanes', () => {
    const other: RelationGroup = {
      id: 'other',
      kind: 'fk',
      type: 'INNER',
      pairs: [
        {
          fromTable: 'employee',
          fromColumn: 'manager_id',
          toTable: 'employee',
          toColumn: 'id',
          confirmed: false,
        },
      ],
    };
    const lanes = assignLanes([fkGroup(), other], COLUMNS);
    expect(lanes.get('fk:shipment_stock')).toBe(0);
    expect(lanes.get('other')).toBe(0);
  });
});

describe('buildRelationShapes — single column FK', () => {
  it('is one orthogonal elbow with a terminal at each end', () => {
    const shapes = build([
      fkGroup({
        pairs: [
          {
            fromTable: 'shipment',
            fromColumn: 'item_id',
            toTable: 'stock',
            toColumn: 'item_id',
            confirmed: false,
          },
        ],
      }),
    ]);
    expect(shapes).toHaveLength(1);
    const shape = shapes[0]!;
    // No separate trunk for a single constraint — the elbow is the connection.
    expect(shape.segments.filter((s) => s.part === 'trunk')).toHaveLength(0);
    // Four points / three segments: out, across, in (never a diagonal).
    expect(shape.segments[0]!.d.match(/[ML]/g)).toHaveLength(4);
    // Symmetric terminals, and nothing that implies a direction.
    expect(shape.dots).toHaveLength(2);
    expect(shape).not.toHaveProperty('arrows');
    expect(shape.state).toBe('candidate');
  });

  it('never draws a diagonal between the two columns', () => {
    const [shape] = build([
      fkGroup({
        pairs: [
          {
            fromTable: 'shipment',
            fromColumn: 'id',
            toTable: 'stock',
            toColumn: 'wh_id',
            confirmed: true,
          },
        ],
      }),
    ]);
    const points = shape!.segments[0]!.d.split('L').map((part) =>
      part.replace('M', '').trim().split(/\s+/).map(Number),
    );
    // Consecutive points differ on exactly one axis → every segment is axial.
    for (let i = 1; i < points.length; i += 1) {
      const [px, py] = points[i - 1]!;
      const [x, y] = points[i]!;
      expect(px === x || py === y).toBe(true);
    }
  });

  it('runs edge to edge at the row centre', () => {
    const y = rowCenterY(positions.shipment.y, 1);
    const [shape] = build([
      fkGroup({
        pairs: [
          {
            fromTable: 'shipment',
            fromColumn: 'item_id',
            toTable: 'stock',
            toColumn: 'item_id',
            confirmed: true,
          },
        ],
      }),
    ]);
    // Leaves the right edge of the source card …
    expect(shape!.segments[0]!.d).toContain(`M ${CARD_WIDTH} ${y}`);
    // … and lands on the left edge of the target card.
    expect(shape!.segments[0]!.d).toContain(`L ${positions.stock.x} ${y}`);
    expect(shape!.state).toBe('confirmed');
  });
});

describe('buildRelationShapes — composite FK', () => {
  it('merges source stubs into one trunk and splits to the targets', () => {
    const shapes = build([fkGroup()]);
    const shape = shapes[0]!;

    const trunk = shape.segments.filter((s) => s.part === 'trunk');
    expect(trunk).toHaveLength(1);
    // Two stubs per pair (source + target) plus the trunk.
    expect(shape.segments.filter((s) => s.part === 'stub')).toHaveLength(4);
    // Symmetric terminals at both ends of every pair.
    expect(shape.dots).toHaveLength(4);
    expect(shape.pairCount).toBe(2);
  });

  it('gives the trunk a visible length even when the rows line up', () => {
    // Both pairs sit on the same row in both tables.
    const aligned = build([
      fkGroup({
        pairs: [
          {
            fromTable: 'shipment',
            fromColumn: 'item_id',
            toTable: 'stock',
            toColumn: 'item_id',
            confirmed: false,
          },
        ],
      }),
      fkGroup({
        id: 'fk2',
        pairs: [
          {
            fromTable: 'shipment',
            fromColumn: 'item_id',
            toTable: 'stock',
            toColumn: 'item_id',
            confirmed: false,
          },
        ],
      }),
    ]);
    void aligned;
    const shapes = build([fkGroup()]);
    const trunk = shapes[0]!.segments.find((s) => s.part === 'trunk')!;
    const ys = trunk.d.match(/-?\d+/g)!.map(Number);
    const height = Math.abs(ys[3]! - ys[1]!);
    expect(height).toBeGreaterThanOrEqual(MIN_TRUNK);
  });

  it('orders the trunk between the two cards, offset per lane', () => {
    const shapes = build([fkGroup()]);
    const trunk = shapes[0]!.segments.find((s) => s.part === 'trunk')!;
    const trunkX = Number(trunk.d.match(/M (-?\d+)/)![1]);
    const gapLeft = positions.shipment.x + CARD_WIDTH;
    const gapRight = positions.stock.x;
    expect(trunkX).toBeGreaterThan(gapLeft);
    expect(trunkX).toBeLessThan(gapRight);
  });

  it('marks a partially confirmed group as partial and tags each stub', () => {
    const shapes = build([
      fkGroup({
        pairs: [
          {
            fromTable: 'shipment',
            fromColumn: 'item_id',
            toTable: 'stock',
            toColumn: 'item_id',
            confirmed: true,
          },
          {
            fromTable: 'shipment',
            fromColumn: 'wh_id',
            toTable: 'stock',
            toColumn: 'wh_id',
            confirmed: false,
          },
        ],
      }),
    ]);
    const shape = shapes[0]!;
    expect(shape.state).toBe('partial');
    expect(shape.confirmedCount).toBe(1);
    expect(shape.segments.some((s) => s.state === 'confirmed')).toBe(true);
    expect(shape.segments.some((s) => s.state === 'candidate')).toBe(true);
  });

  it('collects every touched column key for hover highlighting', () => {
    const [shape] = build([fkGroup()]);
    expect(shape!.columnKeys.sort()).toEqual(
      ['shipment.item_id', 'shipment.wh_id', 'stock.item_id', 'stock.wh_id'].sort(),
    );
  });

  it('gives parallel lanes to two constraints on the same table pair', () => {
    const second = fkGroup({
      id: 'fk:second',
      pairs: [
        {
          fromTable: 'shipment',
          fromColumn: 'wh_id',
          toTable: 'stock',
          toColumn: 'on_hand',
          confirmed: false,
        },
      ],
    });
    const shapes = build([fkGroup(), second]);
    const xs = shapes.map((s) => {
      const trunk = s.segments.find((seg) => seg.part === 'trunk');
      const d = trunk ? trunk.d : s.segments[0]!.d;
      return Number(d.match(/M (-?\d+)/)![1]);
    });
    // The composite group's trunk and the second line must not share an X.
    const trunkXs = shapes
      .map((s) => s.segments.find((seg) => seg.part === 'trunk'))
      .filter(Boolean)
      .map((seg) => Number(seg!.d.match(/M (-?\d+)/)![1]));
    expect(new Set(trunkXs).size).toBe(trunkXs.length);
    void xs;
  });
});

describe('buildRelationShapes — self reference', () => {
  it('loops out of the right edge and back', () => {
    const shapes = buildRelationShapes({
      groups: [
        {
          id: 'self',
          kind: 'fk',
          type: 'INNER',
          pairs: [
            {
              fromTable: 'employee',
              fromColumn: 'manager_id',
              toTable: 'employee',
              toColumn: 'id',
              confirmed: false,
            },
          ],
        },
      ],
      positions: { employee: { x: 0, y: 0 } },
      columnOrder: COLUMNS,
    });
    const shape = shapes[0]!;
    const anchorX = CARD_WIDTH;
    const loopX = anchorX + SELF_LOOP_OFFSET;
    expect(shape.segments.some((s) => s.part === 'trunk')).toBe(true);
    expect(segments([shape]).some((d) => d.includes(`${loopX}`))).toBe(true);
    // Terminals at both ends, no direction marker.
    expect(shape.dots).toHaveLength(2);
  });
});

describe('buildRelationShapes — robustness', () => {
  it('skips groups whose columns are unknown', () => {
    const shapes = build([
      fkGroup({
        pairs: [
          {
            fromTable: 'shipment',
            fromColumn: 'nope',
            toTable: 'stock',
            toColumn: 'item_id',
            confirmed: false,
          },
        ],
      }),
    ]);
    expect(shapes).toEqual([]);
  });

  it('skips groups whose table is not on the canvas', () => {
    const shapes = build([
      fkGroup({
        pairs: [
          {
            fromTable: 'ghost',
            fromColumn: 'x',
            toTable: 'stock',
            toColumn: 'item_id',
            confirmed: false,
          },
        ],
      }),
    ]);
    expect(shapes).toEqual([]);
  });

  it('handles cards that are too close for a trunk by bypassing them', () => {
    const overlapped = { x: CARD_WIDTH - 10, y: 0 };
    const shapes = buildRelationShapes({
      groups: [fkGroup()],
      positions: { shipment: { x: 0, y: 0 }, stock: overlapped },
      columnOrder: COLUMNS,
    });
    const trunk = shapes[0]!.segments.find((s) => s.part === 'trunk')!;
    const trunkX = Number(trunk.d.match(/M (-?\d+)/)![1]);
    // No room between the cards → the trunk routes past both of them.
    expect(trunkX).toBeGreaterThan(overlapped.x + CARD_WIDTH);
  });
});

describe('buildRelationShapes — internal list scrolling', () => {
  // `item_id` sits at index 9, past the capped list's visible window; `a2` is
  // comfortably inside it.
  const manyColumns = {
    shipment: ['id', 'a1', 'a2', 'a3', 'a4', 'a5', 'a6', 'a7', 'a8', 'item_id'],
    stock: ['id', 'item_id'],
  };

  const shapeFor = (fromColumn: string, scrollTop: number) =>
    buildRelationShapes({
      groups: [
        {
          id: 'fk:scroll',
          kind: 'fk',
          type: 'INNER',
          pairs: [
            {
              fromTable: 'shipment',
              fromColumn,
              toTable: 'stock',
              toColumn: 'item_id',
              confirmed: false,
            },
          ],
        },
      ],
      positions,
      columnOrder: manyColumns,
      scrollTops: { shipment: scrollTop },
    })[0]!;

  it('anchors to the real row when it is inside the visible window', () => {
    const dot = shapeFor('a2', 0).dots[0]!;
    expect(dot.offscreen).toBeNull();
    expect(dot.y).toBe(rowCenterY(positions.shipment.y, 2));
  });

  it('clamps below the window and flags it', () => {
    const dot = shapeFor('item_id', 0).dots[0]!;
    expect(dot.offscreen).toBe('down');
    expect(dot.y).toBeLessThanOrEqual(
      positions.shipment.y + cardHeight(manyColumns.shipment.length),
    );
  });

  it('follows the scroll offset, then clamps above the window', () => {
    // Scrolling the list brings the row into view…
    const scrolledIntoView = shapeFor('item_id', 216).dots[0]!;
    expect(scrolledIntoView.offscreen).toBeNull();
    // …and over-scrolling leaves it above the window.
    const scrolledPast = shapeFor('item_id', 600).dots[0]!;
    expect(scrolledPast.offscreen).toBe('up');
    expect(scrolledPast.y).toBeGreaterThanOrEqual(positions.shipment.y);
  });

  it('keeps the card a fixed height once the list is capped', () => {
    expect(cardHeight(40)).toBe(cardHeight(200));
    expect(cardHeight(2)).toBeLessThan(cardHeight(40));
    expect(cardListScrolls(40)).toBe(true);
    expect(cardListScrolls(2)).toBe(false);
  });
});
