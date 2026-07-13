import { useCallback, useEffect, useMemo, useState } from 'react';
import { ChevronDown, ChevronRight, Database, HardDrive } from 'lucide-react';
import { TitleBar } from '../../components/TitleBar';
import { Button } from '../../components/ui/Button';
import { Input } from '../../components/ui/Input';
import { useI18n } from '../../hooks/useI18n';
import { cn } from '../../lib/cn';
import { getDbLabel, getDbIcon, getDbIconColor, DB_REGISTRY } from '../../lib/databaseTypes';
import { getSqlDialect } from '../../lib/sqlDialects';
import type { ConnectionConfig, DatabaseType } from '../../types';

interface DatabaseInfo {
  name: string;
}

function getDbIconCompact(dbType: DatabaseType) {
  return getDbIconColor(dbType);
}

function getDbShortLabel(dbType: DatabaseType) {
  return getDbIcon(dbType).label;
}

export function BackupWindow() {
  const { t } = useI18n();

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
    void (async () => {
      const { invoke } = await import('@tauri-apps/api/core');
      const conns = await invoke<ConnectionConfig[]>('get_connections');
      const grps = await invoke<string[]>('get_groups');
      setConnections(conns);
      setGroups(grps);
    })();
  }, []);

  const grouped = useMemo(() => {
    const q = searchConn.trim().toLowerCase();
    const filtered = connections.filter((c) => {
      if (!DB_REGISTRY[c.databaseType]?.supportsBackup) return false;
      if (!q) return true;
      return `${c.name} ${c.host ?? ''} ${c.database ?? ''}`.toLowerCase().includes(q);
    });
    const map = new Map<string, ConnectionConfig[]>();
    for (const g of groups) map.set(g, []);
    for (const c of filtered) {
      const key = c.group || '';
      if (!map.has(key)) map.set(key, []);
      map.get(key)!.push(c);
    }
    const result: { group: string; connections: ConnectionConfig[] }[] = [];
    for (const g of groups) {
      const conns = map.get(g) ?? [];
      if (conns.length > 0) result.push({ group: g, connections: conns });
    }
    const ungrouped = map.get('');
    if (ungrouped && ungrouped.length > 0) {
      result.push({ group: '', connections: ungrouped });
    }
    return result;
  }, [connections, groups, searchConn]);

  const handleSelectConnection = useCallback(async (conn: ConnectionConfig) => {
    setSelectedConnId(conn.id);
    setSelectedDb(null);
    setDatabases([]);
    setServerVersion('');
    setEnabledOptions(new Set());
    setStatusMessage('');

    try {
      const { invoke } = await import('@tauri-apps/api/core');
      const connectionId = await invoke<string>('connect', { configId: conn.id });
      setConnectedId(connectionId);

      try {
        const info = await invoke<{ serverVersion?: string }>('get_connection_info', { connectionId });
        if (info.serverVersion) setServerVersion(info.serverVersion);
      } catch { /* server version is optional */ }

      const dbs = await invoke<string[]>('get_databases', { connectionId });
      setDatabases(dbs.map((name) => ({ name })));

      if (conn.database && dbs.includes(conn.database)) {
        setSelectedDb(conn.database);
      }
    } catch (e) {
      setStatusMessage(e instanceof Error ? e.message : String(e));
    }
  }, []);

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

  const handleBackup = useCallback(async () => {
    if (!connectedId || !selectedDb) return;

    try {
      const { save } = await import('@tauri-apps/plugin-dialog');
      const ext = compressGzip ? 'sql.gz' : (enabledOptions.has('format-custom') ? 'dump' : 'sql');
      const defaultName = `${fileName}.${ext}`;
      const path = await save({
        title: t('backup.title'),
        defaultPath: defaultName,
        filters: [{ name: 'Backup Files', extensions: [ext] }],
      });
      if (!path) return;

      setBacking(true);
      setStatusMessage(t('backup.inProgress'));

      const { invoke } = await import('@tauri-apps/api/core');
      await invoke('backup_database', {
        connectionId: connectedId,
        database: selectedDb,
        outputPath: path,
        options: Array.from(enabledOptions),
        compress: compressGzip,
      });

      setStatusMessage(`${t('backup.success')}: ${path}`);
    } catch (e) {
      setStatusMessage(e instanceof Error ? e.message : String(e));
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
      <TitleBar title={t('backup.title')} />

      {/* File name row */}
      <div className="flex items-center gap-3 border-b border-edge px-4 py-2">
        <span className="text-xs font-medium text-fg-secondary">{t('backup.fileName')}:</span>
        <Input
          value={fileName}
          onChange={(e) => setFileName(e.target.value)}
          className="h-7 w-48 text-xs"
        />
        <span className="text-[11px] text-fg-muted">{t('backup.fileNameHint')}</span>
      </div>

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
              const displayName = groupName || t('main.ungrouped');
              return (
                <div key={groupName}>
                  <div
                    className="flex cursor-pointer select-none items-center gap-1 px-2 py-1.5 hover:bg-surface-raised/50"
                    onClick={() => toggleGroup(groupName)}
                  >
                    {expanded ? (
                      <ChevronDown className="h-3 w-3 shrink-0 text-fg-muted" />
                    ) : (
                      <ChevronRight className="h-3 w-3 shrink-0 text-fg-muted" />
                    )}
                    <HardDrive className="h-3.5 w-3.5 shrink-0 text-orange-400" />
                    <span className="truncate text-xs font-medium">{displayName}</span>
                  </div>
                  {expanded && groupConns.map((conn) => (
                    <div
                      key={conn.id}
                      className={cn(
                        'flex cursor-pointer items-center gap-2 py-1.5 pl-7 pr-2 transition-colors',
                        'hover:bg-surface-raised',
                        selectedConnId === conn.id && 'bg-blue-600/20 text-blue-400',
                      )}
                      onClick={() => void handleSelectConnection(conn)}
                    >
                      <span className={cn('flex h-5 w-5 items-center justify-center rounded text-[9px] font-bold', getDbIconCompact(conn.databaseType))}>
                        {getDbShortLabel(conn.databaseType)}
                      </span>
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
                  ))}
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

              {/* Options dropdown */}
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

          {/* Status message */}
          {statusMessage && (
            <div className="mb-3 rounded border border-edge bg-surface-alt px-3 py-2 text-xs text-fg-secondary">
              {statusMessage}
            </div>
          )}

          <div className="flex-1" />

          {/* Start backup button */}
          <div className="flex justify-end">
            <Button
              variant="primary"
              disabled={!connectedId || !selectedDb || backing}
              onClick={() => void handleBackup()}
            >
              {backing ? t('backup.inProgress') : t('backup.startBackup')}
            </Button>
          </div>
        </div>
      </div>
    </div>
  );
}
