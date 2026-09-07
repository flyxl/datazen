/**
 * DataZen Plugin SDK — 主应用端实现（向后兼容层）
 *
 * @deprecated 请迁移至 `@datazen/driver-sdk`（驱动契约）、`@datazen/extension-points`（特权扩展点）
 * 或 `@datazen/ui`（通用 UI，arch-p1-ui 轨道）。本模块保留 re-export 以确保存量 import 100% 可用。
 *
 * 插件通过 `@datazen/plugin-sdk` 导入，Vite alias 将其解析到此处。
 * 任何修改都应保持向后兼容，或同步 bump PROTOCOL_VERSION。
 */

/** @deprecated Use `@datazen/driver-sdk` */
export {
  BaseTableSqlGenerator,
  driverCommands,
  syncSchemaTables,
  syncSchemaNamespace,
  registerPathAliases,
  getCachedPathItems,
  cachePathItems,
  subscribeSchemaPathItems,
  bindSchemaStore,
} from '@datazen/driver-sdk';

/** @deprecated Use `@datazen/driver-sdk` */
export type {
  SqlDialectStrategy,
  SqlDialectProfile,
  SqlDialectFamily,
  TableSqlDialect,
  GeneratedSqlType,
  DatabaseTypeMeta,
  ConnectionMode,
  DatabaseObjectKind,
  FunctionEntry,
  FunctionParam,
  ConnectionClipboardFill,
  ConnectionClipboardParser,
  ConnectionFormState,
  PluginFormValidator,
  ExecuteDriverCommandRequest,
  CommandResult,
  BoundSchemaStore,
} from '@datazen/driver-sdk';

/** @deprecated Use host types or `@datazen/driver-sdk` when re-exported */
export type { TableInfo, TableType, TableSchema } from '../types';

// === UI Components (legacy; migrate to @datazen/ui in arch-p2-callers) ===
/** @deprecated Use `@datazen/ui` */
export { Input } from '../components/ui/Input';
/** @deprecated Use `@datazen/ui` */
export { Select } from '../components/ui/Select';
/** @deprecated Use `@datazen/ui` */
export { Button } from '../components/ui/Button';
/** @deprecated Use `@datazen/ui` */
export { Label } from '../components/connection/shared';

/** @deprecated Use `@datazen/ui` */
export { cn } from '../lib/cn';

/** @deprecated Host-only; not part of driver-sdk */
export { useI18n } from '../hooks/useI18n';
/** @deprecated Host-only; not part of driver-sdk */
export type {
  I18nKey,
  MongoTranslationKey,
  PluginTranslationKey,
  TranslationKey,
} from '../locales';

// === Plugin Settings ===
/** @deprecated Host plugin settings helpers */
export type { PluginSettingsContribution } from './settings';
/** @deprecated Host plugin settings helpers */
export {
  mergePluginSettings,
  readBooleanField,
  applySchemaDefaults,
  listBooleanSchemaFields,
  listSchemaPropertyEntries,
} from './settings';

// === Host Extension Points ===
/** @deprecated Use `@datazen/extension-points` */
export type { ExtensionPoint, CreateExtensionPointOptions } from '@datazen/extension-points';
/** @deprecated Use `@datazen/extension-points` */
export {
  createExtensionPoint,
  ExtensionRegistry,
  extensionRegistry,
} from '@datazen/extension-points';
/** @deprecated Use `@datazen/extension-points` */
export { useExtension, useIsExtensionEnhanced } from '@datazen/extension-points';
