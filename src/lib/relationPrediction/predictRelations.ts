/**
 * Foreign-key prediction from metadata alone.
 *
 * Answers "which tables are related?" for schemas that never declared a
 * constraint. It is pure: metadata in, ranked candidates out, no IPC and no data
 * access — so it is testable without a database and safe to run on a whole
 * database, which is what the ER diagram asks of it.
 *
 * Design constraints, each one a defence against a wrong JOIN:
 *
 * - **A relationship can only target a key.** A source column is only considered
 *   against a target's primary key or a unique index. Pointing at an arbitrary
 *   column is not a foreign key, and allowing it is how `status_id` ends up
 *   related to `order_status` merely because the names line up.
 * - **Type family is a gate, not a score.** A `varchar` column is never related to
 *   an `integer` key, however well the names match. Some dialects would compare
 *   them happily through an implicit cast, so this cannot be left to the data.
 * - **Ambiguity abstains.** When two targets score comparably the candidate is
 *   marked and the caller must not apply it automatically.
 * - **Declared foreign keys win.** Columns already covered by a real constraint
 *   are skipped rather than predicted.
 *
 * The data-overlap probe is deliberately absent: it needs to query the database,
 * and it is the part that can hurt a production server. It belongs behind an
 * explicit user action, layered on top of these structural candidates.
 *
 * ## Why names are indexed rather than scanned
 *
 * Every naming rule here is "does this column name *spell out* some target key?".
 * Asking that question by comparing each source column against every target key is
 * quadratic, and measured at ~15s for a 500-table schema — unusable for an ER
 * diagram that runs it over a whole database. Instead the question is inverted:
 * each target key registers the column names that would refer to it, and a source
 * column is a single hash lookup. The rules are unchanged; only the direction of
 * the search is.
 */

import {
  isGenericKeyName,
  normalizeDataType,
  normalizeIdentifier,
  singularize,
  typeFamily,
  type TypeFamily,
} from './normalize';
import type {
  PredictionEvidence,
  PredictionEvidenceCode,
  PredictionOptions,
  PredictionTable,
  RelationCandidate,
} from './types';

/** Evidence weights. Tunable in one place so scoring stays inspectable. */
export const EVIDENCE_WEIGHTS = {
  targetIsPrimaryKey: 0.4,
  targetIsUnique: 0.3,
  nameMatchesTable: 0.4,
  nameMatchesTableStem: 0.4,
  /** Table names are commonly prefixed (`app_user`), the column is not. */
  nameMatchesPrefixedTable: 0.25,
  nameSuffixMatchesTable: 0.15,
  nameMatchesKey: 0.3,
  typeExact: 0.1,
  sourceIndexed: 0.1,
} as const;

export const DEFAULT_HIGH_THRESHOLD = 0.8;
export const DEFAULT_MEDIUM_THRESHOLD = 0.5;
export const DEFAULT_AMBIGUITY_MARGIN = 0.15;

/** A key a relationship may point at: the primary key or a unique index. */
interface TargetKey {
  columns: readonly string[];
  isPrimaryKey: boolean;
}

/** One column of one target key, with everything needed to score against it. */
interface TargetColumnRef {
  tableId: string;
  tableName: string;
  keyIndex: number;
  columnName: string;
  typeFamily: TypeFamily;
  normalizedDataType: string;
}

/** A registered way of naming a target column, and what that naming is worth. */
interface IndexEntry {
  ref: TargetColumnRef;
  weight: number;
  code: PredictionEvidenceCode;
}

/** One scored column-pair match, before grouping. */
interface PairMatch {
  source: PredictionTable;
  sourceColumn: string;
  target: PredictionTable;
  targetKey: TargetKey;
  targetColumn: string;
  score: number;
  evidence: PredictionEvidence[];
}

function evidence(
  code: PredictionEvidence['code'],
  weight: number,
  detail: string,
): PredictionEvidence {
  return { code, weight, detail };
}

/** Every key a relationship may target, per table id. */
function collectTargetKeys(tables: readonly PredictionTable[]): Map<string, TargetKey[]> {
  const keys = new Map<string, TargetKey[]>();
  for (const table of tables) {
    const tableKeys: TargetKey[] = [];
    if (table.primaryKey.length > 0) {
      tableKeys.push({ columns: table.primaryKey, isPrimaryKey: true });
    }
    for (const set of table.uniqueColumnSets) {
      if (set.length === 0) continue;
      // A unique index over the primary key adds no new target.
      const isSameAsPk =
        set.length === table.primaryKey.length &&
        set.every((column) => table.primaryKey.includes(column));
      if (isSameAsPk) continue;
      tableKeys.push({ columns: set, isPrimaryKey: false });
    }
    keys.set(table.id, tableKeys);
  }
  return keys;
}

/** Word-boundary suffixes of a table's stem, longest last: `a_b_c` → `b_c`, `c`. */
function stemSuffixes(stem: string): string[] {
  const words = stem.split('_');
  const suffixes: string[] = [];
  for (let i = 1; i < words.length; i++) suffixes.push(words.slice(i).join('_'));
  return suffixes;
}

/**
 * Register every column name that would refer to `ref`.
 *
 * The rules, in descending strength:
 *
 * - `{table}_{key}` — the convention stated in full.
 * - `{stem}_{key}` — the same, singularized (`users` → `user_id`).
 * - `{key}` alone — a domain-named key referenced by its own name
 *   (`users.user_id` ← `user_id`). Generic names are excluded: every table has an
 *   `id`, so matching on it would relate everything to everything.
 * - `{stemSuffix}_{key}` — a prefixed table (`app_user`) referenced by the
 *   unprefixed word (`user_id`). Weaker, because two prefixed tables can share
 *   that word — which is what the ambiguity check exists for.
 */
function registerTargetColumn(index: Map<string, IndexEntry[]>, ref: TargetColumnRef): void {
  const add = (name: string, weight: number, code: PredictionEvidenceCode) => {
    if (!name) return;
    const entry: IndexEntry = { ref, weight, code };
    const bucket = index.get(name);
    if (bucket) bucket.push(entry);
    else index.set(name, [entry]);
  };

  const table = normalizeIdentifier(ref.tableName);
  const stem = normalizeIdentifier(singularize(ref.tableName));
  const key = normalizeIdentifier(ref.columnName);

  add(`${table}_${key}`, EVIDENCE_WEIGHTS.nameMatchesTable, 'name-matches-table');
  if (stem !== table) {
    add(`${stem}_${key}`, EVIDENCE_WEIGHTS.nameMatchesTableStem, 'name-matches-table-stem');
  }
  if (!isGenericKeyName(ref.columnName)) {
    add(key, EVIDENCE_WEIGHTS.nameMatchesKey, 'name-matches-key');
  }
  for (const suffix of stemSuffixes(stem)) {
    add(`${suffix}_${key}`, EVIDENCE_WEIGHTS.nameMatchesPrefixedTable, 'name-matches-table-stem');
  }
}

/** Columns already covered by a declared foreign key on this table. */
function declaredForeignKeyColumns(table: PredictionTable): Set<string> {
  const covered = new Set<string>();
  for (const fk of table.declaredForeignKeys) {
    for (const column of fk.columns) covered.add(normalizeIdentifier(column));
  }
  return covered;
}

/** Deterministic candidate id, stable across re-prediction. */
function candidateId(
  fromTable: string,
  toTable: string,
  pairs: readonly { left: string; right: string }[],
): string {
  const rendered = pairs.map((pair) => `${pair.left}=${pair.right}`).join(',');
  return `predicted-${fromTable}-${toTable}-${rendered}`;
}

/**
 * Infer relationships among `tables`.
 *
 * Results are sorted by descending score, then by id, so the output is stable.
 */
export function predictRelations(
  tables: readonly PredictionTable[],
  options: PredictionOptions = {},
): RelationCandidate[] {
  if (tables.length < 2) return [];

  const highThreshold = options.highThreshold ?? DEFAULT_HIGH_THRESHOLD;
  const mediumThreshold = options.mediumThreshold ?? DEFAULT_MEDIUM_THRESHOLD;
  const ambiguityMargin = options.ambiguityMargin ?? DEFAULT_AMBIGUITY_MARGIN;
  const allowCrossSchema = options.allowCrossSchema ?? false;

  const tablesById = new Map(tables.map((table) => [table.id, table]));
  const keysByTable = collectTargetKeys(tables);
  const schemaOf = new Map(tables.map((table) => [table.id, table.schema]));

  // ── Build the reverse index: "column name" → "what it would refer to" ──
  const index = new Map<string, IndexEntry[]>();
  for (const table of tables) {
    const tableKeys = keysByTable.get(table.id) ?? [];
    tableKeys.forEach((targetKey, keyIndex) => {
      for (const keyColumn of targetKey.columns) {
        const normalizedKeyColumn = normalizeIdentifier(keyColumn);
        const column = table.columns.find(
          (candidate) => normalizeIdentifier(candidate.name) === normalizedKeyColumn,
        );
        // A key naming a column the table does not report cannot be targeted.
        if (!column) continue;
        registerTargetColumn(index, {
          tableId: table.id,
          tableName: table.name,
          keyIndex,
          columnName: keyColumn,
          typeFamily: typeFamily(column.dataType),
          normalizedDataType: normalizeDataType(column.dataType),
        });
      }
    });
  }

  // ── Match each source column against the index ──
  const matches: PairMatch[] = [];

  for (const source of tables) {
    const covered = declaredForeignKeyColumns(source);

    for (const column of source.columns) {
      // Ground truth first: a declared constraint is not something to predict.
      if (covered.has(normalizeIdentifier(column.name))) continue;

      const sourceName = normalizeIdentifier(column.name);
      if (!sourceName) continue;

      // An exact hit uses the weight the naming rule earned. Failing that, a
      // prefix the column carries but the table does not (`t_user_id`) still
      // names a target, at the weakest weight.
      let entries = index.get(sourceName);
      let weightOverride: number | null = null;
      if (!entries || entries.length === 0) {
        for (const suffix of stemSuffixes(sourceName)) {
          const found = index.get(suffix);
          if (found && found.length > 0) {
            entries = found;
            weightOverride = EVIDENCE_WEIGHTS.nameSuffixMatchesTable;
            break;
          }
        }
      }
      if (!entries || entries.length === 0) continue;

      const sourceFamily = typeFamily(column.dataType);
      const sourceNormalizedType = normalizeDataType(column.dataType);
      // The same target key can be reachable by more than one registered name;
      // only the strongest naming for each key is worth keeping.
      const bestPerKey = new Map<string, { entry: IndexEntry; weight: number }>();

      for (const entry of entries) {
        const { ref } = entry;
        if (ref.tableId === source.id) continue;
        if (!allowCrossSchema && schemaOf.get(ref.tableId) !== source.schema) continue;
        // ── Type gate: families must agree, or the relationship is impossible ──
        if (ref.typeFamily !== sourceFamily) continue;

        const weight =
          weightOverride === null ? entry.weight : Math.min(entry.weight, weightOverride);
        const dedupeKey = `${ref.tableId}\u0000${ref.keyIndex}\u0000${normalizeIdentifier(ref.columnName)}`;
        const existing = bestPerKey.get(dedupeKey);
        if (!existing || weight > existing.weight) bestPerKey.set(dedupeKey, { entry, weight });
      }

      for (const { entry, weight } of bestPerKey.values()) {
        const { ref } = entry;
        const target = tablesById.get(ref.tableId);
        const targetKey = keysByTable.get(ref.tableId)?.[ref.keyIndex];
        if (!target || !targetKey) continue;

        const found: PredictionEvidence[] = [
          evidence(
            entry.code,
            weight,
            entry.code === 'name-matches-key'
              ? `${column.name} shares the key name ${ref.columnName}`
              : entry.code === 'name-matches-table'
                ? `${column.name} names ${ref.tableName}.${ref.columnName}`
                : `${column.name} names the distinguishing word of ${ref.tableName}.${ref.columnName}`,
          ),
        ];

        found.push(
          evidence(
            targetKey.isPrimaryKey ? 'target-is-primary-key' : 'target-is-unique',
            targetKey.isPrimaryKey
              ? EVIDENCE_WEIGHTS.targetIsPrimaryKey
              : EVIDENCE_WEIGHTS.targetIsUnique,
            `${ref.tableName}.${ref.columnName} is ${
              targetKey.isPrimaryKey ? 'the primary key' : 'unique'
            }`,
          ),
        );

        if (sourceNormalizedType === ref.normalizedDataType) {
          found.push(evidence('type-exact', EVIDENCE_WEIGHTS.typeExact, `type ${column.dataType}`));
        } else {
          found.push(
            evidence('type-compatible', 0, `${column.dataType} is compatible with the target key`),
          );
        }

        if (column.indexed) {
          found.push(
            evidence('source-indexed', EVIDENCE_WEIGHTS.sourceIndexed, 'column is indexed'),
          );
        }

        // Recompute the total from the evidence itself so the score and the
        // explanation can never disagree.
        const score = found.reduce((total, item) => total + item.weight, 0);
        matches.push({
          source,
          sourceColumn: column.name,
          target,
          targetKey,
          targetColumn: ref.columnName,
          score,
          evidence: found,
        });
      }
    }
  }

  // ── Ambiguity: one source column matching several targets ──
  const bySourceColumn = new Map<string, PairMatch[]>();
  for (const match of matches) {
    const key = `${match.source.id}\u0000${normalizeIdentifier(match.sourceColumn)}`;
    const bucket = bySourceColumn.get(key);
    if (bucket) bucket.push(match);
    else bySourceColumn.set(key, [match]);
  }

  const ambiguousColumns = new Set<string>();
  for (const [key, bucket] of bySourceColumn) {
    if (bucket.length < 2) continue;
    const sorted = [...bucket].sort((a, b) => b.score - a.score);
    // Distinct targets only: two keys of the same table are not a conflict.
    const topTargets = new Set([sorted[0]!.target.id, sorted[1]!.target.id]);
    if (topTargets.size < 2) continue;
    if (sorted[0]!.score - sorted[1]!.score <= ambiguityMargin) ambiguousColumns.add(key);
  }

  // ── Composite grouping ──
  // Two source columns that both reference the *same composite* key form one
  // relationship. Two columns referencing the same single-column key do not:
  // `orders.billing_address_id` and `orders.shipping_address_id` are two
  // relationships to one table, and merging them would invent an AND that
  // matches nothing.
  const grouped = new Map<string, PairMatch[]>();
  const order: string[] = [];
  for (const match of matches) {
    const composite = match.targetKey.columns.length > 1;
    const groupId = composite
      ? `composite\u0000${match.source.id}\u0000${match.target.id}\u0000${match.targetKey.columns.join(',')}`
      : `pair\u0000${match.source.id}\u0000${match.target.id}\u0000${normalizeIdentifier(match.sourceColumn)}`;
    const bucket = grouped.get(groupId);
    if (bucket) bucket.push(match);
    else {
      grouped.set(groupId, [match]);
      order.push(groupId);
    }
  }

  const candidates: RelationCandidate[] = [];
  for (const groupId of order) {
    const group = grouped.get(groupId)!;
    // A composite relationship is only complete when every column of the key is
    // covered; a partial match would generate an ON clause missing a predicate.
    if (groupId.startsWith('composite\u0000')) {
      const matched = new Set(group.map((m) => normalizeIdentifier(m.targetColumn)));
      const complete = group[0]!.targetKey.columns.every((column) =>
        matched.has(normalizeIdentifier(column)),
      );
      if (!complete) continue;
    }

    const best = group.reduce((a, b) => (b.score > a.score ? b : a));
    const pairs = group
      .map((m) => ({ left: m.sourceColumn, right: m.targetColumn }))
      .sort((a, b) => (a.left < b.left ? -1 : a.left > b.left ? 1 : 0));

    if (best.score < mediumThreshold) continue;

    const ambiguous = group.some((m) =>
      ambiguousColumns.has(`${m.source.id}\u0000${normalizeIdentifier(m.sourceColumn)}`),
    );

    const evidenceList = best.evidence.slice();
    if (ambiguous) {
      evidenceList.push(
        evidence('ambiguous', 0, 'more than one table matches this column equally well'),
      );
    }

    candidates.push({
      id: candidateId(best.source.id, best.target.id, pairs),
      fromTable: best.source.id,
      toTable: best.target.id,
      columnPairs: pairs,
      score: Math.round(best.score * 1000) / 1000,
      // An ambiguous candidate is never high, whatever it scored.
      tier: !ambiguous && best.score >= highThreshold ? 'high' : 'medium',
      evidence: evidenceList,
      ambiguous,
    });
  }

  return candidates.sort((a, b) => b.score - a.score || (a.id < b.id ? -1 : a.id > b.id ? 1 : 0));
}
