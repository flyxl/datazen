import { useEffect } from 'react';
import { WindowChromeSlot } from '@datazen/extension-points';
import { StatusBar } from '../../components/StatusBar';
import { LocaleDomainLoading } from '../../components/LocaleDomainLoading';
import { useI18n } from '../../hooks/useI18n';
import { useLocaleDomains } from '../../hooks/useLocaleDomains';
import { useSettings } from '../../hooks/useSettings';
import { useSettingsStore } from '../../stores/settingsStore';
import { useAiStore } from '../../stores/aiStore';
import { cn } from '../../lib/cn';
import { openDataTransferWindow, openSchemaDiffWindow } from '../../lib/windowManager';
import { MigrationRunHistoryDialog } from '../../components/migration/MigrationRunHistoryDialog';
import { useMigrationJobHydration } from '../../hooks/useMigrationJobHydration';
import { ExecuteBar } from './ExecuteBar';
import { tableKey } from './mappingView';
import { NARROW_WIZARD_STEPS, useDataSyncWizardState } from './useDataSyncWizardState';
import { useDataSyncRefs } from './useDataSyncRefs';
import { useDataSyncFeedback } from './useDataSyncFeedback';
import { useDataSyncRunState } from './useDataSyncRunState';
import { useDataSyncEndpoints } from './useDataSyncEndpoints';
import { useDataSyncSelectionState } from './useDataSyncSelectionState';
import { useDataSyncJobLifecycle } from './useDataSyncJobLifecycle';
import { useDataSyncComparison } from './useDataSyncComparison';
import { useDataSyncOptions } from './useDataSyncOptions';
import { useDataSyncSelection } from './useDataSyncSelection';
import { useDataSyncExecution } from './useDataSyncExecution';
import { useDataSyncProfiles } from './useDataSyncProfiles';
import { useDataSyncExplain } from './useDataSyncExplain';
import { useDataSyncWizardNavigation } from './useDataSyncWizardNavigation';
import { DataSyncJobStatusPanel } from './DataSyncJobStatusPanel';
import { DataSyncWizardStepper } from './DataSyncWizardStepper';
import { DataSyncEndpointStep } from './DataSyncEndpointStep';
import { DataSyncOptionsStep } from './DataSyncOptionsStep';
import { DataSyncObjectsStep } from './DataSyncObjectsStep';
import { DataSyncCompareStep } from './DataSyncCompareStep';
import { DataSyncPreviewStep } from './DataSyncPreviewStep';
import { DataSyncResultStep } from './DataSyncResultStep';
import { DataSyncWizardNav } from './DataSyncWizardNav';
import { DataSyncDialogs } from './DataSyncDialogs';

export function DataSyncWindow() {
  const localesReady = useLocaleDomains(['sync']);
  useSettings();
  const { t } = useI18n();
  const loadSettings = useSettingsStore((s) => s.loadSettings);
  const isAiConfigured = useAiStore((s) => s.isConfigured);
  const loadAiConfig = useAiStore((s) => s.loadConfig);
  const migrationJobs = useMigrationJobHydration('dataSync');

  const refs = useDataSyncRefs();
  const feedback = useDataSyncFeedback();
  const runState = useDataSyncRunState();
  const wizard = useDataSyncWizardState();

  const endpoints = useDataSyncEndpoints({
    refs,
    feedback,
    resetCompareState: wizard.resetCompareState,
  });
  const selection = useDataSyncSelectionState({
    refs,
    sourceId: endpoints.sourceId,
    targetId: endpoints.targetId,
    sourceDatabase: endpoints.sourceDatabase,
    targetDatabase: endpoints.targetDatabase,
    sourceSchema: endpoints.sourceSchema,
    targetSchema: endpoints.targetSchema,
  });
  const jobs = useDataSyncJobLifecycle({ endpoints, feedback });
  const comparison = useDataSyncComparison({
    refs,
    feedback,
    run: runState,
    wizard,
    selection,
    endpoints,
    jobs,
  });
  const options = useDataSyncOptions({ wizard, selection });
  const rowSelection = useDataSyncSelection({ refs, feedback, wizard, selection });
  const execution = useDataSyncExecution({
    refs,
    feedback,
    run: runState,
    wizard,
    selection,
    endpoints,
    jobs,
    rowSelection,
  });
  const profiles = useDataSyncProfiles({
    refs,
    feedback,
    runState,
    wizard,
    selection,
    endpoints,
  });
  const explain = useDataSyncExplain({
    endpoints,
    feedback,
    isAiConfigured,
    mappingResults: wizard.mappingResults,
    compareStats: comparison.compareStats,
  });
  const navigation = useDataSyncWizardNavigation({
    wizard,
    endpoints,
    runState,
    comparison,
  });

  refs.syncStateRef.current = wizard.syncState;

  useEffect(() => {
    void loadSettings();
    void loadAiConfig();
  }, [loadSettings, loadAiConfig]);

  // All hooks above. Gate the body on the `sync` locale pack so the UI never
  // renders raw/un-translated `t('sync.*')` keys before it is imported.
  if (!localesReady) {
    return <LocaleDomainLoading testId="data-sync-locale-loading" />;
  }

  return (
    <div
      data-testid="data-sync-window"
      data-sync-state={wizard.syncState}
      data-sync-step={wizard.step}
      data-write-outcome-uncertain={runState.writeOutcomeUncertain ? 'true' : 'false'}
      className="flex h-screen min-h-0 flex-col bg-surface text-fg"
    >
      <WindowChromeSlot
        title={t('common.dataSyncTitle')}
        rightContent={
          <MigrationRunHistoryDialog
            operation="dataSync"
            onReconcile={profiles.reconcileUnknownRun}
          />
        }
      />

      <DataSyncJobStatusPanel
        pendingVerificationCount={migrationJobs.hydration?.verificationJobs.length ?? 0}
        hydrationError={migrationJobs.hydrationError}
        notExecutedCount={jobs.notExecutedJobs.length}
        durableJobs={jobs.durableJobs}
        verifyingRecoveryJob={jobs.verifyingRecoveryJob}
        cancellingDurableJob={jobs.cancellingDurableJob}
        onVerifyRecovery={(details) => void jobs.verifyDataSyncJobRecovery(details)}
        onCancelJob={(jobId) => void jobs.cancelDurableDataSyncJob(jobId)}
      />

      <DataSyncWizardStepper stepIndex={navigation.stepIndex} />

      <div className="flex min-h-0 flex-1 flex-col overflow-auto">
        <div
          className={cn(
            'mx-auto flex min-h-0 w-full flex-1 flex-col px-6 py-6',
            NARROW_WIZARD_STEPS.includes(wizard.step) ? 'max-w-2xl' : 'max-w-6xl',
          )}
        >
          {wizard.step === 'endpoints' && (
            <DataSyncEndpointStep
              scopeReconfirmationRequired={runState.scopeReconfirmationRequired}
              onConfirmScope={() => runState.setScopeReconfirmationRequired(false)}
              profileName={profiles.profileName}
              onProfileNameChange={profiles.setProfileName}
              profiles={profiles.syncProfiles}
              selectedProfileId={profiles.selectedProfileId}
              onSelectProfile={profiles.setSelectedProfileId}
              onSaveProfile={() => void profiles.saveCurrentProfile()}
              onLoadProfile={profiles.loadSelectedProfile}
              onDeleteProfile={() => void profiles.deleteSelectedProfile()}
              endpoints={{
                layout: 'grid',
                showSwap: false,
                showCompare: false,
                sourceId: endpoints.sourceId,
                targetId: endpoints.targetId,
                sourceDatabase: endpoints.sourceDatabase,
                targetDatabase: endpoints.targetDatabase,
                sourceSchema: endpoints.sourceSchema,
                targetSchema: endpoints.targetSchema,
                sourceDatabases: endpoints.sourceDatabases,
                targetDatabases: endpoints.targetDatabases,
                sourceSchemas: endpoints.sourceSchemas,
                targetSchemas: endpoints.targetSchemas,
                connOptions: endpoints.connOptions,
                targetOptions: endpoints.targetOptions,
                activePairing: endpoints.activePairing,
                experimental: endpoints.experimental,
                busy: navigation.busy,
                compareDisabled: navigation.compareDisabled,
                sourceSessionError: endpoints.sourceSessionError,
                targetSessionError: endpoints.targetSessionError,
                targetReadOnly: endpoints.targetReadOnly,
                onSourceChange: endpoints.handleSourceChange,
                onTargetChange: endpoints.handleTargetChange,
                onSourceDatabaseChange: endpoints.handleSourceDatabaseChange,
                onTargetDatabaseChange: endpoints.handleTargetDatabaseChange,
                onSourceSchemaChange: endpoints.handleSourceSchemaChange,
                onTargetSchemaChange: endpoints.handleTargetSchemaChange,
                onSwap: endpoints.handleSwap,
                onCompare: () => void comparison.handleCompare(),
              }}
            />
          )}

          {wizard.step === 'setup' && (
            <DataSyncOptionsStep
              options={wizard.syncOptions}
              onOptionsChange={options.handleOptionsChange}
              onEnableDelete={options.handleEnableDelete}
            />
          )}

          {wizard.step === 'objects' && (
            <DataSyncObjectsStep
              syncState={wizard.syncState}
              rows={wizard.mappingResults}
              disabledTables={wizard.disabledTables}
              onToggleDisabled={comparison.toggleDisabledTable}
              onUpdateSourceFilter={comparison.updateSourceFilter}
              onOpenSchemaDiff={openSchemaDiffWindow}
              onOpenDataTransfer={openDataTransferWindow}
            />
          )}

          {wizard.step === 'compare' && (
            <DataSyncCompareStep
              syncState={wizard.syncState}
              compared={comparison.compared}
              rows={wizard.mappingResults}
              stats={comparison.compareStats}
              onCopyReport={comparison.handleCopyCompareReport}
              onExplainDiff={explain.handleExplainDiff}
              explainLoading={explain.explainLoading}
              onCancel={() => void comparison.handleCancel()}
              filter={wizard.tableFilter}
              search={wizard.tableSearch}
              selectedTableKey={wizard.selectedTableKey}
              onFilterChange={wizard.setTableFilter}
              onSearchChange={wizard.setTableSearch}
              onSelectTable={wizard.setSelectedTableKey}
              selectedTable={rowSelection.selectedTable}
              options={wizard.syncOptions}
              tableSelections={selection.tableSelections}
              onUpdateRows={(table, rows) => rowSelection.updateTableRows(tableKey(table), rows)}
              onSelectAllOperation={(table, operation) =>
                rowSelection.setTableOperationScope(table, operation)
              }
              onClearAllOperation={(table, operation) =>
                rowSelection.clearTableOperationScope(table, operation)
              }
              isOperationSelected={(table, operation) =>
                rowSelection.isTableOperationScoped(table, operation)
              }
              onPageChange={(table, direction) => void rowSelection.loadTablePage(table, direction)}
              pageLoading={selection.pageLoading}
              pageIndex={selection.pageIndex}
              pageCursors={selection.pageCursors}
            />
          )}

          {wizard.step === 'preview' && (
            <DataSyncPreviewStep
              syncState={wizard.syncState}
              executeProgress={feedback.executeProgress}
              sourceDbSessionId={endpoints.sourceSession?.dbSessionId}
              targetDbSessionId={endpoints.targetSession?.dbSessionId}
              sourceDatabase={endpoints.sourceDatabase}
              targetDatabase={endpoints.targetDatabase}
              sourceSchema={endpoints.sourceSchema}
              targetSchema={endpoints.targetSchema}
              tables={wizard.mappingResults}
              options={wizard.syncOptions}
              selectedRows={selection.selectedRows}
              tableSelections={selection.tableSelections}
            />
          )}

          {wizard.step === 'result' && (
            <DataSyncResultStep
              lastExecutionResult={runState.lastExecutionResult}
              stats={comparison.compareStats}
              onCopyReport={comparison.handleCopyCompareReport}
              onExplainDiff={explain.handleExplainDiff}
              explainLoading={explain.explainLoading}
              onReCompare={comparison.handleReCompare}
            />
          )}
        </div>
      </div>

      {wizard.step === 'preview' && (
        <ExecuteBar
          selectedRows={rowSelection.totalSelectedRows}
          hasDeletes={rowSelection.hasSelectedDeletes}
          targetReadOnly={endpoints.targetReadOnly}
          executing={wizard.syncState === 'executing'}
          canExecute={execution.canExecute}
          onExecute={() => void execution.handleExecute()}
          onCancel={() => void comparison.handleCancel()}
        />
      )}

      <DataSyncWizardNav
        stepIndex={navigation.stepIndex}
        busy={navigation.busy}
        canNext={navigation.canNext}
        showNext={wizard.step !== 'preview' && wizard.step !== 'result'}
        onBack={navigation.goBack}
        onNext={() => void navigation.goNext()}
      />

      <StatusBar
        left={<span className="truncate">{feedback.statusMsg || t('common.dataSync')}</span>}
      />

      <DataSyncDialogs
        errorOpen={feedback.errorOpen}
        errorMsg={feedback.errorMsg}
        onCloseError={() => feedback.setErrorOpen(false)}
        deleteConfirmOpen={options.deleteConfirmOpen}
        onCloseDeleteConfirm={() => options.setDeleteConfirmOpen(false)}
        onConfirmEnableDelete={options.confirmEnableDelete}
        explainOpen={explain.explainOpen}
        explainLoading={explain.explainLoading}
        explainText={explain.explainText}
        onCloseExplain={() => explain.setExplainOpen(false)}
        executeConfirmOpen={execution.executeConfirmOpen}
        onCloseExecuteConfirm={() => execution.setExecuteConfirmOpen(false)}
        onConfirmExecute={() => {
          execution.setExecuteConfirmOpen(false);
          void execution.runExecute();
        }}
      />
    </div>
  );
}
