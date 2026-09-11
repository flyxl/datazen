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
export type { ConnectionFormState } from '../../../src/components/connection/useConnectionForm';

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

// === Driver Commands ===
export { driverCommands } from '../../../src/commands/driver';
export type { ExecuteDriverCommandRequest, CommandResult } from '../../../src/commands/driver';

// === Schema Cache & Auto-complete Sync ===
export {
  syncSchemaTables,
  syncSchemaNamespace,
  registerPathAliases,
  getCachedPathItems,
  cachePathItems,
  subscribeSchemaPathItems,
  bindSchemaStore,
} from './schemaStoreBridge';
export type { BoundSchemaStore } from './schemaStoreBridge';
