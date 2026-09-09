import type { ReactNode } from 'react';
import { ArrowLeftRight, Loader2, RefreshCcw } from 'lucide-react';
import { Button } from '../ui/Button';
import { Select } from '../ui/Select';
import { useI18n } from '../../hooks/useI18n';
import type { I18nKey } from '../../locales';

export type MigrationI18nPrefix = 'sync' | 'schemaDiff' | 'transfer';

export interface MigrationSelectOption {
  value: string;
  label: string;
  disabled?: boolean;
  title?: string;
}

export interface MigrationEndpointsBarProps {
  /** Prefix for data-testid attributes, e.g. `data-sync` or `schema-diff`. */
  testIdPrefix: string;
  /** Domain prefix for i18n keys ('sync' | 'schemaDiff' | 'transfer'). Defaults based on testIdPrefix. */
  i18nPrefix?: MigrationI18nPrefix;
  /** `bar` = Sync/Schema Diff top bar; `grid` = Transfer wizard endpoints card. */
  layout?: 'bar' | 'grid';
  sourceLabelKey?: I18nKey;
  targetLabelKey?: I18nKey;
  sourcePlaceholderKey?: I18nKey;
  targetPlaceholderKey?: I18nKey;
  databaseLabelKey?: I18nKey;
  selectDatabasePlaceholderKey?: I18nKey;
  schemaLabelKey?: I18nKey;
  selectSchemaPlaceholderKey?: I18nKey;
  swapTooltipKey?: I18nKey;
  readOnlyHintKey?: I18nKey;
  emptyConnectionLabelKey?: I18nKey;
  sourceId: string;
  targetId: string;
  sourceDatabase: string;
  targetDatabase: string;
  sourceDatabases: string[];
  targetDatabases: string[];
  sourceSchema?: string;
  targetSchema?: string;
  sourceSchemas?: string[];
  targetSchemas?: string[];
  connOptions: MigrationSelectOption[];
  targetOptions: MigrationSelectOption[];
  busy?: boolean;
  compareDisabled?: boolean;
  targetReadOnly?: boolean;
  sourceSessionError?: string;
  targetSessionError?: string;
  /** Prepends an empty connection option (Transfer wizard). */
  includeEmptyConnectionOption?: boolean;
  /** Hide database picker until a connection is selected (Transfer wizard). */
  hideDatabaseUntilConnected?: boolean;
  showSwap?: boolean;
  showCompare?: boolean;
  /** Shown above Compare in bar layout, or below the grid in wizard layout. */
  actionNote?: ReactNode;
  footerNote?: ReactNode;
  compareLabel?: string;
  onSourceChange: (id: string) => void;
  onTargetChange: (id: string) => void;
  onSourceDatabaseChange: (db: string) => void;
  onTargetDatabaseChange: (db: string) => void;
  onSourceSchemaChange?: (schema: string) => void;
  onTargetSchemaChange?: (schema: string) => void;
  onSwap?: () => void;
  onCompare?: () => void;
}

interface EndpointColumnProps {
  testId: string;
  labelKey: I18nKey;
  connectionId: string;
  connectionOptions: MigrationSelectOption[];
  connectionPlaceholderKey: I18nKey;
  database: string;
  databases: string[];
  databaseLabelKey: I18nKey;
  selectDatabasePlaceholderKey: I18nKey;
  schema?: string;
  schemas: string[];
  schemaLabelKey: I18nKey;
  selectSchemaPlaceholderKey: I18nKey;
  sessionError?: string;
  readOnlyHint?: boolean;
  readOnlyHintKey: I18nKey;
  hideDatabaseUntilConnected: boolean;
  showDatabaseLabel: boolean;
  onConnectionChange: (id: string) => void;
  onDatabaseChange: (db: string) => void;
  onSchemaChange?: (schema: string) => void;
}

function resolveDefaultPrefix(
  testIdPrefix: string,
  explicitPrefix?: MigrationI18nPrefix,
): MigrationI18nPrefix {
  if (explicitPrefix) return explicitPrefix;
  if (testIdPrefix.startsWith('schema-diff')) return 'schemaDiff';
  if (testIdPrefix.startsWith('data-transfer')) return 'transfer';
  return 'sync';
}

function EndpointColumn({
  testId,
  labelKey,
  connectionId,
  connectionOptions,
  connectionPlaceholderKey,
  database,
  databases,
  databaseLabelKey,
  selectDatabasePlaceholderKey,
  schema = '',
  schemas,
  schemaLabelKey,
  selectSchemaPlaceholderKey,
  sessionError,
  readOnlyHint = false,
  readOnlyHintKey,
  hideDatabaseUntilConnected,
  showDatabaseLabel,
  onConnectionChange,
  onDatabaseChange,
  onSchemaChange,
}: EndpointColumnProps) {
  const { t } = useI18n();
  const showDatabase = !hideDatabaseUntilConnected || Boolean(connectionId);
  const showSchema = schemas.length > 0 && onSchemaChange;

  return (
    <div className="min-w-0 flex-1" data-testid={testId}>
      <label className="mb-1 block text-[11px] font-semibold uppercase tracking-wider text-fg-muted">
        {t(labelKey)}
      </label>
      <Select
        value={connectionId}
        options={connectionOptions}
        onChange={onConnectionChange}
        placeholder={t(connectionPlaceholderKey)}
      />
      {showDatabase && (
        <div data-testid={`${testId}-database`} className="mt-2">
          {showDatabaseLabel && (
            <label className="mb-1 block text-[10px] font-medium uppercase tracking-wider text-fg-muted">
              {t(databaseLabelKey)}
            </label>
          )}
          <Select
            value={database || ''}
            options={databases.map((db) => ({ value: db, label: db }))}
            onChange={onDatabaseChange}
            placeholder={t(selectDatabasePlaceholderKey)}
          />
        </div>
      )}
      {readOnlyHint && (
        <p className="mt-2 rounded border border-warning/30 bg-warning/10 px-2 py-1 text-xs text-warning">
          {t(readOnlyHintKey)}
        </p>
      )}
      {showSchema && (
        <div data-testid={`${testId}-schema`} className="mt-2">
          <label className="mb-1 block text-[10px] font-medium uppercase tracking-wider text-fg-muted">
            {t(schemaLabelKey)}
          </label>
          <Select
            value={schema}
            options={schemas.map((s) => ({ value: s, label: s }))}
            onChange={onSchemaChange}
            placeholder={t(selectSchemaPlaceholderKey)}
          />
        </div>
      )}
      {sessionError && (
        <p
          data-testid={`${testId}-session-error`}
          className="mt-1 text-xs text-red-600 dark:text-red-400"
        >
          {sessionError}
        </p>
      )}
    </div>
  );
}

/**
 * Shared source/target endpoint UI for migration tools (Sync, Schema Diff, Transfer).
 */
export function MigrationEndpointsBar({
  testIdPrefix,
  i18nPrefix,
  layout = 'bar',
  sourceLabelKey,
  targetLabelKey,
  sourcePlaceholderKey,
  targetPlaceholderKey,
  databaseLabelKey,
  selectDatabasePlaceholderKey,
  schemaLabelKey,
  selectSchemaPlaceholderKey,
  swapTooltipKey,
  readOnlyHintKey,
  emptyConnectionLabelKey,
  sourceId,
  targetId,
  sourceDatabase,
  targetDatabase,
  sourceSchema = '',
  targetSchema = '',
  sourceDatabases,
  targetDatabases,
  sourceSchemas = [],
  targetSchemas = [],
  connOptions,
  targetOptions,
  busy = false,
  compareDisabled = false,
  targetReadOnly = false,
  sourceSessionError,
  targetSessionError,
  includeEmptyConnectionOption = false,
  hideDatabaseUntilConnected = false,
  showSwap = layout === 'bar',
  showCompare = layout === 'bar',
  actionNote,
  footerNote,
  compareLabel = '',
  onSourceChange,
  onTargetChange,
  onSourceDatabaseChange,
  onTargetDatabaseChange,
  onSourceSchemaChange,
  onTargetSchemaChange,
  onSwap,
  onCompare,
}: MigrationEndpointsBarProps) {
  const { t } = useI18n();
  const prefix = resolveDefaultPrefix(testIdPrefix, i18nPrefix);

  const resolvedSourceLabelKey = sourceLabelKey ?? (`${prefix}.source` as I18nKey);
  const resolvedTargetLabelKey = targetLabelKey ?? (`${prefix}.target` as I18nKey);
  const resolvedSourcePlaceholderKey =
    sourcePlaceholderKey ??
    (includeEmptyConnectionOption
      ? (emptyConnectionLabelKey ?? 'common.selectConnection')
      : (`${prefix}.selectSource` as I18nKey));
  const resolvedTargetPlaceholderKey =
    targetPlaceholderKey ??
    (includeEmptyConnectionOption
      ? (emptyConnectionLabelKey ?? 'common.selectConnection')
      : (`${prefix}.selectTarget` as I18nKey));
  const resolvedDatabaseLabelKey = databaseLabelKey ?? (`${prefix}.database` as I18nKey);
  const resolvedSelectDatabasePlaceholderKey =
    selectDatabasePlaceholderKey ?? (`${prefix}.selectDatabase` as I18nKey);
  const resolvedSchemaLabelKey = schemaLabelKey ?? (`${prefix}.schema` as I18nKey);
  const resolvedSelectSchemaPlaceholderKey =
    selectSchemaPlaceholderKey ?? (`${prefix}.selectSchema` as I18nKey);
  const resolvedSwapTooltipKey = swapTooltipKey ?? (`${prefix}.swapEndpoints` as I18nKey);
  const resolvedReadOnlyHintKey = readOnlyHintKey ?? (`${prefix}.readOnlyHint` as I18nKey);

  const isGrid = layout === 'grid';
  const emptyConn = includeEmptyConnectionOption
    ? [{ value: '', label: t(emptyConnectionLabelKey ?? 'common.selectConnection') }]
    : [];
  const sourceConnOptions = [...emptyConn, ...connOptions];
  const targetConnOptions = [...emptyConn, ...targetOptions];

  const sourceColumn = (
    <EndpointColumn
      testId={`${testIdPrefix}-source`}
      labelKey={resolvedSourceLabelKey}
      connectionId={sourceId}
      connectionOptions={sourceConnOptions}
      connectionPlaceholderKey={resolvedSourcePlaceholderKey}
      database={sourceDatabase}
      databases={sourceDatabases}
      databaseLabelKey={resolvedDatabaseLabelKey}
      selectDatabasePlaceholderKey={resolvedSelectDatabasePlaceholderKey}
      schema={sourceSchema}
      schemas={sourceSchemas}
      schemaLabelKey={resolvedSchemaLabelKey}
      selectSchemaPlaceholderKey={resolvedSelectSchemaPlaceholderKey}
      sessionError={sourceSessionError}
      readOnlyHintKey={resolvedReadOnlyHintKey}
      hideDatabaseUntilConnected={hideDatabaseUntilConnected}
      showDatabaseLabel={!isGrid}
      onConnectionChange={onSourceChange}
      onDatabaseChange={onSourceDatabaseChange}
      onSchemaChange={onSourceSchemaChange}
    />
  );

  const targetColumn = (
    <EndpointColumn
      testId={`${testIdPrefix}-target`}
      labelKey={resolvedTargetLabelKey}
      connectionId={targetId}
      connectionOptions={targetConnOptions}
      connectionPlaceholderKey={resolvedTargetPlaceholderKey}
      database={targetDatabase}
      databases={targetDatabases}
      databaseLabelKey={resolvedDatabaseLabelKey}
      selectDatabasePlaceholderKey={resolvedSelectDatabasePlaceholderKey}
      schema={targetSchema}
      schemas={targetSchemas}
      schemaLabelKey={resolvedSchemaLabelKey}
      selectSchemaPlaceholderKey={resolvedSelectSchemaPlaceholderKey}
      sessionError={targetSessionError}
      readOnlyHint={targetReadOnly}
      readOnlyHintKey={resolvedReadOnlyHintKey}
      hideDatabaseUntilConnected={hideDatabaseUntilConnected}
      showDatabaseLabel={!isGrid}
      onConnectionChange={onTargetChange}
      onDatabaseChange={onTargetDatabaseChange}
      onSchemaChange={onTargetSchemaChange}
    />
  );

  if (isGrid) {
    return (
      <div className="rounded-lg border border-edge bg-surface-alt p-6">
        <div className="grid gap-6 md:grid-cols-2">
          {sourceColumn}
          {targetColumn}
        </div>
        {footerNote ?? actionNote}
      </div>
    );
  }

  return (
    <div className="flex shrink-0 items-center gap-3 border-b border-edge px-6 py-4">
      {sourceColumn}
      {showSwap && (
        <Button
          variant="ghost"
          size="sm"
          className="mt-5 shrink-0"
          data-testid={`${testIdPrefix}-swap`}
          onClick={onSwap}
          disabled={busy || (!sourceId && !targetId)}
          title={t(resolvedSwapTooltipKey)}
        >
          <ArrowLeftRight className="h-4 w-4" />
        </Button>
      )}
      {targetColumn}
      {showCompare && (
        <div className="mt-5 flex shrink-0 flex-col items-end gap-1">
          {actionNote}
          <Button
            variant="primary"
            data-testid={`${testIdPrefix}-compare`}
            onClick={onCompare}
            disabled={busy || compareDisabled}
          >
            {busy ? (
              <Loader2 className="h-4 w-4 animate-spin" />
            ) : (
              <RefreshCcw className="h-4 w-4" />
            )}
            {compareLabel}
          </Button>
        </div>
      )}
    </div>
  );
}

export { TransferPairingNote } from '../../windows/data-transfer/TransferPairingNote';
