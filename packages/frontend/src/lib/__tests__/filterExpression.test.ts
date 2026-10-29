import { describe, expect, it } from 'vitest';
import type { FilterExpression } from '../filterExpression';
import {
  filterExpressionToConditions,
  flattenToConnectors,
  parseFilterExpression,
  parseFilterForApply,
} from '../filterExpression';

const columns = ['status', 'amount', 'active', 'deletedAt', 'customer name'];

function parse(input: string) {
  const result = parseFilterExpression(input, columns);
  expect(result.ok).toBe(true);
  if (!result.ok) throw new Error(result.error);
  return result.value;
}

function condition(
  column: string,
  operator: 'eq' | 'ne' | 'gt' | 'gte' | 'lt' | 'lte' | 'isNull' | 'isNotNull',
  value: string | number | boolean | null,
): FilterExpression {
  return { type: 'condition', column, operator, value };
}

describe('parseFilterExpression', () => {
  it('parses a simple equality with a bound string value', () => {
    expect(parse("status = 'paid'")).toEqual(condition('status', 'eq', 'paid'));
  });

  it('normalizes all supported comparison operators', () => {
    expect(parse('amount != 10')).toEqual(condition('amount', 'ne', 10));
    expect(parse('amount > 10')).toEqual(condition('amount', 'gt', 10));
    expect(parse('amount >= 10')).toEqual(condition('amount', 'gte', 10));
    expect(parse('amount < 10')).toEqual(condition('amount', 'lt', 10));
    expect(parse('amount <= 10')).toEqual(condition('amount', 'lte', 10));
  });

  it('parses string escapes without turning them into SQL text', () => {
    expect(parse("status = 'O''Reilly'")).toEqual(condition('status', 'eq', "O'Reilly"));
    expect(parse(String.raw`status = 'it\'s\\ok'`)).toEqual(
      condition('status', 'eq', "it's\\ok"),
    );
    expect(parse("status = ''")).toEqual(condition('status', 'eq', ''));
  });

  it('parses numeric, boolean, and NULL literals', () => {
    expect(parse('amount = -2.5e2')).toEqual(condition('amount', 'eq', -250));
    expect(parse('active = TRUE')).toEqual(condition('active', 'eq', true));
    expect(parse('active != false')).toEqual(condition('active', 'ne', false));
    expect(parse('deletedAt = NULL')).toEqual(condition('deletedAt', 'eq', null));
  });

  it('parses IS NULL and IS NOT NULL without requiring a value field from the user', () => {
    expect(parse('deletedAt IS NULL')).toEqual(condition('deletedAt', 'isNull', null));
    expect(parse('deletedAt is not null')).toEqual(condition('deletedAt', 'isNotNull', null));
  });

  it('preserves AND/OR precedence in the logical tree', () => {
    expect(parse("status = 'paid' OR amount > 100 AND active = true")).toEqual({
      type: 'logical',
      operator: 'or',
      left: condition('status', 'eq', 'paid'),
      right: {
        type: 'logical',
        operator: 'and',
        left: condition('amount', 'gt', 100),
        right: condition('active', 'eq', true),
      },
    });
  });

  it('honors explicit parentheses while keeping the AST structured', () => {
    expect(parse("(status = 'paid' OR amount > 100) AND active = true")).toEqual({
      type: 'logical',
      operator: 'and',
      left: {
        type: 'logical',
        operator: 'or',
        left: condition('status', 'eq', 'paid'),
        right: condition('amount', 'gt', 100),
      },
      right: condition('active', 'eq', true),
    });
  });

  it('accepts an exact quoted column name from the allow-list', () => {
    expect(parse('"customer name" = \'Alice\'')).toEqual(
      condition('customer name', 'eq', 'Alice'),
    );
  });

  it('flattens leaves in source order for the existing structured condition API', () => {
    const expression = parse("status = 'paid' OR amount > 100 AND active = true");
    expect(filterExpressionToConditions(expression)).toEqual([
      { column: 'status', operator: 'eq', value: 'paid' },
      { column: 'amount', operator: 'gt', value: 100 },
      { column: 'active', operator: 'eq', value: true },
    ]);
  });

  it('returns a structured apply payload without SQL text', () => {
    const result = parseFilterForApply("status = 'paid' AND amount > 100", columns);
    expect(result).toEqual({
      ok: true,
      value: {
        expression: {
          type: 'logical',
          operator: 'and',
          left: condition('status', 'eq', 'paid'),
          right: condition('amount', 'gt', 100),
        },
        conditions: [
          { column: 'status', operator: 'eq', value: 'paid' },
          { column: 'amount', operator: 'gt', value: 100 },
        ],
      },
    });
    expect(JSON.stringify(result)).not.toContain('SELECT');
    expect(JSON.stringify(result)).not.toContain('WHERE');
  });
});

describe('controlled filter expression rejection', () => {
  const invalidExpressions = [
    '',
    'status =',
    'status = \'paid',
    "status = 'paid\\q'",
    'unknown = 1',
    'status LIKE \'paid\'',
    'status IN (\'paid\')',
    'status = 1; DROP TABLE users',
    "status = 'paid' -- comment",
    "status = 'paid' /* comment */",
    'lower(status) = \'paid\'',
    'status = (SELECT 1)',
    'status = 1 AND',
    'status = 1 OR (amount > 2',
    'status <> 1',
    'status == 1',
    'status NOT NULL',
  ];

  it.each(invalidExpressions)('rejects %j', (input) => {
    const result = parseFilterExpression(input, columns);
    expect(result.ok).toBe(false);
    if (result.ok) return;
    expect(result.error).toEqual(expect.any(String));
    expect(result.position).toBeGreaterThanOrEqual(0);
  });

  it('rejects a column that is not in the supplied schema, even when quoted', () => {
    const result = parseFilterExpression('"users.id" = 1', columns);
    expect(result.ok).toBe(false);
    if (!result.ok) expect(result.error).toContain('Unknown filter column');
  });
});

describe('flattenToConnectors', () => {
  it('flatten_keeps_plain_chain_in_order', () => {
    const flattened = flattenToConnectors(parse("status = 'paid' AND amount > 10 AND active = true"));
    expect(flattened).not.toBeNull();
    expect(flattened?.map((entry) => entry.condition.column)).toEqual([
      'status',
      'amount',
      'active',
    ]);
    expect(flattened?.map((entry) => entry.connector)).toEqual([undefined, 'and', 'and']);
    expect(flattened?.[0]?.condition).toEqual(condition('status', 'eq', 'paid'));
    expect(flattened?.[1]?.condition).toEqual(condition('amount', 'gt', 10));
  });

  it('flatten_accepts_left_leaning_chain_across_operators', () => {
    const flattened = flattenToConnectors(parse("(status = 'paid' AND amount > 10) OR active = true"));
    expect(flattened).not.toBeNull();
    expect(flattened?.map((entry) => entry.condition.column)).toEqual([
      'status',
      'amount',
      'active',
    ]);
    // The second condition joins the first one, the third one joins the pair.
    expect(flattened?.map((entry) => entry.connector)).toEqual([undefined, 'and', 'or']);
  });

  it('flatten_rejects_nested_or_in_right_child', () => {
    expect(flattenToConnectors(parse("status = 'paid' AND (amount > 10 OR active = true)"))).toBeNull();
  });

  it('flatten_rejects_nested_after_and_run', () => {
    expect(
      flattenToConnectors(parse("status = 'paid' AND (amount > 10 OR active = true) AND amount < 99")),
    ).toBeNull();
  });

  it('flatten_rejects_an_or_group_nested_under_an_and', () => {
    // `(a OR b) AND c` cannot be written as a flat list either: the OR group
    // sits inside an AND, and the flat list has no place for parentheses.
    expect(flattenToConnectors(parse("(status = 'paid' OR amount > 10) AND active = true"))).toBeNull();
  });

  it('flatten_accepts_a_mixed_chain_that_keeps_the_sql_grouping', () => {
    // `a OR b AND c` is `a OR (b AND c)`, which the flat list can express:
    // the connector of `b` is OR, the one of `c` is AND.
    const flattened = flattenToConnectors(parse("status = 'paid' OR amount > 10 AND active = true"));
    expect(flattened).not.toBeNull();
    expect(flattened?.map((entry) => entry.condition.column)).toEqual([
      'status',
      'amount',
      'active',
    ]);
    expect(flattened?.map((entry) => entry.connector)).toEqual([undefined, 'or', 'and']);
  });

  it('flatten_never_emits_connector_on_first_item', () => {
    for (const input of [
      "status = 'paid'",
      "status = 'paid' AND amount > 10",
      "(status = 'paid' AND amount > 10) OR active = true",
      "status = 'paid' OR amount > 10 AND active = true",
      "status = 'paid' AND amount > 10 OR active = true",
    ]) {
      const flattened = flattenToConnectors(parse(input));
      expect(flattened).not.toBeNull();
      expect(flattened?.[0]?.connector).toBeUndefined();
    }
  });

  it('flatten_output_conditions_carry_no_connector_by_default', () => {
    // The parser layer stays connector-free: a condition that was not given a
    // connector must fall back to the request-wide logic, so it must not
    // pretend to carry one.
    const expression = parse("status = 'paid' AND amount > 10");
    const conditions = filterExpressionToConditions(expression);
    expect(conditions.every((entry) => entry.connector === undefined)).toBe(true);
  });
});

  it('flatten_accepts_a_mixed_chain_without_parentheses', () => {
    // `a AND b OR c` is `(a AND b) OR c`, and the flat list holds it: the
    // connector of `b` is AND, the one of `c` is OR.
    const flattened = flattenToConnectors(parse("status = 'paid' AND amount > 10 OR active = true"));
    expect(flattened).not.toBeNull();
    expect(flattened?.map((entry) => entry.condition.column)).toEqual([
      'status',
      'amount',
      'active',
    ]);
    expect(flattened?.map((entry) => entry.connector)).toEqual([undefined, 'and', 'or']);
  });

  it('the rule the design first wrote would have refused the mixed chain', () => {
    // The rule "the right child has to be a chain of the same operator"
    // refuses `a = 1 or b = 2 and c = 3`: the tree of that expression is
    // `or(a, and(b, c))`, so the right child is an `and` subtree, not an `or`
    // chain. Under that rule the old guard was never replaced — the very
    // expression this change is about would still have been refused. The rule
    // in the code refuses only what a flat list really cannot hold.
    const oldRule = (
      node: FilterExpression,
    ): Array<{ condition: FilterExpression; connector?: 'and' | 'or' }> | null => {
      if (node.type === 'condition') return [{ condition: node }];
      const { operator, left, right } = node;
      const leftParts = oldRule(left);
      if (!leftParts) return null;
      if (right.type === 'condition') {
        return [...leftParts, { condition: right, connector: operator }];
      }
      if (right.operator !== operator) return null;
      const rightParts = oldRule(right);
      if (!rightParts) return null;
      const [first, ...rest] = rightParts;
      if (!first) return null;
      return [...leftParts, { condition: first.condition, connector: operator }, ...rest];
    };
    expect(oldRule(parse("status = 'paid' OR amount > 10 AND active = true"))).toBeNull();
  });
