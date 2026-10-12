import { MigrationEndpointsBar } from '../../components/migration/MigrationEndpointsBar';
import type { SchemaDiffObjectIdentity } from '../../commands/schemaDiff';
import type { SchemaDiffTablePick } from './schemaDiffTableNames';
import { SchemaDiffObjectsStep } from './SchemaDiffObjectsStep';
import { SchemaDiffUnifiedObjectsPicker } from './SchemaDiffUnifiedObjectsPicker';
import type { UseSchemaDiffEndpointsReturn } from './useSchemaDiffEndpoints';
import type { useSchemaDiffUnifiedObjects } from './useSchemaDiffUnifiedObjects';

interface SchemaDiffSelectionStepsProps {
  step: 'endpoints' | 'objects';
  endpoints: UseSchemaDiffEndpointsReturn;
  endpointsCrossDialectNote?: React.ReactNode;
  loading: boolean;
  tablePicks: SchemaDiffTablePick[];
  setTablePicks: React.Dispatch<React.SetStateAction<SchemaDiffTablePick[]>>;
  onToggleTable: (name: string) => void;
  unifiedObjects: ReturnType<typeof useSchemaDiffUnifiedObjects>;
  onToggleUnifiedObject: (side: 'source' | 'target') => (object: SchemaDiffObjectIdentity) => void;
  onClearObjectSelections: () => void;
  onRetryObjects: () => void;
}

export function SchemaDiffSelectionSteps({
  step,
  endpoints,
  endpointsCrossDialectNote,
  loading,
  tablePicks,
  setTablePicks,
  onToggleTable,
  unifiedObjects,
  onToggleUnifiedObject,
  onClearObjectSelections,
  onRetryObjects,
}: SchemaDiffSelectionStepsProps) {
  if (step === 'endpoints') {
    return (
      <MigrationEndpointsBar
        layout="grid"
        testIdPrefix="schema-diff"
        i18nPrefix="schemaDiff"
        showSwap={false}
        showCompare={false}
        sourceId={endpoints.sourceId}
        targetId={endpoints.targetId}
        sourceDatabase={endpoints.sourceDatabase}
        targetDatabase={endpoints.targetDatabase}
        sourceSchema={endpoints.sourceSchema}
        targetSchema={endpoints.targetSchema}
        sourceDatabases={endpoints.sourceDatabases}
        targetDatabases={endpoints.targetDatabases}
        sourceSchemas={endpoints.sourceSchemas}
        targetSchemas={endpoints.targetSchemas}
        connOptions={endpoints.connOptions}
        targetOptions={endpoints.targetOptions}
        footerNote={endpointsCrossDialectNote}
        onSourceChange={endpoints.setSourceId}
        onTargetChange={endpoints.setTargetId}
        onSourceDatabaseChange={endpoints.setSourceDatabase}
        onTargetDatabaseChange={endpoints.setTargetDatabase}
        onSourceSchemaChange={endpoints.setSourceSchema}
        onTargetSchemaChange={endpoints.setTargetSchema}
      />
    );
  }

  return (
    <div className="min-h-0 flex-1 space-y-3 overflow-auto" data-testid="schema-diff-objects-step" data-catalog-load-state={unifiedObjects.catalogLoadState}>
      <SchemaDiffObjectsStep
        loading={loading}
        tables={tablePicks}
        onToggle={onToggleTable}
        onSelectAll={() =>
          setTablePicks((previous) => previous.map((row) => ({ ...row, enabled: true })))
        }
        onSelectNone={() =>
          setTablePicks((previous) => previous.map((row) => ({ ...row, enabled: false })))
        }
      />
      <SchemaDiffUnifiedObjectsPicker
        loading={loading}
        sourceObjects={unifiedObjects.sourceObjects}
        targetObjects={unifiedObjects.targetObjects}
        selectedSourceKeys={unifiedObjects.selectedSourceKeys}
        selectedTargetKeys={unifiedObjects.selectedTargetKeys}
        errors={unifiedObjects.errors}
        crossDialect={endpoints.isCrossDialect}
        onToggleSource={onToggleUnifiedObject('source')}
        onToggleTarget={onToggleUnifiedObject('target')}
        onSelectAll={unifiedObjects.selectAll}
        onClearSelections={onClearObjectSelections}
        onRetry={onRetryObjects}
      />
    </div>
  );
}
