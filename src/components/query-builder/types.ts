/**
 * Shared type definitions for the Visual Query Builder.
 *
 * All QB components, store, and SQL generator import from this file.
 */

/** Query builder comparison operators. */
export type QbOperator =
  | '='
  | '!='
  | '>'
  | '<'
  | '>='
  | '<='
  | 'LIKE'
  | 'NOT LIKE'
  | 'IN'
  | 'NOT IN'
  | 'IS NULL'
  | 'IS NOT NULL';

/** SQL aggregate functions. */
export type QbAggregate = 'COUNT' | 'SUM' | 'AVG' | 'MIN' | 'MAX';

/** JOIN type. */
export type QbJoinType = 'INNER' | 'LEFT' | 'RIGHT' | 'FULL';

/** A single condition row in a WHERE clause. */
export interface QbCondition {
  id: string;
  table: string;
  column: string;
  operator: QbOperator;
  /** Right-hand value. For IN/NOT IN use comma-separated list. For IS NULL/IS NOT NULL this is ignored. */
  value: string | null;
  /** Conjunction linking this condition to the previous one (ignored for the first condition). */
  conjunction: 'AND' | 'OR';
  /**
   * Wrap the left-hand column in an aggregate (`SUM(qty) >= 2000`).
   *
   * Only HAVING exposes this in the UI — `WHERE SUM(x) > 1` is invalid SQL —
   * but the generator honours it wherever it appears so the two clause editors
   * can share one condition editor.
   */
  aggregate?: QbAggregate;
}

/** A group of conditions joined by a common logic operator. Supports nesting. */
export interface QbConditionGroup {
  id: string;
  /** Logic operator joining conditions inside this group. */
  logic: 'AND' | 'OR';
  /** Direct conditions in this group. */
  conditions: QbCondition[];
  /** Nested sub-groups (v1 supports one level of nesting). */
  groups: QbConditionGroup[];
}

/** A single sort clause. */
export interface QbSortItem {
  table: string;
  column: string;
  direction: 'ASC' | 'DESC';
  /**
   * Aggregate to wrap the sort key in, when the sorted column is aggregated.
   * Required so `SUM(x) … GROUP BY y ORDER BY SUM(x)` stays valid SQL.
   */
  aggregate?: QbAggregate;
}

/** A column selection entry with optional alias, aggregate, sort, group-by, and where. */
export interface QbColumnSelection {
  table: string;
  column: string;
  alias?: string;
  aggregate?: QbAggregate;
  /** Sort direction (ASC/DESC). Undefined means no sort. */
  sort?: 'ASC' | 'DESC';
  /** Whether this column participates in GROUP BY. */
  groupBy?: boolean;
  /** Optional per-column WHERE condition. */
  where?: QbCondition;
}

/** A JOIN relationship between two tables. */
export interface QbJoin {
  /** Unique identifier (nanoid). */
  id: string;
  /** JOIN type. */
  type: QbJoinType;
  /** Left (source) table name. */
  leftTable: string;
  /** Left (source) column name. */
  leftColumn: string;
  /** Right (target) table name. */
  rightTable: string;
  /** Right (target) column name. */
  rightColumn: string;
  /** true = manually created, false = auto-detected FK. */
  isManual: boolean;
  /**
   * Identity of the constraint this pair belongs to (see `constraintKey`).
   * Composite foreign keys contribute several pairs that must be confirmed,
   * rendered and removed as one unit, so the linkage has to survive in the join.
   */
  constraint?: string;
}

/** A group-by entry. */
export interface QbGroupByItem {
  table: string;
  column: string;
}
