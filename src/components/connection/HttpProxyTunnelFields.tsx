import { Input } from '../ui/Input';
import { Select } from '../ui/Select';
import { useI18n } from '../../hooks/useI18n';
import { Label } from './shared';
import type { ConnectionFormState } from './useConnectionForm';

export interface HttpProxyTunnelFieldsProps {
  form: ConnectionFormState;
}

export function HttpProxyTunnelFields({ form }: HttpProxyTunnelFieldsProps) {
  const { t } = useI18n();

  return (
    <div className="grid grid-cols-1 gap-3 md:grid-cols-2" data-testid="new-conn-http-proxy-fields">
      <div>
        <Label required>{t('newConn.httpProxyHost')}</Label>
        <Input
          value={form.httpProxyHost}
          onChange={(e) => form.setHttpProxyHost(e.target.value)}
          placeholder="proxy.corp.example"
          data-testid="new-conn-http-proxy-host"
        />
      </div>
      <div>
        <Label required>{t('newConn.httpProxyPort')}</Label>
        <Input
          value={form.httpProxyPort}
          onChange={(e) => form.setHttpProxyPort(e.target.value)}
          placeholder="8080"
          data-testid="new-conn-http-proxy-port"
        />
      </div>
      <div>
        <Label required>{t('newConn.httpProxyScheme')}</Label>
        <Select
          value={form.httpProxyScheme}
          options={[
            { value: 'http', label: 'HTTP' },
            { value: 'https', label: 'HTTPS' },
          ]}
          onChange={(v) => form.setHttpProxyScheme(v === 'https' ? 'https' : 'http')}
          data-testid="new-conn-http-proxy-scheme"
        />
      </div>
      <div>
        <Label>{t('newConn.httpProxyTimeout')}</Label>
        <Input
          value={form.httpProxyTimeout}
          onChange={(e) => form.setHttpProxyTimeout(e.target.value)}
          placeholder="30"
          data-testid="new-conn-http-proxy-timeout"
        />
      </div>
      <div>
        <Label>{t('newConn.httpProxyUsername')}</Label>
        <Input
          value={form.httpProxyUsername}
          onChange={(e) => form.setHttpProxyUsername(e.target.value)}
          data-testid="new-conn-http-proxy-username"
        />
      </div>
      <div>
        <Label>{t('newConn.httpProxyPassword')}</Label>
        <Input
          type="password"
          value={form.httpProxyPassword}
          onChange={(e) => form.setHttpProxyPassword(e.target.value)}
          data-testid="new-conn-http-proxy-password"
        />
      </div>
      <div className="md:col-span-2 text-[11px] text-fg-muted">{t('newConn.httpProxyHint')}</div>
    </div>
  );
}
