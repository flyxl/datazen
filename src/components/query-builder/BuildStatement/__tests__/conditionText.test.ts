/**
 * Chip text for conditions — the part the user reads to know what the clause
 * holds, so it has to mirror the SQL operand/operator/value honestly (and stay
 * short enough to keep a clause row to one line).
 */
import { describe, expect, it } from 'vitest';
import {
  conditionChipParts,
  conditionChipPrefix,
  conditionValueText,
  findGroupLogic,
  firstConditionOf,
} from '../conditionText';
import type { QbCondition, QbConditionGroup } from '../../types';

function cond(patch: Partial<QbCondition> = {}): QbCondition {
  return {
    id: 'c1',
    table: 'sales',
    column: 'qty',
    operator: '>=',
    value: '2000',
    conjunction: 'AND',
    ...patch,
  };
}

const text = (c: QbCondition, aliases: Record<string, string> = {}) => {
  const p = conditionChipParts(c, aliases);
  return `${p.prefix}${p.label}${p.suffix}`;
};

describe('conditionChipParts', () => {
  it('renders operand, operator and value', () => {
    expect(text(cond())).toBe('sales.qty >= 2000');
  });

  it('uses the alias qualifier', () => {
    expect(text(cond(), { sales: 's' })).toBe('s.qty >= 2000');
  });

  it('wraps an aggregated operand', () => {
    expect(text(cond({ aggregate: 'SUM' }))).toBe('SUM(sales.qty) >= 2000');
  });

  it('omits the value for IS NULL / IS NOT NULL', () => {
    expect(conditionValueText(cond({ operator: 'IS NULL', value: null }))).toBe('');
    expect(text(cond({ operator: 'IS NOT NULL', value: null }))).toBe('sales.qty IS NOT NULL');
  });

  it('parenthesises an IN list', () => {
    expect(text(cond({ operator: 'IN', value: 'A, B' }))).toBe('sales.qty IN (A, B)');
  });

  it('elides a long value instead of blowing up the row', () => {
    const long = 'x'.repeat(80);
    const rendered = text(cond({ operator: '=', value: long }));
    expect(rendered.length).toBeLessThan(50);
    expect(rendered).toContain('…');
  });

  it('handles a missing value without printing null', () => {
    expect(text(cond({ operator: '=', value: null }))).toBe('sales.qty =');
  });
});

describe('conditionChipPrefix', () => {
  it('hides the conjunction on the first row of a group', () => {
    expect(conditionChipPrefix(cond(), true)).toBe('');
  });

  it('shows the row conjunction otherwise', () => {
    expect(conditionChipPrefix(cond({ conjunction: 'OR' }), false)).toBe('OR');
  });
});

describe('condition tree helpers', () => {
  const sub: QbConditionGroup = {
    id: 'sub',
    logic: 'OR',
    conditions: [cond({ id: 'a' })],
    groups: [],
  };
  const root: QbConditionGroup = {
    id: 'root',
    logic: 'AND',
    conditions: [cond({ id: 'r' })],
    groups: [sub],
  };

  it('finds a group logic at any depth', () => {
    expect(findGroupLogic(root, 'root')).toBe('AND');
    expect(findGroupLogic(root, 'sub')).toBe('OR');
    expect(findGroupLogic(root, 'missing')).toBeNull();
  });

  it('finds the first condition of a group', () => {
    expect(firstConditionOf(root, 'sub')?.id).toBe('a');
    expect(firstConditionOf(root, 'root')?.id).toBe('r');
  });

  it('returns null for an empty group', () => {
    expect(firstConditionOf({ id: 'x', logic: 'AND', conditions: [], groups: [] }, 'x')).toBeNull();
  });
});
