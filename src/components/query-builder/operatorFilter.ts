/**
 * Filter operator options based on column data type.
 *
 * Not every operator makes sense for every type:
 *   - LIKE / NOT LIKE → text/binary only (pattern matching on numbers or dates
 *     is almost always a mistake and silently returns no rows)
 *   - IN / NOT IN → all types
 *   - Comparison (=, !=, >, <, >=, <=) → all types
 *   - IS [NOT] NULL → all types
 *
 * When the column type is unknown, all operators are returned (backward compat).
 */
import type { SelectOption } from '../ui/Select';
import { classifyColumnType, TypeCategory } from './typeCategory';
import { QB_OPERATOR_OPTIONS } from './BuildStatement/operatorOptions';

/** Operators that only make sense for text-like columns. */
const TEXT_ONLY_OPERATORS = new Set(['LIKE', 'NOT LIKE']);

/**
 * Return the operator options appropriate for the given raw `dataType`.
 *
 * Returns all operators when `dataType` is `undefined` or empty, preserving
 * backward compatibility with callers that have no type information.
 */
export function getOperatorOptions(dataType?: string): SelectOption[] {
  if (!dataType) return QB_OPERATOR_OPTIONS;

  const category = classifyColumnType(dataType);

  // Text and binary columns support LIKE
  if (category === TypeCategory.Text || category === TypeCategory.Binary) {
    return QB_OPERATOR_OPTIONS;
  }

  // All other types: exclude LIKE / NOT LIKE
  return QB_OPERATOR_OPTIONS.filter((opt) => !TEXT_ONLY_OPERATORS.has(opt.value));
}
