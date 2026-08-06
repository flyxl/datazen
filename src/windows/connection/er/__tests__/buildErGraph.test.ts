import { describe, it, expect } from 'vitest';
import { buildErGraph } from '../buildErGraph';
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
    it('creates one node per table', () => {
      const { nodes } = buildErGraph(allSchemas);
      expect(nodes).toHaveLength(4);
      expect(nodes.map((n) => n.id)).toEqual([
        'users',
        'orders',
        'products',
        'order_items',
      ]);
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

    it('order_items has two FK edges', () => {
      const { edges } = buildErGraph(allSchemas);
      const itemEdges = edges.filter((e) => e.source === 'order_items');
      expect(itemEdges).toHaveLength(2);
      expect(itemEdges.map((e) => e.target).sort()).toEqual([
        'orders',
        'products',
      ]);
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
    it('uses grid layout with sqrt-based columns', () => {
      const { nodes } = buildErGraph(allSchemas);
      // 4 tables -> sqrt(4) = 2 columns
      const xValues = new Set(nodes.map((n) => n.position.x));
      expect(xValues.size).toBe(2);
    });

    it('single table is at origin', () => {
      const { nodes } = buildErGraph([usersSchema]);
      expect(nodes[0].position.x).toBe(0);
      expect(nodes[0].position.y).toBe(0);
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
