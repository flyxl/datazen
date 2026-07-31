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

// === Hooks ===
export { useI18n } from '../hooks/useI18n';

// === Types ===
export type { DatabaseTypeMeta, ConnectionMode } from '../lib/databaseMeta';
export type { ConnectionFormState } from '../components/connection/useConnectionForm';
export type { SqlDialectStrategy, SqlDialectFamily } from '../lib/sqlDialects/types';
