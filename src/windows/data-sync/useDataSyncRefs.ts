import { useRef, type MutableRefObject } from 'react';
import type {
  DataSyncSelectedRow,
  DataSyncTableMapping,
  DataSyncTableSelection,
} from '../../commands/sync';
import type { SyncState } from './utils';

/**
 * Mutable cross-cutting state of one Data Sync run.
 *
 * These refs are the fence that survives across async boundaries: job ids,
 * cancellation requests and the comparison generation guard all have to be
 * read from a stable location while a promise is in flight. They are created
 * once per window mount and shared by every domain hook.
 */
export interface DataSyncSharedRefs {
  /** Job id of the comparison / execution currently in flight. */
  jobIdRef: MutableRefObject<string | null>;
  /** Which phase owns {@link DataSyncSharedRefs.jobIdRef}. */
  jobKindRef: MutableRefObject<'compare' | 'execute' | null>;
  /** Job id the user asked to cancel; blocks a late completion. */
  cancelRequestedJobRef: MutableRefObject<string | null>;
  /** Job id whose cancellation is still waiting for the backend. */
  cancellingStatusJobRef: MutableRefObject<string | null>;
  /** Monotonic fence: only the newest comparison may publish a result. */
  compareGenerationRef: MutableRefObject<number>;
  /** True while a database write has no confirmed outcome. */
  writeInFlightRef: MutableRefObject<boolean>;
  /** Mirror of the wizard `syncState` readable from async callbacks. */
  syncStateRef: MutableRefObject<SyncState>;
  /** Mirror of the explicitly selected rows. */
  selectedRowsRef: MutableRefObject<DataSyncSelectedRow[]>;
  /** Mirror of the per-table operation scopes. */
  tableSelectionsRef: MutableRefObject<DataSyncTableSelection[]>;
  /** Page tokens already loaded, so a reload keeps the server selection. */
  loadedPageRef: MutableRefObject<Set<string>>;
  /** Table mappings restored from a profile until the next inspection. */
  pendingProfileMappingsRef: MutableRefObject<DataSyncTableMapping[] | null>;
}

export function useDataSyncRefs(): DataSyncSharedRefs {
  return {
    jobIdRef: useRef<string | null>(null),
    jobKindRef: useRef<'compare' | 'execute' | null>(null),
    cancelRequestedJobRef: useRef<string | null>(null),
    cancellingStatusJobRef: useRef<string | null>(null),
    compareGenerationRef: useRef(0),
    writeInFlightRef: useRef(false),
    syncStateRef: useRef<SyncState>('idle'),
    selectedRowsRef: useRef<DataSyncSelectedRow[]>([]),
    tableSelectionsRef: useRef<DataSyncTableSelection[]>([]),
    loadedPageRef: useRef<Set<string>>(new Set()),
    pendingProfileMappingsRef: useRef<DataSyncTableMapping[] | null>(null),
  };
}
