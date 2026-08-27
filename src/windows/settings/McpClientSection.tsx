import { useEffect, useState } from 'react';
import { Button } from '../../components/ui/Button';
import { PathInput } from '../../components/ui/PathInput';
import { useAiStore } from '../../stores/aiStore';
import { useSettingsStore } from '../../stores/settingsStore';
import { useI18n } from '../../hooks/useI18n';
import type { McpServerConfig } from '../../types';
import { SectionTitle, SettingRow, ToggleRow } from './settingsUi';

const EMPTY_DRAFT: McpServerConfig = {
  id: '',
  name: '',
  transport: 'stdio',
  command: '',
  args: [],
  env: {},
  enabled: true,
};

export function McpClientSection() {
  const { t } = useI18n();
  const savedConfigs = useSettingsStore((s) => s.settings.mcpClientServers ?? []);
  const {
    mcpServers,
    mcpConnecting,
    mcpError,
    connectMcpServer,
    disconnectMcpServer,
    loadMcpServers,
    saveMcpClientServers,
    clearMcpError,
  } = useAiStore();

  const [showForm, setShowForm] = useState(false);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [draft, setDraft] = useState<McpServerConfig>(EMPTY_DRAFT);
  const [argsText, setArgsText] = useState('');
  const [saving, setSaving] = useState(false);

  const connectedIds = new Set(mcpServers.map((s) => s.serverId));

  useEffect(() => {
    void loadMcpServers();
  }, [loadMcpServers]);

  const startAdd = () => {
    setEditingId(null);
    setDraft(EMPTY_DRAFT);
    setArgsText('');
    setShowForm(true);
  };

  const startEdit = (config: McpServerConfig) => {
    setEditingId(config.id);
    setDraft({ ...config });
    setArgsText((config.args ?? []).join('\n'));
    setShowForm(true);
  };

  const handleSave = async () => {
    if (!draft.id.trim() || !draft.command?.trim()) return;
    const config: McpServerConfig = {
      ...draft,
      id: draft.id.trim(),
      name: draft.name.trim() || draft.id.trim(),
      args: argsText
        .split('\n')
        .map((s) => s.trim())
        .filter(Boolean),
    };
    if (!editingId && savedConfigs.some((c) => c.id === config.id)) return;

    setSaving(true);
    try {
      const next = editingId
        ? savedConfigs.map((c) => (c.id === editingId ? config : c))
        : [...savedConfigs, config];
      await saveMcpClientServers(next);
      setShowForm(false);
      setEditingId(null);
      setDraft(EMPTY_DRAFT);
      setArgsText('');
    } finally {
      setSaving(false);
    }
  };

  const handleDelete = async (id: string) => {
    const next = savedConfigs.filter((c) => c.id !== id);
    await saveMcpClientServers(next);
  };

  const handleToggleEnabled = async (id: string, enabled: boolean) => {
    const next = savedConfigs.map((c) => (c.id === id ? { ...c, enabled } : c));
    await saveMcpClientServers(next);
  };

  const inputClass =
    'h-9 w-full rounded-md border border-edge bg-surface px-3 text-sm text-fg outline-none focus:border-blue-500';

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

      <SectionTitle>{t('mcpClient.savedConfigs')}</SectionTitle>

      {savedConfigs.length === 0 && !showForm && (
        <p className="text-xs text-fg-muted">{t('mcpClient.noSavedConfigs')}</p>
      )}

      {savedConfigs.length > 0 && (
        <div className="space-y-1">
          {savedConfigs.map((config) => (
            <div
              key={config.id}
              className="flex items-center justify-between gap-2 rounded-md border border-edge bg-surface p-2"
            >
              <div className="min-w-0 flex-1">
                <div className="text-sm text-fg">{config.name}</div>
                <div className="text-xs text-fg-muted">{config.id}</div>
              </div>
              <div className="flex shrink-0 items-center gap-2">
                <button
                  type="button"
                  role="switch"
                  aria-checked={config.enabled}
                  aria-label={t('mcpClient.enabled')}
                  onClick={() => void handleToggleEnabled(config.id, !config.enabled)}
                  className={`relative inline-flex h-5 w-9 shrink-0 cursor-pointer items-center rounded-full transition-colors ${
                    config.enabled ? 'bg-accent' : 'bg-edge'
                  }`}
                >
                  <span
                    className={`inline-block h-3.5 w-3.5 rounded-full bg-white shadow transition-transform ${
                      config.enabled ? 'translate-x-[18px]' : 'translate-x-[3px]'
                    }`}
                  />
                </button>
                {!connectedIds.has(config.id) && config.enabled && (
                  <Button
                    variant="secondary"
                    disabled={mcpConnecting}
                    onClick={() => void connectMcpServer(config.id)}
                  >
                    {mcpConnecting ? t('mcpClient.connecting') : t('mcpClient.connect')}
                  </Button>
                )}
                <Button variant="secondary" onClick={() => startEdit(config)}>
                  {t('mcpClient.edit')}
                </Button>
                <Button variant="secondary" onClick={() => void handleDelete(config.id)}>
                  {t('mcpClient.delete')}
                </Button>
              </div>
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
              disabled={Boolean(editingId)}
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
          <ToggleRow
            label={t('mcpClient.enabled')}
            checked={draft.enabled}
            onChange={(enabled) => setDraft((d) => ({ ...d, enabled }))}
          />
          <div className="flex gap-2">
            <Button
              variant="primary"
              disabled={saving || !draft.id.trim() || !draft.command?.trim()}
              onClick={() => void handleSave()}
            >
              {saving ? t('mcpClient.saving') : t('mcpClient.save')}
            </Button>
            <Button variant="secondary" onClick={() => setShowForm(false)}>
              {t('common.cancel')}
            </Button>
          </div>
        </div>
      ) : (
        <Button variant="secondary" onClick={startAdd}>
          {t('mcpClient.addServer')}
        </Button>
      )}

      <SectionTitle>{t('mcpClient.runtimeStatus')}</SectionTitle>

      {mcpServers.length === 0 ? (
        <p className="text-xs text-fg-muted">{t('mcpClient.noServers')}</p>
      ) : (
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
              <Button variant="secondary" onClick={() => void disconnectMcpServer(s.serverId)}>
                {t('mcpClient.disconnect')}
              </Button>
            </div>
          ))}
        </div>
      )}
    </>
  );
}
