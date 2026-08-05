/**
 * DataZen Plugin SDK — 主应用端实现
 *
 * 这个文件 re-export 主应用中的实际组件和 hooks，供插件使用。
 * 插件通过 `@datazen/plugin-sdk` 导入，Vite alias 将其解析到此处。
 *
 * 这是插件与主应用之间的稳定 API 契约层。
 * 任何修改都应保持向后兼容，或同步 bump PROTOCOL_VERSION。
 */

// === UI Components ===
export { Input } from '../components/ui/Input';
export { Select } from '../components/ui/Select';
export { Button } from '../components/ui/Button';
export { Label } from '../components/connection/shared';

// === Utilities ===
export { cn } from '../lib/cn';

// === Hooks ===
export { useI18n } from '../hooks/useI18n';

// === Types ===
export type { DatabaseTypeMeta, ConnectionMode } from '../lib/databaseMeta';
export type { ConnectionFormState } from '../components/connection/useConnectionForm';
export type { SqlDialectStrategy, SqlDialectFamily } from '../lib/sqlDialects/types';
export type { TableInfo, TableType } from '../types';

/**
 * Plugin form validator: receives raw field values and i18n `t()`,
 * returns a map of field→error message (empty = valid).
 */
export type PluginFormValidator = (
  fields: { host: string; port: string; database: string; username: string; password: string; schema: string },
  t: (key: string) => string,
) => Record<string, string>;

// === Plugin Commands ===
export { pluginInvoke, hasPluginCommand } from '../plugins/generated';
export type { PluginCommandMeta } from '../plugins/generated';
