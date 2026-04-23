import { useCallback, useMemo, useRef, useState } from 'react';
import type { KeyboardEvent } from 'react';
import type { TranslationKey } from '../../locales';
import { ChevronDown, ChevronRight, KeyRound, FileKey2 } from 'lucide-react';
import { Dialog } from '../../components/ui/Dialog';
import { Input } from '../../components/ui/Input';
import { Select } from '../../components/ui/Select';
import { Button } from '../../components/ui/Button';
import { useConnectionStore } from '../../stores/connectionStore';
import { useI18n } from '../../hooks/useI18n';
import { cn } from '../../lib/cn';
import { DB_REGISTRY } from '../../lib/databaseTypes';
import type { ConnectionConfig, DatabaseType, SslMode, SshTunnelConfig } from '../../types';

function normalizeRedisDatabaseField(s: string): string {
  const u = s.trim();
  if (u === '' || !/^\d+$/.test(u)) return '0';
  return String(Math.min(15, Math.max(0, parseInt(u, 10))));
}

export interface NewConnectionDialogProps {
  open: boolean;
  onClose: () => void;
}

function newId() {
  return `conn_${Math.random().toString(36).slice(2, 10)}`;
}

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

export function NewConnectionDialog({ open, onClose }: NewConnectionDialogProps) {
  const { t } = useI18n();
  const saveConnection = useConnectionStore((s) => s.saveConnection);

  const [name, setName] = useState('');
  const [databaseType, setDatabaseType] = useState<DatabaseType>('postgresql');
  const [host, setHost] = useState('127.0.0.1');
  const [port, setPort] = useState('5432');
  const [database, setDatabase] = useState('postgres');
  const [username, setUsername] = useState('postgres');
  const [password, setPassword] = useState('');
  const [sslMode, setSslMode] = useState<SslMode>('prefer');
  const [group, setGroup] = useState<string>(() => t('newConn.defaultGroup'));
  const [colorTag, setColorTag] = useState<string>('#3b82f6');

  const [showAdvanced, setShowAdvanced] = useState(false);

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
    if (newType === 'redis') {
      setDatabase('0');
      setUsername('');
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
      id: newId(),
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

    if (databaseType === 'redis') {
      return {
        ...base,
        host: host || DB_REGISTRY[databaseType].defaultHost || undefined,
        port: Number(port) || DB_REGISTRY[databaseType].defaultPort || undefined,
        database: normalizeRedisDatabaseField(database),
        password: password || undefined,
      };
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
    onClose();
  }

  const isFileMode = DB_REGISTRY[databaseType].connectionMode === 'file';
  const isSqlite = isFileMode;
  const isRedis = databaseType === 'redis';

  const sslOptions = useMemo(
    () => [
      { value: 'disable', label: `Disable — ${t('newConn.sslNone')}` },
      { value: 'prefer', label: `Prefer — ${t('newConn.sslPrefer')}` },
      { value: 'require', label: `Require — ${t('newConn.sslRequire')}` },
    ],
    [t],
  );

  return (
    <Dialog
      open={open}
      title={t('newConn.title')}
      description={t('newConn.description')}
      onClose={onClose}
      footer={
        <>
          <Button variant="secondary" onClick={onClose}>
            {t('common.cancel')}
          </Button>
          <Button variant="secondary" onClick={() => void onTest()} disabled={testing}>
            {testing ? t('newConn.testing') : t('newConn.testConnection')}
          </Button>
          <Button variant="primary" onClick={() => void onSave()}>
            {t('common.save')}
          </Button>
        </>
      }
    >
      <div className="max-h-[60vh] overflow-y-auto pr-1">
        {/* Basic information */}
        <div className="grid grid-cols-1 gap-3 md:grid-cols-2">
          <div className="md:col-span-2">
            <Label required>{t('newConn.connName')}</Label>
            <Input value={name} onChange={(e) => setName(e.target.value)} placeholder={t('newConn.namePlaceholder')} />
          </div>

          <div>
            <Label required>{t('newConn.dbType')}</Label>
            <Select
              value={databaseType}
              options={Object.entries(DB_REGISTRY).map(([value, meta]) => ({
                value,
                label: meta.label,
              }))}
              onChange={(v) => handleDatabaseTypeChange(v as DatabaseType)}
            />
          </div>

          <div>
            <Label>{t('newConn.group')}</Label>
            <Select
              value={group}
              options={[
                { value: '生产环境', label: t('newConn.groupProd') },
                { value: '开发环境', label: t('newConn.groupDev') },
                { value: '测试环境', label: t('newConn.groupTest') },
              ]}
              onChange={setGroup}
            />
          </div>

          {isSqlite ? (
            <div className="md:col-span-2">
              <Label required>{t('newConn.dbFilePath')}</Label>
              <Input value={database} onChange={(e) => setDatabase(e.target.value)} placeholder="/path/to/db.sqlite" />
            </div>
          ) : isRedis ? (
            <>
              <div>
                <Label required>{t('newConn.host')}</Label>
                <Input value={host} onChange={(e) => setHost(e.target.value)} placeholder="127.0.0.1" />
              </div>
              <div>
                <Label required>{t('newConn.port')}</Label>
                <Input value={port} onChange={(e) => setPort(e.target.value)} />
              </div>
              <div className="md:col-span-2">
                <Label>{t('newConn.databaseIndex')}</Label>
                <Input
                  type="number"
                  min={0}
                  max={15}
                  value={database}
                  onChange={(e) => {
                    const v = e.target.value;
                    if (v === '') {
                      setDatabase('');
                      return;
                    }
                    setDatabase(
                      String(Math.min(15, Math.max(0, parseInt(v, 10) || 0))),
                    );
                  }}
                  onBlur={() => {
                    if (database.trim() === '') setDatabase('0');
                  }}
                  placeholder="0"
                />
              </div>
              <div className="md:col-span-2">
                <Label>{t('newConn.password')}</Label>
                <Input type="password" value={password} onChange={(e) => setPassword(e.target.value)} />
              </div>
            </>
          ) : (
            <>
              <div>
                <Label required>{t('newConn.host')}</Label>
                <Input value={host} onChange={(e) => setHost(e.target.value)} />
              </div>
              <div>
                <Label required>{t('newConn.port')}</Label>
                <Input value={port} onChange={(e) => setPort(e.target.value)} />
              </div>
              <div>
                <Label>{t('newConn.database')}</Label>
                <Input value={database} onChange={(e) => setDatabase(e.target.value)} />
              </div>
              <div>
                <Label>{t('newConn.username')}</Label>
                <Input value={username} onChange={(e) => setUsername(e.target.value)} />
              </div>
              <div className="md:col-span-2">
                <Label>{t('newConn.password')}</Label>
                <Input type="password" value={password} onChange={(e) => setPassword(e.target.value)} />
              </div>
            </>
          )}
        </div>

        {/* Advanced settings (collapsible) */}
        <button
          type="button"
          className="mt-4 flex w-full items-center gap-2 rounded-md border border-edge bg-surface px-3 py-2.5 text-sm text-fg-secondary hover:text-fg"
          onClick={() => setShowAdvanced((v) => !v)}
        >
          {showAdvanced ? <ChevronDown className="h-4 w-4" /> : <ChevronRight className="h-4 w-4" />}
          {t('newConn.advanced')}
          {sshEnabled && (
            <span className="ml-auto rounded bg-blue-500/20 px-1.5 py-0.5 text-xs text-blue-400">SSH</span>
          )}
        </button>

        {showAdvanced && (
          <div className="mt-3 space-y-4 rounded-md border border-edge bg-surface-alt p-4">
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
                  <div className="mt-3 grid grid-cols-1 gap-3 rounded-md border border-edge bg-surface p-3 md:grid-cols-2">
                    <div>
                      <Label required>{t('newConn.sshHost')}</Label>
                      <Input
                        value={sshHost}
                        onChange={(e) => setSshHost(e.target.value)}
                        placeholder="ssh.example.com"
                      />
                    </div>
                    <div>
                      <Label required>{t('newConn.sshPort')}</Label>
                      <Input value={sshPort} onChange={(e) => setSshPort(e.target.value)} placeholder="22" />
                    </div>
                    <div className="md:col-span-2">
                      <Label required>{t('newConn.sshUsername')}</Label>
                      <Input
                        value={sshUsername}
                        onChange={(e) => setSshUsername(e.target.value)}
                        placeholder="root"
                        onKeyDown={tabFill(setSshUsername)}
                      />
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
                        <Input
                          type="password"
                          value={sshPassword}
                          onChange={(e) => setSshPassword(e.target.value)}
                        />
                      </div>
                    ) : (
                      <>
                        <div className="md:col-span-2">
                          <Label required>{t('newConn.privateKey')}</Label>
                          <Input
                            value={sshKeyPath}
                            onChange={(e) => setSshKeyPath(e.target.value)}
                            placeholder="~/.ssh/id_rsa"
                            onKeyDown={tabFill(setSshKeyPath)}
                          />
                        </div>
                        <div className="md:col-span-2">
                          <Label>{t('newConn.passphrase')}</Label>
                          <Input
                            type="password"
                            value={sshPassphrase}
                            onChange={(e) => setSshPassphrase(e.target.value)}
                            placeholder={t('newConn.passphraseHint')}
                          />
                        </div>
                      </>
                    )}
                  </div>
                )}
              </div>
            )}

            {/* SSL mode */}
            {!isRedis && (
              <div>
                <Label>{t('newConn.sslMode')}</Label>
                <Select
                  value={sslMode}
                  options={sslOptions}
                  onChange={(v) => setSslMode(v as SslMode)}
                />
              </div>
            )}

            {/* Color tag */}
            <div>
              <Label>{t('newConn.colorTag')}</Label>
              <div className="flex items-center gap-2">
                {COLOR_KEYS.map((c) => (
                  <button
                    key={c.value}
                    type="button"
                    title={t(c.key as TranslationKey)}
                    onClick={() => setColorTag(c.value)}
                    className={cn(
                      'h-6 w-6 rounded-full border-2 transition-transform hover:scale-110',
                      colorTag === c.value ? 'border-white scale-110' : 'border-transparent',
                    )}
                    style={{ backgroundColor: c.value }}
                  />
                ))}
              </div>
            </div>
          </div>
        )}

        {/* Test result */}
        <div ref={testResultRef}>
          {testOk && (
            <div className="mt-4 rounded-md border border-green-500/20 bg-green-500/10 p-3 text-xs text-green-400">
              {t('newConn.testSuccess')}
              {testOk}
            </div>
          )}
          {testErr && (
            <div className="mt-4 rounded-md border border-red-500/20 bg-red-500/10 p-3 text-xs text-red-400 break-all">
              {testErr}
            </div>
          )}
        </div>
      </div>
    </Dialog>
  );
}
