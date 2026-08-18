import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { ChevronDown, ChevronRight, Database, HardDrive } from 'lucide-react';
import { TitleBar } from '../../components/TitleBar';
import { Button } from '../../components/ui/Button';
import { Input } from '../../components/ui/Input';
import { useSettings } from '../../hooks/useSettings';
import { useI18n } from '../../hooks/useI18n';
import { useConfirmDialog } from '../../hooks/useConfirmDialog';
import { useSettingsStore } from '../../stores/settingsStore';
import { groupConnections } from '../../stores/connectionStore';
import { formatGroupLabel } from '../../lib/connectionGroups';
import {
  backupProgressRatio,
  createProgressLogPump,
  formatBackupProgress,
  formatRestoreProgress,
  type BackupProgressPayload,
} from '../../lib/backupProgress';
import { ProgressLog } from './ProgressLog';
import { cn } from '../../lib/cn';
import { listenCrossWindow } from '../../lib/crossWindowBus';
import { DbTypeBadge } from '../../components/DbTypeBadge';
import { getDbLabel, DB_REGISTRY } from '../../lib/databaseTypes';
import { getSqlDialect } from '../../lib/sqlDialects';
import { getUrlParam } from '../../lib/windowKind';
import type { ConnectionConfig, TableInfo } from '../../types';

interface DatabaseInfo {
  name: string;
}

export function BackupWindow() {
  useSettings();
  const { t } = useI18n();
  const [confirmRestore, confirmRestoreDialog] = useConfirmDialog();
  const loadSettings = useSettingsStore((s) => s.loadSettings);
  const isRestore = getUrlParam('mode') === 'restore';

  const [connections, setConnections] = useState<ConnectionConfig[]>([]);
  const [groups, setGroups] = useState<string[]>([]);
  const [expandedGroups, setExpandedGroups] = useState<Set<string>>(new Set());

  const [selectedConnId, setSelectedConnId] = useState<string | null>(null);
  const [connectedId, setConnectedId] = useState<string | null>(null);
  const [serverVersion, setServerVersion] = useState('');
  const [databases, setDatabases] = useState<DatabaseInfo[]>([]);
  const [selectedDb, setSelectedDb] = useState<string | null>(null);

  const [enabledOptions, setEnabledOptions] = useState<Set<string>>(new Set());
  const [compressGzip, setCompressGzip] = useState(false);
  const [fileName, setFileName] = useState('untitled');
  const [optionDropdownOpen, setOptionDropdownOpen] = useState(false);

  const [backing, setBacking] = useState(false);
  const [statusMessage, setStatusMessage] = useState('');
  const [progress, setProgress] = useState<BackupProgressPayload | null>(null);
  const [progressLog, setProgressLog] = useState<string[]>([]);
  const logPumpRef = useRef(createProgressLogPump(setProgressLog));
  const [searchConn, setSearchConn] = useState('');
  const [searchDb, setSearchDb] = useState('');

  const selectedConn = useMemo(
    () => connections.find((c) => c.id === selectedConnId) ?? null,
    [connections, selectedConnId],
  );

  const backupOptions = useMemo(() => {
    if (!selectedConn) return [];
    const meta = DB_REGISTRY[selectedConn.databaseType];
    if (!meta?.supportsBackup) return [];
    return getSqlDialect(selectedConn.databaseType)?.backupOptions ?? [];
  }, [selectedConn]);

  useEffect(() => {
    void loadSettings();
  }, [loadSettings]);

  useEffect(() => {
    let cleanup: (() => void) | undefined;
    listenCrossWindow('datazen:connection-closed', (payload) => {
      const { connectionId } = (payload ?? {}) as { connectionId?: string };
      if (!connectionId) return;
      setConnectedId((prev) => (prev === connectionId ? null : prev));
    }).then((fn) => {
      cleanup = fn;
    });
    return () => cleanup?.();
  }, []);

  useEffect(() => {
    void (async () => {
      const { invoke } = await import('@tauri-apps/api/core');
      const conns = await invoke<ConnectionConfig[]>('get_connections');
      const grps = await invoke<string[]>('get_groups');
      setConnections(conns);
      setGroups(grps);
      const keys = new Set<string>([...grps, '']);
      for (const c of conns) keys.add(c.group || '');
      setExpandedGroups(keys);
    })();
  }, []);

  const grouped = useMemo(
    () => groupConnections(connections, groups, searchConn),
    [connections, groups, searchConn],
  );

  const handleSelectConnection = useCallback(
    async (conn: ConnectionConfig) => {
      setSelectedConnId(conn.id);
      setSelectedDb(null);
      setDatabases([]);
      setServerVersion('');
      setEnabledOptions(new Set());
      setStatusMessage('');
      setProgress(null);

      if (!DB_REGISTRY[conn.databaseType]?.supportsBackup) {
        setConnectedId(null);
        setStatusMessage(t('backup.unsupportedType'));
        return;
      }

      try {
        const { invoke } = await import('@tauri-apps/api/core');
        const connectionId = await invoke<string>('connect', { configId: conn.id });
        setConnectedId(connectionId);

        try {
          const info = await invoke<{ serverVersion?: string }>('get_connection_info', {
            connectionId,
          });
          if (info.serverVersion) setServerVersion(info.serverVersion);
        } catch {
          /* server version is optional */
        }

        const dbs = await invoke<string[]>('get_databases', { connectionId });
        setDatabases(dbs.map((name) => ({ name })));

        if (conn.database && dbs.includes(conn.database)) {
          setSelectedDb(conn.database);
        }

        const defaults = new Set<string>();
        for (const opt of getSqlDialect(conn.databaseType)?.backupOptions ?? []) {
          if (opt.id === 'routines' || opt.id === 'triggers') {
            defaults.add(opt.id);
          }
        }
        setEnabledOptions(defaults);
      } catch (e) {
        setStatusMessage(e instanceof Error ? e.message : String(e));
      }
    },
    [t],
  );

  const toggleGroup = useCallback((group: string) => {
    setExpandedGroups((prev) => {
      const next = new Set(prev);
      if (next.has(group)) next.delete(group);
      else next.add(group);
      return next;
    });
  }, []);

  const toggleOption = useCallback((optionId: string) => {
    setEnabledOptions((prev) => {
      const next = new Set(prev);
      if (next.has(optionId)) next.delete(optionId);
      else next.add(optionId);
      return next;
    });
  }, []);

  const handleRestore = useCallback(async () => {
    if (!connectedId || !selectedDb) return;
    try {
      const { invoke } = await import('@tauri-apps/api/core');
      await invoke('use_database', { connectionId: connectedId, database: selectedDb });

      const tables = await invoke<TableInfo[]>('get_tables', {
        connectionId: connectedId,
        database: selectedDb,
      }).catch(() => [] as TableInfo[]);

      const options: string[] = [];
      if (tables.length > 0) {
        const ok = await confirmRestore({
          title: t('backup.restoreTitle'),
          message: t('backup.restoreOverwriteConfirm', {
            database: selectedDb,
            count: tables.length,
          }),
          kind: 'warning',
        });
        if (!ok) return;
        options.push('overwrite');
      }

      setBacking(true);
      setProgress(null);
      logPumpRef.current.reset([t('backup.restoring')]);
      setStatusMessage(t('backup.restoring'));

      const { listen } = await import('@tauri-apps/api/event');
      const unlisten = await listen<BackupProgressPayload>('restore-progress', (event) => {
        const line = formatRestoreProgress(event.payload, t);
        setProgress(event.payload);
        setStatusMessage(line);
        logPumpRef.current.push(line);
      });

      try {
        const restored = await invoke<boolean>('restore_database_with_dialog', {
          connectionId: connectedId,
          database: selectedDb,
          options,
        });
        if (!restored) {
          setStatusMessage('');
          setProgress(null);
          logPumpRef.current.reset([]);
          return;
        }
        setStatusMessage(t('backup.restoreSuccess'));
        setProgress({ current: 0, total: 0, objectName: '', phase: 'done' });
        logPumpRef.current.push(t('backup.restoreSuccess'));
        logPumpRef.current.flush();
      } finally {
        unlisten();
        setBacking(false);
      }
    } catch (e) {
      const message = e instanceof Error ? e.message : String(e);
      setStatusMessage(message);
      setProgress(null);
      logPumpRef.current.push(message);
      logPumpRef.current.flush();
      setBacking(false);
    }
  }, [connectedId, selectedDb, t]);

  const handleBackup = useCallback(async () => {
    if (!connectedId || !selectedDb) return;

    try {
      const ext = compressGzip ? 'gz' : enabledOptions.has('format-custom') ? 'dump' : 'sql';
      const defaultName = `${fileName}.${compressGzip ? 'sql.gz' : ext}`;

      setBacking(true);
      setProgress(null);
      logPumpRef.current.reset([t('backup.progressPreparing')]);
      setStatusMessage(t('backup.progressPreparing'));

      const { listen } = await import('@tauri-apps/api/event');
      const unlisten = await listen<BackupProgressPayload>('backup-progress', (event) => {
        const line = formatBackupProgress(event.payload, t);
        setProgress(event.payload);
        setStatusMessage(line);
        logPumpRef.current.push(line);
      });

      try {
        const { invoke } = await import('@tauri-apps/api/core');
        const saved = await invoke<boolean>('backup_database_with_dialog', {
          connectionId: connectedId,
          database: selectedDb,
          defaultFileName: defaultName,
          filterExtension: ext,
          options: Array.from(enabledOptions),
          compress: compressGzip,
        });
        if (!saved) {
          setStatusMessage('');
          setProgress(null);
          logPumpRef.current.reset([]);
          return;
        }

        setStatusMessage(t('backup.success'));
        setProgress({ current: 0, total: 0, objectName: '', phase: 'done' });
        logPumpRef.current.push(t('backup.success'));
        logPumpRef.current.flush();
      } finally {
        unlisten();
      }
    } catch (e) {
      const message = e instanceof Error ? e.message : String(e);
      setStatusMessage(message);
      setProgress(null);
      logPumpRef.current.push(message);
      logPumpRef.current.flush();
    } finally {
      setBacking(false);
    }
  }, [connectedId, selectedDb, fileName, enabledOptions, compressGzip, t]);

  const filteredDbs = useMemo(() => {
    const q = searchDb.trim().toLowerCase();
    if (!q) return databases;
    return databases.filter((d) => d.name.toLowerCase().includes(q));
  }, [databases, searchDb]);

  const dbTypeDisplayLabel = selectedConn
    ? `${getDbLabel(selectedConn.databaseType)}${serverVersion ? ` ${serverVersion}` : ''}`
    : '';

  return (
    <div className="flex h-screen min-h-0 flex-col bg-surface text-fg">
      <TitleBar title={isRestore ? t('backup.restoreTitle') : t('backup.title')} />

      {!isRestore && (
        <div className="flex items-center gap-3 border-b border-edge px-4 py-2">
          <span className="text-xs font-medium text-fg-secondary">{t('backup.fileName')}:</span>
          <Input
            value={fileName}
            onChange={(e) => setFileName(e.target.value)}
            className="h-7 w-48 text-xs"
          />
          <span className="text-[11px] text-fg-muted">{t('backup.fileNameHint')}</span>
        </div>
      )}

      {/* Main content: 3 columns */}
      <div className="flex min-h-0 flex-1">
        {/* Column 1: Connection list */}
        <div className="flex w-[200px] shrink-0 flex-col border-r border-edge">
          <div className="p-2">
            <Input
              placeholder={t('backup.searchConnection')}
              value={searchConn}
              onChange={(e) => setSearchConn(e.target.value)}
              className="h-7 text-xs"
            />
          </div>
          <div className="min-h-0 flex-1 overflow-y-auto">
            {grouped.map(({ group: groupName, connections: groupConns }) => {
              const expanded = expandedGroups.has(groupName);
              const displayName = groupName ? formatGroupLabel(groupName, t) : t('main.ungrouped');
              return (
                <div key={groupName || '__ungrouped'}>
                  <div
                    className="flex cursor-pointer select-none items-center gap-1 px-2 py-1.5 hover:bg-surface-raised/50"
                    onClick={() => toggleGroup(groupName)}
                    data-testid="backup-group-header"
                    data-group={groupName}
                  >
                    {expanded ? (
                      <ChevronDown className="h-3 w-3 shrink-0 text-fg-muted" />
                    ) : (
                      <ChevronRight className="h-3 w-3 shrink-0 text-fg-muted" />
                    )}
                    <HardDrive className="h-3.5 w-3.5 shrink-0 text-orange-400" />
                    <span className="truncate text-xs font-medium">{displayName}</span>
                  </div>
                  {expanded &&
                    groupConns.map((conn) => {
                      const canBackup = Boolean(DB_REGISTRY[conn.databaseType]?.supportsBackup);
                      return (
                        <div
                          key={conn.id}
                          className={cn(
                            'flex cursor-pointer items-center gap-2 py-1.5 pl-7 pr-2 transition-colors',
                            'hover:bg-surface-raised',
                            selectedConnId === conn.id && 'bg-blue-600/20 text-blue-400',
                            !canBackup && 'opacity-50',
                          )}
                          onClick={() => void handleSelectConnection(conn)}
                          data-testid="backup-connection-row"
                        >
                          <DbTypeBadge databaseType={conn.databaseType} size={20} />
                          <div className="min-w-0 flex-1">
                            <div className="truncate text-xs font-medium">
                              {conn.name}
                              {conn.database && (
                                <span className="text-fg-muted"> ({conn.database})</span>
                              )}
                            </div>
                            <div className="truncate text-[10px] text-fg-muted">
                              {conn.host ?? 'localhost'} : {conn.database ?? ''}
                            </div>
                          </div>
                        </div>
                      );
                    })}
                </div>
              );
            })}
          </div>
        </div>

        {/* Column 2: Database list */}
        <div className="flex w-[200px] shrink-0 flex-col border-r border-edge">
          <div className="p-2">
            <Input
              placeholder={t('backup.searchDatabase')}
              value={searchDb}
              onChange={(e) => setSearchDb(e.target.value)}
              className="h-7 text-xs"
            />
          </div>
          <div className="min-h-0 flex-1 overflow-y-auto">
            {filteredDbs.map((db) => (
              <div
                key={db.name}
                className={cn(
                  'flex cursor-pointer items-center gap-2 px-3 py-1.5 transition-colors',
                  'hover:bg-surface-raised',
                  selectedDb === db.name && 'bg-blue-600/20 text-blue-400',
                )}
                onClick={() => setSelectedDb(db.name)}
              >
                <Database className="h-3.5 w-3.5 shrink-0 text-fg-muted" />
                <span className="truncate text-xs">{db.name}</span>
              </div>
            ))}
            {databases.length === 0 && selectedConnId && (
              <div className="p-3 text-center text-xs text-fg-muted">
                {statusMessage || t('backup.selectConnectionFirst')}
              </div>
            )}
            {!selectedConnId && (
              <div className="p-3 text-center text-xs text-fg-muted">
                {t('backup.selectConnectionFirst')}
              </div>
            )}
          </div>
        </div>

        {/* Column 3: Options */}
        <div className="flex min-w-0 flex-1 flex-col p-3">
          {selectedConn && (
            <>
              {/* DB type & version */}
              <div className="mb-3 rounded border border-edge bg-surface-alt px-3 py-2 text-xs text-fg-secondary">
                {dbTypeDisplayLabel}
              </div>

              {!isRestore && (
                <>
                  <div className="relative mb-3">
                    <button
                      type="button"
                      className="flex h-8 w-full items-center justify-between rounded border border-edge bg-surface-alt px-3 text-xs"
                      onClick={() => setOptionDropdownOpen(!optionDropdownOpen)}
                    >
                      <span>{t('backup.addOption')}</span>
                      <ChevronDown className="h-3 w-3" />
                    </button>
                    {optionDropdownOpen && (
                      <div className="absolute top-full z-20 mt-1 w-full rounded border border-edge bg-surface shadow-lg">
                        {backupOptions.map((opt) => (
                          <label
                            key={opt.id}
                            className="flex cursor-pointer items-center gap-2 px-3 py-1.5 text-xs hover:bg-surface-raised"
                          >
                            <input
                              type="checkbox"
                              checked={enabledOptions.has(opt.id)}
                              onChange={() => toggleOption(opt.id)}
                              className="h-3 w-3 rounded"
                            />
                            {opt.label}
                          </label>
                        ))}
                      </div>
                    )}
                  </div>

                  {/* Selected options display */}
                  {enabledOptions.size > 0 && (
                    <div className="mb-3 flex flex-wrap gap-1">
                      {Array.from(enabledOptions).map((id) => {
                        const opt = backupOptions.find((o) => o.id === id);
                        return opt ? (
                          <span
                            key={id}
                            className="inline-flex items-center gap-1 rounded bg-blue-600/20 px-2 py-0.5 text-[11px] text-blue-400"
                          >
                            {opt.label}
                            <button
                              type="button"
                              className="ml-0.5 text-blue-400/60 hover:text-blue-400"
                              onClick={() => toggleOption(id)}
                            >
                              ×
                            </button>
                          </span>
                        ) : null;
                      })}
                    </div>
                  )}

                  {/* Compress */}
                  <label className="mb-4 flex items-center gap-2 text-xs">
                    <input
                      type="checkbox"
                      checked={compressGzip}
                      onChange={(e) => setCompressGzip(e.target.checked)}
                      className="h-3 w-3 rounded"
                    />
                    {t('backup.compressGzip')}
                  </label>
                </>
              )}
            </>
          )}

          {/* Status message — fixed-height single line so the progress bar never jumps */}
          {statusMessage && (
            <div
              className="mb-3 rounded border border-edge bg-surface-alt px-3 py-2 text-xs text-fg-secondary"
              data-testid="backup-status"
            >
              <div className="h-5 truncate leading-5" title={statusMessage}>
                {statusMessage}
              </div>
              {(backing || progress) && (
                <div className="mt-2 h-1.5 overflow-hidden rounded bg-edge">
                  <div
                    className={cn(
                      'h-full rounded bg-blue-500 transition-all',
                      backing && !progress && 'w-1/3 animate-pulse',
                    )}
                    style={
                      progress
                        ? { width: `${Math.round(backupProgressRatio(progress) * 100)}%` }
                        : undefined
                    }
                    data-testid="backup-progress-bar"
                  />
                </div>
              )}
            </div>
          )}

          {progressLog.length > 0 ? (
            <ProgressLog lines={progressLog} />
          ) : (
            <div className="flex-1" />
          )}

          {/* Start backup button */}
          <div className="flex justify-end">
            <Button
              variant="primary"
              disabled={!connectedId || !selectedDb || backing}
              onClick={() => void (isRestore ? handleRestore() : handleBackup())}
              data-testid={isRestore ? 'backup-start-restore' : 'backup-start-backup'}
            >
              {backing
                ? isRestore
                  ? t('backup.restoring')
                  : t('backup.inProgress')
                : isRestore
                  ? t('backup.startRestore')
                  : t('backup.startBackup')}
            </Button>
          </div>
        </div>
      </div>
      {confirmRestoreDialog}
    </div>
  );
}
