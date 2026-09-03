/**
 * DataZen Plugin SDK — 主应用端实现
 *
 * 这个文件 re-export 主应用中的实际组件和 hooks，供插件使用。
 * 插件通过 `@datazen/plugin-sdk` 导入，Vite alias 将其解析到此处。
 *
 * 这是插件与主应用之间的稳定 API 契约层。
 * 任何修改都应保持向后兼容，或同步 bump PROTOCOL_VERSION。
 */

import { useSchemaStore } from '../stores/schemaStore';
import type { TableInfo } from '../types';

// === UI Components ===
export { Input } from '../components/ui/Input';
export { Select } from '../components/ui/Select';
export { Button } from '../components/ui/Button';
export { Label } from '../components/connection/shared';

// === Utilities ===
export { cn } from '../lib/cn';

// === Hooks ===
export { useI18n } from '../hooks/useI18n';
export type {
  I18nKey,
  MongoTranslationKey,
  PluginTranslationKey,
  TranslationKey,
} from '../locales';

// === Types ===
export type { DatabaseTypeMeta, ConnectionMode } from '../lib/databaseMeta';
export type {
  ConnectionClipboardFill,
  ConnectionClipboardParser,
} from '../lib/connectionClipboardTypes';
export type { ConnectionFormState } from '../components/connection/useConnectionForm';
export type {
  SqlDialectStrategy,
  SqlDialectFamily,
  TableSqlDialect,
  GeneratedSqlType,
} from '../lib/sqlDialects/types';
export { BaseTableSqlGenerator } from '../lib/sqlDialects/baseTableSql';
export type { TableInfo, TableType } from '../types';

/**
 * Sync fetched tables into the host schema store (SQL editor autocomplete).
 * Pass `dbSessionId` for custom schema trees that never call `loadForConnection`.
 */
export function syncSchemaTables(
  database: string,
  tables: TableInfo[],
  dbSessionId?: string,
): void {
  if (dbSessionId) {
    useSchemaStore.setState({ dbSessionId });
  }
  useSchemaStore.getState().setLoadedTables(database, tables);
}

export function syncSchemaNamespace(
  segments: string[],
  kind: 'branch' | 'tables',
  names: string[],
  options?: { dbSessionId?: string },
): void {
  if (options?.dbSessionId) {
    useSchemaStore.setState({ dbSessionId: options.dbSessionId });
  }
  useSchemaStore.getState().mergeNamespace(segments, kind, names);
}

/**
 * Register SQL display-name → fetch-path-root aliases and seed top-level namespace branches.
 * Plugins that use opaque path roots (e.g. numeric ids) call this after listing databases.
 */
export function registerPathAliases(
  entries: { name: string; id: string }[],
  dbSessionId?: string,
): void {
  if (dbSessionId) {
    useSchemaStore.setState({ dbSessionId });
  }
  useSchemaStore.getState().registerPathAliases(entries);
}

/** Cached `get_tables` rows for a fetch path (`dbId` or `dbId/catalog[/schema]`). */
export function getCachedPathItems(fetchPath: string): TableInfo[] | undefined {
  return useSchemaStore.getState().pathItems[fetchPath];
}

/** Store `get_tables` rows so autocomplete and the schema tree share one fetch. */
export function cachePathItems(fetchPath: string, items: TableInfo[]): void {
  useSchemaStore.getState().cachePathItems(fetchPath, items);
}

/** Subscribe to the shared path-item cache (custom trees hydrate from autocomplete). */
export function subscribeSchemaPathItems(
  listener: (items: Record<string, TableInfo[]>) => void,
): () => void {
  listener(useSchemaStore.getState().pathItems);
  return useSchemaStore.subscribe((state, prev) => {
    if (state.pathItems !== prev.pathItems) listener(state.pathItems);
  });
}

/**
 * Plugin form validator: receives raw field values and i18n `t()`,
 * returns a map of field→error message (empty = valid).
 */
export type PluginFormValidator = (
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

// === Plugin Settings ===
export type { PluginSettingsContribution } from './settings';
export {
  mergePluginSettings,
  readBooleanField,
  applySchemaDefaults,
  listBooleanSchemaFields,
  listSchemaPropertyEntries,
} from './settings';

// === Plugin Commands ===
export { pluginInvoke, hasPluginCommand } from '../plugins/generated';
export type { PluginCommandMeta } from '../plugins/generated';
export { driverCommands } from '../commands/driver';
export type { ExecuteDriverCommandRequest, CommandResult } from '../commands/driver';
