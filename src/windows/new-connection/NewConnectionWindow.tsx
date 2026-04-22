import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import type { KeyboardEvent } from 'react';
import type { TranslationKey } from '../../locales';
import {
  ChevronDown,
  ChevronRight,
  Database,
  FileKey2,
  KeyRound,
} from 'lucide-react';
import { TitleBar } from '../../components/TitleBar';
import { Input } from '../../components/ui/Input';
import { Select } from '../../components/ui/Select';
import { Button } from '../../components/ui/Button';
import { useConnectionStore } from '../../stores/connectionStore';
import { useSettingsStore } from '../../stores/settingsStore';
import { useThemeListener } from '../../hooks/useThemeListener';
import { useI18n } from '../../hooks/useI18n';
import { cn } from '../../lib/cn';
import { getUrlParam } from '../../lib/windowKind';
import { DB_REGISTRY } from '../../lib/databaseTypes';
import type { ConnectionConfig, DatabaseType, SslMode, SshTunnelConfig } from '../../types';

function newId() {
  return `conn_${Math.random().toString(36).slice(2, 10)}`;
}

const DB_TYPES: { value: DatabaseType; label: string; color: string }[] = (
  Object.entries(DB_REGISTRY) as [DatabaseType, (typeof DB_REGISTRY)[DatabaseType]][]
).map(([value, meta]) => ({
  value,
  label: meta.label,
  color: meta.iconBg,
}));

const COLOR_KEYS = [
  { value: '#ef4444', key: 'newConn.colorRed' },
  { value: '#f59e0b', key: 'newConn.colorOrange' },
  { value: '#22c55e', key: 'newConn.colorGreen' },
  { value: '#3b82f6', key: 'newConn.colorBlue' },
  { value: '#8b5cf6', key: 'newConn.colorPurple' },
  { value: '#ec4899', key: 'newConn.colorPink' },
  { value: '#64748b', key: 'newConn.colorGray' },
] as const;

function Label({ children, required }: { children: React.ReactNode; required?: boolean }) {
  return (
    <div className="mb-1 text-xs text-fg-secondary">
      {children}
      {required && <span className="ml-0.5 text-red-400">*</span>}
    </div>
  );
}

function closeWindow() {
  if ('__TAURI_INTERNALS__' in window) {
    import('@tauri-apps/api/window').then(({ getCurrentWindow }) => {
      void getCurrentWindow().close();
    });
  } else {
    window.close();
  }
}

export function NewConnectionWindow() {
  useThemeListener();
  const { t } = useI18n();

  const loadSettings = useSettingsStore((s) => s.loadSettings);
  useEffect(() => { void loadSettings(); }, [loadSettings]);

  const saveConnection = useConnectionStore((s) => s.saveConnection);
  const fetchConnections = useConnectionStore((s) => s.fetchConnections);
  const connections = useConnectionStore((s) => s.connections);
  const fetchGroups = useConnectionStore((s) => s.fetchGroups);
  const groups = useConnectionStore((s) => s.groups);

  const [editId] = useState(() => getUrlParam('editId'));
  const [loaded, setLoaded] = useState(false);

  const [databaseType, setDatabaseType] = useState<DatabaseType>('postgresql');
  const [name, setName] = useState('');
  const [host, setHost] = useState('127.0.0.1');
  const [port, setPort] = useState('5432');
  const [database, setDatabase] = useState('');
  const [username, setUsername] = useState('');
  const [password, setPassword] = useState('');
  const [sslMode, setSslMode] = useState<SslMode>('prefer');
  const [group, setGroup] = useState<string>(() => t('newConn.defaultGroup'));
  const [colorTag, setColorTag] = useState<string>('#3b82f6');

  const [showAdvanced, setShowAdvanced] = useState(false);

  useEffect(() => {
    void fetchConnections();
    void fetchGroups();
  }, [fetchConnections, fetchGroups]);

  useEffect(() => {
    if (!editId || loaded || connections.length === 0) return;
    const existing = connections.find((c) => c.id === editId);
    if (!existing) return;
    setDatabaseType(existing.databaseType);
    setName(existing.name);
    setHost(existing.host ?? '127.0.0.1');
    setPort(String(existing.port ?? (DB_REGISTRY[existing.databaseType].defaultPort || '')));
    setDatabase(existing.database ?? '');
    setUsername(existing.username ?? '');
    setPassword(existing.password ?? '');
    setSslMode(existing.sslMode);
    setGroup(existing.group ?? '');
    setColorTag(existing.colorTag ?? '#3b82f6');
    if (existing.sshTunnel?.enabled) {
      setSshEnabled(true);
      setSshHost(existing.sshTunnel.host);
      setSshPort(String(existing.sshTunnel.port));
      setSshUsername(existing.sshTunnel.username);
      setSshAuthMethod(existing.sshTunnel.authMethod);
      setSshPassword(existing.sshTunnel.password ?? '');
      setSshKeyPath(existing.sshTunnel.privateKeyPath ?? '');
      setSshPassphrase(existing.sshTunnel.passphrase ?? '');
    }
    setShowAdvanced(true);
    setLoaded(true);
  }, [editId, loaded, connections]);

  const [sshEnabled, setSshEnabled] = useState(false);
  const [sshHost, setSshHost] = useState('');
  const [sshPort, setSshPort] = useState('22');
  const [sshUsername, setSshUsername] = useState('');
  const [sshAuthMethod, setSshAuthMethod] = useState<'password' | 'private_key'>('password');
  const [sshPassword, setSshPassword] = useState('');
  const [sshKeyPath, setSshKeyPath] = useState('');
  const [sshPassphrase, setSshPassphrase] = useState('');

  const [testing, setTesting] = useState(false);
  const [testOk, setTestOk] = useState<string | null>(null);
  const [testErr, setTestErr] = useState<string | null>(null);
  const testResultRef = useRef<HTMLDivElement>(null);

  const tabFill = useCallback(
    (setter: (v: string) => void) =>
      (e: KeyboardEvent<HTMLInputElement>) => {
        if (e.key === 'Tab' && !e.currentTarget.value && e.currentTarget.placeholder) {
          e.preventDefault();
          setter(e.currentTarget.placeholder);
        }
      },
    [],
  );

  function handleDatabaseTypeChange(newType: DatabaseType) {
    setDatabaseType(newType);
    const meta = DB_REGISTRY[newType];
    setPort(meta.defaultPort ? String(meta.defaultPort) : '');
    if (!meta.supportsSSH) {
      setSshEnabled(false);
    }
  }

  const sshTunnel: SshTunnelConfig | undefined = sshEnabled
    ? {
        enabled: true,
        host: sshHost,
        port: Number(sshPort) || 22,
        username: sshUsername,
        authMethod: sshAuthMethod,
        password: sshAuthMethod === 'password' ? sshPassword || undefined : undefined,
        privateKeyPath: sshAuthMethod === 'private_key' ? sshKeyPath || undefined : undefined,
        passphrase: sshAuthMethod === 'private_key' ? sshPassphrase || undefined : undefined,
      }
    : undefined;

  const draft = useMemo((): ConnectionConfig => {
    const base: ConnectionConfig = {
      id: editId ?? newId(),
      name: name || t('newConn.unnamed'),
      databaseType: databaseType,
      sslMode: sslMode,
      group: group || undefined,
      colorTag: colorTag || undefined,
      sshTunnel: sshTunnel,
    };

    if (DB_REGISTRY[databaseType].connectionMode === 'file') {
      return { ...base, database };
    }

    return {
      ...base,
      host: host || DB_REGISTRY[databaseType].defaultHost || undefined,
      port: Number(port) || DB_REGISTRY[databaseType].defaultPort || undefined,
      database: database || undefined,
      username: username || DB_REGISTRY[databaseType].defaultUser || undefined,
      password: password || undefined,
    };
  }, [colorTag, database, databaseType, group, host, name, password, port, sslMode, sshTunnel, t, username]);

  async function onTest() {
    setTesting(true);
    setTestOk(null);
    setTestErr(null);
    try {
      const info = await useConnectionStore.getState().testConnection(draft);
      setTestOk(info.serverVersion);
    } catch (e) {
      setTestErr(typeof e === 'string' ? e : e instanceof Error ? e.message : t('newConn.testFailed'));
    } finally {
      setTesting(false);
      setTimeout(() => {
        testResultRef.current?.scrollIntoView({ behavior: 'smooth', block: 'end' });
      }, 50);
    }
  }

  async function onSave() {
    await saveConnection(draft);
    closeWindow();
  }

  const isFileMode = DB_REGISTRY[databaseType].connectionMode === 'file';
  const isSqlite = isFileMode;

  const sslOptions = useMemo(
    () => [
      { value: 'disable', label: `Disable — ${t('newConn.sslNone')}` },
      { value: 'prefer', label: `Prefer — ${t('newConn.sslPrefer')}` },
      { value: 'require', label: `Require — ${t('newConn.sslRequire')}` },
    ],
    [t],
  );

  return (
    <div className="flex h-screen min-h-0 flex-col bg-surface-alt text-fg">
      {/* Title bar */}
      <TitleBar title={editId ? t('newConn.editTitle') : t('newConn.title')} />

      <div className="flex min-h-0 flex-1">
        {/* DB type selector */}
        <aside className="flex w-[200px] shrink-0 flex-col border-r border-edge bg-surface p-4">
          <div className="mb-3 text-[11px] font-semibold uppercase tracking-wider text-fg-muted">
            {t('newConn.selectDbType')}
          </div>
          <div className="flex flex-col gap-1.5">
            {DB_TYPES.map((db) => (
              <button
                key={db.value}
                type="button"
                onClick={() => handleDatabaseTypeChange(db.value)}
                className={cn(
                  'flex items-center gap-3 rounded-lg px-3 py-3 text-left text-sm transition-colors',
                  databaseType === db.value
                    ? 'bg-surface-raised text-fg'
                    : 'text-fg-secondary hover:bg-surface-alt hover:text-fg',
                )}
              >
                <div
                  className="flex h-8 w-8 items-center justify-center rounded"
                  style={{ backgroundColor: `${db.color}20` }}
                >
                  <Database className="h-4 w-4" style={{ color: db.color }} />
                </div>
                <div>
                  <div className="font-medium">{db.label}</div>
                </div>
              </button>
            ))}
          </div>
        </aside>

        {/* Form */}
        <main className="flex min-w-0 flex-1 flex-col">
          <div className="flex-1 overflow-y-auto p-6">
            <div className="mb-4 text-[11px] font-semibold uppercase tracking-wider text-fg-muted">
              {t('newConn.connectionConfig')}
            </div>

            <div className="grid grid-cols-1 gap-3 md:grid-cols-2">
              <div className="md:col-span-2">
                <Label required>{t('newConn.connName')}</Label>
                <Input value={name} onChange={(e) => setName(e.target.value)} placeholder={t('newConn.namePlaceholder')} autoFocus />
              </div>

              {isSqlite ? (
                <div className="md:col-span-2">
                  <Label required>{t('newConn.dbFilePath')}</Label>
                  <Input value={database} onChange={(e) => setDatabase(e.target.value)} placeholder="/path/to/db.sqlite" />
                </div>
              ) : (
                <>
                  <div>
                    <Label required>{t('newConn.host')}</Label>
                    <Input value={host} onChange={(e) => setHost(e.target.value)} placeholder="prod-db.example.com" />
                  </div>
                  <div>
                    <Label required>{t('newConn.port')}</Label>
                    <Input value={port} onChange={(e) => setPort(e.target.value)} />
                  </div>
                  <div className="md:col-span-2">
                    <Label>{t('newConn.database')}</Label>
                    <Input value={database} onChange={(e) => setDatabase(e.target.value)} placeholder="myapp_production" />
                  </div>
                  <div>
                    <Label>{t('newConn.username')}</Label>
                    <Input value={username} onChange={(e) => setUsername(e.target.value)} placeholder="postgres" />
                  </div>
                  <div>
                    <Label>{t('newConn.password')}</Label>
                    <Input type="password" value={password} onChange={(e) => setPassword(e.target.value)} />
                  </div>
                </>
              )}
            </div>

            {/* Advanced settings */}
            <button
              type="button"
              className="mt-5 flex w-full items-center gap-2 rounded-md border border-edge bg-surface px-3 py-2.5 text-sm text-fg-secondary hover:text-fg"
              onClick={() => setShowAdvanced((v) => !v)}
            >
              {showAdvanced ? <ChevronDown className="h-4 w-4" /> : <ChevronRight className="h-4 w-4" />}
              {t('newConn.advanced')}
              {sshEnabled && (
                <span className="ml-auto rounded bg-blue-500/20 px-1.5 py-0.5 text-xs text-blue-400">SSH</span>
              )}
            </button>

            {showAdvanced && (
              <div className="mt-3 space-y-4 rounded-md border border-edge bg-surface p-4">
                {/* SSH tunnel */}
                {!isSqlite && (
                  <div>
                    <label className="flex items-center gap-2 text-sm text-fg-secondary">
                      <input
                        type="checkbox"
                        checked={sshEnabled}
                        onChange={(e) => setSshEnabled(e.target.checked)}
                        className="h-4 w-4 rounded border-edge bg-surface text-blue-500 focus:ring-blue-500/25"
                      />
                      {t('newConn.sshTunnel')}
                    </label>

                    {sshEnabled && (
                      <div className="mt-3 grid grid-cols-1 gap-3 rounded-md border border-edge bg-surface-alt p-3 md:grid-cols-2">
                        <div>
                          <Label required>{t('newConn.sshHost')}</Label>
                          <Input value={sshHost} onChange={(e) => setSshHost(e.target.value)} placeholder="ssh.example.com" />
                        </div>
                        <div>
                          <Label required>{t('newConn.sshPort')}</Label>
                          <Input value={sshPort} onChange={(e) => setSshPort(e.target.value)} placeholder="22" />
                        </div>
                        <div className="md:col-span-2">
                          <Label required>{t('newConn.sshUsername')}</Label>
                          <Input value={sshUsername} onChange={(e) => setSshUsername(e.target.value)} placeholder="root" onKeyDown={tabFill(setSshUsername)} />
                        </div>
                        <div className="md:col-span-2">
                          <Label required>{t('newConn.authMethod')}</Label>
                          <div className="flex gap-2">
                            <button
                              type="button"
                              onClick={() => setSshAuthMethod('password')}
                              className={cn(
                                'flex flex-1 items-center justify-center gap-1.5 rounded-md border px-3 py-2 text-xs transition-colors',
                                sshAuthMethod === 'password'
                                  ? 'border-blue-500 bg-blue-500/10 text-blue-400'
                                  : 'border-edge bg-surface text-fg-secondary',
                              )}
                            >
                              <KeyRound className="h-3.5 w-3.5" />
                              {t('newConn.authPassword')}
                            </button>
                            <button
                              type="button"
                              onClick={() => setSshAuthMethod('private_key')}
                              className={cn(
                                'flex flex-1 items-center justify-center gap-1.5 rounded-md border px-3 py-2 text-xs transition-colors',
                                sshAuthMethod === 'private_key'
                                  ? 'border-blue-500 bg-blue-500/10 text-blue-400'
                                  : 'border-edge bg-surface text-fg-secondary',
                              )}
                            >
                              <FileKey2 className="h-3.5 w-3.5" />
                              {t('newConn.authKey')}
                            </button>
                          </div>
                        </div>
                        {sshAuthMethod === 'password' ? (
                          <div className="md:col-span-2">
                            <Label required>{t('newConn.sshPassword')}</Label>
                            <Input type="password" value={sshPassword} onChange={(e) => setSshPassword(e.target.value)} />
                          </div>
                        ) : (
                          <>
                            <div className="md:col-span-2">
                              <Label required>{t('newConn.privateKey')}</Label>
                              <Input value={sshKeyPath} onChange={(e) => setSshKeyPath(e.target.value)} placeholder="~/.ssh/id_rsa" onKeyDown={tabFill(setSshKeyPath)} />
                            </div>
                            <div className="md:col-span-2">
                              <Label>{t('newConn.passphrase')}</Label>
                              <Input type="password" value={sshPassphrase} onChange={(e) => setSshPassphrase(e.target.value)} placeholder={t('newConn.passphraseHint')} />
                            </div>
                          </>
                        )}
                      </div>
                    )}
                  </div>
                )}

                <div>
                  <Label>{t('newConn.sslMode')}</Label>
                  <Select
                    value={sslMode}
                    options={sslOptions}
                    onChange={(v) => setSslMode(v as SslMode)}
                  />
                </div>

                <div className="grid grid-cols-1 gap-4 md:grid-cols-2">
                  <div>
                    <Label>{t('newConn.colorTag')}</Label>
                    <div className="flex items-center gap-2 pt-1">
                      {COLOR_KEYS.map((c) => (
                        <button
                          key={c.value}
                          type="button"
                          title={t(c.key as TranslationKey)}
                          onClick={() => setColorTag(c.value)}
                          className={cn(
                            'h-6 w-6 rounded-full border-2 transition-transform hover:scale-110',
                            colorTag === c.value ? 'border-fg scale-110' : 'border-transparent',
                          )}
                          style={{ backgroundColor: c.value }}
                        />
                      ))}
                    </div>
                  </div>
                  <div>
                    <Label>{t('newConn.group')}</Label>
                    <Select
                      value={group}
                      options={[
                        { value: '', label: t('newConn.noGroup') },
                        ...groups.map((g) => ({ value: g, label: g })),
                      ]}
                      onChange={setGroup}
                    />
                  </div>
                </div>
              </div>
            )}

            <div ref={testResultRef}>
              {testOk && (
                <div className="mt-4 rounded-md border border-green-500/20 bg-green-500/10 p-3 text-xs text-green-400">
                  {t('newConn.testSuccess')}{testOk}
                </div>
              )}
              {testErr && (
                <div className="mt-4 rounded-md border border-red-500/20 bg-red-500/10 p-3 text-xs text-red-400 break-all">
                  {testErr}
                </div>
              )}
            </div>
          </div>

          <footer className="flex shrink-0 items-center justify-end gap-3 border-t border-edge bg-surface-alt px-6 py-3">
            <Button variant="secondary" onClick={() => void onTest()} disabled={testing}>
              {testing ? t('newConn.testing') : t('newConn.testConnection')}
            </Button>
            <Button variant="secondary" onClick={closeWindow}>
              {t('common.cancel')}
            </Button>
            <Button variant="primary" onClick={() => void onSave()}>
              {t('common.save')}
            </Button>
          </footer>
        </main>
      </div>
    </div>
  );
}
