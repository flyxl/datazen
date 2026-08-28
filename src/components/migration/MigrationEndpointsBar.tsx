import type { ReactNode } from 'react';
import { ArrowLeftRight, Loader2, RefreshCcw } from 'lucide-react';
import { Button } from '../ui/Button';
import { Select } from '../ui/Select';
import { useI18n } from '../../hooks/useI18n';
import { cn } from '../../lib/cn';

export interface MigrationSelectOption {
  value: string;
  label: string;
  disabled?: boolean;
  title?: string;
}

export interface MigrationEndpointsBarProps {
  /** Prefix for data-testid attributes, e.g. `data-sync` or `schema-diff`. */
  testIdPrefix: string;
  /** `bar` = Sync/Schema Diff top bar; `grid` = Transfer wizard endpoints card. */
  layout?: 'bar' | 'grid';
  sourceLabelKey?: string;
  targetLabelKey?: string;
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
  labelKey: string;
  connectionId: string;
  connectionOptions: MigrationSelectOption[];
  connectionPlaceholderKey: string;
  database: string;
  databases: string[];
  schema?: string;
  schemas: string[];
  sessionError?: string;
  readOnlyHint?: boolean;
  hideDatabaseUntilConnected: boolean;
  showDatabaseLabel: boolean;
  onConnectionChange: (id: string) => void;
  onDatabaseChange: (db: string) => void;
  onSchemaChange?: (schema: string) => void;
}

function EndpointColumn({
  testId,
  labelKey,
  connectionId,
  connectionOptions,
  connectionPlaceholderKey,
  database,
  databases,
  schema = '',
  schemas,
  sessionError,
  readOnlyHint = false,
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
              {t('query.database')}
            </label>
          )}
          <Select
            value={database || ''}
            options={databases.map((db) => ({ value: db, label: db }))}
            onChange={onDatabaseChange}
            placeholder={t('common.selectDatabase')}
          />
        </div>
      )}
      {readOnlyHint && (
        <p className="mt-2 rounded border border-warning/30 bg-warning/10 px-2 py-1 text-xs text-warning">
          {t('transfer.readOnlyHint')}
        </p>
      )}
      {showSchema && (
        <div data-testid={`${testId}-schema`} className="mt-2">
          <label className="mb-1 block text-[10px] font-medium uppercase tracking-wider text-fg-muted">
            {t('sync.schema')}
          </label>
          <Select
            value={schema}
            options={schemas.map((s) => ({ value: s, label: s }))}
            onChange={onSchemaChange}
            placeholder={t('sync.selectSchema')}
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
  layout = 'bar',
  sourceLabelKey = 'sync.source',
  targetLabelKey = 'sync.target',
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
  const isGrid = layout === 'grid';
  const emptyConn = includeEmptyConnectionOption
    ? [{ value: '', label: t('common.selectConnection') }]
    : [];
  const sourceConnOptions = [...emptyConn, ...connOptions];
  const targetConnOptions = [...emptyConn, ...targetOptions];
  const connectionPlaceholderKey = includeEmptyConnectionOption
    ? 'common.selectConnection'
    : 'sync.selectSource';

  const sourceColumn = (
    <EndpointColumn
      testId={`${testIdPrefix}-source`}
      labelKey={sourceLabelKey}
      connectionId={sourceId}
      connectionOptions={sourceConnOptions}
      connectionPlaceholderKey={connectionPlaceholderKey}
      database={sourceDatabase}
      databases={sourceDatabases}
      schema={sourceSchema}
      schemas={sourceSchemas}
      sessionError={sourceSessionError}
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
      labelKey={targetLabelKey}
      connectionId={targetId}
      connectionOptions={targetConnOptions}
      connectionPlaceholderKey={
        includeEmptyConnectionOption ? 'common.selectConnection' : 'sync.selectTarget'
      }
      database={targetDatabase}
      databases={targetDatabases}
      schema={targetSchema}
      schemas={targetSchemas}
      sessionError={targetSessionError}
      readOnlyHint={targetReadOnly}
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
          title={t('sync.swapEndpoints')}
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

/** Pairing/path badge for Data Transfer endpoints step. */
export function TransferPairingNote({
  supported,
  path,
  reason,
}: {
  supported: boolean;
  path?: string;
  reason?: string | null;
}) {
  const { t } = useI18n();
  return (
    <p
      data-testid="data-transfer-path"
      className={cn(
        'mt-4 inline-block rounded border border-edge bg-surface px-2 py-1 text-xs text-fg-muted',
      )}
    >
      {supported ? t(`transfer.path.${path}`) : (reason ?? t('common.unsupportedPair'))}
    </p>
  );
}
