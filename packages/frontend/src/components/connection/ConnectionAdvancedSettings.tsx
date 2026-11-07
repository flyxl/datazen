import { useEffect, useState } from 'react';
import type { TranslationKey } from '../../locales';
import { ChevronDown, ChevronRight } from 'lucide-react';
import { Select } from '../ui/Select';
import { Button } from '../ui/Button';
import { ErrorBanner } from '../ui/ErrorBanner';
import { useI18n } from '../../hooks/useI18n';
import { cn } from '../../lib/cn';
import { openSettingsWindow } from '../../lib/windowManager';
import { COLOR_KEYS, Label } from './shared';
import { SshTunnelFields } from './SshTunnelFields';
import { HttpProxyTunnelFields } from './HttpProxyTunnelFields';
import { WebSocketTunnelFields } from './WebSocketTunnelFields';
import { SaveTunnelDialog } from './SaveTunnelDialog';
import { getDriverConnectionAdvanced } from '../../extensions/generated';
import type { ConnectionFormState } from './useConnectionForm';
import type { SslMode, TunnelKind, TunnelSource } from '../../types';

export interface ConnectionAdvancedSettingsProps {
  form: ConnectionFormState;
  groupOptions?: { value: string; label: string }[];
  variant?: 'dialog' | 'window';
}

export function ConnectionAdvancedSettings({
  form,
  groupOptions,
  variant = 'dialog',
}: ConnectionAdvancedSettingsProps) {
  const { t } = useI18n();
  const isWindow = variant === 'window';
  const DriverAdvanced = getDriverConnectionAdvanced(form.formVariant);
  const [showTunnel, setShowTunnel] = useState(
    form.tunnelSource !== 'none' || form.tunnelId !== null,
  );
  const [saveDialogOpen, setSaveDialogOpen] = useState(false);

  useEffect(() => {
    if (form.tunnelSource !== 'none' || form.tunnelId !== null) setShowTunnel(true);
  }, [form.tunnelSource, form.tunnelId]);

  /** Tunnel source is the single master control; `tunnelKind` is its inline sub-selector. */
  const sourceOptions: { value: TunnelSource; label: string; disabled?: boolean }[] = [
    { value: 'none', label: t('newConn.tunnelNone') },
    {
      value: 'saved',
      label: t('newConn.savedTunnel'),
      disabled: form.savedTunnels.length === 0,
    },
    { value: 'inline', label: t('newConn.savedTunnelNone') },
  ];

  const inlineKindOptions: { value: TunnelKind; label: string }[] = [
    ...(form.supportsSSH ? [{ value: 'ssh' as const, label: t('newConn.tunnelSsh') }] : []),
    { value: 'httpProxy' as const, label: t('newConn.tunnelHttpProxy') },
    { value: 'websocket' as const, label: t('newConn.tunnelWebSocket') },
  ];

  const savedTunnelOptions = form.savedTunnels.map((tunnel) => ({
    value: tunnel.id,
    label: `${tunnel.name} (${tunnel.kind})`,
  }));

  const badgeKind = form.effectiveTunnelKind;
  const tunnelBadge =
    badgeKind === 'ssh'
      ? 'SSH'
      : badgeKind === 'httpProxy'
        ? 'HTTP'
        : badgeKind === 'websocket'
          ? 'WS'
          : null;

  return (
    <>
      <button
        type="button"
        className={cn(
          'flex w-full items-center gap-2 rounded-md border border-edge bg-surface px-3 py-2.5 text-sm text-fg-secondary hover:text-fg',
          isWindow ? 'mt-5' : 'mt-4',
        )}
        onClick={() => form.setShowAdvanced((v) => !v)}
        data-testid="new-conn-advanced-toggle"
      >
        {form.showAdvanced ? (
          <ChevronDown className="h-4 w-4" />
        ) : (
          <ChevronRight className="h-4 w-4" />
        )}
        {t('newConn.advanced')}
      </button>

      {form.showAdvanced && (
        <div
          className={cn(
            'mt-3 space-y-4 rounded-md border border-edge p-4',
            isWindow ? 'bg-surface' : 'bg-surface-alt',
          )}
        >
          {DriverAdvanced ? (
            <DriverAdvanced form={form} />
          ) : form.supportsSSL ? (
            <div data-testid="new-conn-ssl-mode">
              <Label>{t('newConn.sslMode')}</Label>
              <Select
                value={form.sslMode}
                options={form.sslOptions}
                onChange={(v) => form.setSslMode(v as SslMode)}
              />
            </div>
          ) : null}

          <label
            className={cn(
              'flex items-start gap-2 text-sm text-fg-secondary',
              form.driverReadOnly && 'cursor-not-allowed opacity-80',
            )}
          >
            <input
              type="checkbox"
              className="mt-0.5 accent-accent"
              checked={form.driverReadOnly || form.readOnly}
              disabled={form.driverReadOnly}
              onChange={(e) => {
                if (!form.driverReadOnly) {
                  form.setReadOnly(e.target.checked);
                }
              }}
            />
            <span>
              <span className="block">{t('newConn.readOnly')}</span>
              <span className="block text-[11px] text-fg-muted">
                {form.driverReadOnly
                  ? t('newConn.driverReadOnlyLocked')
                  : t('newConn.readOnlyHint')}
              </span>
            </span>
          </label>

          {groupOptions ? (
            <div className="grid grid-cols-1 gap-4 md:grid-cols-2">
              <div>
                <Label>{t('newConn.colorTag')}</Label>
                <div className="flex items-center gap-2 pt-1">
                  {COLOR_KEYS.map((c) => (
                    <button
                      key={c.value}
                      type="button"
                      title={t(c.key as TranslationKey)}
                      onClick={() => form.setColorTag(c.value)}
                      className={cn(
                        'h-6 w-6 rounded-full border-2 transition-transform hover:scale-110',
                        form.colorTag === c.value
                          ? isWindow
                            ? 'border-fg scale-110'
                            : 'border-white scale-110'
                          : 'border-transparent',
                      )}
                      style={{ backgroundColor: c.value }}
                    />
                  ))}
                </div>
              </div>
              <div data-testid="new-conn-group">
                <Label>{t('newConn.group')}</Label>
                <Select
                  value={form.group}
                  options={groupOptions}
                  onChange={(value) => form.setGroup(value)}
                />
              </div>
            </div>
          ) : (
            <div>
              <Label>{t('newConn.colorTag')}</Label>
              <div className="flex items-center gap-2">
                {COLOR_KEYS.map((c) => (
                  <button
                    key={c.value}
                    type="button"
                    title={t(c.key as TranslationKey)}
                    onClick={() => form.setColorTag(c.value)}
                    className={cn(
                      'h-6 w-6 rounded-full border-2 transition-transform hover:scale-110',
                      form.colorTag === c.value ? 'border-white scale-110' : 'border-transparent',
                    )}
                    style={{ backgroundColor: c.value }}
                  />
                ))}
              </div>
            </div>
          )}
        </div>
      )}

      <button
        type="button"
        className="mt-3 flex w-full items-center gap-2 rounded-md border border-edge bg-surface px-3 py-2.5 text-sm text-fg-secondary hover:text-fg"
        onClick={() => setShowTunnel((value) => !value)}
        data-testid="new-conn-tunnel-toggle"
        aria-expanded={showTunnel}
      >
        {showTunnel ? <ChevronDown className="h-4 w-4" /> : <ChevronRight className="h-4 w-4" />}
        {t('newConn.tunnel')}
        {tunnelBadge && (
          <span className="ml-auto rounded bg-accent/20 px-1.5 py-0.5 text-xs text-accent">
            {tunnelBadge}
          </span>
        )}
      </button>

      {showTunnel && (
        <div
          className={cn(
            'mt-3 space-y-4 rounded-md border border-edge p-4',
            isWindow ? 'bg-surface' : 'bg-surface-alt',
          )}
          data-testid="new-conn-tunnel-panel"
        >
          {form.tunnelRefMissing && (
            /* Deliberately not an ErrorBanner: an amber notice is a *warning*
               ("you picked a tunnel that no longer exists"), announced as one.
               Routing it through the error component would mislabel it. */
            <div
              role="alert"
              data-testid="new-conn-tunnel-missing"
              className="rounded-md border border-amber-500/30 bg-amber-500/10 p-2.5 text-xs text-amber-300"
            >
              {t('newConn.tunnelMissing')}
              {/* Only mention picking another tunnel when one is actually listed:
                  the guidance must never point at an unreachable action (BUG-002). */}
              {form.savedTunnels.length > 0 && ` ${t('newConn.tunnelMissingAlt')}`}
            </div>
          )}

          <div data-testid="new-conn-tunnel-source">
            <Label>{t('newConn.tunnelSource')}</Label>
            <Select
              value={form.tunnelSource}
              options={sourceOptions}
              onChange={(v) => form.setTunnelSource(v as TunnelSource)}
            />
          </div>

          {form.tunnelSource !== 'inline' && form.savedTunnels.length === 0 && (
            <div
              data-testid="new-conn-tunnel-empty"
              className="space-y-2 rounded-md border border-dashed border-edge p-3"
            >
              <p className="text-xs text-fg-muted">{t('newConn.tunnelEmptyHint')}</p>
              <div className="flex flex-wrap items-center gap-2">
                <Button
                  size="sm"
                  variant="secondary"
                  onClick={() => form.setTunnelSource('inline')}
                  data-testid="new-conn-tunnel-create-entry"
                >
                  {t('newConn.tunnelCreateEntry')}
                </Button>
                <Button
                  size="sm"
                  variant="ghost"
                  onClick={() => openSettingsWindow('tunnels')}
                  data-testid="new-conn-tunnel-manage-entry"
                >
                  {t('newConn.tunnelManage')}
                </Button>
              </div>
            </div>
          )}

          {form.tunnelSource === 'saved' && (
            <div className="space-y-3" data-testid="new-conn-saved-tunnel">
              <p className="text-xs text-fg-muted">{t('newConn.tunnelSavedHint')}</p>
              <div>
                <Label>{t('newConn.savedTunnel')}</Label>
                <Select
                  value={form.tunnelId ?? ''}
                  options={savedTunnelOptions}
                  onChange={(v) => form.setTunnelId(v || null)}
                />
              </div>
              {form.savedTunnel && (
                <div
                  data-testid="new-conn-saved-tunnel-summary"
                  className="flex items-center gap-2 text-xs text-fg"
                >
                  <span className="font-medium">{form.savedTunnel.name}</span>
                  <span className="rounded bg-accent/20 px-1.5 py-0.5 text-[11px] text-accent">
                    {form.savedTunnel.kind}
                  </span>
                </div>
              )}
              {/* Unbinding refills the inline fields from the stored entity. When
                  that entity is gone there is nothing to refill, and dropping the
                  reference silently is deliberately refused — so the action does
                  not apply to this state at all and must not be offered as a
                  clickable dead end (tunnel-form-BUG-002). Switching the source to
                  "None (direct)" is the reachable exit, as the alert states. */}
              {!form.tunnelRefMissing && (
                <Button
                  size="sm"
                  variant="secondary"
                  disabled={form.tunnelBusy}
                  onClick={() => void form.unbindTunnel()}
                  data-testid="new-conn-tunnel-unbind"
                >
                  {t('newConn.tunnelUnbind')}
                </Button>
              )}
              {form.tunnelError && <ErrorBanner>{form.tunnelError}</ErrorBanner>}
            </div>
          )}

          {form.tunnelSource === 'inline' && (
            <div className="space-y-3" data-testid="new-conn-inline-tunnel">
              <div data-testid="new-conn-tunnel-kind">
                <Label>{t('newConn.tunnelKind')}</Label>
                <Select
                  value={form.tunnelKind}
                  options={inlineKindOptions}
                  onChange={(v) => form.setTunnelKind(v as TunnelKind)}
                />
              </div>

              {form.tunnelKind === 'ssh' && form.supportsSSH && (
                <SshTunnelFields
                  form={form}
                  innerPanelClassName={isWindow ? 'bg-surface-alt' : 'bg-surface'}
                />
              )}
              {form.tunnelKind === 'httpProxy' && <HttpProxyTunnelFields form={form} />}
              {form.tunnelKind === 'websocket' && <WebSocketTunnelFields form={form} />}

              <div className="flex flex-wrap items-center gap-2">
                <Button
                  size="sm"
                  variant="secondary"
                  disabled={!form.tunnelInlineValid || form.tunnelBusy}
                  onClick={() => setSaveDialogOpen(true)}
                  data-testid="new-conn-tunnel-save-as"
                >
                  {t('newConn.tunnelSaveAs')}
                </Button>
                {form.tunnelError && <ErrorBanner as="span">{form.tunnelError}</ErrorBanner>}
              </div>
            </div>
          )}

          <SaveTunnelDialog
            open={saveDialogOpen}
            onClose={() => setSaveDialogOpen(false)}
            onSubmit={form.saveAsTunnel}
            busy={form.tunnelBusy}
            error={form.tunnelError}
          />
        </div>
      )}
    </>
  );
}
