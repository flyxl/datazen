import { useEffect, useMemo, useState } from 'react';
import { getCachedTableSchema } from '../lib/schemaCache';
import { indexForeignKeys, type FkGroupAffordance } from '../lib/fkNavigation';

/**
 * The foreign keys of one table, as the grid needs them (DB-07).
 *
 * The table data does **not** carry the foreign keys: `get_table_data` answers
 * with rows and columns, and the constraints live in the relation schema. The
 * schema cache is the one entry point for it — the same one the "foreign keys"
 * structure tab reads — so the grid and the structure tab can never disagree
 * about a constraint.
 *
 * A failure is a **degradation, not an error**: the metadata may be unreadable
 * (no permission, a table that vanished, a driver that cannot describe it).
 * Then the grid simply has no arrows. The rest of the grid must keep working,
 * so the error is returned as a value and never thrown.
 */
export interface UseTableForeignKeysParams {
  /** The runtime session the table belongs to. */
  dbSessionId: string;
  /** The table the constraints belong to. */
  tableName: string;
  /** The database of the table. */
  database: string;
  /** The namespace of the table; `null` when the driver has none. */
  schema: string | null;
  /** The connection of the table; part of the metadata cache key. */
  connectionId?: string | null;
}

export interface UseTableForeignKeysResult {
  /** The columns that take part in a foreign key, mapped to their group. */
  index: ReadonlyMap<string, FkGroupAffordance>;
  /** The metadata is being read. */
  loading: boolean;
  /** Why the metadata is missing; `null` when it was read. */
  error: string | null;
}

/** The empty index, shared so the grid does not re-render on every failure. */
const EMPTY_INDEX: ReadonlyMap<string, FkGroupAffordance> = new Map();

export function useTableForeignKeys(
  params: UseTableForeignKeysParams,
): UseTableForeignKeysResult {
  const { dbSessionId, tableName, database, schema, connectionId } = params;
  const [index, setIndex] = useState<ReadonlyMap<string, FkGroupAffordance>>(EMPTY_INDEX);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setError(null);
    getCachedTableSchema(dbSessionId, tableName, database, schema, connectionId ?? undefined)
      .then((tableSchema) => {
        if (cancelled) return;
        setIndex(indexForeignKeys(tableSchema.foreignKeys ?? []));
        setLoading(false);
      })
      .catch((cause: unknown) => {
        if (cancelled) return;
        // Degrade: no arrows, the grid keeps working.
        setIndex(EMPTY_INDEX);
        setError(cause instanceof Error ? cause.message : String(cause));
        setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [dbSessionId, tableName, database, schema, connectionId]);

  return useMemo(
    () => ({ index, loading, error }),
    [index, loading, error],
  );
}
