import { Input, Select } from '@datazen/ui';
import { useI18n } from '@datazen/ui';
import { Label } from '@datazen/ui';
import type { ConnectionFormState } from '@datazen/driver-sdk';
import { mergeSqlServerOptions, readSqlServerOptions } from './connectionOptions';

export function SqlServerConnectionFields({ form }: { form: ConnectionFormState }) {
  const { t } = useI18n();
  return (
    <>
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
          placeholder="1433"
          className={form.validationErrors.port ? 'border-danger' : ''}
        />
        {form.validationErrors.port && (
          <p className="mt-1 text-xs text-danger">{form.validationErrors.port}</p>
        )}
      </div>
      <div className="md:col-span-2">
        <Label>{t('newConn.database')}</Label>
        <Input value={form.database} onChange={(e) => form.setDatabase(e.target.value)} />
      </div>
      <div>
        <Label>{t('newConn.username')}</Label>
        <Input
          value={form.username}
          onChange={(e) => form.setUsername(e.target.value)}
          placeholder="sa"
        />
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
  );
}

export function SqlServerConnectionAdvanced({ form }: { form: ConnectionFormState }) {
  const { t } = useI18n();
  const options = readSqlServerOptions(form.options);
  const update = (patch: Parameters<typeof mergeSqlServerOptions>[1]) => {
    form.setOptions(mergeSqlServerOptions(form.options, patch));
  };

  return (
    <div className="space-y-4">
      <div>
        <Label>{t('sqlserver.applicationName')}</Label>
        <Input
          value={options.applicationName ?? ''}
          onChange={(e) => update({ applicationName: e.target.value })}
          placeholder="DataZen"
        />
      </div>

      <div>
        <Label>{t('sqlserver.encryption')}</Label>
        <Select
          value={form.sslMode}
          options={[
            { value: 'disable', label: t('newConn.sslNone') },
            { value: 'prefer', label: t('newConn.sslPrefer') },
            { value: 'require', label: t('newConn.sslRequire') },
          ]}
          onChange={(value) => form.setSslMode(value as typeof form.sslMode)}
        />
      </div>

      <label className="flex items-start gap-2 text-sm text-fg-secondary">
        <input
          type="checkbox"
          className="mt-0.5 accent-accent"
          checked={options.trustServerCertificate !== false}
          onChange={(e) => update({ trustServerCertificate: e.target.checked })}
        />
        <span>
          <span className="block">{t('sqlserver.trustServerCertificate')}</span>
          <span className="block text-[11px] text-fg-muted">
            {t('sqlserver.trustServerCertificateHint')}
          </span>
        </span>
      </label>
    </div>
  );
}
