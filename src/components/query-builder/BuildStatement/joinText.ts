import type { QbJoinStep } from '../../../lib/sqlDialects/queryBuilder';
import { qualifiedRef } from './columnOptions';

/**
 * The `ON` predicates of one join step as SQL text — the single formatter used
 * by both the FROM chip and its options dialog, so the dialog can never show a
 * different predicate than the one the SQL will emit.
 */
export function joinStepOnText(step: QbJoinStep, aliases: Record<string, string>): string {
  return step.predicates
    .map(
      (p) =>
        `${qualifiedRef(p.sourceTable, p.sourceColumn, aliases)} = ` +
        `${qualifiedRef(step.targetTable, p.targetColumn, aliases)}`,
    )
    .join(' AND ');
}

/** `SUM(s.qty)` when the entry is aggregated, else `s.qty`. */
export function orderEntryLabel(
  table: string,
  column: string,
  aggregate: string | undefined,
  aliases: Record<string, string>,
): string {
  const ref = qualifiedRef(table, column, aliases);
  return aggregate ? `${aggregate}(${ref})` : ref;
}
