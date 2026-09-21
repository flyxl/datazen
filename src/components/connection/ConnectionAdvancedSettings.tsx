import { useEffect, useState } from 'react';
import type { TranslationKey } from '../../locales';
import { ChevronDown, ChevronRight } from 'lucide-react';
import { Select } from '../ui/Select';
import { useI18n } from '../../hooks/useI18n';
import { cn } from '../../lib/cn';
import { COLOR_KEYS, Label } from './shared';
import { SshTunnelFields } from './SshTunnelFields';
import { HttpProxyTunnelFields } from './HttpProxyTunnelFields';
import { WebSocketTunnelFields } from './WebSocketTunnelFields';
import { getDriverConnectionAdvanced } from '../../extensions/generated';
import type { ConnectionFormState } from './useConnectionForm';
import type { SslMode, TunnelKind } from '../../types';

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
  const [showTunnel, setShowTunnel] = useState(form.tunnelKind !== 'none' || form.sshEnabled);

  useEffect(() => {
    if (form.tunnelKind !== 'none' || form.sshEnabled) setShowTunnel(true);
  }, [form.tunnelKind, form.sshEnabled]);

  const tunnelOptions: { value: TunnelKind; label: string }[] = [
    { value: 'none', label: t('newConn.tunnelNone') },
    ...(form.supportsSSH ? [{ value: 'ssh' as const, label: t('newConn.tunnelSsh') }] : []),
    { value: 'httpProxy', label: t('newConn.tunnelHttpProxy') },
    { value: 'websocket', label: t('newConn.tunnelWebSocket') },
  ];

  const tunnelBadge =
    form.tunnelKind === 'ssh'
      ? 'SSH'
      : form.tunnelKind === 'httpProxy'
        ? 'HTTP'
        : form.tunnelKind === 'websocket'
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
        >
          <div data-testid="new-conn-tunnel-kind">
            <Label>{t('newConn.tunnelKind')}</Label>
            <Select
              value={form.tunnelKind}
              options={tunnelOptions}
              onChange={(v) => {
                form.setTunnelId(null);
                form.setTunnelKind(v as TunnelKind);
              }}
            />
          </div>

          {form.savedTunnels && form.savedTunnels.length > 0 && (
            <div data-testid="new-conn-saved-tunnel">
              <Label>{t('newConn.savedTunnel')}</Label>
              <Select
                value={form.tunnelId ?? ''}
                options={[
                  { value: '', label: t('newConn.savedTunnelNone') },
                  ...form.savedTunnels.map((tun) => ({
                    value: tun.id,
                    label: `${tun.name} (${tun.kind})`,
                  })),
                ]}
                onChange={(v) => form.setTunnelId(v || null)}
              />
            </div>
          )}

          {!form.tunnelId && form.tunnelKind === 'ssh' && form.supportsSSH && (
            <SshTunnelFields
              form={form}
              innerPanelClassName={isWindow ? 'bg-surface-alt' : 'bg-surface'}
            />
          )}
          {!form.tunnelId && form.tunnelKind === 'httpProxy' && (
            <HttpProxyTunnelFields form={form} />
          )}
          {!form.tunnelId && form.tunnelKind === 'websocket' && (
            <WebSocketTunnelFields form={form} />
          )}
        </div>
      )}
    </>
  );
}
