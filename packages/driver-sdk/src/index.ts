/**
 * @datazen/driver-sdk — pure database driver frontend contracts.
 *
 * Zero DOM / no React UI components. Safe for headless and dialect-only modules.
 */

// === Base SQL Generators & Dialect Profiles ===
export { BaseTableSqlGenerator } from '../../../src/lib/sqlDialects/baseTableSql';
export type {
  SqlDialectStrategy,
  SqlDialectProfile,
  SqlDialectFamily,
  TableSqlDialect,
  GeneratedSqlType,
} from '../../../src/lib/sqlDialects/types';

// === Database metadata ===
export type { DatabaseTypeMeta, ConnectionMode } from '../../../src/lib/databaseMeta';
export type { DatabaseObjectKind, TableSchema, TableInfo } from '../../../src/types';

// === SQL function catalog types ===
export type { FunctionEntry, FunctionParam } from '../../../src/lib/sqlFunctionTypes';

// === Connection clipboard & form models ===
export type {
  ConnectionClipboardFill,
  ConnectionClipboardParser,
} from '../../../src/lib/connectionClipboardTypes';
export type { ConnectionFormState } from './types/connection-form';

// === Key-Value (Redis) data contracts ===
export type { KeyEntry, KeyScanResult } from './types/kv';

// === Web context-menu item defs (type-only contract) ===
export type { NativeMenuPredefined, NativeMenuItemDef } from './types/menu';

// === Connection view props (host shell ↔ driver view) ===
export type {
  NodeContextMenuPayload,
  ConnectionOpenTarget,
  ConnectionViewActions,
  ConnectionViewProps,
} from './types/connection-view';

/**
 * Driver form validator: receives raw field values and i18n `t()`,
 * returns a map of field→error message (empty = valid).
 */
export type DriverFormValidator = (
  fields: {
    host: string;
    port: string;
    database: string;
    username: string;
    password: string;
    schema: string;
    options?: Record<string, unknown>;
  },
  t: (key: string) => string,
) => Record<string, string>;

// === Driver Commands (generic driver IPC gateway) ===
export { driverCommands } from './ipc/driverCommands';
export type {
  ExecuteDriverCommandRequest,
  ExecuteDriverCommandStreamRequest,
  CommandResult,
} from './ipc/driverCommands';

// === Native-dialog file IO (shared with driver UI) ===
export { fileCommands } from './ipc/fileCommands';
export type { OpenedBinaryFile } from './ipc/fileCommands';

// === Driver settings helpers (pure) ===
export {
  mergeDriverSettings,
  readBooleanField,
  applySchemaDefaults,
  listBooleanSchemaFields,
  listSchemaPropertyEntries,
} from './driverSettings';
export type { DriverSettingsContribution } from './driverSettings';

// === Editor font resolution (pure) ===
export { resolveEditorFontFamily, HOST_DEFAULT_EDITOR_FONT } from './resolveEditorFontFamily';

// === Web context menu (implementation + host bridge) ===
export {
  bindContextMenuBridge,
  showNativeContextMenu,
  hideNativeContextMenu,
  normalizeNativeMenuItems,
  nativeEditMenuItems,
  createNativeContextMenuHandler,
} from './nativeContextMenu';
export type { ContextMenuBridge, ContextMenuPosition } from './nativeContextMenu';

// === Settings Store bridge ===
export { bindSettingsStore, useBoundSettingsStore } from './settingsStoreBridge';
export type {
  BoundSettingsStore,
  SettingsBridgeState,
  UseBoundSettingsStore,
} from './settingsStoreBridge';

// === Connection Store bridge ===
export { bindConnectionStore, useBoundConnectionStore } from './connectionStoreBridge';
export type {
  BoundConnectionStore,
  ConnectionBridgeItem,
  ConnectionBridgeState,
  UseBoundConnectionStore,
} from './connectionStoreBridge';

// === Confirm Dialog bridge ===
export { bindConfirmDialog, useBoundConfirmDialog } from './confirmDialogBridge';
export type {
  BoundConfirmDialogHook,
  ConfirmDialogFn,
  ConfirmDialogOptions,
} from './confirmDialogBridge';

// === Schema Cache & Auto-complete Sync ===
export {
  syncSchemaTables,
  syncSchemaNamespace,
  registerPathAliases,
  getCachedPathItems,
  cachePathItems,
  subscribeSchemaPathItems,
  useBoundSchemaStore,
  bindSchemaStore,
} from './schemaStoreBridge';
export type { BoundSchemaStore, SchemaStoreState, UseBoundSchemaStore } from './schemaStoreBridge';
