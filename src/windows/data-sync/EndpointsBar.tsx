import { useI18n } from '../../hooks/useI18n';
import { MigrationEndpointsBar } from '../../components/migration/MigrationEndpointsBar';
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
  compareDisabled?: boolean;
  targetReadOnly?: boolean;
  sourceSessionError?: string;
  targetSessionError?: string;
  onSourceChange: (id: string) => void;
  onTargetChange: (id: string) => void;
  onSourceDatabaseChange: (db: string) => void;
  onTargetDatabaseChange: (db: string) => void;
  onSourceSchemaChange: (schema: string) => void;
  onTargetSchemaChange: (schema: string) => void;
  onSwap: () => void;
  onCompare: () => void;
}

export function EndpointsBar(props: EndpointsBarProps) {
  const { t } = useI18n();
  const { activePairing, ...rest } = props;

  const actionNote =
    activePairing != null ? (
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
    ) : undefined;

  return (
    <MigrationEndpointsBar
      testIdPrefix="data-sync"
      compareLabel={t('sync.compare')}
      actionNote={actionNote}
      {...rest}
    />
  );
}
