import { useCallback } from 'react';
import { open } from '@tauri-apps/plugin-dialog';
import { Input, Label } from '@datazen/ui';
import { PathInput } from '../../../../src/components/ui/PathInput';
import { Button } from '../../../../src/components/ui/Button';
import { useI18n } from '../../../../src/hooks/useI18n';
import type { ConnectionFormState } from '../../../../src/components/connection/useConnectionForm';
import type { PluginFormValidator } from '@datazen/driver-sdk';
import { inferDriverClass, JDBC_DRIVER_PRESETS } from './inferDriverClass';

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
  const inferred = inferDriverClass(jdbc.jdbcUrl);

  const patch = (partial: Record<string, unknown>) => {
    form.setOptions({
      ...form.options,
      ...partial,
      ...(partial.jdbcUrl != null ? { url: partial.jdbcUrl } : {}),
    });
  };

  const onJdbcUrlChange = (value: string) => {
    const next: Record<string, unknown> = { jdbcUrl: value, url: value };
    // Auto-fill driver class only when empty so user overrides stick.
    if (!jdbc.driverClass.trim()) {
      const guess = inferDriverClass(value);
      if (guess) next.driverClass = guess.driverClass;
    }
    form.setOptions({ ...form.options, ...next });
  };

  const addJars = useCallback(async () => {
    try {
      const selected = await open({
        multiple: true,
        filters: [{ name: 'JAR', extensions: ['jar'] }],
      });
      if (selected == null) return;
      const paths = Array.isArray(selected) ? selected : [selected];
      const merged = Array.from(new Set([...jdbc.jars, ...paths.filter(Boolean)]));
      patch({ jars: merged });
    } catch {
      // dialog cancelled / unavailable in browser dev
    }
  }, [jdbc.jars]);

  return (
    <>
      <div className="md:col-span-2 rounded-md border border-edge/60 bg-surface-2/40 px-3 py-2 text-xs text-fg-muted">
        {t('jdbc.form.capabilityHint')}
        {inferred ? (
          <span className="ml-1 text-fg">
            {t('jdbc.form.inferredProduct', { product: inferred.label })}
          </span>
        ) : null}
      </div>

      <div className="md:col-span-2">
        <Label required>{t('jdbc.form.jdbcUrl')}</Label>
        <Input
          value={jdbc.jdbcUrl}
          onChange={(e) => onJdbcUrlChange(e.target.value)}
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
        <div className="flex flex-col gap-1 sm:flex-row sm:items-center">
          <Input
            value={jdbc.driverClass}
            onChange={(e) => patch({ driverClass: e.target.value })}
            placeholder={inferred?.driverClass ?? 'org.h2.Driver'}
            className="min-w-0 flex-1"
            data-testid="jdbc-driver-class"
          />
          {inferred && jdbc.driverClass.trim() !== inferred.driverClass ? (
            <Button
              type="button"
              variant="ghost"
              className="h-9 shrink-0 text-xs"
              onClick={() => patch({ driverClass: inferred.driverClass })}
            >
              {t('jdbc.form.useInferred')}
            </Button>
          ) : null}
        </div>
        <p className="mt-1 text-xs text-fg-muted">{t('jdbc.form.driverClassHint')}</p>
        <div className="mt-1 flex flex-wrap gap-1">
          {JDBC_DRIVER_PRESETS.slice(0, 8).map((p) => (
            <button
              key={p.driverClass}
              type="button"
              className="rounded border border-edge px-1.5 py-0.5 text-[10px] text-fg-muted hover:bg-surface-2"
              onClick={() => patch({ driverClass: p.driverClass })}
              title={p.driverClass}
            >
              {p.label}
            </button>
          ))}
        </div>
      </div>

      <div className="md:col-span-2">
        <div className="mb-1 flex items-center justify-between gap-2">
          <Label required>{t('jdbc.form.jars')}</Label>
          <Button
            type="button"
            variant="ghost"
            className="h-8 text-xs"
            onClick={() => void addJars()}
            data-testid="jdbc-jars-browse"
          >
            {t('jdbc.form.browseJars')}
          </Button>
        </div>
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
          dialogOptions={{
            filters: [{ name: 'JAR', extensions: ['jar'] }],
          }}
        />
        <p className="mt-1 text-xs text-fg-muted">{t('jdbc.form.agentJarHint')}</p>
      </div>
    </>
  );
}
