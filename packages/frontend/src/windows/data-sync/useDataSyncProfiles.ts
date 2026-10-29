import { useCallback, useEffect, useState } from 'react';
import { legacyInvoke as invoke } from '@datazen/backend-client';
import {
  syncCommands,
  DEFAULT_SYNC_OPTIONS,
  type DataSyncTableMapping,
  type SyncProfile,
} from '../../commands/sync';
import type { MigrationRunRecord } from '../../commands/history';
import { useI18n } from '../../hooks/useI18n';
import { listDatabasesDedicated } from '../../lib/dedicatedDbSession';
import type { ConnectionConfig } from '../../types';
import type { DataSyncWizardState } from './dataSyncTypes';
import type { DataSyncEndpoints } from './useDataSyncEndpoints';
import type { DataSyncFeedback } from './useDataSyncFeedback';
import type { DataSyncRunState } from './useDataSyncRunState';
import type { DataSyncSharedRefs } from './useDataSyncRefs';
import type { DataSyncSelectionState } from './useDataSyncSelectionState';

export interface UseDataSyncProfilesOptions {
  refs: DataSyncSharedRefs;
  feedback: DataSyncFeedback;
  runState: DataSyncRunState;
  wizard: DataSyncWizardState;
  selection: DataSyncSelectionState;
  endpoints: DataSyncEndpoints;
}

/**
 * Saved sync profiles of the Data Sync window.
 *
 * A profile stores the endpoint scope, the reviewed table mapping and the
 * sync options so a comparison can be replayed. Loading a profile also
 * restores the mapping that the profile was saved with, which is applied to
 * the next inspection.
 */
export function useDataSyncProfiles({
  refs,
  feedback,
  runState,
  wizard,
  selection,
  endpoints,
}: UseDataSyncProfilesOptions) {
  const { t } = useI18n();
  const { setErrorMsg, setErrorOpen, setStatusMsg } = feedback;
  const { setLastExecutionResult, setWriteOutcomeUncertain, setScopeReconfirmationRequired } =
    runState;
  const {
    syncOptions,
    setSyncOptions,
    mappingResults,
    disabledTables,
    resetCompareState,
    setStep,
    setSyncState,
  } = wizard;
  const { setSelectedRows, setTableSelections, setPageIndex, setPageCursors } = selection;
  const {
    connections,
    setConnections,
    sourceId,
    setSourceId,
    targetId,
    setTargetId,
    sourceDatabase,
    setSourceDatabase,
    targetDatabase,
    setTargetDatabase,
    sourceSchema,
    setSourceSchema,
    targetSchema,
    setTargetSchema,
    setSourceDatabases,
    setTargetDatabases,
    setSourceSchemas,
    setTargetSchemas,
  } = endpoints;
  const {
    pendingProfileMappingsRef,
    compareGenerationRef,
    jobIdRef,
    jobKindRef,
    cancelRequestedJobRef,
    writeInFlightRef,
    loadedPageRef,
  } = refs;

  const [syncProfiles, setSyncProfiles] = useState<SyncProfile[]>([]);
  const [profileName, setProfileName] = useState('');
  const [selectedProfileId, setSelectedProfileId] = useState('');

  const loadSyncProfiles = useCallback(() => {
    void syncCommands
      .getSyncProfiles()
      .then(setSyncProfiles)
      .catch(() => setSyncProfiles([]));
  }, []);

  const currentProfileTables = useCallback((): DataSyncTableMapping[] => {
    if (mappingResults.length === 0) return pendingProfileMappingsRef.current ?? [];
    return mappingResults
      .filter((row) => row.sourceTable.trim() && row.targetTable.trim())
      .map((row) => ({
        sourceTable: row.sourceTable,
        targetTable: row.targetTable,
        enabled: !disabledTables.has(row.sourceTable) && row.status !== 'DISABLED',
        sourceFilter: row.sourceFilter,
      }));
  }, [disabledTables, mappingResults]);

  const saveCurrentProfile = useCallback(async () => {
    const name = profileName.trim();
    if (!name || !sourceId || !targetId) {
      setErrorMsg(t('sync.profile.missingFields'));
      setErrorOpen(true);
      return;
    }
    const existing = syncProfiles.find((profile) => profile.id === selectedProfileId);
    const now = new Date().toISOString();
    const profile: SyncProfile = {
      version: 1,
      id: existing?.id ?? crypto.randomUUID(),
      name,
      sourceConnectionId: sourceId,
      targetConnectionId: targetId,
      sourceDatabase: sourceDatabase || null,
      targetDatabase: targetDatabase || null,
      sourceSchema: sourceSchema || null,
      targetSchema: targetSchema || null,
      tables: currentProfileTables(),
      options: syncOptions,
      createdAt: existing?.createdAt ?? now,
      updatedAt: now,
    };
    try {
      await syncCommands.saveSyncProfile(profile);
      setSelectedProfileId(profile.id);
      setProfileName(profile.name);
      loadSyncProfiles();
      setStatusMsg(t('sync.profile.saved'));
    } catch (error) {
      setErrorMsg(error instanceof Error ? error.message : String(error));
      setErrorOpen(true);
    }
  }, [
    currentProfileTables,
    loadSyncProfiles,
    profileName,
    selectedProfileId,
    sourceDatabase,
    sourceId,
    sourceSchema,
    syncOptions,
    syncProfiles,
    t,
    targetDatabase,
    targetId,
    targetSchema,
  ]);

  const loadSelectedProfile = useCallback(() => {
    const profile = syncProfiles.find((candidate) => candidate.id === selectedProfileId);
    if (!profile) return;
    const sourceExists = connections.some(
      (connection) => connection.id === profile.sourceConnectionId,
    );
    const targetExists = connections.some(
      (connection) => connection.id === profile.targetConnectionId,
    );
    if (!sourceExists || !targetExists) {
      setErrorMsg(t('sync.profile.missingConnection'));
      setErrorOpen(true);
      return;
    }
    pendingProfileMappingsRef.current = profile.tables;
    setProfileName(profile.name);
    setSourceId(profile.sourceConnectionId);
    setTargetId(profile.targetConnectionId);
    setSourceDatabase(profile.sourceDatabase ?? '');
    setTargetDatabase(profile.targetDatabase ?? '');
    setSourceSchema(profile.sourceSchema ?? '');
    setTargetSchema(profile.targetSchema ?? '');
    setSyncOptions(profile.options);
    resetCompareState();
    setSelectedRows([]);
    setTableSelections([]);
    setStep('endpoints');
    setStatusMsg(t('sync.profile.loaded'));
  }, [
    connections,
    resetCompareState,
    selectedProfileId,
    setSyncOptions,
    setStep,
    syncProfiles,
    t,
  ]);

  const deleteSelectedProfile = useCallback(async () => {
    if (!selectedProfileId) return;
    try {
      await syncCommands.deleteSyncProfile(selectedProfileId);
      setSelectedProfileId('');
      setProfileName('');
      loadSyncProfiles();
      setStatusMsg(t('sync.profile.deleted'));
    } catch (error) {
      setErrorMsg(error instanceof Error ? error.message : String(error));
      setErrorOpen(true);
    }
  }, [loadSyncProfiles, selectedProfileId, t]);

  const reconcileUnknownRun = useCallback(
    async (run: MigrationRunRecord): Promise<boolean> => {
      if (run.operation !== 'dataSync' || run.outcome !== 'unknown') return false;
      if (writeInFlightRef.current) {
        setErrorMsg(t('migrationHistory.syncReconcileBusy'));
        setErrorOpen(true);
        return false;
      }

      const [latestProfiles, latestConnections] = await Promise.all([
        syncCommands.getSyncProfiles(),
        invoke<ConnectionConfig[]>('get_connections'),
      ]);
      setSyncProfiles(latestProfiles);
      setConnections(latestConnections);
      const savedProfile = run.profileId
        ? latestProfiles.find((profile) => profile.id === run.profileId)
        : undefined;
      const sameRevision =
        !!savedProfile &&
        !!run.profileRevision &&
        Date.parse(savedProfile.updatedAt) === Date.parse(run.profileRevision);
      const reusableProfile =
        sameRevision &&
        (!run.sourceConnectionId || run.sourceConnectionId === savedProfile.sourceConnectionId) &&
        (!run.targetConnectionId || run.targetConnectionId === savedProfile.targetConnectionId)
          ? savedProfile
          : undefined;
      const sourceConnectionId =
        run.sourceConnectionId ?? reusableProfile?.sourceConnectionId ?? '';
      const targetConnectionId =
        run.targetConnectionId ?? reusableProfile?.targetConnectionId ?? '';
      const connectionIds = new Set(latestConnections.map((connection) => connection.id));
      if (
        (sourceConnectionId && !connectionIds.has(sourceConnectionId)) ||
        (targetConnectionId && !connectionIds.has(targetConnectionId))
      ) {
        setErrorMsg(t('migrationHistory.syncReconcileMissingConnection'));
        setErrorOpen(true);
        return false;
      }

      let profileScopeCanBeRestored = Boolean(reusableProfile);
      let sourceDatabasesForRestore: string[] = [];
      let targetDatabasesForRestore: string[] = [];
      if (reusableProfile) {
        try {
          const [sourceCatalog, targetCatalog] = await Promise.all([
            listDatabasesDedicated(sourceConnectionId),
            listDatabasesDedicated(targetConnectionId),
          ]);
          sourceDatabasesForRestore = sourceCatalog.databases ?? [];
          targetDatabasesForRestore = targetCatalog.databases ?? [];
          profileScopeCanBeRestored = Boolean(
            reusableProfile.sourceDatabase &&
              reusableProfile.targetDatabase &&
              sourceDatabasesForRestore.includes(reusableProfile.sourceDatabase) &&
              targetDatabasesForRestore.includes(reusableProfile.targetDatabase),
          );
        } catch {
          profileScopeCanBeRestored = false;
        }
      }

      compareGenerationRef.current += 1;
      jobIdRef.current = null;
      jobKindRef.current = null;
      cancelRequestedJobRef.current = null;
      const restoredProfile = profileScopeCanBeRestored ? reusableProfile : undefined;
      pendingProfileMappingsRef.current = restoredProfile?.tables ?? null;
      setSelectedProfileId(restoredProfile?.id ?? '');
      setProfileName(restoredProfile?.name ?? '');
      setSourceId(sourceConnectionId);
      setTargetId(targetConnectionId);
      setSourceDatabase(restoredProfile?.sourceDatabase ?? '');
      setTargetDatabase(restoredProfile?.targetDatabase ?? '');
      setSourceSchema(restoredProfile?.sourceSchema ?? '');
      setTargetSchema(restoredProfile?.targetSchema ?? '');
      setSourceDatabases(sourceDatabasesForRestore);
      setTargetDatabases(targetDatabasesForRestore);
      setSourceSchemas([]);
      setTargetSchemas([]);
      setSyncOptions(restoredProfile?.options ?? DEFAULT_SYNC_OPTIONS);
      resetCompareState();
      setSelectedRows([]);
      setTableSelections([]);
      setPageIndex({});
      setPageCursors({});
      loadedPageRef.current.clear();
      setLastExecutionResult(null);
      setWriteOutcomeUncertain(true);
      setScopeReconfirmationRequired(!restoredProfile);
      setSyncState('unknown');
      setStep('endpoints');
      setStatusMsg(
        restoredProfile
          ? t('migrationHistory.syncReconcileProfileReady')
          : reusableProfile
            ? t('migrationHistory.syncReconcileScopeUnavailable')
            : run.profileId
              ? t('migrationHistory.syncReconcileProfileChanged')
              : t('migrationHistory.syncReconcileNeedsScope'),
      );
      return true;
    },
    [resetCompareState, setStep, setSyncOptions, t],
  );

  useEffect(() => {
    loadSyncProfiles();
  }, [loadSyncProfiles]);

  return {
    syncProfiles,
    profileName,
    setProfileName,
    selectedProfileId,
    setSelectedProfileId,
    loadSyncProfiles,
    saveCurrentProfile,
    loadSelectedProfile,
    deleteSelectedProfile,
    reconcileUnknownRun,
  };
}

export type DataSyncProfiles = ReturnType<typeof useDataSyncProfiles>;
