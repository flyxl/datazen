import { useEffect, useState } from 'react';
import { useSettingsStore } from '../../stores/settingsStore';
import { useConnectionStore } from '../../stores/connectionStore';
import { useI18n } from '../../hooks/useI18n';
import { cn } from '../../lib/cn';
import { aiCommands } from '../../commands/ai';
import { buildMcpAgentSnippet, type McpAgentTarget } from '../../lib/mcpAgentConfig';
import type { AppSettings, McpPermissionMode } from '../../types';
import type { TranslationKey } from '../../locales';
import { SectionTitle, SettingRow, ToggleRow } from './settingsUi';

const MCP_PERMISSION_MODES: {
  value: McpPermissionMode;
  labelKey: TranslationKey;
  hintKey: TranslationKey;
}[] = [
  {
    value: 'read_only',
    labelKey: 'mcp.permission.readOnly',
    hintKey: 'mcp.permission.readOnlyHint',
  },
  {
    value: 'safe_write',
    labelKey: 'mcp.permission.safeWrite',
    hintKey: 'mcp.permission.safeWriteHint',
  },
  {
    value: 'high_risk_write',
    labelKey: 'mcp.permission.highRiskWrite',
    hintKey: 'mcp.permission.highRiskWriteHint',
  },
];

export interface McpSettingsSectionProps {
  settings?: AppSettings;
}

export function McpSettingsSection({ settings: draftSettings }: McpSettingsSectionProps = {}) {
  const { t } = useI18n();
  const storedSettings = useSettingsStore((s) => s.settings);
  const updateSettings = useSettingsStore((s) => s.updateSettings);
  const settings = draftSettings ?? storedSettings;
  const connections = useConnectionStore((s) => s.connections);
  const fetchConnections = useConnectionStore((s) => s.fetchConnections);
  const [allTools, setAllTools] = useState<string[]>([]);
  const [disabledTools, setDisabledTools] = useState<string[]>([]);
  const [allowedIds, setAllowedIds] = useState<string[]>([]);
  const [running, setRunning] = useState(false);
  const [toggling, setToggling] = useState(false);
  const [toggleError, setToggleError] = useState<string | null>(null);
  const [agentTarget, setAgentTarget] = useState<McpAgentTarget>('cursor');
  const [copied, setCopied] = useState(false);

  const saveSettings = (partial: Partial<AppSettings>) => {
    void updateSettings(partial);
  };

  useEffect(() => {
    void aiCommands
      .mcpListAllTools()
      .then(setAllTools)
      .catch(() => {});
    void fetchConnections().catch(() => {});
  }, [fetchConnections]);

  useEffect(() => {
    setDisabledTools(settings.mcpDisabledTools ?? []);
  }, [settings.mcpDisabledTools]);

  useEffect(() => {
    setAllowedIds(settings.mcpAllowedConnectionIds ?? []);
  }, [settings.mcpAllowedConnectionIds]);

  const refreshStatus = async () => {
    try {
      const status = await aiCommands.mcpGetStatus();
      setRunning(status.running);
    } catch {
      setRunning(false);
    }
  };

  useEffect(() => {
    void refreshStatus();
    const id = window.setInterval(() => void refreshStatus(), 3000);
    return () => window.clearInterval(id);
  }, []);

  const reloadMcpIfRunning = async () => {
    if (!running) return;
    await aiCommands.mcpReload();
    await refreshStatus();
  };

  const handleToggleEnabled = async (enabled: boolean) => {
    setToggling(true);
    setToggleError(null);
    try {
      saveSettings({ mcpServerEnabled: enabled });
      if (enabled) {
        await aiCommands.mcpStartStdio();
      } else {
        await aiCommands.mcpStop();
      }
      await refreshStatus();
    } catch (e) {
      setToggleError(e instanceof Error ? e.message : t('mcp.toggleError'));
      if (enabled) {
        saveSettings({ mcpServerEnabled: false });
      }
    } finally {
      setToggling(false);
    }
  };

  const handlePermissionChange = async (mode: McpPermissionMode) => {
    saveSettings({ mcpPermissionMode: mode });
    try {
      await reloadMcpIfRunning();
    } catch (e) {
      setToggleError(e instanceof Error ? e.message : t('mcp.toggleError'));
    }
  };

  const toggleTool = (toolName: string) => {
    const next = disabledTools.includes(toolName)
      ? disabledTools.filter((n) => n !== toolName)
      : [...disabledTools, toolName];
    setDisabledTools(next);
    saveSettings({ mcpDisabledTools: next });
  };

  const toggleAllowed = (id: string) => {
    const next = allowedIds.includes(id) ? allowedIds.filter((x) => x !== id) : [...allowedIds, id];
    setAllowedIds(next);
    saveSettings({ mcpAllowedConnectionIds: next });
  };

  const handleEnableAll = () => {
    setDisabledTools([]);
    saveSettings({ mcpDisabledTools: [] });
  };
  const handleDisableAll = () => {
    setDisabledTools([...allTools]);
    saveSettings({ mcpDisabledTools: [...allTools] });
  };

  const snippet = buildMcpAgentSnippet(agentTarget);
  const handleCopySnippet = async () => {
    try {
      await navigator.clipboard.writeText(snippet.json);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch {
      /* ignore */
    }
  };

  return (
    <>
      <SectionTitle>{t('mcp.title')}</SectionTitle>
      <p className="text-xs text-fg-muted">{t('mcp.description')}</p>

      <ToggleRow
        label={t('mcp.enabled')}
        checked={settings.mcpServerEnabled ?? false}
        onChange={(v) => {
          if (!toggling) void handleToggleEnabled(v);
        }}
      />
      <p className="text-xs text-fg-muted -mt-1">{t('mcp.enabledHint')}</p>

      <SettingRow label={t('mcp.status')}>
        <div className="flex items-center gap-2 pt-2">
          <span
            className={`inline-block h-2 w-2 rounded-full ${running ? 'bg-green-500' : 'bg-fg-muted'}`}
          />
          <span className="text-sm text-fg">{running ? t('mcp.running') : t('mcp.stopped')}</span>
        </div>
      </SettingRow>

      {toggleError && <p className="text-xs text-red-500">{toggleError}</p>}

      <SettingRow label={t('mcp.permission.title')}>
        <div className="space-y-2 pt-1">
          {MCP_PERMISSION_MODES.map(({ value, labelKey, hintKey }) => {
            const selected = (settings.mcpPermissionMode ?? 'safe_write') === value;
            return (
              <label
                key={value}
                className={`flex cursor-pointer items-start gap-3 rounded-md border px-3 py-2 transition-colors ${
                  selected ? 'border-accent bg-accent/5' : 'border-edge bg-surface'
                }`}
              >
                <input
                  type="radio"
                  name="mcp-permission-mode"
                  value={value}
                  checked={selected}
                  onChange={() => void handlePermissionChange(value)}
                  className="mt-0.5 accent-accent"
                />
                <span className="min-w-0">
                  <span className="block text-sm text-fg">{t(labelKey)}</span>
                  <span className="block text-xs text-fg-muted">{t(hintKey)}</span>
                </span>
              </label>
            );
          })}
        </div>
      </SettingRow>
      <p className="text-xs text-fg-muted -mt-3">{t('mcp.permission.applyHint')}</p>

      <SettingRow label={t('mcp.allowlist.title')}>
        <div className="space-y-2 pt-1">
          <p className="text-xs text-fg-muted">{t('mcp.allowlist.description')}</p>
          {connections.length === 0 ? (
            <p className="text-xs text-fg-muted">{t('mcp.allowlist.empty')}</p>
          ) : (
            <div className="max-h-48 space-y-1 overflow-y-auto">
              {connections.map((c) => {
                const checked = allowedIds.includes(c.id);
                return (
                  <label
                    key={c.id}
                    className="flex cursor-pointer items-center gap-2 rounded-md border border-edge bg-surface px-3 py-2"
                  >
                    <input
                      type="checkbox"
                      checked={checked}
                      onChange={() => toggleAllowed(c.id)}
                      className="accent-accent"
                    />
                    <span className="min-w-0 truncate text-sm text-fg">
                      {c.name}
                      <span className="ml-2 font-mono text-xs text-fg-muted">
                        {c.id.slice(0, 8)}
                      </span>
                    </span>
                  </label>
                );
              })}
            </div>
          )}
          <p className="text-xs text-fg-muted">{t('mcp.allowlist.applyHint')}</p>
        </div>
      </SettingRow>

      <div className="rounded-md border border-edge bg-surface p-3 space-y-2">
        <p className="text-xs text-fg-muted">{t('mcp.usage')}</p>
        <div className="flex items-center gap-2">
          {(['cursor', 'claude'] as const).map((target) => (
            <button
              key={target}
              type="button"
              onClick={() => setAgentTarget(target)}
              className={cn(
                'rounded px-2 py-1 text-xs',
                agentTarget === target
                  ? 'bg-accent text-white'
                  : 'bg-surface-raised text-fg-secondary hover:text-fg',
              )}
            >
              {t(target === 'cursor' ? 'mcp.config.cursor' : 'mcp.config.claude')}
            </button>
          ))}
          <button
            type="button"
            onClick={() => void handleCopySnippet()}
            className="ml-auto text-xs text-accent hover:underline"
          >
            {copied ? t('mcp.config.copied') : t('mcp.config.copy')}
          </button>
        </div>
        <p className="text-xs text-fg-muted">
          {t('mcp.config.pathHint', { path: snippet.configPathHint })}
        </p>
        <pre className="text-xs font-mono text-fg-secondary whitespace-pre-wrap break-all">
          {snippet.json}
        </pre>
        <p className="text-xs text-fg-muted">{t('mcp.config.commandHint')}</p>
      </div>

      {allTools.length > 0 && (
        <>
          <div className="flex items-center justify-between">
            <h3 className="text-sm font-medium text-fg">{t('mcp.tools')}</h3>
            <div className="flex items-center gap-2">
              <button
                type="button"
                onClick={handleEnableAll}
                className="text-xs text-accent hover:underline"
              >
                {t('mcp.tools.enableAll')}
              </button>
              <span className="text-fg-muted">|</span>
              <button
                type="button"
                onClick={handleDisableAll}
                className="text-xs text-accent hover:underline"
              >
                {t('mcp.tools.disableAll')}
              </button>
            </div>
          </div>
          <p className="text-xs text-fg-muted">{t('mcp.tools.description')}</p>

          <div className="space-y-1">
            {allTools.map((tool) => {
              const enabled = !disabledTools.includes(tool);
              return (
                <div
                  key={tool}
                  className="flex items-center justify-between rounded-md border border-edge bg-surface px-3 py-2"
                >
                  <span className="text-sm font-mono text-fg">{tool}</span>
                  <button
                    type="button"
                    role="switch"
                    aria-checked={enabled}
                    onClick={() => toggleTool(tool)}
                    className={`relative inline-flex h-5 w-9 shrink-0 cursor-pointer items-center rounded-full transition-colors ${
                      enabled ? 'bg-accent' : 'bg-edge'
                    }`}
                  >
                    <span
                      className={`inline-block h-3.5 w-3.5 rounded-full bg-white shadow transition-transform ${
                        enabled ? 'translate-x-[18px]' : 'translate-x-[3px]'
                      }`}
                    />
                  </button>
                </div>
              );
            })}
          </div>

          <p className="text-xs text-fg-muted">{t('mcp.tools.applyHint')}</p>
        </>
      )}
    </>
  );
}
