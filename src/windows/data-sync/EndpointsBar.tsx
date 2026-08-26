import { ArrowLeftRight, Loader2, RefreshCcw } from 'lucide-react';
import { Button } from '../../components/ui/Button';
import { Select } from '../../components/ui/Select';
import { useI18n } from '../../hooks/useI18n';
import { cn } from '../../lib/cn';
import type { SyncPairingResult } from '../../lib/syncPairing';

interface EndpointsBarProps {
  sourceId: string;
  targetId: string;
  sourceDatabase: string;
  targetDatabase: string;
  sourceSchema: string;
  targetSchema: string;
  sourceDatabases: string[];
  targetDatabases: string[];
  sourceSchemas: string[];
  targetSchemas: string[];
  connOptions: { value: string; label: string }[];
  targetOptions: { value: string; label: string; disabled?: boolean; title?: string }[];
  activePairing: SyncPairingResult | null;
  busy: boolean;
  onSourceChange: (id: string) => void;
  onTargetChange: (id: string) => void;
  onSourceDatabaseChange: (db: string) => void;
  onTargetDatabaseChange: (db: string) => void;
  onSourceSchemaChange: (schema: string) => void;
  onTargetSchemaChange: (schema: string) => void;
  onSwap: () => void;
  onCompare: () => void;
}

export function EndpointsBar({
  sourceId,
  targetId,
  sourceDatabase,
  targetDatabase,
  sourceSchema,
  targetSchema,
  sourceDatabases,
  targetDatabases,
  sourceSchemas,
  targetSchemas,
  connOptions,
  targetOptions,
  activePairing,
  busy,
  onSourceChange,
  onTargetChange,
  onSourceDatabaseChange,
  onTargetDatabaseChange,
  onSourceSchemaChange,
  onTargetSchemaChange,
  onSwap,
  onCompare,
}: EndpointsBarProps) {
  const { t } = useI18n();
  const showSourceSchema = sourceSchemas.length > 0;
  const showTargetSchema = targetSchemas.length > 0;

  return (
    <div className="flex shrink-0 items-center gap-3 border-b border-edge px-6 py-4">
      <div className="min-w-0 flex-1" data-testid="data-sync-source">
        <label className="mb-1 block text-[11px] font-semibold uppercase tracking-wider text-fg-muted">
          {t('sync.source')}
        </label>
        <Select
          value={sourceId}
          options={connOptions}
          onChange={onSourceChange}
          placeholder={t('sync.selectSource')}
        />
        <div data-testid="data-sync-source-database" className="mt-2">
          <Select
            value={sourceDatabase}
            options={sourceDatabases.map((db) => ({ value: db, label: db }))}
            onChange={onSourceDatabaseChange}
            placeholder={t('common.selectDatabase')}
          />
        </div>
        {showSourceSchema && (
          <div data-testid="data-sync-source-schema" className="mt-2">
            <label className="mb-1 block text-[10px] font-medium uppercase tracking-wider text-fg-muted">
              {t('sync.schema')}
            </label>
            <Select
              value={sourceSchema}
              options={sourceSchemas.map((schema) => ({ value: schema, label: schema }))}
              onChange={onSourceSchemaChange}
              placeholder={t('sync.selectSchema')}
            />
          </div>
        )}
      </div>

      <Button
        variant="ghost"
        size="sm"
        className="mt-5 shrink-0"
        data-testid="data-sync-swap"
        onClick={onSwap}
        disabled={busy || (!sourceId && !targetId)}
        title={t('sync.swapEndpoints')}
      >
        <ArrowLeftRight className="h-4 w-4" />
      </Button>

      <div className="min-w-0 flex-1" data-testid="data-sync-target">
        <label className="mb-1 block text-[11px] font-semibold uppercase tracking-wider text-fg-muted">
          {t('sync.target')}
        </label>
        <Select
          value={targetId}
          options={targetOptions}
          onChange={onTargetChange}
          placeholder={t('sync.selectTarget')}
        />
        <div data-testid="data-sync-target-database" className="mt-2">
          <Select
            value={targetDatabase || ''}
            options={targetDatabases.map((db) => ({ value: db, label: db }))}
            onChange={onTargetDatabaseChange}
            placeholder={t('common.selectDatabase')}
          />
        </div>
        {showTargetSchema && (
          <div data-testid="data-sync-target-schema" className="mt-2">
            <label className="mb-1 block text-[10px] font-medium uppercase tracking-wider text-fg-muted">
              {t('sync.schema')}
            </label>
            <Select
              value={targetSchema}
              options={targetSchemas.map((schema) => ({ value: schema, label: schema }))}
              onChange={onTargetSchemaChange}
              placeholder={t('sync.selectSchema')}
            />
          </div>
        )}
      </div>

      <div className="mt-5 flex shrink-0 flex-col items-end gap-1">
        {activePairing && (
          <span
            data-testid="data-sync-path"
            className={cn(
              'text-[10px] font-medium uppercase tracking-wide',
              activePairing.supported ? 'text-fg-muted' : 'text-amber-600 dark:text-amber-400',
            )}
          >
            {activePairing.supported
              ? activePairing.path === 'direct'
                ? t('sync.pathDirect')
                : t('sync.pathIr')
              : t('sync.useTransferHint')}
          </span>
        )}
        <Button
          variant="primary"
          data-testid="data-sync-compare"
          onClick={onCompare}
          disabled={busy}
        >
          {busy ? <Loader2 className="h-4 w-4 animate-spin" /> : <RefreshCcw className="h-4 w-4" />}
          {t('sync.compare')}
        </Button>
      </div>
    </div>
  );
}
