import { useMemo, useState } from 'react';
import { Input } from '../../../../src/components/ui/Input';
import { PathInput } from '../../../../src/components/ui/PathInput';
import { Button } from '../../../../src/components/ui/Button';
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

const STEPS = ['topology', 'endpoints', 'tls'] as const;
type WizardStep = (typeof STEPS)[number];

export const redisValidate: PluginFormValidator = (fields, t) =>
  validateRedisConnection(fields, t);

export function RedisConnectionWizard({ form }: { form: ConnectionFormState }) {
  const { t } = useI18n();
  const [step, setStep] = useState<WizardStep>('topology');

  const redisOptions = useMemo(
    () => readRedisOptions(form.options),
    [form.options],
  );
  const topology = redisOptions.topology ?? 'standalone';
  const stepIndex = STEPS.indexOf(step);

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

  return (
    <div className="md:col-span-2 space-y-4">
      <div className="flex flex-wrap items-center gap-2 text-xs text-fg-muted">
        {STEPS.map((name, idx) => (
          <span key={name} className="inline-flex items-center gap-2">
            <span
              className={
                idx === stepIndex
                  ? 'font-medium text-fg'
                  : idx < stepIndex
                    ? 'text-fg-secondary'
                    : undefined
              }
            >
              {idx + 1}. {t(`redis.wizard.${name}` as 'redis.wizard.topology')}
            </span>
            {idx < STEPS.length - 1 && <span className="text-edge">→</span>}
          </span>
        ))}
      </div>

      {step === 'topology' && (
        <div className="grid grid-cols-1 gap-2 md:grid-cols-3">
          {(['standalone', 'cluster', 'sentinel'] as RedisTopology[]).map((value) => (
            <button
              key={value}
              type="button"
              className={`rounded-md border px-3 py-3 text-left text-sm transition-colors ${
                topology === value
                  ? 'border-blue-500/50 bg-blue-500/10 text-fg'
                  : 'border-edge bg-surface-alt text-fg-secondary hover:border-edge-strong hover:text-fg'
              }`}
              onClick={() => setTopology(value)}
            >
              <div className="font-medium">{t(`redis.wizard.topology${capitalize(value)}` as 'redis.wizard.topologyStandalone')}</div>
              <div className="mt-1 text-xs text-fg-muted">
                {t(`redis.wizard.topology${capitalize(value)}Hint` as 'redis.wizard.topologyStandaloneHint')}
              </div>
            </button>
          ))}
        </div>
      )}

      {step === 'endpoints' && topology === 'standalone' && (
        <div className="grid grid-cols-1 gap-3 md:grid-cols-2">
          <div>
            <Label required>{t('newConn.host')}</Label>
            <Input
              value={form.host}
              onChange={(e) => form.setHost(e.target.value)}
              placeholder="127.0.0.1"
              className={form.validationErrors.host ? 'border-red-500' : ''}
            />
            {form.validationErrors.host && (
              <p className="mt-1 text-xs text-red-400">{form.validationErrors.host}</p>
            )}
          </div>
          <div>
            <Label required>{t('newConn.port')}</Label>
            <Input
              value={form.port}
              onChange={(e) => form.setPort(e.target.value)}
              className={form.validationErrors.port ? 'border-red-500' : ''}
            />
            {form.validationErrors.port && (
              <p className="mt-1 text-xs text-red-400">{form.validationErrors.port}</p>
            )}
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
          <div className="md:col-span-2">
            <Label>{t('newConn.password')}</Label>
            <Input
              type="password"
              value={form.password}
              onChange={(e) => form.setPassword(e.target.value)}
            />
          </div>
        </div>
      )}

      {step === 'endpoints' && topology === 'cluster' && (
        <div className="space-y-3">
          <div>
            <Label required>{t('redis.wizard.clusterNodes')}</Label>
            <textarea
              value={formatNodeLines(redisOptions.clusterNodes)}
              onChange={(e) =>
                updateOptions({ clusterNodes: parseNodeLines(e.target.value) })
              }
              rows={4}
              placeholder={'10.0.0.1:7000\n10.0.0.2:7000'}
              className={`w-full rounded-md border bg-surface px-3 py-2 font-mono text-xs text-fg outline-none ${
                form.validationErrors.clusterNodes ? 'border-red-500' : 'border-edge'
              }`}
            />
            {form.validationErrors.clusterNodes && (
              <p className="mt-1 text-xs text-red-400">{form.validationErrors.clusterNodes}</p>
            )}
            <p className="mt-1 text-xs text-fg-muted">{t('redis.wizard.nodeListHint')}</p>
          </div>
          <div className="grid grid-cols-1 gap-3 md:grid-cols-2">
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
          </div>
        </div>
      )}

      {step === 'endpoints' && topology === 'sentinel' && (
        <div className="space-y-3">
          <div>
            <Label required>{t('redis.wizard.sentinelMasterName')}</Label>
            <Input
              value={redisOptions.sentinelMasterName ?? ''}
              onChange={(e) => updateOptions({ sentinelMasterName: e.target.value })}
              placeholder="mymaster"
              className={form.validationErrors.sentinelMasterName ? 'border-red-500' : ''}
            />
            {form.validationErrors.sentinelMasterName && (
              <p className="mt-1 text-xs text-red-400">
                {form.validationErrors.sentinelMasterName}
              </p>
            )}
          </div>
          <div>
            <Label required>{t('redis.wizard.sentinelNodes')}</Label>
            <textarea
              value={formatNodeLines(redisOptions.sentinelNodes)}
              onChange={(e) =>
                updateOptions({ sentinelNodes: parseNodeLines(e.target.value) })
              }
              rows={4}
              placeholder={'127.0.0.1:26379\n127.0.0.1:26380'}
              className={`w-full rounded-md border bg-surface px-3 py-2 font-mono text-xs text-fg outline-none ${
                form.validationErrors.sentinelNodes ? 'border-red-500' : 'border-edge'
              }`}
            />
            {form.validationErrors.sentinelNodes && (
              <p className="mt-1 text-xs text-red-400">{form.validationErrors.sentinelNodes}</p>
            )}
          </div>
          <div className="grid grid-cols-1 gap-3 md:grid-cols-2">
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
          </div>
        </div>
      )}

      {step === 'tls' && (
        <div className="space-y-3">
          {topology === 'sentinel' &&
            (redisOptions.tls?.caPath || redisOptions.tls?.certPath) && (
              <div className="rounded-md border border-amber-500/30 bg-amber-500/10 px-3 py-2 text-xs text-amber-300">
                {t('redis.wizard.sentinelMtlsLimitation')}
              </div>
            )}
          <label className="flex items-start gap-2 text-sm">
            <input
              type="checkbox"
              checked={redisOptions.tls?.enabled === true}
              onChange={(e) => updateOptions({ tls: { enabled: e.target.checked } })}
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
          <label className="flex items-start gap-2 text-sm text-amber-400/90">
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
      )}

      <div className="flex items-center gap-2">
        {stepIndex > 0 && (
          <Button
            variant="secondary"
            className="h-8 px-3 text-xs"
            type="button"
            onClick={() => setStep(STEPS[stepIndex - 1])}
          >
            {t('redis.wizard.back')}
          </Button>
        )}
        {stepIndex < STEPS.length - 1 && (
          <Button
            variant="primary"
            className="h-8 px-3 text-xs"
            type="button"
            onClick={() => setStep(STEPS[stepIndex + 1])}
          >
            {t('redis.wizard.next')}
          </Button>
        )}
      </div>
    </div>
  );
}

function capitalize(value: RedisTopology): string {
  return value.charAt(0).toUpperCase() + value.slice(1);
}

export { validateRedisConnection };
