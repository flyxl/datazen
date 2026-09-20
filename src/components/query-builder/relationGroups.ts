import { constraintKey, type ForeignKeyRelation } from './hooks/useAutoJoin';
import type { RelationGroup, RelationPair } from './DiagramCanvas/fkGeometry';
import type { QbJoin, QbJoinType } from './types';

/**
 * Fold the two data sources of the diagram — confirmed `joins` and detected
 * foreign keys — into the single list of **relation groups** the canvas draws.
 *
 * One group per constraint (or per manual join), because that is the smallest
 * unit that can be confirmed, re-typed or removed without producing an invalid
 * statement: half of a composite FK in the SQL is a wrong query.
 */
export interface BuildRelationGroupsInput {
  /** Confirmed joins (both FK-derived and manual). */
  joins: QbJoin[];
  /** Detected FK column pairs, including ones already confirmed. */
  fkRelations: ForeignKeyRelation[];
  /** Tables currently on the canvas. */
  selectedTables: string[];
  /**
   * Group id → JOIN type picked on an unconfirmed candidate (from `autoJoins`).
   * Without it the popover radio never moves when a type is chosen before
   * confirming, which reads as a dead control.
   */
  candidateTypes?: Record<string, QbJoinType>;
}

const pairId = (left: string, leftColumn: string, right: string, rightColumn: string) =>
  `${left}.${leftColumn}->${right}.${rightColumn}`;

export function buildRelationGroups({
  joins,
  fkRelations,
  selectedTables,
  candidateTypes = {},
}: BuildRelationGroupsInput): RelationGroup[] {
  const onCanvas = new Set(selectedTables);
  const groups: RelationGroup[] = [];

  // ── FK-derived groups, grouped by constraint ──
  const byConstraint = new Map<string, ForeignKeyRelation[]>();
  for (const relation of fkRelations) {
    if (!onCanvas.has(relation.fromTable) || !onCanvas.has(relation.toTable)) continue;
    if (relation.fromTable === relation.toTable) continue; // can't be expressed
    const key = constraintKey(relation.fromTable, relation.constraint);
    const list = byConstraint.get(key) ?? [];
    list.push(relation);
    byConstraint.set(key, list);
  }

  const joinsByPair = new Map<string, QbJoin>();
  for (const join of joins) {
    joinsByPair.set(
      pairId(join.leftTable, join.leftColumn, join.rightTable, join.rightColumn),
      join,
    );
  }

  for (const [key, relations] of byConstraint) {
    const pairs: RelationPair[] = relations.map((relation) => ({
      fromTable: relation.fromTable,
      fromColumn: relation.fromColumn,
      toTable: relation.toTable,
      toColumn: relation.toColumn,
      confirmed: joinsByPair.has(
        pairId(relation.fromTable, relation.fromColumn, relation.toTable, relation.toColumn),
      ),
    }));

    // Type comes from the confirmed joins; a candidate group previews INNER.
    const confirmedJoin = relations
      .map((relation) =>
        joinsByPair.get(
          pairId(relation.fromTable, relation.fromColumn, relation.toTable, relation.toColumn),
        ),
      )
      .find(Boolean);

    groups.push({
      id: key,
      kind: 'fk',
      // Confirmed joins own the type; a candidate previews the user's pick,
      // defaulting to INNER.
      type: confirmedJoin?.type ?? candidateTypes[key] ?? 'INNER',
      constraint: relations[0]!.constraint,
      pairs,
    });
  }

  // ── Safety net: confirmed joins not covered by a detected constraint ──
  // (schema cache miss, or the FK disappeared upstream). Without this a join
  // that is in the SQL would have no line on the canvas at all.
  const coveredPairs = new Set<string>();
  for (const relations of byConstraint.values()) {
    for (const relation of relations) {
      coveredPairs.add(
        pairId(relation.fromTable, relation.fromColumn, relation.toTable, relation.toColumn),
      );
    }
  }
  for (const join of joins) {
    if (join.isManual) continue;
    const id = pairId(join.leftTable, join.leftColumn, join.rightTable, join.rightColumn);
    if (coveredPairs.has(id)) continue;
    groups.push({
      id: `join:${join.id}`,
      kind: 'fk',
      type: join.type,
      pairs: [
        {
          fromTable: join.leftTable,
          fromColumn: join.leftColumn,
          toTable: join.rightTable,
          toColumn: join.rightColumn,
          confirmed: true,
        },
      ],
    });
  }

  // ── Manual joins: one group each, always confirmed ──
  for (const join of joins.filter((candidate) => candidate.isManual)) {
    groups.push({
      id: `manual:${join.id}`,
      kind: 'manual',
      type: join.type,
      pairs: [
        {
          fromTable: join.leftTable,
          fromColumn: join.leftColumn,
          toTable: join.rightTable,
          toColumn: join.rightColumn,
          confirmed: true,
        },
      ],
    });
  }

  return groups;
}
