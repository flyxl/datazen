import { useI18n } from '../../hooks/useI18n';
import { MigrationEndpointsBar } from '../../components/migration/MigrationEndpointsBar';
import { cn } from '../../lib/cn';

export interface SchemaDiffEndpointsBarProps {
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
  isCrossDialect?: boolean;
  busy?: boolean;
  onSourceChange: (id: string) => void;
  onTargetChange: (id: string) => void;
  onSourceDatabaseChange: (db: string) => void;
  onTargetDatabaseChange: (db: string) => void;
  onSourceSchemaChange: (schema: string) => void;
  onTargetSchemaChange: (schema: string) => void;
  onSwap: () => void;
  onCompare: () => void;
}

export function SchemaDiffEndpointsBar({
  isCrossDialect = false,
  busy = false,
  ...rest
}: SchemaDiffEndpointsBarProps) {
  const { t } = useI18n();

  const actionNote = isCrossDialect ? (
    <span
      data-testid="schema-diff-cross-dialect-note"
      className={cn('max-w-[12rem] text-right text-[10px] font-medium text-fg-muted')}
    >
      {t('schemaDiff.crossDialectNote')}
    </span>
  ) : undefined;

  return (
    <MigrationEndpointsBar
      testIdPrefix="schema-diff"
      compareLabel={t('schemaDiff.compare')}
      busy={busy}
      actionNote={actionNote}
      {...rest}
    />
  );
}
