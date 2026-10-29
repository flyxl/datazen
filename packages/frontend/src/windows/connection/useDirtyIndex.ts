import { useMemo, useRef } from 'react';
import { emptyTableState } from '../../stores/tableData/connectionState';
import { buildDirtyIndex, type DirtyIndex } from '../../stores/tableData/dirtyIndex';
import type { TableState } from '../../stores/tableData/types';

/**
 * The dirty index of one table panel (DB-06).
 *
 * The index is derived, never stored: it is a pure function of the panel's
 * staged changes and its current page. The hook only adds the two things a
 * pure function cannot do on its own:
 *
 * - it rebuilds the index **only** when one of its inputs changed (the store's
 *   own references — a fresh array per render would rebuild it every time);
 * - it keeps the previous round's index so untouched rows keep their object
 *   identity and `VirtualRow`'s `memo` survives an edit in another row.
 */
export function useDirtyIndex(ts: TableState | undefined): DirtyIndex {
  const previous = useRef<DirtyIndex | null>(null);
  return useMemo(() => {
    const next = buildDirtyIndex(ts ?? emptyTableState(), previous.current);
    previous.current = next;
    return next;
  }, [ts?.pendingChanges, ts?.rowIdentityAnchors, ts?.rows, ts?.columns]);
}
