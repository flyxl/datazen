import type { QbAggregate, QbCondition, QbConditionGroup } from '../types';
import { qualifiedRef } from './columnOptions';

/** AND/OR of the group with `groupId`, or null when it is not in this tree. */
export function findGroupLogic(group: QbConditionGroup, groupId: string): 'AND' | 'OR' | null {
  if (group.id === groupId) return group.logic;
  for (const sub of group.groups) {
    const found = findGroupLogic(sub, groupId);
    if (found) return found;
  }
  return null;
}

/** First condition of the group with `groupId` (null when it has none). */
export function firstConditionOf(group: QbConditionGroup, groupId: string): QbCondition | null {
  if (group.id === groupId) return group.conditions[0] ?? null;
  for (const sub of group.groups) {
    const found = firstConditionOf(sub, groupId);
    if (found) return found;
  }
  return null;
}

/** Operators whose right-hand side is a list, not a single value. */
const LIST_OPERATORS = new Set<string>(['IN', 'NOT IN']);
/** Operators with no right-hand side at all. */
const NULL_OPERATORS = new Set<string>(['IS NULL', 'IS NOT NULL']);

/** Longest value rendered inside a chip before it is elided. */
const MAX_VALUE_CHARS = 24;

function elide(value: string): string {
  const trimmed = value.trim();
  if (trimmed.length <= MAX_VALUE_CHARS) return trimmed;
  return `${trimmed.slice(0, MAX_VALUE_CHARS - 1)}…`;
}

/** `SUM(` / `)` around a condition's column reference, when aggregated. */
export function aggregateWrapper(aggregate: QbAggregate | undefined): {
  prefix: string;
  suffix: string;
} {
  return aggregate ? { prefix: `${aggregate}(`, suffix: ')' } : { prefix: '', suffix: '' };
}

/**
 * The right-hand side of a condition, as it appears in the generated SQL:
 * values are shown verbatim (the generator quotes them; the chip mirrors what
 * the user typed rather than the SQL literal).
 */
export function conditionValueText(condition: QbCondition): string {
  if (NULL_OPERATORS.has(condition.operator)) return '';
  if (LIST_OPERATORS.has(condition.operator)) {
    return `(${elide(condition.value ?? '')})`;
  }
  return elide(condition.value ?? '');
}

/**
 * Chip text for one condition: `ec.city IN (A, B)`, `SUM(s.qty) >= 2000`.
 *
 * Split into parts so the column reference can carry the aggregate wrapper the
 * way the SELECT chips do, and so a long value can be truncated independently.
 */
export function conditionChipParts(
  condition: QbCondition,
  aliases: Record<string, string>,
): { label: string; prefix: string; suffix: string } {
  const wrapper = aggregateWrapper(condition.aggregate);
  const ref = qualifiedRef(condition.table, condition.column, aliases);
  const value = conditionValueText(condition);
  return {
    label: ref,
    prefix: wrapper.prefix,
    suffix: `${wrapper.suffix} ${condition.operator}${value ? ` ${value}` : ''}`,
  };
}

/**
 * Conjunction shown on a condition chip. The first row of a group has nothing
 * above it, so it carries no badge.
 */
export function conditionChipPrefix(condition: QbCondition, isFirst: boolean): string {
  return isFirst ? '' : condition.conjunction;
}
