import { Input } from '@datazen/ui';
import { PathInput } from '../../../../src/components/ui/PathInput';
import { useI18n } from '../../../../src/hooks/useI18n';
import { Label } from '@datazen/ui';
import type { ConnectionFormState } from '../../../../src/components/connection/useConnectionForm';
import type { PluginFormValidator } from '@datazen/driver-sdk';

function readJdbcOptions(options: Record<string, unknown> | undefined) {
  const o = options ?? {};
  const jarsRaw = o.jars;
  const jars = Array.isArray(jarsRaw)
    ? jarsRaw.filter((x): x is string => typeof x === 'string')
    : typeof jarsRaw === 'string'
      ? jarsRaw
          .split(/[\n,]/)
          .map((s) => s.trim())
          .filter(Boolean)
      : [];
  return {
    jdbcUrl: typeof o.jdbcUrl === 'string' ? o.jdbcUrl : typeof o.url === 'string' ? o.url : '',
    driverClass: typeof o.driverClass === 'string' ? o.driverClass : '',
    jars,
    agentJar: typeof o.agentJar === 'string' ? o.agentJar : '',
  };
}

export const jdbcValidate: PluginFormValidator = (fields, t) => {
  const errors: Record<string, string> = {};
  const o = readJdbcOptions(fields.options as Record<string, unknown> | undefined);
  if (!o.jdbcUrl.trim()) {
    errors.jdbcUrl = t('jdbc.form.urlRequired');
  } else if (!o.jdbcUrl.trim().toLowerCase().startsWith('jdbc:')) {
    errors.jdbcUrl = t('jdbc.form.urlMustStartWithJdbc');
  }
  if (o.jars.length === 0) {
    errors.jars = t('jdbc.form.jarsRequired');
  }
  return errors;
};

export function JdbcConnectionFields({ form }: { form: ConnectionFormState }) {
  const { t } = useI18n();
  const jdbc = readJdbcOptions(form.options);

  const patch = (partial: Record<string, unknown>) => {
    form.setOptions({
      ...form.options,
      ...partial,
      ...(partial.jdbcUrl != null ? { url: partial.jdbcUrl } : {}),
    });
  };

  return (
    <>
      <div className="md:col-span-2 rounded-md border border-edge/60 bg-surface-2/40 px-3 py-2 text-xs text-fg-muted">
        {t('jdbc.form.capabilityHint')}
      </div>

      <div className="md:col-span-2">
        <Label required>{t('jdbc.form.jdbcUrl')}</Label>
        <Input
          value={jdbc.jdbcUrl}
          onChange={(e) => patch({ jdbcUrl: e.target.value })}
          placeholder="jdbc:h2:mem:demo;DB_CLOSE_DELAY=-1"
          className={form.validationErrors.jdbcUrl ? 'border-danger' : ''}
          data-testid="jdbc-url"
        />
        {form.validationErrors.jdbcUrl && (
          <p className="mt-1 text-xs text-danger">{form.validationErrors.jdbcUrl}</p>
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

      <div className="md:col-span-2">
        <Label>{t('jdbc.form.driverClass')}</Label>
        <Input
          value={jdbc.driverClass}
          onChange={(e) => patch({ driverClass: e.target.value })}
          placeholder="org.h2.Driver"
          data-testid="jdbc-driver-class"
        />
        <p className="mt-1 text-xs text-fg-muted">{t('jdbc.form.driverClassHint')}</p>
      </div>

      <div className="md:col-span-2">
        <Label required>{t('jdbc.form.jars')}</Label>
        <textarea
          value={jdbc.jars.join('\n')}
          onChange={(e) =>
            patch({
              jars: e.target.value
                .split(/[\n,]/)
                .map((s) => s.trim())
                .filter(Boolean),
            })
          }
          rows={3}
          placeholder={'/path/to/h2.jar'}
          className={`w-full rounded-md border bg-surface px-3 py-2 font-mono text-xs text-fg outline-none ${
            form.validationErrors.jars ? 'border-danger' : 'border-edge'
          }`}
          data-testid="jdbc-jars"
        />
        {form.validationErrors.jars && (
          <p className="mt-1 text-xs text-danger">{form.validationErrors.jars}</p>
        )}
        <p className="mt-1 text-xs text-fg-muted">{t('jdbc.form.jarsHint')}</p>
      </div>

      <div className="md:col-span-2">
        <Label>{t('jdbc.form.agentJar')}</Label>
        <PathInput
          value={jdbc.agentJar}
          onChange={(path) => patch({ agentJar: path })}
          placeholder="datazen-jdbc-agent.jar"
        />
        <p className="mt-1 text-xs text-fg-muted">{t('jdbc.form.agentJarHint')}</p>
      </div>
    </>
  );
}
