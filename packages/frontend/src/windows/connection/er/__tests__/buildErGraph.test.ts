import { describe, it, expect } from 'vitest';
import { buildErGraph, defaultCollapsedTables } from '../buildErGraph';
import type { ErPredictedRelation } from '../buildErGraph';
import { ER_AUTO_COLLAPSE_COLUMNS, ER_NODE_WIDTH, erNodeHeight } from '../nodeMetrics';
import { ER_LAYOUT_MARGIN } from '../layoutErGraph';
import { ER_DECLARED_COLOR, ER_PREDICTED_COLOR, ER_PREDICTED_DASH } from '../relationStyle';
import type { TableSchema } from '../../../../types';

function makeSchema(
  tableName: string,
  columns: { name: string; dataType: string }[],
  primaryKeys: string[] = [],
  foreignKeys: TableSchema['foreignKeys'] = [],
): TableSchema {
  return {
    tableName,
    columns: columns.map((c) => ({
      name: c.name,
      dataType: c.dataType,
      nullable: true,
      isNullable: true,
      isPrimaryKey: primaryKeys.includes(c.name),
    })),
    primaryKeys,
    indexes: [],
    foreignKeys,
  };
}

const usersSchema = makeSchema(
  'users',
  [
    { name: 'id', dataType: 'INT' },
    { name: 'name', dataType: 'VARCHAR' },
    { name: 'email', dataType: 'VARCHAR' },
  ],
  ['id'],
);

const ordersSchema = makeSchema(
  'orders',
  [
    { name: 'id', dataType: 'INT' },
    { name: 'user_id', dataType: 'INT' },
    { name: 'total', dataType: 'DECIMAL' },
  ],
  ['id'],
  [
    {
      name: 'fk_orders_user',
      columns: ['user_id'],
      referencedTable: 'users',
      referencedColumns: ['id'],
      onUpdate: 'CASCADE',
      onDelete: 'SET NULL',
    },
  ],
);

const productsSchema = makeSchema(
  'products',
  [
    { name: 'id', dataType: 'INT' },
    { name: 'name', dataType: 'VARCHAR' },
    { name: 'price', dataType: 'DECIMAL' },
  ],
  ['id'],
);

const orderItemsSchema = makeSchema(
  'order_items',
  [
    { name: 'id', dataType: 'INT' },
    { name: 'order_id', dataType: 'INT' },
    { name: 'product_id', dataType: 'INT' },
    { name: 'quantity', dataType: 'INT' },
  ],
  ['id'],
  [
    {
      name: 'fk_items_order',
      columns: ['order_id'],
      referencedTable: 'orders',
      referencedColumns: ['id'],
      onUpdate: 'CASCADE',
      onDelete: 'CASCADE',
    },
    {
      name: 'fk_items_product',
      columns: ['product_id'],
      referencedTable: 'products',
      referencedColumns: ['id'],
      onUpdate: 'CASCADE',
      onDelete: 'RESTRICT',
    },
  ],
);

const allSchemas = [usersSchema, ordersSchema, productsSchema, orderItemsSchema];

describe('buildErGraph', () => {
  describe('basic graph generation', () => {
    it('creates one node per table in the order it receives them', () => {
      const { nodes } = buildErGraph(allSchemas);
      expect(nodes).toHaveLength(4);
      // Where each node is *placed* is the layered layout's business (see the
      // `layout` tests); the builder only has to hand back every table it was
      // given, in the order it was given them.
      expect(nodes.map((n) => n.id)).toEqual(allSchemas.map((s) => s.tableName));
    });

    it('all nodes use tableNode type', () => {
      const { nodes } = buildErGraph(allSchemas);
      for (const node of nodes) {
        expect(node.type).toBe('tableNode');
      }
    });

    it('nodes contain correct column data', () => {
      const { nodes } = buildErGraph(allSchemas);
      const usersNode = nodes.find((n) => n.id === 'users')!;
      const columns = usersNode.data.columns as {
        name: string;
        type: string;
        isPk: boolean;
        isFk: boolean;
      }[];

      expect(columns).toHaveLength(3);
      expect(columns[0]).toEqual({
        name: 'id',
        type: 'INT',
        isPk: true,
        isFk: false,
      });
      expect(columns[1]).toEqual({
        name: 'name',
        type: 'VARCHAR',
        isPk: false,
        isFk: false,
      });
    });

    it('marks FK columns correctly', () => {
      const { nodes } = buildErGraph(allSchemas);
      const ordersNode = nodes.find((n) => n.id === 'orders')!;
      const columns = ordersNode.data.columns as {
        name: string;
        isPk: boolean;
        isFk: boolean;
      }[];

      const userIdCol = columns.find((c) => c.name === 'user_id')!;
      expect(userIdCol.isFk).toBe(true);
      expect(userIdCol.isPk).toBe(false);

      const idCol = columns.find((c) => c.name === 'id')!;
      expect(idCol.isPk).toBe(true);
      expect(idCol.isFk).toBe(false);
    });

    it('nodes have valid positions', () => {
      const { nodes } = buildErGraph(allSchemas);
      for (const node of nodes) {
        expect(typeof node.position.x).toBe('number');
        expect(typeof node.position.y).toBe('number');
        expect(node.position.x).toBeGreaterThanOrEqual(0);
        expect(node.position.y).toBeGreaterThanOrEqual(0);
      }
    });

    it('no two nodes share the same position', () => {
      const { nodes } = buildErGraph(allSchemas);
      const positions = nodes.map((n) => `${n.position.x},${n.position.y}`);
      expect(new Set(positions).size).toBe(positions.length);
    });
  });

  describe('edges', () => {
    it('creates edges for foreign keys', () => {
      const { edges } = buildErGraph(allSchemas);
      expect(edges.length).toBeGreaterThanOrEqual(3);
    });

    it('edge connects source to target correctly', () => {
      const { edges } = buildErGraph(allSchemas);
      const ordersFk = edges.find((e) => e.id === 'orders-fk_orders_user')!;
      expect(ordersFk).toBeDefined();
      expect(ordersFk.source).toBe('orders');
      expect(ordersFk.target).toBe('users');
    });

    it('edges have labels showing column names', () => {
      const { edges } = buildErGraph(allSchemas);
      const ordersFk = edges.find((e) => e.id === 'orders-fk_orders_user')!;
      expect(ordersFk.label).toBe('user_id');
    });

    it('anchors a foreign key on the two columns it joins', () => {
      // A line that leaves the node's vertical centre reads as a claim about
      // whatever row happens to sit there, which is how a join on `user_id` ended
      // up looking like a join on `product_name`.
      const { edges } = buildErGraph(allSchemas);
      const ordersFk = edges.find((e) => e.id === 'orders-fk_orders_user')!;
      expect(ordersFk.sourceHandle).toBe('user_id:s-r');
      expect(ordersFk.targetHandle).toBe('id:t-l');
    });

    it('falls back to a column the target actually reports', () => {
      const broken = makeSchema(
        'broken_fk',
        [{ name: 'ghost_id', dataType: 'INT' }],
        [],
        [
          {
            name: 'fk_to_absent_column',
            columns: ['ghost_id'],
            referencedTable: 'users',
            referencedColumns: ['no_such_column'],
            onUpdate: 'CASCADE',
            onDelete: 'CASCADE',
          },
        ],
      );
      const { edges } = buildErGraph([broken, usersSchema]);
      const edge = edges.find((e) => e.source === 'broken_fk')!;
      // Losing the handle would drop the edge entirely; anchor on a real row instead.
      expect(edge.sourceHandle).toBe('ghost_id:s-r');
      expect(edge.targetHandle).toBe('id:t-l');
    });

    it('order_items has two FK edges', () => {
      const { edges } = buildErGraph(allSchemas);
      const itemEdges = edges.filter((e) => e.source === 'order_items');
      expect(itemEdges).toHaveLength(2);
      expect(itemEdges.map((e) => e.target).sort()).toEqual(['orders', 'products']);
    });

    it('edges are animated smoothstep type', () => {
      const { edges } = buildErGraph(allSchemas);
      for (const edge of edges) {
        expect(edge.type).toBe('smoothstep');
        expect(edge.animated).toBe(true);
      }
    });
  });

  describe('empty input', () => {
    it('handles empty schema list', () => {
      const { nodes, edges } = buildErGraph([]);
      expect(nodes).toHaveLength(0);
      expect(edges).toHaveLength(0);
    });
  });

  describe('single table without FK', () => {
    it('creates node with no edges', () => {
      const { nodes, edges } = buildErGraph([productsSchema]);
      expect(nodes).toHaveLength(1);
      expect(edges).toHaveLength(0);
    });
  });

  describe('focusTable mode', () => {
    it('shows only focused table and its relations', () => {
      const { nodes } = buildErGraph(allSchemas, 'orders');
      const names = nodes.map((n) => n.id).sort();
      // orders references users (outgoing FK)
      // order_items references orders (incoming FK)
      expect(names).toEqual(['order_items', 'orders', 'users']);
    });

    it('does not include unrelated tables', () => {
      const { nodes } = buildErGraph(allSchemas, 'orders');
      const names = nodes.map((n) => n.id);
      // products is not directly related to orders
      expect(names).not.toContain('products');
    });

    it('highlights the focused table', () => {
      const { nodes } = buildErGraph(allSchemas, 'orders');
      const ordersNode = nodes.find((n) => n.id === 'orders')!;
      expect(ordersNode.data.highlighted).toBe(true);

      const usersNode = nodes.find((n) => n.id === 'users')!;
      expect(usersNode.data.highlighted).toBe(false);
    });

    it('includes edges only between visible tables', () => {
      const { edges } = buildErGraph(allSchemas, 'orders');
      for (const edge of edges) {
        const nodeIds = ['orders', 'users', 'order_items'];
        expect(nodeIds).toContain(edge.source);
        expect(nodeIds).toContain(edge.target);
      }
    });

    it('falls back to all tables if focusTable not found', () => {
      const { nodes } = buildErGraph(allSchemas, 'nonexistent');
      expect(nodes).toHaveLength(4);
    });

    it('shows only the single table if it has no relations', () => {
      const { nodes, edges } = buildErGraph(allSchemas, 'products');
      // products has no outgoing FK and only order_items references it
      const names = nodes.map((n) => n.id).sort();
      expect(names).toEqual(['order_items', 'products']);
      expect(edges.length).toBeGreaterThanOrEqual(1);
    });
  });

  describe('layout', () => {
    it('places a referenced table to the right of the table referencing it', () => {
      // `order_items` references `orders`, which references `users`. The layered
      // layout must run that chain left to right, so an edge leaves the child's
      // right side and enters the parent's left — the handles TableNode draws.
      const { nodes } = buildErGraph(allSchemas);
      const x = (id: string) => nodes.find((n) => n.id === id)!.position.x;
      expect(x('users')).toBeGreaterThan(x('orders'));
      expect(x('orders')).toBeGreaterThan(x('order_items'));
    });

    it('gives every node a declared size the layout can pack against', () => {
      const { nodes } = buildErGraph(allSchemas);
      for (const node of nodes) {
        expect(node.width).toBe(ER_NODE_WIDTH);
        expect(node.height).toBeGreaterThan(0);
      }
    });

    it('leaves a single table at the canvas margin', () => {
      const { nodes } = buildErGraph([usersSchema]);
      // dagre positions centres and pads by the graph margin; React Flow needs
      // the top-left corner, so the node must not be centred on itself.
      expect(nodes[0].position.x).toBe(ER_LAYOUT_MARGIN);
      expect(nodes[0].position.y).toBe(ER_LAYOUT_MARGIN);
    });

    it('never overlaps two nodes', () => {
      // The bug this replaced: a wide table above a narrow one overlapped it by
      // 192px, because the row's y used the node's own height.
      const { nodes } = buildErGraph(allSchemas);
      for (let i = 0; i < nodes.length; i++) {
        for (let j = i + 1; j < nodes.length; j++) {
          const a = nodes[i]!;
          const b = nodes[j]!;
          const overlapX =
            a.position.x < b.position.x + (b.width ?? 0) &&
            b.position.x < a.position.x + (a.width ?? 0);
          const overlapY =
            a.position.y < b.position.y + (b.height ?? 0) &&
            b.position.y < a.position.y + (a.height ?? 0);
          expect(overlapX && overlapY).toBe(false);
        }
      }
    });

    it('lays out tables with no relationships instead of dropping them', () => {
      const orphan = makeSchema('orphan', [{ name: 'id', dataType: 'INT' }], ['id']);
      const { nodes } = buildErGraph([...allSchemas, orphan]);
      expect(nodes.map((n) => n.id)).toContain('orphan');
      expect(nodes.find((n) => n.id === 'orphan')!.position).toBeDefined();
    });

    it('keeps a wide table from colliding with the next rank', () => {
      const wide = makeSchema(
        'wide',
        Array.from({ length: 40 }, (_, i) => ({ name: `c${i}`, dataType: 'INT' })),
        ['c0'],
      );
      const narrow = makeSchema('narrow', [{ name: 'id', dataType: 'INT' }], ['id']);
      const { nodes } = buildErGraph([wide, narrow]);
      for (const node of nodes) {
        expect(Number.isFinite(node.position.x)).toBe(true);
        expect(Number.isFinite(node.position.y)).toBe(true);
      }
    });

    it('places every related pair on neighbouring ranks', () => {
      const { nodes, edges } = buildErGraph(allSchemas);
      const ranks = [...new Set(nodes.map((n) => n.position.x))].sort((a, b) => a - b);
      const rankOf = (id: string) => ranks.indexOf(nodes.find((n) => n.id === id)!.position.x);
      for (const edge of edges) {
        // The layered layout runs a relationship over a single rank hop, so no
        // line has to cross the whole canvas to reach a join partner.
        expect(Math.abs(rankOf(edge.source) - rankOf(edge.target))).toBeLessThanOrEqual(1);
      }
    });

    it('does not overlap rows when tables differ in height', () => {
      const wide = makeSchema(
        'wide_table',
        Array.from({ length: 12 }, (_, i) => ({ name: `c${i}`, dataType: 'INT' })),
        ['c0'],
      );
      const { nodes } = buildErGraph([wide, usersSchema, ordersSchema, productsSchema]);
      const ys = nodes.map((n) => n.position.y);
      const firstRowY = Math.min(...ys);
      const secondRowY = Math.max(...ys);
      const tallestFirstRow = Math.max(
        ...nodes
          .filter((n) => n.position.y === firstRowY)
          .map((n) => 40 + (n.data.columns as unknown[]).length * 24),
      );
      expect(secondRowY).toBeGreaterThanOrEqual(firstRowY + tallestFirstRow);
    });
  });

  describe('multi-column FK', () => {
    it('handles composite foreign key columns in label', () => {
      const schema = makeSchema(
        'composite_fk',
        [
          { name: 'a', dataType: 'INT' },
          { name: 'b', dataType: 'INT' },
        ],
        [],
        [
          {
            name: 'fk_composite',
            columns: ['a', 'b'],
            referencedTable: 'users',
            referencedColumns: ['id', 'name'],
            onUpdate: 'CASCADE',
            onDelete: 'CASCADE',
          },
        ],
      );
      const { edges } = buildErGraph([schema, usersSchema]);
      const edge = edges.find((e) => e.source === 'composite_fk')!;
      expect(edge.label).toBe('a, b');
    });
  });

  describe('node data properties', () => {
    it('all nodes have highlighted=false by default', () => {
      const { nodes } = buildErGraph(allSchemas);
      for (const node of nodes) {
        expect(node.data.highlighted).toBe(false);
      }
    });

    it('only focused table has highlighted=true', () => {
      const { nodes } = buildErGraph(allSchemas, 'users');
      const usersNode = nodes.find((n) => n.id === 'users')!;
      expect(usersNode.data.highlighted).toBe(true);

      const otherNodes = nodes.filter((n) => n.id !== 'users');
      for (const node of otherNodes) {
        expect(node.data.highlighted).toBe(false);
      }
    });

    it('each node data contains tableName matching node id', () => {
      const { nodes } = buildErGraph(allSchemas);
      for (const node of nodes) {
        expect(node.data.tableName).toBe(node.id);
      }
    });

    it('column count in data matches schema columns', () => {
      const { nodes } = buildErGraph(allSchemas);
      for (const node of nodes) {
        const schema = allSchemas.find((s) => s.tableName === node.id)!;
        const columns = node.data.columns as { name: string }[];
        expect(columns.length).toBe(schema.columns.length);
      }
    });
  });

  describe('edge properties', () => {
    it('all edges have marker end with arrow', () => {
      const { edges } = buildErGraph(allSchemas);
      for (const edge of edges) {
        expect(edge.markerEnd).toBeDefined();
      }
    });

    it('edge ids are unique', () => {
      const { edges } = buildErGraph(allSchemas);
      const ids = edges.map((e) => e.id);
      expect(new Set(ids).size).toBe(ids.length);
    });

    it('does not create edge if referenced table is not visible', () => {
      const isolated = makeSchema(
        'isolated',
        [{ name: 'ref_id', dataType: 'INT' }],
        [],
        [
          {
            name: 'fk_to_missing',
            columns: ['ref_id'],
            referencedTable: 'missing_table',
            referencedColumns: ['id'],
            onUpdate: 'CASCADE',
            onDelete: 'CASCADE',
          },
        ],
      );
      const { edges } = buildErGraph([isolated]);
      expect(edges).toHaveLength(0);
    });
  });

  describe('large dataset', () => {
    it('handles 50 tables without error', () => {
      const largeSchemas = Array.from({ length: 50 }, (_, i) =>
        makeSchema(
          `table_${i}`,
          [
            { name: 'id', dataType: 'INT' },
            { name: 'name', dataType: 'VARCHAR' },
          ],
          ['id'],
          i > 0
            ? [
                {
                  name: `fk_${i}`,
                  columns: ['name'],
                  referencedTable: `table_${i - 1}`,
                  referencedColumns: ['id'],
                  onUpdate: 'CASCADE',
                  onDelete: 'CASCADE',
                },
              ]
            : [],
        ),
      );
      const { nodes, edges } = buildErGraph(largeSchemas);
      expect(nodes).toHaveLength(50);
      expect(edges).toHaveLength(49);
    });
  });

  describe('self-referencing FK', () => {
    it('creates edge from table to itself', () => {
      const schema = makeSchema(
        'categories',
        [
          { name: 'id', dataType: 'INT' },
          { name: 'parent_id', dataType: 'INT' },
        ],
        ['id'],
        [
          {
            name: 'fk_parent',
            columns: ['parent_id'],
            referencedTable: 'categories',
            referencedColumns: ['id'],
            onUpdate: 'CASCADE',
            onDelete: 'SET NULL',
          },
        ],
      );
      const { nodes, edges } = buildErGraph([schema]);
      expect(nodes).toHaveLength(1);
      expect(edges).toHaveLength(1);
      expect(edges[0].source).toBe('categories');
      expect(edges[0].target).toBe('categories');
    });
  });
});

describe('buildErGraph with inferred relationships', () => {
  const predicted: ErPredictedRelation[] = [
    {
      id: 'predicted-orders-users-user_id=id',
      fromTable: 'orders',
      toTable: 'users',
      columnPairs: [{ left: 'user_id', right: 'id' }],
      score: 0.9,
    },
  ];

  it('draws an inferred relationship as its own edge', () => {
    const { edges } = buildErGraph(allSchemas, undefined, predicted);
    const inferred = edges.filter((e) => e.data?.kind === 'predicted');
    expect(inferred).toHaveLength(1);
    expect(inferred[0]!.id).toBe('predicted-orders-users-user_id=id');
    expect(inferred[0]!.source).toBe('orders');
    expect(inferred[0]!.target).toBe('users');
  });

  it('keeps an inference visually distinct from a constraint', () => {
    // The whole point of the feature is that a guess is never presented with the
    // same certainty as something the database enforces.
    const { edges } = buildErGraph(allSchemas, undefined, predicted);
    const declared = edges.find((e) => e.data?.kind === 'declared')!;
    const inferred = edges.find((e) => e.data?.kind === 'predicted')!;

    expect(inferred.style?.strokeDasharray).toBeTruthy();
    expect(inferred.animated).toBe(false);
    expect(inferred.style?.stroke).not.toBe(declared.style?.stroke);
    expect(inferred.markerEnd).toBeTruthy();
  });

  it('draws each kind with the stroke the legend documents', () => {
    // The legend renders these same constants, so this is what keeps the key and
    // the canvas from drifting apart.
    const { edges } = buildErGraph(allSchemas, undefined, predicted);
    const declared = edges.find((e) => e.data?.kind === 'declared')!;
    const inferred = edges.find((e) => e.data?.kind === 'predicted')!;

    expect(declared.style?.stroke).toBe(ER_DECLARED_COLOR);
    expect(declared.style?.strokeDasharray).toBeUndefined();
    expect(inferred.style?.stroke).toBe(ER_PREDICTED_COLOR);
    expect(inferred.style?.strokeDasharray).toBe(ER_PREDICTED_DASH);
  });

  it('keeps a constraint and an inference that share a pair as two edges', () => {
    // `allSchemas` already declares orders → users and `predicted` infers it again,
    // which is the ordinary shape that used to take the layout down. The layout
    // collapses the pair before dagre sees it (see `layoutEdges`), but React Flow
    // must still receive both edges — one solid, one dashed.
    const { nodes, edges } = buildErGraph(allSchemas, undefined, predicted);
    const shared = edges.filter((e) => e.source === 'orders' && e.target === 'users');
    expect(shared).toHaveLength(2);
    expect(shared.map((e) => e.data?.kind).sort()).toEqual(['declared', 'predicted']);
    for (const node of nodes) {
      expect(Number.isFinite(node.position.x)).toBe(true);
      expect(Number.isFinite(node.position.y)).toBe(true);
    }
  });

  it('marks inferred columns as foreign keys on the node', () => {
    const { nodes } = buildErGraph(allSchemas, undefined, predicted);
    const ordersNode = nodes.find((n) => n.id === 'orders')!;
    const columns = ordersNode.data.columns as { name: string; isFk: boolean }[];
    expect(columns.find((c) => c.name === 'user_id')!.isFk).toBe(true);
  });

  it('draws no inferred edges by default', () => {
    const { edges } = buildErGraph(allSchemas);
    expect(edges.some((e) => e.data?.kind === 'predicted')).toBe(false);
  });

  it('skips an inference whose tables are not both present', () => {
    const orphan: ErPredictedRelation[] = [
      {
        id: 'predicted-orders-ghosts-order_id=id',
        fromTable: 'orders',
        toTable: 'ghosts',
        columnPairs: [{ left: 'order_id', right: 'id' }],
        score: 0.9,
      },
    ];
    const { edges } = buildErGraph(allSchemas, undefined, orphan);
    expect(edges.some((e) => e.data?.kind === 'predicted')).toBe(false);
  });

  it('includes an inferred neighbour when focusing a table', () => {
    // A focus view that ignored inferences would hide exactly the relationships
    // the feature was added to surface.
    const isolated = makeSchema('audit_log', [{ name: 'id', dataType: 'INT' }], ['id']);
    const inferred: ErPredictedRelation[] = [
      {
        id: 'predicted-audit_log-users-actor_id=id',
        fromTable: 'audit_log',
        toTable: 'users',
        columnPairs: [{ left: 'actor_id', right: 'id' }],
        score: 0.8,
      },
    ];
    const { nodes, edges } = buildErGraph([...allSchemas, isolated], 'users', inferred);
    expect(nodes.map((n) => n.id)).toContain('audit_log');
    expect(edges.some((e) => e.data?.kind === 'predicted')).toBe(true);
  });

  it('labels an inferred edge with its source column', () => {
    const { edges } = buildErGraph(allSchemas, undefined, predicted);
    const inferred = edges.find((e) => e.data?.kind === 'predicted')!;
    expect(inferred.label).toBe('user_id');
  });

  it('anchors an inferred edge on the columns it joins', () => {
    const { edges } = buildErGraph(allSchemas, undefined, predicted);
    const inferred = edges.find((e) => e.data?.kind === 'predicted')!;
    expect(inferred.sourceHandle).toBe('user_id:s-r');
    expect(inferred.targetHandle).toBe('id:t-l');
  });

  it('draws a guess the engine made from both sides only once', () => {
    // Three tables sharing a `order_id` key each predict the link to each of the
    // others in both directions; the second copy adds nothing but crossing lines.
    const bothWays: ErPredictedRelation[] = [
      {
        id: 'predicted-orders-users-user_id=id',
        fromTable: 'orders',
        toTable: 'users',
        columnPairs: [{ left: 'user_id', right: 'id' }],
        score: 0.9,
      },
      {
        id: 'predicted-users-orders-id=user_id',
        fromTable: 'users',
        toTable: 'orders',
        columnPairs: [{ left: 'id', right: 'user_id' }],
        score: 0.7,
      },
    ];
    const { edges } = buildErGraph(allSchemas, undefined, bothWays);
    const inferred = edges.filter((e) => e.data?.kind === 'predicted');
    expect(inferred).toHaveLength(1);
    expect(inferred[0]!.id).toBe('predicted-orders-users-user_id=id');
    expect(inferred[0]!.source).toBe('orders');
  });

  it('keeps two different guesses about the same table pair apart', () => {
    const twoLinks: ErPredictedRelation[] = [
      {
        id: 'predicted-orders-users-id=name',
        fromTable: 'orders',
        toTable: 'users',
        columnPairs: [{ left: 'id', right: 'name' }],
        score: 0.6,
      },
      {
        id: 'predicted-orders-users-name=email',
        fromTable: 'orders',
        toTable: 'users',
        columnPairs: [{ left: 'name', right: 'email' }],
        score: 0.6,
      },
    ];
    const { edges } = buildErGraph(allSchemas, undefined, twoLinks);
    expect(edges.filter((e) => e.data?.kind === 'predicted')).toHaveLength(2);
  });

  it('renders an ambiguous guess as a faint, unlabelled line', () => {
    const ambiguous: ErPredictedRelation[] = [
      {
        id: 'predicted-orders-users-user_id=id',
        fromTable: 'orders',
        toTable: 'users',
        columnPairs: [{ left: 'user_id', right: 'id' }],
        score: 0.8,
        ambiguous: true,
      },
    ];
    const { edges } = buildErGraph(allSchemas, undefined, ambiguous);
    const edge = edges.find((e) => e.id === 'predicted-orders-users-user_id=id')!;
    expect(edge.label).toBeFalsy();
    expect(edge.style?.opacity).toBeLessThan(1);
    expect(edge.data?.ambiguous).toBe(true);
  });

  it('fades an ambiguous guess with its arrowhead', () => {
    // `<marker>` paint is not reached by the line's opacity, so an unfaded chevron
    // would be the one loud thing left on a faint line.
    const faded: ErPredictedRelation[] = [
      {
        id: 'predicted-orders-users-user_id=id',
        fromTable: 'orders',
        toTable: 'users',
        columnPairs: [{ left: 'user_id', right: 'id' }],
        score: 0.8,
        ambiguous: true,
      },
    ];
    const { edges } = buildErGraph(allSchemas, undefined, faded);
    const edge = edges.find((e) => e.data?.kind === 'predicted')!;
    expect(String((edge.markerEnd as { color: string }).color)).toContain('color-mix');
  });

  it('paints every edge from theme tokens, not the nonexistent --color-* namespace', () => {
    // A `--color-*` reference resolves to neither theme and silently drops the
    // paint; `src/styles/globals.css` documents the same trap for the query builder.
    const { edges } = buildErGraph(allSchemas, undefined, predicted);
    expect(edges.length).toBeGreaterThan(0);
    for (const edge of edges) {
      const painted = [
        edge.style?.stroke,
        edge.labelStyle?.fill,
        edge.labelBgStyle?.fill,
        (edge.markerEnd as { color?: string } | undefined)?.color,
      ];
      for (const value of painted) {
        if (typeof value !== 'string') continue;
        expect(value).not.toMatch(/--color-/);
        if (value.startsWith('var(')) expect(value).toMatch(/^var\(--c-/);
      }
    }
  });
});

describe('buildErGraph with collapsed tables', () => {
  it('marks a collapsed table and shrinks the height it reserves', () => {
    const expanded = buildErGraph(allSchemas);
    const collapsed = buildErGraph(allSchemas, undefined, [], new Set(['orders']));

    const heightOf = (laid: ReturnType<typeof buildErGraph>, id: string) =>
      laid.nodes.find((n) => n.id === id)!.height!;
    const collapsedFlag = (laid: ReturnType<typeof buildErGraph>, id: string) =>
      laid.nodes.find((n) => n.id === id)!.data.collapsed;

    expect(collapsedFlag(expanded, 'orders')).toBe(false);
    expect(collapsedFlag(collapsed, 'orders')).toBe(true);
    expect(heightOf(collapsed, 'orders')).toBeLessThan(heightOf(expanded, 'orders'));
  });

  it('leaves other tables expanded', () => {
    const collapsed = buildErGraph(allSchemas, undefined, [], new Set(['orders']));
    expect(collapsed.nodes.find((n) => n.id === 'users')!.data.collapsed).toBe(false);
  });

  it('re-runs the layout so the freed space is reclaimed', () => {
    // Collapsing must move the remaining nodes, otherwise the diagram keeps the
    // collapsed node's old footprint and a later expansion can overlap a neighbour.
    const expanded = buildErGraph(allSchemas);
    const collapsed = buildErGraph(allSchemas, undefined, [], new Set(['orders']));
    const positions = (laid: ReturnType<typeof buildErGraph>) =>
      laid.nodes.map((n) => `${n.id}:${n.position.x},${n.position.y}`).join(' ');
    expect(positions(collapsed)).not.toBe(positions(expanded));
  });

  it('still never overlaps once a table is collapsed', () => {
    const collapsed = buildErGraph(
      allSchemas,
      undefined,
      [],
      new Set(['orders', 'order_items', 'products']),
    );
    for (let i = 0; i < collapsed.nodes.length; i++) {
      for (let j = i + 1; j < collapsed.nodes.length; j++) {
        const a = collapsed.nodes[i]!;
        const b = collapsed.nodes[j]!;
        const overlapX =
          a.position.x < b.position.x + (b.width ?? 0) &&
          b.position.x < a.position.x + (a.width ?? 0);
        const overlapY =
          a.position.y < b.position.y + (b.height ?? 0) &&
          b.position.y < a.position.y + (a.height ?? 0);
        expect(overlapX && overlapY).toBe(false);
      }
    }
  });

  it('lays out the same way when the collapsed set is empty', () => {
    expect(buildErGraph(allSchemas, undefined, [], new Set()).nodes.map((n) => n.position)).toEqual(
      buildErGraph(allSchemas).nodes.map((n) => n.position),
    );
  });
});

describe('buildErGraph edge handle routing', () => {
  const handleOf = (laid: ReturnType<typeof buildErGraph>, id: string) => {
    const edge = laid.edges.find((e) => e.id === id)!;
    return { source: edge.sourceHandle, target: edge.targetHandle };
  };

  it('points a forward edge at the rows it joins', () => {
    // `orders.user_id` references `users.id`. The layout puts `users` on the
    // right, so the edge leaves the child's right and enters the parent's left.
    const { edges } = buildErGraph(allSchemas);
    const declared = edges.find((e) => e.id === 'orders-fk_orders_user')!;
    expect(declared.sourceHandle).toBe('user_id:s-r');
    expect(declared.targetHandle).toBe('id:t-l');
  });

  it('mirrors the sides for an edge that points leftwards', () => {
    // Two tables referencing each other: the layered layout cannot keep both
    // pointing right, so one must be routed from the left.
    const a = makeSchema(
      'a',
      [
        { name: 'id', dataType: 'INT' },
        { name: 'b_id', dataType: 'INT' },
      ],
      ['id'],
      [{ name: 'fk_a_b', columns: ['b_id'], referencedTable: 'b', referencedColumns: ['id'] }],
    );
    const b = makeSchema(
      'b',
      [
        { name: 'id', dataType: 'INT' },
        { name: 'a_id', dataType: 'INT' },
      ],
      ['id'],
      [{ name: 'fk_b_a', columns: ['a_id'], referencedTable: 'a', referencedColumns: ['id'] }],
    );
    const laid = buildErGraph([a, b]);
    const x = (id: string) => laid.nodes.find((n) => n.id === id)!.position.x;

    const fromA = handleOf(laid, 'a-fk_a_b');
    const fromB = handleOf(laid, 'b-fk_b_a');
    // Whichever table ended up on the right must route from its left side.
    if (x('b') > x('a')) {
      expect(fromA).toEqual({ source: 'b_id:s-r', target: 'id:t-l' });
      expect(fromB).toEqual({ source: 'a_id:s-l', target: 'id:t-r' });
    } else {
      expect(fromA).toEqual({ source: 'b_id:s-l', target: 'id:t-r' });
      expect(fromB).toEqual({ source: 'a_id:s-r', target: 'id:t-l' });
    }
  });

  it('falls back to the node-level handles when a table is collapsed', () => {
    // A collapsed node renders no rows, so there is no row to point at.
    const { edges } = buildErGraph(allSchemas, undefined, [], new Set(['orders']));
    const declared = edges.find((e) => e.id === 'orders-fk_orders_user')!;
    expect(declared.sourceHandle).toBe('node:s-r');
    expect(declared.targetHandle).toBe('id:t-l');
  });

  it('falls back to the node-level handles for a self-reference', () => {
    const employee = makeSchema(
      'employee',
      [
        { name: 'id', dataType: 'INT' },
        { name: 'manager_id', dataType: 'INT' },
      ],
      ['id'],
      [
        {
          name: 'fk_self',
          columns: ['manager_id'],
          referencedTable: 'employee',
          referencedColumns: ['id'],
        },
      ],
    );
    const { edges } = buildErGraph([employee]);
    expect(edges[0].sourceHandle).toBe('node:s-r');
    expect(edges[0].targetHandle).toBe('node:t-l');
  });

  it('reports only the columns an edge touches', () => {
    // A wide table must not carry connection points on every row.
    const { nodes } = buildErGraph(allSchemas);
    const orders = nodes.find((n) => n.id === 'orders')!;
    const handleColumns = orders.data.handleColumns as string[];
    expect(handleColumns).toContain('user_id');
    expect(handleColumns).toContain('id');
    expect(handleColumns).not.toContain('total');
  });

  it('gives every edge handles it can actually use', () => {
    const laid = buildErGraph(allSchemas);
    for (const edge of laid.edges) {
      expect(typeof edge.sourceHandle).toBe('string');
      expect(typeof edge.targetHandle).toBe('string');
      const source = laid.nodes.find((n) => n.id === edge.source)!;
      const target = laid.nodes.find((n) => n.id === edge.target)!;
      for (const [node, handle, role] of [
        [source, edge.sourceHandle, 's'],
        [target, edge.targetHandle, 't'],
      ] as const) {
        // Either a node-level fallback, or a column that is declared on the node.
        if (String(handle).startsWith('node:')) {
          expect(String(handle)).toBe(`node:${role}-${String(handle).endsWith('-l') ? 'l' : 'r'}`);
        } else {
          const column = String(handle).split(':')[0]!;
          expect(node.data.handleColumns as string[]).toContain(column);
          expect((node.data.columns as { name: string }[]).map((c) => c.name)).toContain(column);
        }
      }
    }
  });
});

describe('buildErGraph wide tables', () => {
  const wide = (count: number) =>
    makeSchema(
      'wide',
      Array.from({ length: count }, (_, i) => ({ name: `c${i}`, dataType: 'INT' })),
      ['c0'],
    );

  it('seeds a very wide table as collapsed', () => {
    // Without an internal scroll a 60-column table would be a 1478px node.
    expect(defaultCollapsedTables([wide(60)])).toEqual(new Set(['wide']));
  });

  it('leaves a table at or below the threshold expanded', () => {
    expect(defaultCollapsedTables([wide(ER_AUTO_COLLAPSE_COLUMNS)])).toEqual(new Set());
    expect(defaultCollapsedTables([wide(ER_AUTO_COLLAPSE_COLUMNS + 1)])).toEqual(new Set(['wide']));
  });

  it('stays expanded when the user expands a wide table', () => {
    // The seed is a starting point, not a rule: the chevron must work both ways,
    // so the graph builder must not re-collapse a wide table behind the user.
    const { nodes } = buildErGraph([wide(60)], undefined, [], new Set());
    expect(nodes[0].data.collapsed).toBe(false);
    expect(nodes[0].height).toBe(erNodeHeight(60));
  });

  it('collapses a wide table when the seeded set is applied', () => {
    const schemas = [wide(60)];
    const { nodes } = buildErGraph(schemas, undefined, [], defaultCollapsedTables(schemas));
    expect(nodes[0].data.collapsed).toBe(true);
  });

  it('leaves an ordinary schema untouched', () => {
    const { nodes } = buildErGraph(allSchemas, undefined, [], defaultCollapsedTables(allSchemas));
    for (const node of nodes) expect(node.data.collapsed).toBe(false);
  });
});
