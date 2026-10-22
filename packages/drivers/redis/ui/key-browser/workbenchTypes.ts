import type { ReactNode } from 'react';
import type { KvSlotState } from '@datazen/driver-sdk';
import type { KeyDetail } from '../shared/types';
import type { RedisPendingAction } from '../overview/overviewNavigation';

/**
 * Props passed to the `renderRightPanel` callback, giving the caller full
 * access to the detail state needed to render a custom right panel (e.g. the
 * tabbed panel with 键详情/命令行/发布订阅/慢日志).
 */
export interface RightPanelRenderProps {
  dbSessionId: string;
  /**
   * Persisted connection config id (the host's `connectionId`), when the host
   * view forwards one. The right panel's Pub/Sub tab filters the
   * `redis-pubsub-message` stream on it, so it has to reach that tab.
   */
  connectionId?: string;
  dbIndex: number;
  selectedKey: string | null;
  detail: KeyDetail | null;
  detailLoading: boolean;
  modules: string[] | null;
  onRefresh: () => void;
  onRenamed: (newKey: string) => void;
  onDirtyChange?: (dirty: boolean) => void;
}

/**
 * Public prop/handle surface of `RedisWorkbench` (屏 B). Kept in its own module
 * so the sidebar, toolbar and column components can type against the workbench
 * contract without importing the component (and its hook tree) — D-0 split.
 */

export interface RedisWorkbenchProps {
  dbSessionId: string;
  /**
   * Persistent connection id, when the host view forwards one. D-3 uses it as
   * the key-tree preference bucket, and the right panel's Pub/Sub tab uses it
   * to attribute the `redis-pubsub-message` stream to this connection.
   * Absent ⇒ preferences stay scoped to `dbSessionId`, which is the only
   * identity `ConnectionViewProps` guarantees every host mount provides.
   */
  connectionId?: string;
  initialDatabase?: string;
  hideSidebar?: boolean;
  onDbIndexChange?: (dbIndex: number) => void;
  onDatabaseChange?: (database: string) => void;
  onKeysChange?: (keys: string[]) => void;
  /**
   * Host-owned selection/dirty atom of this panel (`driver-sdk` `KvSlotState`),
   * forwarded by `RedisConnectionView`. The workbench is the only writer: the
   * host-rendered KV slots read selection and dirtiness from here instead of
   * calling back into the driver (PRD §7-2, contract F-2). Absent when the
   * driver declares no KV slot capability, so every publish below is optional.
   */
  kvSlotState?: KvSlotState;
  /**
   * One-shot action from the overview page (屏 A → 屏 B jump).  Consumed
   * exactly once: tab switch, import/export dialog, new-key dialog, or key
   * selection.  The ref-based consumption inside the workbench ensures the
   * action fires even when the workbench mounts on a later render tick.
   */
  pendingAction?: RedisPendingAction;
  /**
   * Optional render prop for the right panel content.  When provided, replaces
   * the default `DetailColumn` in the split layout.  The caller receives the
   * full detail state and can render a tabbed panel (键详情/命令行/发布订阅/慢日志)
   * or any other custom right-panel content.  When absent, the workbench falls
   * back to rendering `DetailColumn` directly.
   */
  renderRightPanel?: (props: RightPanelRenderProps) => ReactNode;
}

export interface RedisWorkbenchHandle {
  refreshKeys: () => void;
  selectDatabase: (db: string) => void;
  /**
   * Jump to `key` in the given database. If the target db differs from the
   * current one, switches first; then selects the key to open its editor.
   * The draft-gate inside `selectKey` handles any unsaved state.
   */
  selectKey: (key: string, dbIndex?: number) => void;
}
