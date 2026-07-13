import type { TranslationKey } from '../../locales';
import { ChevronDown, ChevronRight } from 'lucide-react';
import { Select } from '../ui/Select';
import { useI18n } from '../../hooks/useI18n';
import { cn } from '../../lib/cn';
import { COLOR_KEYS, Label } from './shared';
import { SshTunnelFields } from './SshTunnelFields';
import type { ConnectionFormState } from './useConnectionForm';
import type { SslMode } from '../../types';

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

  return (
    <>
      <button
        type="button"
        className={cn(
          'flex w-full items-center gap-2 rounded-md border border-edge bg-surface px-3 py-2.5 text-sm text-fg-secondary hover:text-fg',
          isWindow ? 'mt-5' : 'mt-4',
        )}
        onClick={() => form.setShowAdvanced((v) => !v)}
      >
        {form.showAdvanced ? <ChevronDown className="h-4 w-4" /> : <ChevronRight className="h-4 w-4" />}
        {t('newConn.advanced')}
        {form.sshEnabled && (
          <span className="ml-auto rounded bg-blue-500/20 px-1.5 py-0.5 text-xs text-blue-400">SSH</span>
        )}
      </button>

      {form.showAdvanced && (
        <div
          className={cn(
            'mt-3 space-y-4 rounded-md border border-edge p-4',
            isWindow ? 'bg-surface' : 'bg-surface-alt',
          )}
        >
          <SshTunnelFields
            form={form}
            innerPanelClassName={isWindow ? 'bg-surface-alt' : 'bg-surface'}
          />

          {form.meta.supportsSSL && (
            <div>
              <Label>{t('newConn.sslMode')}</Label>
              <Select
                value={form.sslMode}
                options={form.sslOptions}
                onChange={(v) => form.setSslMode(v as SslMode)}
              />
            </div>
          )}

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
              <div>
                <Label>{t('newConn.group')}</Label>
                <Select value={form.group} options={groupOptions} onChange={form.setGroup} />
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
    </>
  );
}
