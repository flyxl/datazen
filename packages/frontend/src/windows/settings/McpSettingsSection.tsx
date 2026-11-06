import { useCallback, useEffect, useMemo, useState } from 'react';
import { Button } from '../../components/ui/Button';
import { useCopyFeedback } from '../../components/ui/useCopyFeedback';
import { LocaleDomainLoading } from '../../components/LocaleDomainLoading';
import { useSettingsStore } from '../../stores/settingsStore';
import { useConnectionStore } from '../../stores/connectionStore';
import { useI18n } from '../../hooks/useI18n';
import { useLocaleDomains } from '../../hooks/useLocaleDomains';
import { cn } from '../../lib/cn';
import { aiCommands } from '../../commands/ai';
import {
  buildMcpAgentSnippet,
  useAppExecutablePath,
  type McpAgentTarget,
} from '../../lib/mcpAgentConfig';
import type { AppSettings, McpAuditRecord, McpPermissionMode } from '../../types';
import type { TranslationKey } from '../../locales';
import { SectionTitle, SettingRow, ToggleRow } from './settingsUi';
import { SettingHint } from './SettingHint';

/** How long the "copied" config-snippet label stays before reverting. */
const COPIED_FEEDBACK_MS = 2000;

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
  onSettingsChange?: (partial: Partial<AppSettings>) => void;
}

export function McpSettingsSection({
  settings: draftSettings,
  onSettingsChange,
}: McpSettingsSectionProps = {}) {
  const localesReady = useLocaleDomains(['mcp']);
  const { t } = useI18n();
  const storedSettings = useSettingsStore((s) => s.settings);
  const updateSettings = useSettingsStore((s) => s.updateSettings);
  const settings = draftSettings ?? storedSettings;
  const connections = useConnectionStore((s) => s.connections);
  const fetchConnections = useConnectionStore((s) => s.fetchConnections);
  const [allTools, setAllTools] = useState<string[]>([]);
  const [disabledTools, setDisabledTools] = useState<string[]>([]);
  const [allowedIds, setAllowedIds] = useState<string[]>([]);
  const [databaseScope, setDatabaseScope] = useState('');
  const [namespaceScope, setNamespaceScope] = useState('');
  const [scopeSaved, setScopeSaved] = useState(false);
  const [auditRows, setAuditRows] = useState<McpAuditRecord[]>([]);
  const [auditLoading, setAuditLoading] = useState(false);
  const [auditError, setAuditError] = useState<string | null>(null);
  const [saved, setSaved] = useState(false);
  const [running, setRunning] = useState(false);
  const [toggling, setToggling] = useState(false);
  const [toggleError, setToggleError] = useState<string | null>(null);
  const [agentTarget, setAgentTarget] = useState<McpAgentTarget>('cursor');
  const { copied, copy } = useCopyFeedback(COPIED_FEEDBACK_MS);

  const updateAppSettings = (partial: Partial<AppSettings>) => {
    if (onSettingsChange) {
      onSettingsChange(partial);
    } else {
      void updateSettings(partial);
    }
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

  useEffect(() => {
    setDatabaseScope((settings.mcpAllowedDatabases ?? []).join(', '));
  }, [settings.mcpAllowedDatabases]);

  useEffect(() => {
    setNamespaceScope((settings.mcpAllowedNamespaces ?? []).join(', '));
  }, [settings.mcpAllowedNamespaces]);

  const loadAudit = useCallback(async () => {
    setAuditLoading(true);
    setAuditError(null);
    try {
      setAuditRows(await aiCommands.mcpListAuditLog(50));
    } catch (e) {
      setAuditError(e instanceof Error ? e.message : t('mcp.audit.error'));
    } finally {
      setAuditLoading(false);
    }
  }, [t]);

  useEffect(() => {
    void loadAudit();
  }, [loadAudit]);

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
      updateAppSettings({ mcpServerEnabled: enabled });
      if (enabled) {
        await aiCommands.mcpStartStdio();
      } else {
        await aiCommands.mcpStop();
      }
      await refreshStatus();
    } catch (e) {
      setToggleError(e instanceof Error ? e.message : t('mcp.toggleError'));
      if (enabled) {
        updateAppSettings({ mcpServerEnabled: false });
      }
    } finally {
      setToggling(false);
    }
  };

  const handlePermissionChange = async (mode: McpPermissionMode) => {
    updateAppSettings({ mcpPermissionMode: mode });
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
    updateAppSettings({ mcpDisabledTools: next });
  };

  const toggleAllowed = (id: string) => {
    const next = allowedIds.includes(id) ? allowedIds.filter((x) => x !== id) : [...allowedIds, id];
    setAllowedIds(next);
    updateAppSettings({ mcpAllowedConnectionIds: next });
  };

  const handleSaveTools = async () => {
    updateAppSettings({
      mcpDisabledTools: disabledTools,
      mcpAllowedConnectionIds: allowedIds,
    });
    try {
      await reloadMcpIfRunning();
    } catch (e) {
      setToggleError(e instanceof Error ? e.message : t('mcp.toggleError'));
    }
    setSaved(true);
    setTimeout(() => setSaved(false), 2000);
  };

  const parseScope = (text: string): string[] =>
    Array.from(
      new Set(
        text
          .split(/[\n,]+/)
          .map((entry) => entry.trim())
          .filter((entry) => entry.length > 0),
      ),
    );

  const scopeDirty =
    JSON.stringify(parseScope(databaseScope).sort()) !==
      JSON.stringify([...(settings.mcpAllowedDatabases ?? [])].sort()) ||
    JSON.stringify(parseScope(namespaceScope).sort()) !==
      JSON.stringify([...(settings.mcpAllowedNamespaces ?? [])].sort());

  const handleSaveScope = async () => {
    updateAppSettings({
      mcpAllowedDatabases: parseScope(databaseScope),
      mcpAllowedNamespaces: parseScope(namespaceScope),
    });
    try {
      await reloadMcpIfRunning();
    } catch (e) {
      setToggleError(e instanceof Error ? e.message : t('mcp.toggleError'));
    }
    setScopeSaved(true);
    setTimeout(() => setScopeSaved(false), 2000);
  };

  const handleEnableAll = () => {
    setDisabledTools([]);
    updateAppSettings({ mcpDisabledTools: [] });
  };
  const handleDisableAll = () => {
    setDisabledTools([...allTools]);
    updateAppSettings({ mcpDisabledTools: [...allTools] });
  };

  const toolsDirty =
    JSON.stringify([...disabledTools].sort()) !==
      JSON.stringify([...(settings.mcpDisabledTools ?? [])].sort()) ||
    JSON.stringify([...allowedIds].sort()) !==
      JSON.stringify([...(settings.mcpAllowedConnectionIds ?? [])].sort());

  const appExecutablePath = useAppExecutablePath();
  const snippet = useMemo(
    () => buildMcpAgentSnippet(agentTarget, appExecutablePath),
    [agentTarget, appExecutablePath],
  );
  const handleCopySnippet = () => copy(snippet.json);

  // All hooks above. Gate the section on the `mcp` locale pack so the UI never
  // renders raw/un-translated `t('mcp.*')` keys before it is imported.
  if (!localesReady) {
    return <LocaleDomainLoading variant="section" testId="mcp-settings-locale-loading" />;
  }

  return (
    <>
      <SectionTitle hint={t('mcp.description')}>{t('mcp.title')}</SectionTitle>

      <ToggleRow
        label={t('mcp.enabled')}
        hint={t('mcp.enabledHint')}
        checked={settings.mcpServerEnabled ?? false}
        onChange={(v) => {
          if (!toggling) void handleToggleEnabled(v);
        }}
      />

      <SettingRow label={t('mcp.status')}>
        <div className="flex items-center gap-2 pt-2">
          <span
            className={`inline-block h-2 w-2 rounded-full ${running ? 'bg-green-500' : 'bg-fg-muted'}`}
          />
          <span className="text-sm text-fg">{running ? t('mcp.running') : t('mcp.stopped')}</span>
        </div>
      </SettingRow>

      {toggleError && <p className="select-text text-xs text-red-500">{toggleError}</p>}

      <SettingRow label={t('mcp.permission.title')} hint={t('mcp.permission.applyHint')}>
        <div className="space-y-2 pt-1">
          {MCP_PERMISSION_MODES.map(({ value, labelKey, hintKey }) => {
            const selected = (settings.mcpPermissionMode ?? 'safe_write') === value;
            return (
              <div
                key={value}
                className={`flex items-center rounded-md border px-3 py-2 transition-colors ${
                  selected ? 'border-accent bg-accent/5' : 'border-edge bg-surface'
                }`}
              >
                <label className="flex min-w-0 flex-1 cursor-pointer items-center gap-3">
                  <input
                    type="radio"
                    name="mcp-permission-mode"
                    value={value}
                    checked={selected}
                    onChange={() => void handlePermissionChange(value)}
                    className="accent-accent"
                  />
                  <span className="text-sm text-fg">{t(labelKey)}</span>
                </label>
                <SettingHint label={t(labelKey)} text={t(hintKey)} />
              </div>
            );
          })}
        </div>
      </SettingRow>

      <SettingRow
        label={t('mcp.allowlist.title')}
        hint={`${t('mcp.allowlist.description')} ${t('mcp.allowlist.applyHint')}`}
      >
        <div className="space-y-2 pt-1">
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
        </div>
      </SettingRow>

      <SettingRow
        label={t('mcp.scope.title')}
        hint={`${t('mcp.scope.description')} ${t('mcp.scope.hint')}`}
      >
        <div className="space-y-3 pt-1">
          <label className="block space-y-1">
            <span className="text-xs text-fg-muted">{t('mcp.scope.databases')}</span>
            <textarea
              value={databaseScope}
              onChange={(event) => setDatabaseScope(event.target.value)}
              placeholder={t('mcp.scope.placeholder')}
              rows={2}
              className="w-full rounded-md border border-edge bg-surface px-3 py-2 font-mono text-xs text-fg placeholder:text-fg-muted focus:border-accent focus:outline-none"
            />
          </label>
          <label className="block space-y-1">
            <span className="text-xs text-fg-muted">{t('mcp.scope.namespaces')}</span>
            <textarea
              value={namespaceScope}
              onChange={(event) => setNamespaceScope(event.target.value)}
              placeholder={t('mcp.scope.placeholder')}
              rows={2}
              className="w-full rounded-md border border-edge bg-surface px-3 py-2 font-mono text-xs text-fg placeholder:text-fg-muted focus:border-accent focus:outline-none"
            />
          </label>
          <div className="flex items-center gap-3">
            {scopeSaved && <span className="text-xs text-green-500">{t('mcp.scope.saved')}</span>}
            {!onSettingsChange && (
              <Button
                variant="primary"
                disabled={!scopeDirty}
                onClick={() => void handleSaveScope()}
              >
                {t('mcp.scope.save')}
              </Button>
            )}
          </div>
        </div>
      </SettingRow>

      <div className="rounded-md border border-edge bg-surface p-3 space-y-2">
        <div className="flex items-center gap-2">
          {(['cursor', 'claude'] as const).map((target) => (
            <button
              key={target}
              type="button"
              onClick={() => setAgentTarget(target)}
              className={cn(
                'rounded px-2 py-1 text-xs',
                agentTarget === target
                  ? 'bg-accent text-on-accent'
                  : 'bg-surface-raised text-fg-secondary hover:text-fg',
              )}
            >
              {t(target === 'cursor' ? 'mcp.config.cursor' : 'mcp.config.claude')}
            </button>
          ))}
          <SettingHint
            label={t(agentTarget === 'cursor' ? 'mcp.config.cursor' : 'mcp.config.claude')}
            text={`${t('mcp.usage')} ${t('mcp.config.commandHint')}`}
          />
          <button
            type="button"
            onClick={handleCopySnippet}
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
      </div>

      {allTools.length > 0 && (
        <>
          <div className="flex items-center justify-between">
            <h3 className="text-sm font-medium text-fg">
              {t('mcp.tools')}
              <SettingHint
                label={t('mcp.tools')}
                text={`${t('mcp.tools.description')} ${t('mcp.tools.applyHint')}`}
              />
            </h3>
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

          <div className="flex items-center gap-3">
            {!onSettingsChange && saved && (
              <span className="text-xs text-green-500">{t('mcp.saved')}</span>
            )}
            {!onSettingsChange && (
              <Button
                variant="primary"
                disabled={!toolsDirty}
                onClick={() => void handleSaveTools()}
              >
                {t('common.save')}
              </Button>
            )}
          </div>
        </>
      )}

      {allTools.length === 0 && toolsDirty && (
        <div className="flex items-center gap-3">
          {!onSettingsChange && saved && (
            <span className="text-xs text-green-500">{t('mcp.saved')}</span>
          )}
          {!onSettingsChange && (
            <Button variant="primary" onClick={() => void handleSaveTools()}>
              {t('common.save')}
            </Button>
          )}
        </div>
      )}

      <div className="space-y-2">
        <div className="flex items-center justify-between gap-3">
          <div>
            <h3 className="text-sm font-medium text-fg">{t('mcp.audit.title')}</h3>
            <p className="text-xs text-fg-muted">{t('mcp.audit.description')}</p>
          </div>
          <button
            type="button"
            className="shrink-0 text-xs text-accent hover:underline disabled:opacity-50"
            disabled={auditLoading}
            onClick={() => void loadAudit()}
          >
            {t('mcp.audit.refresh')}
          </button>
        </div>
        {auditError && <p className="select-text text-xs text-red-500">{auditError}</p>}
        {!auditError && auditRows.length === 0 && !auditLoading && (
          <p className="text-xs text-fg-muted">{t('mcp.audit.empty')}</p>
        )}
        {auditRows.length > 0 && (
          <div className="overflow-x-auto rounded-md border border-edge">
            <table className="w-full text-left text-xs">
              <thead className="bg-surface-raised text-fg-muted">
                <tr>
                  <th className="px-2 py-1.5 font-medium">{t('mcp.audit.when')}</th>
                  <th className="px-2 py-1.5 font-medium">{t('mcp.audit.who')}</th>
                  <th className="px-2 py-1.5 font-medium">{t('mcp.audit.connection')}</th>
                  <th className="px-2 py-1.5 font-medium">{t('mcp.audit.tool')}</th>
                  <th className="px-2 py-1.5 font-medium">{t('mcp.audit.decision')}</th>
                  <th className="px-2 py-1.5 font-medium">{t('mcp.audit.requestId')}</th>
                  <th className="px-2 py-1.5 font-medium">{t('mcp.audit.rows')}</th>
                </tr>
              </thead>
              <tbody>
                {auditRows.map((row) => (
                  <tr key={row.id} className="border-t border-edge">
                    <td className="whitespace-nowrap px-2 py-1.5 font-mono text-fg-muted">
                      {row.createdAt}
                    </td>
                    <td className="px-2 py-1.5 font-mono text-fg">{row.principalId}</td>
                    <td className="px-2 py-1.5 font-mono text-fg">
                      {row.connectionId ?? '—'}
                      {row.databaseName ? `/${row.databaseName}` : ''}
                      {row.namespace ? `/${row.namespace}` : ''}
                    </td>
                    <td className="px-2 py-1.5 font-mono text-fg">{row.tool}</td>
                    <td className="px-2 py-1.5 text-fg">{row.decision}</td>
                    <td className="px-2 py-1.5 font-mono text-fg-muted">{row.requestId ?? '—'}</td>
                    <td className="px-2 py-1.5 text-fg">{row.rowCount ?? '—'}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </div>
    </>
  );
}
