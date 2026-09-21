/**
 * Which tables are related to the ones already in play.
 *
 * The editor's completion list is built from the schema tree, so it already
 * contains every candidate table. What it cannot know is which of them the user
 * is *likely* to want next — and that is exactly what a relationship tells us.
 * Boosting beats filtering here: a related table rises to the top, while an
 * unrelated one stays reachable, so the ranking never removes a valid choice.
 *
 * Only two evidence sources are used, because both are cheap:
 *
 * - **Declared foreign keys.** A loaded relation names its targets outright, so
 *   the target needs no extra fetch to be boosted.
 * - **Predicted relationships among the loaded relations.** These are the tables
 *   the statement already mentions, so no metadata beyond what is loaded is
 *   required.
 *
 * Prediction over the *whole* schema is deliberately not attempted: it would need
 * every table's primary key and column types, turning a keystroke into a
 * database-wide metadata fetch.
 */

import { predictRelations } from './predictRelations';
import type { PredictionTable } from './types';

/** A table worth surfacing, and why. */
export interface RelatedTableHint {
  /** Table name as it should be matched against a completion label. */
  name: string;
  /** Declared constraints outrank inferences. */
  origin: 'declared' | 'predicted';
  /** Added to the completion's base boost; higher sorts first. */
  boost: number;
  /** The loaded table that leads here, for the completion's detail text. */
  via: string;
}

/** A declared foreign key target is near-certain; an inference is a nudge. */
export const DECLARED_RELATION_BOOST = 20;
export const PREDICTED_RELATION_BOOST = 12;

/**
 * Hints for tables related to `loaded`, most confident first.
 *
 * `loaded` must be the relations the editor already has metadata for — normally
 * the tables referenced in the statement.
 */
export function relatedTableHints(loaded: readonly PredictionTable[]): RelatedTableHint[] {
  if (loaded.length === 0) return [];

  const hints: RelatedTableHint[] = [];
  const seen = new Map<string, RelatedTableHint>();

  const push = (hint: RelatedTableHint) => {
    const existing = seen.get(hint.name);
    // Keep the strongest reason for each table; a declared constraint already
    // covers anything a prediction could add.
    if (existing && existing.boost >= hint.boost) return;
    seen.set(hint.name, hint);
  };

  for (const table of loaded) {
    for (const fk of table.declaredForeignKeys) {
      if (fk.referencedTable === table.name) continue;
      push({
        name: fk.referencedTable,
        origin: 'declared',
        boost: DECLARED_RELATION_BOOST,
        via: table.name,
      });
    }
  }

  for (const candidate of predictRelations(loaded)) {
    const from = loaded.find((t) => t.id === candidate.fromTable);
    const to = loaded.find((t) => t.id === candidate.toTable);
    if (!from || !to) continue;
    // An ambiguous candidate is a poor hint; offering it would push a coin flip
    // to the top of the list.
    if (candidate.ambiguous) continue;
    push({
      name: to.name,
      origin: 'predicted',
      boost: PREDICTED_RELATION_BOOST,
      via: from.name,
    });
  }

  hints.push(...seen.values());
  return hints.sort(
    (a, b) => b.boost - a.boost || (a.name < b.name ? -1 : a.name > b.name ? 1 : 0),
  );
}

/**
 * Collapse hints into a boost lookup keyed by folded table name.
 *
 * Folded so `Orders` and `orders` resolve to one entry: the completion label is
 * quoted per dialect, and the caller folds both sides the same way.
 */
export function relatedTableBoostMap(
  hints: readonly RelatedTableHint[],
  fold: (name: string) => string,
): Map<string, RelatedTableHint> {
  const map = new Map<string, RelatedTableHint>();
  for (const hint of hints) {
    const key = fold(hint.name);
    const existing = map.get(key);
    if (!existing || existing.boost < hint.boost) map.set(key, hint);
  }
  return map;
}
