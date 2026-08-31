import { describe, expect, it } from 'vitest';
import type { FilterExpression } from '../filterExpression';
import {
  filterExpressionToConditions,
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
