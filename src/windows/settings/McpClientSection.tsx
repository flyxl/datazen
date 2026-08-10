import { useEffect, useState } from 'react';
import { Button } from '../../components/ui/Button';
import { PathInput } from '../../components/ui/PathInput';
import { useAiStore } from '../../stores/aiStore';
import { useI18n } from '../../hooks/useI18n';
import type { McpServerConfig } from '../../types';
import { SectionTitle, SettingRow } from './settingsUi';

export function McpClientSection() {
  const { t } = useI18n();
  const {
    mcpServers,
    mcpConnecting,
    mcpError,
    connectMcpServer,
    disconnectMcpServer,
    loadMcpServers,
    clearMcpError,
  } = useAiStore();

  const [showForm, setShowForm] = useState(false);
  const [draft, setDraft] = useState<McpServerConfig>({
    id: '',
    name: '',
    transport: 'stdio',
    command: '',
    args: [],
    env: {},
    enabled: true,
  });
  const [argsText, setArgsText] = useState('');

  useEffect(() => {
    void loadMcpServers();
  }, [loadMcpServers]);

  const handleConnect = async () => {
    if (!draft.id.trim() || !draft.command?.trim()) return;
    const config: McpServerConfig = {
      ...draft,
      args: argsText
        .split('\n')
        .map((s) => s.trim())
        .filter(Boolean),
    };
    try {
      await connectMcpServer(config);
      setShowForm(false);
      setDraft({ id: '', name: '', transport: 'stdio', command: '', args: [], env: {}, enabled: true });
      setArgsText('');
    } catch {
      // error is already set in store by connectMcpServer
    }
  };

  const inputClass = 'h-9 w-full rounded-md border border-edge bg-surface px-3 text-sm text-fg outline-none focus:border-blue-500';

  return (
    <>
      <SectionTitle>{t('mcpClient.title')}</SectionTitle>
      <p className="text-xs text-fg-muted">{t('mcpClient.description')}</p>

      {mcpError && (
        <div className="flex items-center justify-between rounded-md border border-red-500/30 bg-red-500/10 px-3 py-1.5">
          <span className="text-xs text-red-500">{mcpError}</span>
          <button onClick={clearMcpError} className="text-xs text-red-500 underline">
            {t('common.close')}
          </button>
        </div>
      )}

      {mcpServers.length === 0 && !showForm && (
        <p className="text-xs text-fg-muted">{t('mcpClient.noServers')}</p>
      )}

      {mcpServers.length > 0 && (
        <div className="space-y-1">
          {mcpServers.map((s) => (
            <div
              key={s.serverId}
              className="flex items-center justify-between rounded-md border border-edge bg-surface p-2"
            >
              <div>
                <span className="text-sm text-fg">{s.serverName}</span>
                <span className="ml-2 text-xs text-fg-muted">
                  ({s.toolsCount} {t('mcpClient.tools')})
                </span>
              </div>
              <Button
                variant="secondary"
                onClick={() => void disconnectMcpServer(s.serverId)}
              >
                {t('mcpClient.disconnect')}
              </Button>
            </div>
          ))}
        </div>
      )}

      {showForm ? (
        <div className="space-y-3 rounded-md border border-edge bg-surface-alt p-3">
          <SettingRow label="ID">
            <input
              type="text"
              value={draft.id}
              onChange={(e) => setDraft((d) => ({ ...d, id: e.target.value }))}
              placeholder="my-mcp-server"
              className={inputClass}
            />
          </SettingRow>
          <SettingRow label={t('mcpClient.serverName')}>
            <input
              type="text"
              value={draft.name}
              onChange={(e) => setDraft((d) => ({ ...d, name: e.target.value }))}
              placeholder="My Server"
              className={inputClass}
            />
          </SettingRow>
          <SettingRow label={t('mcpClient.command')}>
            <PathInput
              value={draft.command ?? ''}
              onChange={(command) => setDraft((d) => ({ ...d, command }))}
              placeholder="/usr/local/bin/my-mcp"
            />
          </SettingRow>
          <SettingRow label={t('mcpClient.args')}>
            <textarea
              value={argsText}
              onChange={(e) => setArgsText(e.target.value)}
              placeholder="--flag1&#10;--flag2"
              rows={3}
              autoCapitalize="off"
              autoCorrect="off"
              spellCheck={false}
              className="w-full rounded-md border border-edge bg-surface px-3 py-2 text-sm text-fg outline-none focus:border-blue-500"
            />
          </SettingRow>
          <div className="flex gap-2">
            <Button
              variant="primary"
              disabled={mcpConnecting || !draft.id.trim() || !draft.command?.trim()}
              onClick={() => void handleConnect()}
            >
              {mcpConnecting ? t('mcpClient.connecting') : t('mcpClient.connect')}
            </Button>
            <Button variant="secondary" onClick={() => setShowForm(false)}>
              {t('common.cancel')}
            </Button>
          </div>
        </div>
      ) : (
        <Button variant="secondary" onClick={() => setShowForm(true)}>
          {t('mcpClient.addServer')}
        </Button>
      )}
    </>
  );
}
