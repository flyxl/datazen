/**
 * Contracts for foreign-key prediction.
 *
 * Real schemas frequently have no declared foreign keys — ORM-managed, legacy and
 * sharded databases all rely on naming conventions instead of constraints. This
 * engine infers those relationships from metadata alone.
 *
 * The governing rule is that **a wrong match is worse than no match**: a wrong
 * JOIN silently changes the result set, while a missing one costs the user a
 * manual click. Everything here is therefore explainable and tiered, and the
 * caller decides what to do with each tier.
 */

/** One column of a table under consideration. */
export interface PredictionColumn {
  name: string;
  /** Driver-reported type string, e.g. `integer`, `varchar`. */
  dataType: string;
  nullable?: boolean;
  /** True when the column is indexed (any index), a weak signal of FK usage. */
  indexed?: boolean;
}

/** A declared foreign key, so prediction never restates ground truth. */
export interface DeclaredForeignKey {
  columns: readonly string[];
  referencedTable: string;
  referencedColumns: readonly string[];
}

/** A table to consider as a source and/or target of a relationship. */
export interface PredictionTable {
  /** Stable identity used in evidence and output. */
  id: string;
  /** Bare table name, used for name matching. */
  name: string;
  /** Schema the table lives in; prediction does not cross schemas. */
  schema?: string;
  columns: readonly PredictionColumn[];
  primaryKey: readonly string[];
  /**
   * Every column set covered by a unique index (single-column or composite).
   * A relationship can only target a key, never an arbitrary column.
   */
  uniqueColumnSets: readonly (readonly string[])[];
  declaredForeignKeys: readonly DeclaredForeignKey[];
}

/** Why a candidate was proposed — surfaced so the user can judge it. */
export type PredictionEvidenceCode =
  | 'target-is-primary-key'
  | 'target-is-unique'
  | 'name-matches-table'
  | 'name-matches-table-stem'
  | 'name-suffix-matches-table'
  | 'name-matches-key'
  | 'type-exact'
  | 'type-compatible'
  | 'source-indexed'
  | 'ambiguous';

export interface PredictionEvidence {
  code: PredictionEvidenceCode;
  weight: number;
  /** Human-readable reason, already localized by the caller if needed. */
  detail: string;
}

/** How much the caller may trust a candidate. */
export type PredictionTier = 'high' | 'medium';

export interface RelationCandidate {
  /** Deterministic id, so a user's removal survives re-prediction. */
  id: string;
  fromTable: string;
  toTable: string;
  columnPairs: readonly { left: string; right: string }[];
  score: number;
  tier: PredictionTier;
  evidence: readonly PredictionEvidence[];
  /**
   * Set when more than one target scored comparably. The caller must not apply
   * such a candidate automatically — the engine refuses to guess.
   */
  ambiguous: boolean;
}

/** Options for {@link predictRelations}. */
export interface PredictionOptions {
  /** Include candidates whose target sits in another schema. Default false. */
  allowCrossSchema?: boolean;
  /** Score at or above which a candidate is `high`. Default 0.8. */
  highThreshold?: number;
  /** Score at or below which a candidate is dropped entirely. Default 0.5. */
  mediumThreshold?: number;
  /**
   * How close a runner-up may be before a match counts as ambiguous.
   * Default 0.15.
   */
  ambiguityMargin?: number;
}
