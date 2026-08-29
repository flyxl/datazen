import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { Input } from '../../../../src/components/ui/Input';
import { PathInput } from '../../../../src/components/ui/PathInput';
import { Select } from '../../../../src/components/ui/Select';
import { useI18n } from '../../../../src/hooks/useI18n';
import { Label } from '../../../../src/components/connection/shared';
import type { ConnectionFormState } from '../../../../src/components/connection/useConnectionForm';
import type { PluginFormValidator } from '../../../../src/plugin-sdk';
import {
  formatNodeLines,
  parseNodeLines,
  readRedisOptions,
  type RedisTopology,
} from './connectionOptions';
import { mergeRedisOptions, validateRedisConnection } from './connectionWizardValidate';
import {
  applyRedisClipboardToForm,
  clipboardHasRedisScheme,
  isPristineRedisForm,
  looksLikeRedisClipboard,
  parseRedisClipboard,
} from './parseRedisClipboard';

const TOPOLOGIES: RedisTopology[] = ['standalone', 'cluster', 'sentinel'];

export const redisValidate: PluginFormValidator = (fields, t) =>
  validateRedisConnection(fields, t);

function useRedisForm(form: ConnectionFormState) {
  const redisOptions = useMemo(
    () => readRedisOptions(form.options),
    [form.options],
  );
  const topology = redisOptions.topology ?? 'standalone';

  const updateOptions = (patch: Parameters<typeof mergeRedisOptions>[1]) => {
    form.setOptions(mergeRedisOptions(form.options, patch));
  };

  const setTopology = (next: RedisTopology) => {
    updateOptions({ topology: next });
    if (next === 'standalone') {
      form.setHost(form.host || '127.0.0.1');
      form.setPort(form.port || '6379');
    }
  };

  return { redisOptions, topology, updateOptions, setTopology };
}

export function RedisConnectionWizard({ form }: { form: ConnectionFormState }) {
  const { t } = useI18n();
  const { redisOptions, topology, updateOptions, setTopology } = useRedisForm(form);
  const formRef = useRef(form);
  formRef.current = form;
  const autoFilledRef = useRef(false);
  const [banner, setBanner] = useState<'pasted' | 'empty' | null>(null);

  const applyClipboardText = useCallback(
    (text: string, opts: { auto: boolean; fromPasswordField?: boolean }): boolean => {
      const parsed = parseRedisClipboard(text);
      if (!parsed) return false;
      if (opts.fromPasswordField && !clipboardHasRedisScheme(text)) return false;
      if (opts.auto && !looksLikeRedisClipboard(text, parsed)) return false;
      if (opts.auto && !isPristineRedisForm(formRef.current)) return false;
      applyRedisClipboardToForm(formRef.current, parsed);
      autoFilledRef.current = true;
      setBanner('pasted');
      return true;
    },
    [],
  );

  const fillFromClipboard = useCallback(
    async (mode: 'auto' | 'button') => {
      if (mode === 'auto' && (autoFilledRef.current || !isPristineRedisForm(formRef.current))) {
        return;
      }
      try {
        const text = await navigator.clipboard.readText();
        if (!applyClipboardText(text, { auto: mode === 'auto' }) && mode === 'button') {
          setBanner('empty');
        }
      } catch {
        if (mode === 'button') setBanner('empty');
      }
    },
    [applyClipboardText],
  );

  useEffect(() => {
    void fillFromClipboard('auto');
    const onFocus = () => {
      if (!autoFilledRef.current) void fillFromClipboard('auto');
    };
    const onPaste = (event: ClipboardEvent) => {
      const text = event.clipboardData?.getData('text/plain') ?? '';
      const target = event.target;
      const fromPasswordField =
        target instanceof HTMLElement && Boolean(target.closest('input[type="password"]'));
      if (applyClipboardText(text, { auto: false, fromPasswordField })) {
        event.preventDefault();
      }
    };
    window.addEventListener('focus', onFocus);
    window.addEventListener('paste', onPaste);
    return () => {
      window.removeEventListener('focus', onFocus);
      window.removeEventListener('paste', onPaste);
    };
  }, [applyClipboardText, fillFromClipboard]);

  useEffect(() => {
    if (!banner) return;
    const timer = window.setTimeout(() => setBanner(null), 3000);
    return () => window.clearTimeout(timer);
  }, [banner]);

  return (
    <>
      <div className="md:col-span-2 flex items-center justify-between gap-3">
        <p className="text-xs text-fg-muted">{t('redis.wizard.clipboardHint')}</p>
        <button
          type="button"
          data-testid="redis-fill-clipboard"
          className="shrink-0 text-xs text-accent hover:underline"
          onClick={() => void fillFromClipboard('button')}
        >
          {t('redis.wizard.pasteClipboard')}
        </button>
      </div>
      {banner === 'pasted' && (
        <p className="md:col-span-2 text-xs text-green-400" data-testid="redis-clipboard-status">
          {t('redis.wizard.pastedFromClipboard')}
        </p>
      )}
      {banner === 'empty' && (
        <p className="md:col-span-2 text-xs text-warning" data-testid="redis-clipboard-status">
          {t('redis.wizard.clipboardEmpty')}
        </p>
      )}
      <div data-testid="redis-topology">
        <Label>{t('redis.wizard.topology')}</Label>
        <Select
          value={topology}
          title={t('redis.wizard.topology')}
          options={TOPOLOGIES.map((value) => ({
            value,
            label: t(`redis.wizard.topology${capitalize(value)}` as 'redis.wizard.topologyStandalone'),
          }))}
          onChange={(v) => setTopology(v as RedisTopology)}
        />
      </div>

      {topology === 'standalone' && (
        <>
          <div>
            <Label>{t('newConn.databaseIndex')}</Label>
            <Input
              type="number"
              min={0}
              max={15}
              value={form.database}
              onChange={(e) => form.setDatabase(e.target.value)}
              placeholder="0"
            />
          </div>
          <div>
            <Label required>{t('newConn.host')}</Label>
            <Input
              value={form.host}
              onChange={(e) => form.setHost(e.target.value)}
              placeholder="127.0.0.1"
              className={form.validationErrors.host ? 'border-danger' : ''}
            />
            {form.validationErrors.host && (
              <p className="mt-1 text-xs text-danger">{form.validationErrors.host}</p>
            )}
          </div>
          <div>
            <Label required>{t('newConn.port')}</Label>
            <Input
              value={form.port}
              onChange={(e) => form.setPort(e.target.value)}
              className={form.validationErrors.port ? 'border-danger' : ''}
            />
            {form.validationErrors.port && (
              <p className="mt-1 text-xs text-danger">{form.validationErrors.port}</p>
            )}
          </div>
          <div>
            <Label>{t('newConn.username')}</Label>
            <Input value={form.username} onChange={(e) => form.setUsername(e.target.value)} />
          </div>
          <div>
            <Label>{t('newConn.password')}</Label>
            <Input
              type="password"
              value={form.password}
              onChange={(e) => form.setPassword(e.target.value)}
            />
          </div>
        </>
      )}

      {topology === 'cluster' && (
        <>
          <div className="md:col-span-2">
            <Label required>{t('redis.wizard.clusterNodes')}</Label>
            <textarea
              value={formatNodeLines(redisOptions.clusterNodes)}
              onChange={(e) =>
                updateOptions({ clusterNodes: parseNodeLines(e.target.value) })
              }
              rows={4}
              placeholder={'10.0.0.1:7000\n10.0.0.2:7000'}
              className={`w-full rounded-md border bg-surface px-3 py-2 font-mono text-xs text-fg outline-none ${
                form.validationErrors.clusterNodes ? 'border-danger' : 'border-edge'
              }`}
            />
            {form.validationErrors.clusterNodes && (
              <p className="mt-1 text-xs text-danger">{form.validationErrors.clusterNodes}</p>
            )}
            <p className="mt-1 text-xs text-fg-muted">{t('redis.wizard.nodeListHint')}</p>
          </div>
          <div>
            <Label>{t('newConn.username')}</Label>
            <Input value={form.username} onChange={(e) => form.setUsername(e.target.value)} />
          </div>
          <div>
            <Label>{t('newConn.password')}</Label>
            <Input
              type="password"
              value={form.password}
              onChange={(e) => form.setPassword(e.target.value)}
            />
          </div>
        </>
      )}

      {topology === 'sentinel' && (
        <>
          <div>
            <Label required>{t('redis.wizard.sentinelMasterName')}</Label>
            <Input
              value={redisOptions.sentinelMasterName ?? ''}
              onChange={(e) => updateOptions({ sentinelMasterName: e.target.value })}
              placeholder="mymaster"
              className={form.validationErrors.sentinelMasterName ? 'border-danger' : ''}
            />
            {form.validationErrors.sentinelMasterName && (
              <p className="mt-1 text-xs text-danger">
                {form.validationErrors.sentinelMasterName}
              </p>
            )}
          </div>
          <div className="md:col-span-2">
            <Label required>{t('redis.wizard.sentinelNodes')}</Label>
            <textarea
              value={formatNodeLines(redisOptions.sentinelNodes)}
              onChange={(e) =>
                updateOptions({ sentinelNodes: parseNodeLines(e.target.value) })
              }
              rows={4}
              placeholder={'127.0.0.1:26379\n127.0.0.1:26380'}
              className={`w-full rounded-md border bg-surface px-3 py-2 font-mono text-xs text-fg outline-none ${
                form.validationErrors.sentinelNodes ? 'border-danger' : 'border-edge'
              }`}
            />
            {form.validationErrors.sentinelNodes && (
              <p className="mt-1 text-xs text-danger">{form.validationErrors.sentinelNodes}</p>
            )}
          </div>
          <div>
            <Label>{t('redis.wizard.sentinelNodePassword')}</Label>
            <Input
              type="password"
              value={redisOptions.sentinelNodePassword ?? ''}
              onChange={(e) => updateOptions({ sentinelNodePassword: e.target.value })}
            />
          </div>
          <div>
            <Label>{t('newConn.databaseIndex')}</Label>
            <Input
              type="number"
              min={0}
              max={15}
              value={form.database}
              onChange={(e) => form.setDatabase(e.target.value)}
              placeholder="0"
            />
          </div>
          <div>
            <Label>{t('newConn.username')}</Label>
            <Input value={form.username} onChange={(e) => form.setUsername(e.target.value)} />
          </div>
          <div>
            <Label>{t('newConn.password')}</Label>
            <Input
              type="password"
              value={form.password}
              onChange={(e) => form.setPassword(e.target.value)}
            />
          </div>
        </>
      )}
    </>
  );
}

/** TLS / mTLS fields rendered inside host Advanced settings via formVariant slot. */
export function RedisTlsFields({ form }: { form: ConnectionFormState }) {
  const { t } = useI18n();
  const { redisOptions, topology, updateOptions } = useRedisForm(form);

  return (
    <div className="space-y-3">
      <div className="text-sm font-medium text-fg">{t('redis.wizard.tls')}</div>
      {topology === 'sentinel' &&
        (redisOptions.tls?.caPath || redisOptions.tls?.certPath) && (
          <div className="rounded-md border border-warning/30 bg-warning/10 px-3 py-2 text-xs text-warning">
            {t('redis.wizard.sentinelMtlsLimitation')}
          </div>
        )}
      <label className="flex items-start gap-2 text-sm">
        <input
          type="checkbox"
          checked={redisOptions.tls?.enabled === true}
          onChange={(e) => {
            updateOptions({ tls: { enabled: e.target.checked } });
            form.setSslMode(e.target.checked ? 'require' : 'disable');
          }}
        />
        <span>{t('redis.wizard.tlsEnabled')}</span>
      </label>

      <div>
        <Label>{t('redis.wizard.tlsCa')}</Label>
        <PathInput
          value={redisOptions.tls?.caPath ?? ''}
          onChange={(path) => updateOptions({ tls: { caPath: path } })}
          placeholder="/path/to/ca.pem"
        />
      </div>
      <div>
        <Label>{t('redis.wizard.tlsCert')}</Label>
        <PathInput
          value={redisOptions.tls?.certPath ?? ''}
          onChange={(path) => updateOptions({ tls: { certPath: path } })}
          placeholder="/path/to/client.crt"
        />
      </div>
      <div>
        <Label>{t('redis.wizard.tlsKey')}</Label>
        <PathInput
          value={redisOptions.tls?.keyPath ?? ''}
          onChange={(path) => updateOptions({ tls: { keyPath: path } })}
          placeholder="/path/to/client.key"
        />
      </div>
      <div>
        <Label>{t('redis.wizard.tlsKeyPassphrase')}</Label>
        <Input
          type="password"
          value={redisOptions.tls?.keyPassphrase ?? ''}
          onChange={(e) => updateOptions({ tls: { keyPassphrase: e.target.value } })}
        />
      </div>
      <label className="flex items-start gap-2 text-sm text-warning">
        <input
          type="checkbox"
          checked={redisOptions.tls?.insecureSkipVerify === true}
          onChange={(e) =>
            updateOptions({ tls: { insecureSkipVerify: e.target.checked } })
          }
        />
        <span>{t('redis.wizard.tlsInsecureSkipVerify')}</span>
      </label>
    </div>
  );
}

function capitalize(value: RedisTopology): string {
  return value.charAt(0).toUpperCase() + value.slice(1);
}

export { validateRedisConnection };
