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
import type { AppSettings, McpAuditRecord, McpHttpStatus, McpPermissionMode } from '../../types';
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
  const [httpStatus, setHttpStatus] = useState<McpHttpStatus | null>(null);
  const [httpPort, setHttpPort] = useState(String(settings.mcpHttpPort ?? 0));
  const [httpToggling, setHttpToggling] = useState(false);
  const [httpError, setHttpError] = useState<string | null>(null);
  const [httpToken, setHttpToken] = useState<string | null>(null);
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

  useEffect(() => {
    setHttpPort(String(settings.mcpHttpPort ?? 0));
  }, [settings.mcpHttpPort]);

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

  const refreshHttpStatus = async () => {
    try {
      setHttpStatus(await aiCommands.mcpHttpStatus());
    } catch {
      setHttpStatus(null);
    }
  };

  useEffect(() => {
    void refreshStatus();
    void refreshHttpStatus();
    const id = window.setInterval(() => {
      void refreshStatus();
      void refreshHttpStatus();
    }, 3000);
    return () => window.clearInterval(id);
  }, []);

  const reloadMcpIfRunning = async () => {
    if (!running && !httpStatus?.running) return;
    await aiCommands.mcpReload();
    await Promise.all([refreshStatus(), refreshHttpStatus()]);
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

  const parseHttpPort = (): number => {
    const value = Number(httpPort);
    if (!Number.isInteger(value) || value < 0 || value > 65535) return -1;
    return value;
  };

  const handleToggleHttp = async (enabled: boolean) => {
    setHttpToggling(true);
    setHttpError(null);
    try {
      if (enabled) {
        const port = parseHttpPort();
        if (port < 0) throw new Error(t('mcp.http.portInvalid'));
        const next = await aiCommands.mcpHttpEnable(port);
        setHttpStatus(next);
        if (next.token) setHttpToken(next.token);
        updateAppSettings({ mcpHttpEnabled: true, mcpHttpPort: next.configuredPort });
      } else {
        const next = await aiCommands.mcpHttpDisable();
        setHttpStatus(next);
        setHttpToken(null);
        updateAppSettings({ mcpHttpEnabled: false });
      }
    } catch (e) {
      setHttpError(e instanceof Error ? e.message : t('mcp.http.toggleError'));
      if (!enabled) await refreshHttpStatus();
    } finally {
      setHttpToggling(false);
    }
  };

  const handleApplyHttpPort = async () => {
    const port = parseHttpPort();
    if (port < 0) {
      setHttpError(t('mcp.http.portInvalid'));
      return;
    }
    setHttpToggling(true);
    setHttpError(null);
    try {
      updateAppSettings({ mcpHttpPort: port });
      if (httpStatus?.running || settings.mcpHttpEnabled) {
        const next = await aiCommands.mcpHttpEnable(port);
        setHttpStatus(next);
        if (next.token) setHttpToken(next.token);
      } else {
        await refreshHttpStatus();
      }
    } catch (e) {
      setHttpError(e instanceof Error ? e.message : t('mcp.http.toggleError'));
    } finally {
      setHttpToggling(false);
    }
  };

  const handleRegenerateHttpToken = async () => {
    setHttpToggling(true);
    setHttpError(null);
    setHttpToken(null);
    try {
      const next = await aiCommands.mcpHttpRegenerateToken();
      setHttpStatus(next);
      setHttpToken(next.token ?? null);
    } catch (e) {
      setHttpError(e instanceof Error ? e.message : t('mcp.http.toggleError'));
    } finally {
      setHttpToggling(false);
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

      <SettingRow label={t('mcp.http.title')} hint={t('mcp.http.description')}>
        <div className="space-y-3 pt-1">
          <div className="flex items-center justify-between gap-3 rounded-md border border-edge bg-surface px-3 py-2">
            <div className="min-w-0">
              <div className="text-sm text-fg">{t('mcp.http.enable')}</div>
              <div className="text-xs text-fg-muted">{t('mcp.http.enableHint')}</div>
            </div>
            <input
              type="checkbox"
              role="switch"
              aria-label={t('mcp.http.enable')}
              data-testid="mcp-http-toggle"
              checked={httpStatus?.enabled ?? settings.mcpHttpEnabled ?? false}
              disabled={httpToggling}
              onChange={(event) => void handleToggleHttp(event.target.checked)}
              className="accent-accent"
            />
          </div>

          <div className="flex flex-wrap items-end gap-2">
            <label className="space-y-1">
              <span className="block text-xs text-fg-muted">{t('mcp.http.port')}</span>
              <input
                type="number"
                min={0}
                max={65535}
                aria-label={t('mcp.http.port')}
                data-testid="mcp-http-port"
                value={httpPort}
                disabled={httpToggling}
                onChange={(event) => setHttpPort(event.target.value)}
                className="w-32 rounded-md border border-edge bg-surface px-3 py-2 font-mono text-xs text-fg focus:border-accent focus:outline-none"
              />
            </label>
            <Button
              variant="secondary"
              data-testid="mcp-http-apply-port"
              disabled={httpToggling}
              onClick={() => void handleApplyHttpPort()}
            >
              {t('mcp.http.applyPort')}
            </Button>
            <span className="text-xs text-fg-muted">{t('mcp.http.portHint')}</span>
          </div>

          <div className="space-y-1 text-xs text-fg-muted">
            <div className="flex items-center gap-2">
              <span
                className={`inline-block h-2 w-2 rounded-full ${
                  httpStatus?.running ? 'bg-green-500' : 'bg-fg-muted'
                }`}
              />
              <span>{httpStatus?.running ? t('mcp.http.running') : t('mcp.http.stopped')}</span>
              {httpStatus?.url && (
                <code className="select-text break-all rounded bg-surface px-1 py-0.5 text-fg">
                  {httpStatus.url}
                </code>
              )}
            </div>
            {httpStatus?.running && httpStatus.port != null && (
              <div>{t('mcp.http.actualPort').replace('{port}', String(httpStatus.port))}</div>
            )}
          </div>

          <div className="space-y-2 rounded-md border border-edge bg-surface p-3">
            <div className="flex flex-wrap items-center justify-between gap-2">
              <div className="text-xs text-fg-muted">
                {httpStatus?.tokenLast4
                  ? t('mcp.http.tokenLast4').replace('{last4}', httpStatus.tokenLast4)
                  : t('mcp.http.noToken')}
              </div>
              <Button
                variant="secondary"
                data-testid="mcp-http-regenerate"
                disabled={httpToggling}
                onClick={() => void handleRegenerateHttpToken()}
              >
                {httpStatus?.tokenConfigured
                  ? t('mcp.http.regenerateToken')
                  : t('mcp.http.generateToken')}
              </Button>
            </div>
            {httpToken && (
              <div className="space-y-1">
                <div className="text-xs font-medium text-amber-500">{t('mcp.http.tokenOnce')}</div>
                <div className="flex gap-2">
                  <input
                    readOnly
                    value={httpToken}
                    data-testid="mcp-http-token"
                    aria-label={t('mcp.http.token')}
                    className="min-w-0 flex-1 rounded-md border border-edge bg-surface-inset px-3 py-2 font-mono text-xs text-fg"
                  />
                  <Button
                    variant="secondary"
                    data-testid="mcp-http-copy"
                    onClick={() => copy(httpToken)}
                  >
                    {copied ? t('mcp.http.copied') : t('mcp.http.copyToken')}
                  </Button>
                </div>
              </div>
            )}
          </div>
          {httpError && <p className="select-text text-xs text-red-500">{httpError}</p>}
        </div>
      </SettingRow>

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
