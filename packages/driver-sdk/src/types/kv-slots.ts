/**
 * KV workspace slot contracts (host shell ↔ driver UI).
 *
 * The connection workspace exposes four slots that a key-value driver may fill
 * (48px context bar, bottom status bar, key-props sidebar, connection home).
 * The host owns the slot *geometry* and the panel lifecycle; the driver owns the
 * *content*. Nothing here references a concrete driver — a driver only gets a
 * slot when its {@link DatabaseTypeMeta.kvWorkspace} capability says so and the
 * build registered a component for it (see `scripts/resolve-drivers.mjs`).
 *
 * Type-only module: no host runtime import, safe for headless driver packages.
 */
import type { DatabaseType } from '@datazen/extension-points';

/**
 * Slots a driver can contribute to the KV workspace.
 *
 * - `contextBar` — the 48px content-toolbar left cluster of an open KV panel.
 * - `statusBar` — the bottom status-bar centre cluster of an open KV panel.
 * - `keyPropsSidebar` — the detail drawer body of an open KV panel.
 * - `connectionHome` — the landing screen of a connected session with no panel.
 */
export type KvSlotName = 'contextBar' | 'statusBar' | 'keyPropsSidebar' | 'connectionHome';

/**
 * Per-panel KV state, created and owned by the **host** and handed to every
 * in-panel slot *and* to the panel's connection view (`ConnectionViewProps.kvSlotState`).
 *
 * Why a host-owned relay instead of a driver module singleton: the workbench
 * (which knows the selected key and the unsaved-draft flag) and the toolbar /
 * status-bar / sidebar slots render in three different React subtrees, and the
 * host decides when the panel — and therefore this state — dies. A driver-side
 * module cache keyed by session would outlive the panel.
 *
 * The getters plus `subscribe` are intentionally
 * `useSyncExternalStore`-shaped so a slot subscribes to exactly the field it
 * renders (`useSyncExternalStore(state.subscribe, state.getSelectedKey)`).
 * The host does **not** subscribe on the render path: selection changes are far
 * too frequent to re-render the whole workspace, which is why the selected key
 * is *not* also passed as a plain prop.
 *
 * **Every getter returns a scalar or `null` — no exceptions.** A getter that
 * builds an array/object per call hands `useSyncExternalStore` a new snapshot
 * every render, which is an infinite re-render loop; that is a ruling, not a
 * style preference (W3-A §1.1). Anything aggregate therefore arrives as the
 * *count* or *label* a slot can render on its own (`getLoadedCount()`,
 * `getSelectionCount()`), and the rich collection (the selected keys themselves)
 * stays inside the driver workbench that owns it.
 *
 * Setters are the driver's write side: the key tree publishes what it knows
 * (scan progress, selection size) and the editors publish writes. A setter that
 * receives the value it already holds **must not** notify subscribers — the tree
 * reports on every scroll/scan tick, and a chatty relay would re-render every
 * slot continuously. The host implementation enforces that; see `src/lib/kvSlotState.ts`.
 *
 * Widen-only rule: the five original members keep their names and signatures —
 * Wave-2 driver slots (`ui/kv-bar/**`) are already built against them.
 */
export interface KvSlotState {
  /** Register a change listener; returns the unsubscribe function. */
  subscribe(listener: () => void): () => void;
  /** Key currently selected in the driver's key tree, `null` when none. */
  getSelectedKey(): string | null;
  /** Publish the selected key (called by the driver's workbench/tree). */
  selectKey(key: string | null): void;
  /** Whether the panel holds unsaved edits (drives the dirty gate, PRD I-1). */
  getDirty(): boolean;
  /** Publish the dirty flag (called by the driver's editors). */
  setDirty(dirty: boolean): void;

  // ── W3-A §1.1 widening: status bar (6 items) + context-bar scan cluster ──

  /** Keys currently materialised in the tree (status bar `loaded n`). */
  getLoadedCount(): number;
  /** Publish {@link getLoadedCount}. */
  setLoadedCount(n: number): void;
  /**
   * Last SCAN cursor reported by the tree. `'0'` means the scan completed;
   * any other value means it stopped early (PRD I-2/I-4).
   */
  getScanCursor(): string;
  /** Publish {@link getScanCursor}. */
  setScanCursor(cursor: string): void;
  /** Whether a scan is in flight right now (drives the progress cluster). */
  isScanning(): boolean;
  /** Publish {@link isScanning}. */
  setScanning(scanning: boolean): void;
  /**
   * COUNT already consumed by the current user action. `0` = budget not in play.
   * Read together with {@link getScanBudgetTotal} for `扫描中 12k/50k`.
   */
  getScanBudgetUsed(): number;
  /** Budget ceiling for the current user action. `0` = unknown / unlimited. */
  getScanBudgetTotal(): number;
  /** Publish both budget halves in one call (one notification, not two). */
  setScanBudget(used: number, total: number): void;
  /**
   * Multi-selection size. Only the count crosses this relay — the selected keys
   * themselves stay in the driver workbench that mutates them (PRD I-8).
   */
  getSelectionCount(): number;
  /** Publish {@link getSelectionCount}. */
  setSelectionCount(n: number): void;
  /** Last write the server acknowledged, e.g. `SET app:cache:session:1`. */
  getLastWriteCommand(): string | null;
  /** Round-trip of {@link getLastWriteCommand} in ms, `null` when unknown. */
  getLastWriteDurationMs(): number | null;
  /** Record a completed write and its duration as one fact (one notification). */
  recordWrite(command: string, durationMs: number): void;
}

/**
 * A request from an in-panel slot back to the **host** (W3-A §1.2).
 *
 * This is the only reverse channel: slots render facts from {@link KvSlotState}
 * and ask for things through `request`. It deliberately has no answer — a slot
 * must not assume anybody is listening, because the host ignores what it cannot
 * do (no-op plus one developer warning, never a throw). Callback-style escape
 * hatches on the state relay (`onRefresh?: () => void`) are banned: they would
 * fork the one channel into N undocumented ones.
 *
 * Where an action goes to the drive side of the panel (a scan-budget change, a
 * flush), the host dispatcher is still the single decision point: dangerous
 * actions pass its write gate before anything else happens.
 */
export type KvSlotAction =
  | { type: 'refresh' }
  | { type: 'newKey' }
  | { type: 'import' }
  | { type: 'export' }
  | { type: 'flushDb' }
  | { type: 'openMonitor' }
  | { type: 'openSettings' }
  | { type: 'setScanBudget'; value: number }
  /**
   * Ask the host to make `database` the panel's database — "switch the db
   * selector" in PRD §3.4 terms.
   *
   * Same class of member as the `setScanBudget` above: the host owns which
   * database the workspace is showing, so the slot states the intent and the
   * host decides how to satisfy it (today: activate the KV panel already bound
   * to that db, or open one). A host that does not handle it yet is a no-op +
   * warning like any other unwired action — the slot must still render its
   * selector, because hiding a control the user can see a need for is the
   * dead-surface failure PRD P-3 exists to prevent.
   *
   * Deliberately an action and **not** a member of {@link KvSlotState}: the
   * relay carries subscribable per-panel *state*, while this is a one-shot
   * request with no value to observe back.
   */
  | { type: 'selectDatabase'; database: string };

/** Props every in-panel KV slot receives from the host. */
export interface KvPanelSlotProps {
  /** Persisted connection-config id (stable across restarts). */
  connectionId: string;
  /** Live database session id (runtime only, never persisted). */
  dbSessionId: string;
  /** Display name of the connection, for titles/empty states. */
  connectionName: string;
  /** Registry key of the driver owning this panel. */
  databaseType: DatabaseType;
  /**
   * Logical database the panel is bound to, e.g. `db0`.
   * `null` while the session has not resolved one yet.
   */
  database: string | null;
  /**
   * Numeric index of {@link database} for index-addressed drivers
   * (`databaseFieldType: 'index'`, i.e. Redis). Undefined otherwise.
   */
  dbIndex?: number;
  /**
   * Host-owned per-panel state relay: selection, dirty flag and the scan /
   * selection / last-write facts the status bar and context bar render.
   */
  state: KvSlotState;
}

/** Content-toolbar 48px context bar (`kvWorkspace.contextBar`). */
export interface KvContextBarProps extends KvPanelSlotProps {
  /**
   * Whether the toolbar is below its expanded-width threshold. The host measures
   * the toolbar; a driver decides what to drop first (chips → icons → overflow).
   */
  compact: boolean;
  /**
   * Ask the host to carry out {@link KvSlotAction}. Present on this slot only —
   * the status bar and the key-props sidebar are read-only surfaces, so the
   * shared {@link KvPanelSlotProps} bundle stays free of it.
   *
   * Fire-and-forget: never await it for a "did that work?" answer, and never
   * hide a control because you expect the host to have nothing to do with it.
   */
  request(action: KvSlotAction): void;
}

/** Bottom status bar centre cluster (`kvWorkspace.statusBar`). */
export type KvStatusBarProps = KvPanelSlotProps;

/** Detail-drawer key-props sidebar (`kvWorkspace.keyPropsSidebar`). */
export interface KeyPropsSidebarProps extends KvPanelSlotProps {
  /** Whether the drawer is expanded (the host owns the open state). */
  open: boolean;
  /** Collapse request wired to the host's toggle. */
  onClose: () => void;
}

/**
 * Connection home / landing screen contribution (`kvWorkspace.home`).
 *
 * Rendered instead of the host's connected-home banner when the driver declares
 * the capability and contributed the slot; the host passes no panel state here
 * because no panel is open yet on this screen.
 */
export interface ConnectionHomeSlotProps {
  /** Persisted connection-config id. */
  connectionId: string;
  /** Live database session id of the connected session. */
  dbSessionId: string;
  /** Display name of the connection. */
  connectionName: string;
  /** Registry key of the driver owning this connection. */
  databaseType: DatabaseType;
  /** Database saved on the connection config, when the user set one. */
  initialDatabase?: string;
}
