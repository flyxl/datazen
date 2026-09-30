import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type Dispatch,
  type SetStateAction,
} from 'react';
import {
  schemaDiffCommands,
  type ColumnTypeOverride,
  type SchemaDiffConfigJson,
  type SchemaDiffDeployResult,
  type SchemaDiffObjectIdentity,
  type SchemaDiffPlan,
  type SchemaDiffProfile,
} from '../../commands/schemaDiff';
import { databaseCommands } from '../../commands/database';
import { fileCommands } from '../../commands/file';
import { useI18n } from '../../hooks/useI18n';
import type { TableSchemaDiff } from '../../types';
import {
  enabledSourceTableNames,
  enabledTargetOnlyTableNames,
  mergeSchemaDiffTablePicks,
  type SchemaDiffTablePick,
} from './schemaDiffTableNames';
import type { UseSchemaDiffEndpointsReturn } from './useSchemaDiffEndpoints';
import { schemaDiffObjectIdentityKey } from './schemaDiffObjectIdentity';
import type { useSchemaDiffUnifiedObjects } from './useSchemaDiffUnifiedObjects';

const SCHEMA_DIFF_OBJECT_KINDS = new Set<SchemaDiffObjectIdentity['kind']>([
  'view',
  'type',
  'sequence',
  'function',
  'procedure',
  'trigger',
]);

function parseObjectSelections(
  value: unknown,
  invalidConfigMessage: string,
): SchemaDiffObjectIdentity[] {
  if (value === undefined) return [];
  if (!Array.isArray(value)) throw new Error(invalidConfigMessage);

  const identities = value.map((candidate) => {
    if (candidate === null || typeof candidate !== 'object' || Array.isArray(candidate)) {
      throw new Error(invalidConfigMessage);
    }
    const object = candidate as Record<string, unknown>;
    if (
      typeof object.kind !== 'string' ||
      !SCHEMA_DIFF_OBJECT_KINDS.has(object.kind as SchemaDiffObjectIdentity['kind']) ||
      typeof object.name !== 'string' ||
      !object.name.trim()
    ) {
      throw new Error(invalidConfigMessage);
    }
    const optionalIdentityPart = (key: string, allowEmpty: boolean): string | null => {
      const part = object[key];
      if (part === undefined || part === null) return null;
      if (typeof part !== 'string' || (!allowEmpty && !part.trim())) {
        throw new Error(invalidConfigMessage);
      }
      return part;
    };

    const identity: SchemaDiffObjectIdentity = {
      kind: object.kind as SchemaDiffObjectIdentity['kind'],
      schema: optionalIdentityPart('schema', false),
      name: object.name,
      signature: optionalIdentityPart('signature', true),
      targetSchema: optionalIdentityPart('targetSchema', false),
      targetName: optionalIdentityPart('targetName', false),
    };
    if (
      (identity.kind === 'function' || identity.kind === 'procedure') &&
      (identity.signature == null || (identity.signature.length > 0 && !identity.signature.trim()))
    ) {
      throw new Error(invalidConfigMessage);
    }
    if (identity.kind === 'trigger' && identity.targetName === null) {
      throw new Error(invalidConfigMessage);
    }
    return identity;
  });
  const keys = identities.map(schemaDiffObjectIdentityKey);
  if (new Set(keys).size !== keys.length) throw new Error(invalidConfigMessage);
  return identities;
}

interface UseSchemaDiffSavedSetupsOptions {
  endpoints: UseSchemaDiffEndpointsReturn;
  tablePicks: SchemaDiffTablePick[];
  setTablePicks: Dispatch<SetStateAction<SchemaDiffTablePick[]>>;
  selectedTables: string[];
  unifiedObjects: ReturnType<typeof useSchemaDiffUnifiedObjects>;
  objectsLoading: boolean;
  setObjectsLoading: Dispatch<SetStateAction<boolean>>;
  allowDestructive: boolean;
  setAllowDestructive: Dispatch<SetStateAction<boolean>>;
  includeIndexes: boolean;
  setIncludeIndexes: Dispatch<SetStateAction<boolean>>;
  requireRollback: boolean;
  setRequireRollback: Dispatch<SetStateAction<boolean>>;
  typeOverrides: ColumnTypeOverride[];
  setTypeOverrides: Dispatch<SetStateAction<ColumnTypeOverride[]>>;
  setStep: (step: 'objects' | 'plan') => void;
  setPlan: Dispatch<SetStateAction<SchemaDiffPlan | null>>;
  setDiffs: Dispatch<SetStateAction<TableSchemaDiff[]>>;
  setDeployResult: Dispatch<SetStateAction<SchemaDiffDeployResult | null>>;
  setError: (message: string) => void;
  planAutoRequestedRef: { current: boolean };
  onConfigExported: () => void;
}

interface SchemaDiffConfigEndpointIdentity {
  sourceConnectionId: string;
  targetConnectionId: string;
  sourceDatabase: string;
  targetDatabase: string;
  sourceSchema: string;
  targetSchema: string;
}

export function useSchemaDiffSavedSetups({
  endpoints,
  tablePicks,
  setTablePicks,
  selectedTables,
  unifiedObjects,
  objectsLoading,
  setObjectsLoading,
  allowDestructive,
  setAllowDestructive,
  includeIndexes,
  setIncludeIndexes,
  requireRollback,
  setRequireRollback,
  typeOverrides,
  setTypeOverrides,
  setStep,
  setPlan,
  setDiffs,
  setDeployResult,
  setError,
  planAutoRequestedRef,
  onConfigExported,
}: UseSchemaDiffSavedSetupsOptions) {
  const { t } = useI18n();
  const [profiles, setProfiles] = useState<SchemaDiffProfile[]>([]);
  const [selectedProfileId, setSelectedProfileId] = useState('');
  const [profileDialogOpen, setProfileDialogOpen] = useState(false);
  const [profileName, setProfileName] = useState('');
  const [profileError, setProfileError] = useState('');
  const [pendingProfileLoad, setPendingProfileLoad] = useState<SchemaDiffProfile | null>(null);
  const [pendingConfigLoad, setPendingConfigLoad] = useState<SchemaDiffConfigJson | null>(null);
  const [importConfigOpen, setImportConfigOpen] = useState(false);
  const [importConfigText, setImportConfigText] = useState('');
  const [importConfigError, setImportConfigError] = useState('');
  const profileLoadEndpointRef = useRef<SchemaDiffProfile | null>(null);
  const configLoadEndpointRef = useRef<SchemaDiffConfigEndpointIdentity | null>(null);
  const endpointStateRef = useRef({ endpoints, pendingProfileLoad, pendingConfigLoad });
  endpointStateRef.current = { endpoints, pendingProfileLoad, pendingConfigLoad };

  const shouldPreserveEndpointChange = useCallback(() => {
    const {
      endpoints: current,
      pendingProfileLoad: pendingProfile,
      pendingConfigLoad: pendingConfig,
    } = endpointStateRef.current;
    const profile = profileLoadEndpointRef.current;
    const config = configLoadEndpointRef.current;
    const endpointMatchesProfile =
      profile &&
      current.sourceId === profile.sourceConnectionId &&
      current.targetId === profile.targetConnectionId &&
      current.sourceDatabase === profile.sourceDatabase &&
      current.targetDatabase === profile.targetDatabase &&
      current.sourceSchema === (profile.sourceSchema ?? '') &&
      current.targetSchema === (profile.targetSchema ?? '');
    const endpointMatchesConfig =
      config &&
      current.sourceId === config.sourceConnectionId &&
      current.targetId === config.targetConnectionId &&
      current.sourceDatabase === config.sourceDatabase &&
      current.targetDatabase === config.targetDatabase &&
      current.sourceSchema === config.sourceSchema &&
      current.targetSchema === config.targetSchema;
    const pendingConfigConnectionChange =
      pendingConfig &&
      (current.sourceId !== pendingConfig.sourceConnectionId ||
        current.targetId !== pendingConfig.targetConnectionId);
    if (
      pendingProfile ||
      pendingConfigConnectionChange ||
      endpointMatchesProfile ||
      endpointMatchesConfig
    ) {
      if (endpointMatchesProfile) profileLoadEndpointRef.current = null;
      if (endpointMatchesConfig) configLoadEndpointRef.current = null;
      return true;
    }
    if (config) configLoadEndpointRef.current = null;
    return false;
  }, []);

  const refreshProfiles = useCallback(async () => {
    try {
      setProfiles(await schemaDiffCommands.getProfiles());
    } catch (e) {
      setProfiles([]);
      setError(e instanceof Error ? e.message : String(e));
    }
  }, [setError]);

  useEffect(() => {
    void refreshProfiles();
  }, [refreshProfiles]);

  const inspectImportedObjects = useCallback(
    async (config: SchemaDiffConfigJson) => {
      try {
        const srcConnId = await endpoints.ensureConnected('source');
        const tgtConnId = await endpoints.ensureConnected('target');
        if (!srcConnId || !tgtConnId) return;
        const catalog = await unifiedObjects.load(
          srcConnId,
          tgtConnId,
          endpoints.sourceSchema,
          endpoints.targetSchema,
        );
        if (!catalog) return;
        const missing = unifiedObjects.restoreSelection(
          catalog,
          config.sourceObjects ?? [],
          config.targetObjects ?? [],
        );
        const missingCount = missing.missingSource + missing.missingTarget;
        if (missingCount > 0) {
          setError(t('schemaDiff.savedObjectsMissing', { count: missingCount }));
        }
      } catch (e) {
        setError(e instanceof Error ? e.message : String(e));
      } finally {
        setObjectsLoading(false);
      }
    },
    [endpoints, setError, t, unifiedObjects.load, unifiedObjects.restoreSelection],
  );

  const applyImportedConfig = useCallback(
    (text: string) => {
      setImportConfigError('');
      try {
        const cfg = JSON.parse(text) as SchemaDiffConfigJson;
        if (cfg.version !== 2 || !cfg.sourceConnectionId || !cfg.targetConnectionId) {
          throw new Error(t('schemaDiff.invalidConfig'));
        }
        const importedConfig: SchemaDiffConfigJson = {
          ...cfg,
          sourceObjects: parseObjectSelections(cfg.sourceObjects, t('schemaDiff.invalidConfig')),
          targetObjects: parseObjectSelections(cfg.targetObjects, t('schemaDiff.invalidConfig')),
        };
        configLoadEndpointRef.current = {
          sourceConnectionId: importedConfig.sourceConnectionId,
          targetConnectionId: importedConfig.targetConnectionId,
          sourceDatabase: endpoints.sourceDatabase,
          targetDatabase: endpoints.targetDatabase,
          sourceSchema: endpoints.sourceSchema,
          targetSchema: endpoints.targetSchema,
        };
        endpoints.setSourceId(cfg.sourceConnectionId);
        endpoints.setTargetId(cfg.targetConnectionId);
        const targetOnly = new Set(cfg.targetOnlyTables ?? []);
        setTablePicks(
          (cfg.tables ?? []).map((name) =>
            targetOnly.has(name)
              ? { name, enabled: true, origin: 'target-only' as const, targetName: name }
              : { name, enabled: true, origin: 'source-only' as const, sourceName: name },
          ),
        );
        unifiedObjects.clear();
        setObjectsLoading(true);
        setError('');
        setAllowDestructive(Boolean(cfg.allowDestructive));
        setIncludeIndexes(cfg.includeIndexes ?? true);
        setRequireRollback(Boolean(cfg.requireRollback));
        setPlan(null);
        setDiffs([]);
        setDeployResult(null);
        planAutoRequestedRef.current = false;
        setStep('objects');
        setPendingConfigLoad(importedConfig);
        setImportConfigOpen(false);
        setImportConfigText('');
      } catch (e) {
        setImportConfigError(e instanceof Error ? e.message : String(e));
      }
    },
    [
      endpoints,
      setAllowDestructive,
      setDeployResult,
      setDiffs,
      setError,
      setIncludeIndexes,
      setPlan,
      setRequireRollback,
      setStep,
      setTablePicks,
      t,
      unifiedObjects.clear,
    ],
  );

  useEffect(() => {
    const config = pendingConfigLoad;
    if (
      !config ||
      endpoints.sourceId !== config.sourceConnectionId ||
      endpoints.targetId !== config.targetConnectionId
    ) {
      return;
    }
    setPendingConfigLoad(null);
    void inspectImportedObjects(config);
  }, [endpoints, inspectImportedObjects, pendingConfigLoad]);

  const inspectProfileSource = useCallback(
    async (profile: SchemaDiffProfile) => {
      setObjectsLoading(true);
      setError('');
      unifiedObjects.clear();
      try {
        const srcConnId = await endpoints.ensureConnected('source');
        const tgtConnId = await endpoints.ensureConnected('target');
        if (!srcConnId || !tgtConnId) return;
        const [sourceRows, targetRows] = await Promise.all([
          databaseCommands.listTables(srcConnId, profile.sourceDatabase),
          databaseCommands.listTables(tgtConnId, profile.targetDatabase),
        ]);
        const catalog = await unifiedObjects.load(
          srcConnId,
          tgtConnId,
          profile.sourceSchema ?? '',
          profile.targetSchema ?? '',
        );
        if (catalog) {
          const missing = unifiedObjects.restoreSelection(
            catalog,
            profile.sourceObjects ?? [],
            profile.targetObjects ?? [],
          );
          const missingCount = missing.missingSource + missing.missingTarget;
          if (missingCount > 0) {
            setError(t('schemaDiff.savedObjectsMissing', { count: missingCount }));
          }
        }
        const selected = new Set([...profile.tables, ...(profile.targetOnlyTables ?? [])]);
        const picks = mergeSchemaDiffTablePicks(
          sourceRows,
          targetRows,
          profile.sourceSchema || undefined,
          profile.targetSchema || undefined,
        )
          .map((row) => ({
            ...row,
            enabled: [row.name, row.sourceName, row.targetName].some(
              (name) => name !== undefined && selected.has(name),
            ),
          }))
          .filter((row) => row.enabled);
        setTablePicks(picks);
        setStep('objects');
      } catch (e) {
        setError(e instanceof Error ? e.message : String(e));
        setTablePicks([]);
      } finally {
        setObjectsLoading(false);
      }
    },
    [
      endpoints,
      setError,
      setTablePicks,
      setStep,
      t,
      unifiedObjects.clear,
      unifiedObjects.load,
      unifiedObjects.restoreSelection,
    ],
  );

  useEffect(() => {
    const profile = pendingProfileLoad;
    if (!profile) return;
    if (
      endpoints.sourceId !== profile.sourceConnectionId ||
      endpoints.targetId !== profile.targetConnectionId ||
      endpoints.sourceDatabase !== profile.sourceDatabase ||
      endpoints.targetDatabase !== profile.targetDatabase ||
      endpoints.sourceSchema !== (profile.sourceSchema ?? '') ||
      endpoints.targetSchema !== (profile.targetSchema ?? '')
    ) {
      return;
    }
    setPendingProfileLoad(null);
    void inspectProfileSource(profile);
  }, [endpoints, inspectProfileSource, pendingProfileLoad]);

  const handleLoadProfile = useCallback(() => {
    const profile = profiles.find((item) => item.id === selectedProfileId);
    if (!profile) return;
    profileLoadEndpointRef.current = profile;
    endpoints.setSourceId(profile.sourceConnectionId);
    endpoints.setTargetId(profile.targetConnectionId);
    endpoints.setSourceDatabase(profile.sourceDatabase);
    endpoints.setTargetDatabase(profile.targetDatabase);
    endpoints.setSourceSchema(profile.sourceSchema ?? '');
    endpoints.setTargetSchema(profile.targetSchema ?? '');
    setTablePicks([
      ...profile.tables.map((name) => ({
        name,
        enabled: true,
        origin: 'source-only' as const,
        sourceName: name,
      })),
      ...(profile.targetOnlyTables ?? []).map((name) => ({
        name,
        enabled: true,
        origin: 'target-only' as const,
        targetName: name,
      })),
    ]);
    unifiedObjects.clear();
    setAllowDestructive(profile.allowDestructive);
    setIncludeIndexes(profile.includeIndexes);
    setRequireRollback(profile.requireRollback);
    setTypeOverrides(profile.typeOverrides ?? []);
    setPlan(null);
    setDiffs([]);
    setDeployResult(null);
    planAutoRequestedRef.current = false;
    setPendingProfileLoad(profile);
    setError('');
  }, [
    endpoints,
    profiles,
    selectedProfileId,
    setAllowDestructive,
    setDeployResult,
    setDiffs,
    setError,
    setIncludeIndexes,
    setPlan,
    setRequireRollback,
    setTablePicks,
    setTypeOverrides,
    unifiedObjects.clear,
  ]);

  const handleSaveProfile = useCallback(async () => {
    const name = profileName.trim();
    if (!name) {
      setProfileError(t('schemaDiff.profileNameRequired'));
      return;
    }
    let sourceObjects: SchemaDiffObjectIdentity[];
    let targetObjects: SchemaDiffObjectIdentity[];
    try {
      sourceObjects = parseObjectSelections(
        unifiedObjects.selectedSourceObjects,
        t('schemaDiff.objectIdentityIncomplete'),
      );
      targetObjects = parseObjectSelections(
        unifiedObjects.selectedTargetObjects,
        t('schemaDiff.objectIdentityIncomplete'),
      );
    } catch (e) {
      setProfileError(e instanceof Error ? e.message : String(e));
      return;
    }
    if (
      !endpoints.validateEndpoints() ||
      (selectedTables.length === 0 && sourceObjects.length === 0 && targetObjects.length === 0)
    ) {
      setProfileError(t('schemaDiff.profileSetupRequired'));
      return;
    }
    const existing = profiles.find((item) => item.id === selectedProfileId);
    const now = new Date().toISOString();
    const profile: SchemaDiffProfile = {
      version: 1,
      id: existing?.id ?? `schema-diff-${Date.now()}`,
      name,
      sourceConnectionId: endpoints.sourceId,
      targetConnectionId: endpoints.targetId,
      sourceDatabase: endpoints.sourceDatabase,
      targetDatabase: endpoints.targetDatabase,
      sourceSchema: endpoints.sourceSchema || null,
      targetSchema: endpoints.targetSchema || null,
      tables: enabledSourceTableNames(tablePicks),
      targetOnlyTables: enabledTargetOnlyTableNames(tablePicks),
      sourceObjects,
      targetObjects,
      allowDestructive,
      includeIndexes,
      requireRollback,
      typeOverrides,
      createdAt: existing?.createdAt ?? now,
      updatedAt: now,
    };
    try {
      await schemaDiffCommands.saveProfile(profile);
      await refreshProfiles();
      setSelectedProfileId(profile.id);
      setProfileDialogOpen(false);
      setProfileName('');
      setProfileError('');
    } catch (e) {
      setProfileError(e instanceof Error ? e.message : String(e));
    }
  }, [
    allowDestructive,
    endpoints,
    includeIndexes,
    profileName,
    profiles,
    refreshProfiles,
    requireRollback,
    selectedProfileId,
    selectedTables,
    tablePicks,
    t,
    typeOverrides,
    unifiedObjects.selectedSourceObjects,
    unifiedObjects.selectedTargetObjects,
  ]);

  const handleDeleteProfile = useCallback(async () => {
    if (!selectedProfileId) return;
    try {
      await schemaDiffCommands.deleteProfile(selectedProfileId);
      setSelectedProfileId('');
      await refreshProfiles();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }, [refreshProfiles, selectedProfileId, setError]);

  const handleExportConfig = useCallback(async () => {
    let sourceObjects: SchemaDiffObjectIdentity[];
    let targetObjects: SchemaDiffObjectIdentity[];
    try {
      sourceObjects = parseObjectSelections(
        unifiedObjects.selectedSourceObjects,
        t('schemaDiff.objectIdentityIncomplete'),
      );
      targetObjects = parseObjectSelections(
        unifiedObjects.selectedTargetObjects,
        t('schemaDiff.objectIdentityIncomplete'),
      );
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      return;
    }
    const config: SchemaDiffConfigJson = {
      version: 2,
      sourceConnectionId: endpoints.sourceId,
      targetConnectionId: endpoints.targetId,
      tables: selectedTables,
      targetOnlyTables: enabledTargetOnlyTableNames(tablePicks),
      sourceObjects,
      targetObjects,
      allowDestructive,
      includeIndexes,
      requireRollback,
    };
    try {
      const saved = await fileCommands.saveTextWithDialog(
        JSON.stringify(config, null, 2),
        'schema-diff-config.json',
        'JSON',
        ['json'],
      );
      if (saved) onConfigExported();
    } catch {
      setError(t('schemaDiff.exportConfigFailed'));
    }
  }, [
    allowDestructive,
    endpoints,
    includeIndexes,
    onConfigExported,
    selectedTables,
    setError,
    tablePicks,
    t,
    unifiedObjects.selectedSourceObjects,
    unifiedObjects.selectedTargetObjects,
    requireRollback,
  ]);

  const openImportConfig = useCallback(() => {
    setError('');
    setImportConfigText('');
    setImportConfigError('');
    setImportConfigOpen(true);
  }, [setError]);

  const openProfileSave = useCallback(() => {
    const selected = profiles.find((profile) => profile.id === selectedProfileId);
    setProfileName(selected?.name ?? '');
    setProfileError('');
    setProfileDialogOpen(true);
  }, [profiles, selectedProfileId]);

  return {
    profiles,
    selectedProfileId,
    setSelectedProfileId,
    profileDialogOpen,
    setProfileDialogOpen,
    profileName,
    setProfileName,
    profileError,
    setProfileError,
    importConfigOpen,
    setImportConfigOpen,
    importConfigText,
    setImportConfigText,
    importConfigError,
    setImportConfigError,
    objectsLoading,
    shouldPreserveEndpointChange,
    applyImportedConfig,
    openImportConfig,
    openProfileSave,
    handleExportConfig,
    handleLoadProfile,
    handleSaveProfile,
    handleDeleteProfile,
  };
}
