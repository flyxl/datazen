import { ChevronDown, ChevronRight, Trash2 } from 'lucide-react';
import { useEffect, useState } from 'react';
import { Button } from '../../components/ui/Button';
import { PathInput } from '../../components/ui/PathInput';
import { useAiStore } from '../../stores/aiStore';
import { useSettingsStore } from '../../stores/settingsStore';
import { useI18n } from '../../hooks/useI18n';
import type { McpServerConfig } from '../../types';
import { isValidMcpServerId } from '../../types';
import { SectionTitle, SettingRow, ToggleRow } from './settingsUi';

const EMPTY_DRAFT: McpServerConfig = {
  id: '',
  name: '',
  transport: 'stdio',
  command: '',
  args: [],
  env: {},
  enabled: true,
  enabledForAi: true,
};

type EnvRow = { key: string; value: string };

function envToRows(env?: Record<string, string>): EnvRow[] {
  return Object.entries(env ?? {}).map(([key, value]) => ({ key, value }));
}

function rowsToEnv(rows: EnvRow[]): Record<string, string> {
  const env: Record<string, string> = {};
  for (const row of rows) {
    const key = row.key.trim();
    if (key) env[key] = row.value;
  }
  return env;
}

export function McpClientSection() {
  const { t } = useI18n();
  const savedConfigs = useSettingsStore((s) => s.settings.mcpClientServers ?? []);
  const {
    mcpServers,
    mcpTools,
    mcpConnecting,
    mcpConnectingServerId,
    mcpError,
    mcpServerErrors,
    connectMcpServer,
    disconnectMcpServer,
    loadMcpServers,
    loadMcpTools,
    saveMcpClientServers,
    clearMcpError,
    clearMcpServerError,
  } = useAiStore();

  const [expandedServers, setExpandedServers] = useState<Set<string>>(new Set());

  const [showForm, setShowForm] = useState(false);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [draft, setDraft] = useState<McpServerConfig>(EMPTY_DRAFT);
  const [argsText, setArgsText] = useState('');
  const [envRows, setEnvRows] = useState<EnvRow[]>([]);
  const [saving, setSaving] = useState(false);
  const [saveError, setSaveError] = useState<string | null>(null);

  const trimmedDraftId = draft.id.trim();
  const idInvalid = trimmedDraftId.length > 0 && !isValidMcpServerId(trimmedDraftId);
  const idDuplicate =
    !editingId && trimmedDraftId.length > 0 && savedConfigs.some((c) => c.id === trimmedDraftId);

  const connectedIds = new Set(mcpServers.map((s) => s.serverId));

  useEffect(() => {
    void loadMcpServers();
    void loadMcpTools();
  }, [loadMcpServers, loadMcpTools]);

  const toggleServerTools = (serverId: string) => {
    setExpandedServers((prev) => {
      const next = new Set(prev);
      if (next.has(serverId)) {
        next.delete(serverId);
      } else {
        next.add(serverId);
      }
      return next;
    });
  };

  const startAdd = () => {
    setEditingId(null);
    setDraft(EMPTY_DRAFT);
    setArgsText('');
    setEnvRows([]);
    setSaveError(null);
    setShowForm(true);
  };

  const startEdit = (config: McpServerConfig) => {
    setEditingId(config.id);
    setDraft({ ...config });
    setArgsText((config.args ?? []).join('\n'));
    setEnvRows(envToRows(config.env));
    setSaveError(null);
    setShowForm(true);
  };

  const handleSave = async () => {
    if (!draft.id.trim() || !draft.command?.trim()) return;
    const trimmedId = draft.id.trim();
    if (!isValidMcpServerId(trimmedId)) {
      setSaveError(t('mcpClient.invalidId'));
      return;
    }
    if (!editingId && savedConfigs.some((c) => c.id === trimmedId)) {
      setSaveError(t('mcpClient.duplicateId'));
      return;
    }
    const config: McpServerConfig = {
      ...draft,
      id: trimmedId,
      name: draft.name.trim() || draft.id.trim(),
      args: argsText
        .split('\n')
        .map((s) => s.trim())
        .filter(Boolean),
      env: rowsToEnv(envRows),
    };

    setSaving(true);
    setSaveError(null);
    try {
      const next = editingId
        ? savedConfigs.map((c) => (c.id === editingId ? config : c))
        : [...savedConfigs, config];
      await saveMcpClientServers(next);
      setShowForm(false);
      setEditingId(null);
      setDraft(EMPTY_DRAFT);
      setArgsText('');
      setEnvRows([]);
    } finally {
      setSaving(false);
    }
  };

  const handleDelete = async (id: string) => {
    const next = savedConfigs.filter((c) => c.id !== id);
    await saveMcpClientServers(next);
    clearMcpServerError(id);
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

      <div data-testid="mcp-saved-configs">
        <SectionTitle>{t('mcpClient.savedConfigs')}</SectionTitle>
      </div>

      {savedConfigs.length === 0 && !showForm && (
        <p className="text-xs text-fg-muted">{t('mcpClient.noSavedConfigs')}</p>
      )}

      {savedConfigs.length > 0 && (
        <div className="space-y-1">
          {savedConfigs.map((config) => {
            const serverError = mcpServerErrors[config.id];
            const isConnecting = mcpConnecting && mcpConnectingServerId === config.id;
            const isConnected = connectedIds.has(config.id);

            return (
              <div key={config.id} className="rounded-md border border-edge bg-surface p-2">
                <div className="flex items-center justify-between gap-2">
                  <div className="min-w-0 flex-1">
                    <div className="flex items-center gap-2">
                      <div className="text-sm text-fg">{config.name}</div>
                      {serverError && (
                        <span
                          className="rounded bg-red-500/15 px-1.5 py-0.5 text-[10px] font-medium text-red-500"
                          title={serverError}
                        >
                          {t('mcpClient.connectFailed')}
                        </span>
                      )}
                    </div>
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
                    {!isConnected && config.enabled && !serverError && (
                      <Button
                        variant="secondary"
                        disabled={mcpConnecting}
                        onClick={() => void connectMcpServer(config.id)}
                      >
                        {isConnecting ? t('mcpClient.connecting') : t('mcpClient.connect')}
                      </Button>
                    )}
                    {!isConnected && config.enabled && serverError && (
                      <Button
                        variant="secondary"
                        disabled={mcpConnecting}
                        onClick={() => void connectMcpServer(config.id)}
                      >
                        {isConnecting ? t('mcpClient.connecting') : t('mcpClient.reconnect')}
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
                {serverError && (
                  <div className="mt-1.5 flex items-start justify-between gap-2 rounded border border-red-500/20 bg-red-500/5 px-2 py-1">
                    <span className="text-xs text-red-500">{serverError}</span>
                    <button
                      type="button"
                      onClick={() => clearMcpServerError(config.id)}
                      className="shrink-0 text-xs text-red-500 underline"
                    >
                      {t('common.close')}
                    </button>
                  </div>
                )}
              </div>
            );
          })}
        </div>
      )}

      {showForm ? (
        <div className="space-y-3 rounded-md border border-edge bg-surface-alt p-3">
          <SettingRow label="ID">
            <div className="space-y-1">
              <input
                type="text"
                value={draft.id}
                onChange={(e) => {
                  setDraft((d) => ({ ...d, id: e.target.value }));
                  setSaveError(null);
                }}
                placeholder="my-mcp-server"
                disabled={Boolean(editingId)}
                className={inputClass}
              />
              {idInvalid && <p className="text-xs text-red-500">{t('mcpClient.invalidId')}</p>}
            </div>
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
          <div>
            <div className="mb-2 flex items-center justify-between">
              <span className="text-sm text-fg-secondary">{t('mcpClient.envVars')}</span>
              <button
                type="button"
                onClick={() => setEnvRows((rows) => [...rows, { key: '', value: '' }])}
                className="text-xs text-accent hover:underline"
                data-testid="mcp-add-env"
              >
                + {t('mcpClient.addEnv')}
              </button>
            </div>
            {envRows.length === 0 && (
              <p className="text-xs text-fg-muted">{t('mcpClient.noEnvVars')}</p>
            )}
            {envRows.map((row, i) => (
              <div key={i} className="mb-2 flex items-center gap-2">
                <input
                  type="text"
                  value={row.key}
                  onChange={(e) => {
                    const next = [...envRows];
                    next[i] = { ...next[i], key: e.target.value };
                    setEnvRows(next);
                  }}
                  placeholder={t('mcpClient.envKey')}
                  className={`${inputClass} flex-1`}
                  data-testid="mcp-env-key"
                />
                <input
                  type="text"
                  value={row.value}
                  onChange={(e) => {
                    const next = [...envRows];
                    next[i] = { ...next[i], value: e.target.value };
                    setEnvRows(next);
                  }}
                  placeholder={t('mcpClient.envValue')}
                  className={`${inputClass} flex-1`}
                  data-testid="mcp-env-value"
                />
                <button
                  type="button"
                  onClick={() => setEnvRows((rows) => rows.filter((_, j) => j !== i))}
                  className="p-1 text-fg-muted hover:text-red-400"
                  aria-label={t('mcpClient.removeEnv')}
                >
                  <Trash2 className="h-3.5 w-3.5" />
                </button>
              </div>
            ))}
          </div>
          <ToggleRow
            label={t('mcpClient.enabled')}
            checked={draft.enabled}
            onChange={(enabled) => setDraft((d) => ({ ...d, enabled }))}
          />
          <ToggleRow
            label={t('mcpClient.enabledForAi')}
            checked={draft.enabledForAi ?? true}
            onChange={(enabledForAi) => setDraft((d) => ({ ...d, enabledForAi }))}
          />
          {(saveError || idDuplicate) && (
            <p className="text-xs text-red-500">{saveError ?? t('mcpClient.duplicateId')}</p>
          )}
          <div className="flex gap-2">
            <Button
              variant="primary"
              disabled={
                saving || !draft.id.trim() || !draft.command?.trim() || idInvalid || idDuplicate
              }
              onClick={() => void handleSave()}
              data-testid="mcp-save"
            >
              {saving ? t('mcpClient.saving') : t('mcpClient.save')}
            </Button>
            <Button variant="secondary" onClick={() => setShowForm(false)}>
              {t('common.cancel')}
            </Button>
          </div>
        </div>
      ) : (
        <Button variant="secondary" onClick={startAdd} data-testid="mcp-add-server">
          {t('mcpClient.addServer')}
        </Button>
      )}

      <SectionTitle>{t('mcpClient.runtimeStatus')}</SectionTitle>

      {mcpServers.length === 0 ? (
        <p className="text-xs text-fg-muted">{t('mcpClient.noServers')}</p>
      ) : (
        <div className="space-y-1">
          {mcpServers.map((s) => {
            const serverTools = (mcpTools ?? []).filter((tool) => tool.serverId === s.serverId);
            const expanded = expandedServers.has(s.serverId);
            return (
              <div key={s.serverId} className="rounded-md border border-edge bg-surface p-2">
                <div className="flex items-center justify-between">
                  <button
                    type="button"
                    className="flex min-w-0 flex-1 items-center gap-1 text-left"
                    onClick={() => toggleServerTools(s.serverId)}
                    aria-expanded={expanded}
                  >
                    {expanded ? (
                      <ChevronDown className="h-3.5 w-3.5 shrink-0 text-fg-muted" />
                    ) : (
                      <ChevronRight className="h-3.5 w-3.5 shrink-0 text-fg-muted" />
                    )}
                    <span className="text-sm text-fg">{s.serverName}</span>
                    <span className="text-xs text-fg-muted">
                      ({s.toolsCount} {t('mcpClient.tools')})
                    </span>
                  </button>
                  <Button variant="secondary" onClick={() => void disconnectMcpServer(s.serverId)}>
                    {t('mcpClient.disconnect')}
                  </Button>
                </div>
                {expanded && serverTools.length > 0 && (
                  <div className="mt-2 border-t border-edge pt-2">
                    <div className="mb-1.5 text-[11px] font-medium uppercase tracking-wide text-fg-muted">
                      {t('mcpClient.toolList')}
                    </div>
                    <ul className="space-y-1">
                      {serverTools.map((tool) => (
                        <li
                          key={tool.qualifiedName}
                          className="rounded border border-edge/60 bg-surface-alt px-2 py-1.5"
                        >
                          <div className="text-sm font-medium text-fg">{tool.toolName}</div>
                          {tool.description && (
                            <div className="text-xs text-fg-muted">{tool.description}</div>
                          )}
                          <div className="mt-0.5 font-mono text-xs text-fg-muted">
                            {tool.qualifiedName}
                          </div>
                        </li>
                      ))}
                    </ul>
                  </div>
                )}
                {expanded && serverTools.length === 0 && (
                  <p className="mt-2 border-t border-edge pt-2 text-xs text-fg-muted">
                    {t('mcpClient.noTools')}
                  </p>
                )}
              </div>
            );
          })}
        </div>
      )}
    </>
  );
}
