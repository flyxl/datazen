import { useMemo, useState } from 'react';
import { ChevronDown, ChevronRight, KeyRound, FileKey2 } from 'lucide-react';
import { Dialog } from '../../components/ui/Dialog';
import { Input } from '../../components/ui/Input';
import { Select } from '../../components/ui/Select';
import { Button } from '../../components/ui/Button';
import { useConnectionStore } from '../../stores/connectionStore';
import { cn } from '../../lib/cn';
import type { ConnectionConfig, DatabaseType, SslMode, SshTunnelConfig } from '../../types';

export interface NewConnectionDialogProps {
  open: boolean;
  onClose: () => void;
}

function newId() {
  return `conn_${Math.random().toString(36).slice(2, 10)}`;
}

const DEFAULT_PORTS: Record<DatabaseType, string> = {
  postgresql: '5432',
  mysql: '3306',
  mariadb: '3306',
  sqlite: '',
};

const COLOR_OPTIONS = [
  { value: '#ef4444', label: '红色' },
  { value: '#f59e0b', label: '橙色' },
  { value: '#22c55e', label: '绿色' },
  { value: '#3b82f6', label: '蓝色' },
  { value: '#8b5cf6', label: '紫色' },
  { value: '#ec4899', label: '粉色' },
  { value: '#64748b', label: '灰色' },
];

function Label({ children, required }: { children: React.ReactNode; required?: boolean }) {
  return (
    <div className="mb-1 text-xs text-fg-secondary">
      {children}
      {required && <span className="ml-0.5 text-red-400">*</span>}
    </div>
  );
}

export function NewConnectionDialog({ open, onClose }: NewConnectionDialogProps) {
  const saveConnection = useConnectionStore((s) => s.saveConnection);

  const [name, setName] = useState('新连接');
  const [databaseType, setDatabaseType] = useState<DatabaseType>('postgresql');
  const [host, setHost] = useState('127.0.0.1');
  const [port, setPort] = useState('5432');
  const [database, setDatabase] = useState('postgres');
  const [username, setUsername] = useState('postgres');
  const [password, setPassword] = useState('');
  const [sslMode, setSslMode] = useState<SslMode>('prefer');
  const [group, setGroup] = useState<string>('开发环境');
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

  function handleDatabaseTypeChange(newType: DatabaseType) {
    setDatabaseType(newType);
    setPort(DEFAULT_PORTS[newType]);
    if (newType === 'sqlite') {
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
      id: newId(),
      name,
      databaseType: databaseType,
      sslMode: sslMode,
      group: group || undefined,
      colorTag: colorTag || undefined,
      sshTunnel: sshTunnel,
    };

    if (databaseType === 'sqlite') {
      return { ...base, database };
    }

    return {
      ...base,
      host,
      port: Number(port || '0') || undefined,
      database,
      username,
      password: password || undefined,
    };
  }, [colorTag, database, databaseType, group, host, name, password, port, sslMode, sshTunnel, username]);

  async function onTest() {
    setTesting(true);
    setTestOk(null);
    setTestErr(null);
    try {
      const info = await useConnectionStore.getState().testConnection(draft);
      setTestOk(info.serverVersion);
    } catch (e) {
      setTestErr(typeof e === 'string' ? e : e instanceof Error ? e.message : '测试失败');
    } finally {
      setTesting(false);
    }
  }

  async function onSave() {
    await saveConnection(draft);
    onClose();
  }

  const isSqlite = databaseType === 'sqlite';

  return (
    <Dialog
      open={open}
      title="新建连接"
      description="配置数据库连接信息"
      onClose={onClose}
      footer={
        <>
          <Button variant="secondary" onClick={onClose}>
            取消
          </Button>
          <Button variant="secondary" onClick={() => void onTest()} disabled={testing}>
            {testing ? '测试中…' : '测试连接'}
          </Button>
          <Button variant="primary" onClick={() => void onSave()}>
            保存
          </Button>
        </>
      }
    >
      <div className="max-h-[60vh] overflow-y-auto pr-1">
        {/* ── 基本信息 ── */}
        <div className="grid grid-cols-1 gap-3 md:grid-cols-2">
          <div className="md:col-span-2">
            <Label required>连接名称</Label>
            <Input value={name} onChange={(e) => setName(e.target.value)} />
          </div>

          <div>
            <Label required>数据库类型</Label>
            <Select
              value={databaseType}
              options={[
                { value: 'postgresql', label: 'PostgreSQL' },
                { value: 'mysql', label: 'MySQL' },
                { value: 'mariadb', label: 'MariaDB' },
                { value: 'sqlite', label: 'SQLite' },
              ]}
              onChange={(v) => handleDatabaseTypeChange(v as DatabaseType)}
            />
          </div>

          <div>
            <Label>分组</Label>
            <Select
              value={group}
              options={[
                { value: '生产环境', label: '生产环境' },
                { value: '开发环境', label: '开发环境' },
                { value: '测试环境', label: '测试环境' },
              ]}
              onChange={setGroup}
            />
          </div>

          {isSqlite ? (
            <div className="md:col-span-2">
              <Label required>数据库文件路径</Label>
              <Input value={database} onChange={(e) => setDatabase(e.target.value)} placeholder="/path/to/db.sqlite" />
            </div>
          ) : (
            <>
              <div>
                <Label required>主机</Label>
                <Input value={host} onChange={(e) => setHost(e.target.value)} />
              </div>
              <div>
                <Label required>端口</Label>
                <Input value={port} onChange={(e) => setPort(e.target.value)} />
              </div>
              <div>
                <Label>数据库</Label>
                <Input value={database} onChange={(e) => setDatabase(e.target.value)} />
              </div>
              <div>
                <Label>用户名</Label>
                <Input value={username} onChange={(e) => setUsername(e.target.value)} />
              </div>
              <div className="md:col-span-2">
                <Label>密码</Label>
                <Input type="password" value={password} onChange={(e) => setPassword(e.target.value)} />
              </div>
            </>
          )}
        </div>

        {/* ── 高级设置（折叠） ── */}
        <button
          type="button"
          className="mt-4 flex w-full items-center gap-2 rounded-md border border-edge bg-surface px-3 py-2.5 text-sm text-fg-secondary hover:text-fg"
          onClick={() => setShowAdvanced((v) => !v)}
        >
          {showAdvanced ? <ChevronDown className="h-4 w-4" /> : <ChevronRight className="h-4 w-4" />}
          高级设置
          {sshEnabled && (
            <span className="ml-auto rounded bg-blue-500/20 px-1.5 py-0.5 text-xs text-blue-400">SSH</span>
          )}
        </button>

        {showAdvanced && (
          <div className="mt-3 space-y-4 rounded-md border border-edge bg-surface-alt p-4">
            {/* SSH 隧道 */}
            {!isSqlite && (
              <div>
                <label className="flex items-center gap-2 text-sm text-fg-secondary">
                  <input
                    type="checkbox"
                    checked={sshEnabled}
                    onChange={(e) => setSshEnabled(e.target.checked)}
                    className="h-4 w-4 rounded border-edge bg-surface text-blue-500 focus:ring-blue-500/25"
                  />
                  通过 SSH 隧道连接
                </label>

                {sshEnabled && (
                  <div className="mt-3 grid grid-cols-1 gap-3 rounded-md border border-edge bg-surface p-3 md:grid-cols-2">
                    <div>
                      <Label required>SSH 主机</Label>
                      <Input
                        value={sshHost}
                        onChange={(e) => setSshHost(e.target.value)}
                        placeholder="ssh.example.com"
                      />
                    </div>
                    <div>
                      <Label required>SSH 端口</Label>
                      <Input value={sshPort} onChange={(e) => setSshPort(e.target.value)} placeholder="22" />
                    </div>
                    <div className="md:col-span-2">
                      <Label required>SSH 用户名</Label>
                      <Input
                        value={sshUsername}
                        onChange={(e) => setSshUsername(e.target.value)}
                        placeholder="root"
                      />
                    </div>

                    <div className="md:col-span-2">
                      <Label required>认证方式</Label>
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
                          密码
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
                          私钥
                        </button>
                      </div>
                    </div>

                    {sshAuthMethod === 'password' ? (
                      <div className="md:col-span-2">
                        <Label required>SSH 密码</Label>
                        <Input
                          type="password"
                          value={sshPassword}
                          onChange={(e) => setSshPassword(e.target.value)}
                        />
                      </div>
                    ) : (
                      <>
                        <div className="md:col-span-2">
                          <Label required>私钥路径</Label>
                          <Input
                            value={sshKeyPath}
                            onChange={(e) => setSshKeyPath(e.target.value)}
                            placeholder="~/.ssh/id_rsa"
                          />
                        </div>
                        <div className="md:col-span-2">
                          <Label>私钥密码（可选）</Label>
                          <Input
                            type="password"
                            value={sshPassphrase}
                            onChange={(e) => setSshPassphrase(e.target.value)}
                            placeholder="如果私钥有密码保护"
                          />
                        </div>
                      </>
                    )}
                  </div>
                )}
              </div>
            )}

            {/* SSL 模式 */}
            <div>
              <Label>SSL 模式</Label>
              <Select
                value={sslMode}
                options={[
                  { value: 'disable', label: 'Disable — 不加密' },
                  { value: 'prefer', label: 'Prefer — 优先加密' },
                  { value: 'require', label: 'Require — 强制加密' },
                ]}
                onChange={(v) => setSslMode(v as SslMode)}
              />
            </div>

            {/* 颜色标签 */}
            <div>
              <Label>颜色标签</Label>
              <div className="flex items-center gap-2">
                {COLOR_OPTIONS.map((c) => (
                  <button
                    key={c.value}
                    type="button"
                    title={c.label}
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

        {/* ── 测试结果 ── */}
        {testOk && (
          <div className="mt-4 rounded-md border border-green-500/20 bg-green-500/10 p-3 text-xs text-green-400">
            连接成功：{testOk}
          </div>
        )}
        {testErr && (
          <div className="mt-4 rounded-md border border-red-500/20 bg-red-500/10 p-3 text-xs text-red-400">
            {testErr}
          </div>
        )}
      </div>
    </Dialog>
  );
}
