import { Input } from '../ui/Input';
import { Select } from '../ui/Select';
import { useI18n } from '../../hooks/useI18n';
import { Label } from './shared';
import type { ConnectionFormState } from './useConnectionForm';

export interface WebSocketTunnelFieldsProps {
  form: ConnectionFormState;
}

export function WebSocketTunnelFields({ form }: WebSocketTunnelFieldsProps) {
  const { t } = useI18n();

  return (
    <div className="grid grid-cols-1 gap-3 md:grid-cols-2" data-testid="new-conn-ws-fields">
      <div className="md:col-span-2">
        <Label required>{t('newConn.wsUrl')}</Label>
        <Input
          value={form.wsUrl}
          onChange={(e) => form.setWsUrl(e.target.value)}
          placeholder="wss://relay.example.com/v1/tunnel"
          data-testid="new-conn-ws-url"
        />
      </div>
      <div>
        <Label>{t('newConn.wsMode')}</Label>
        <Select
          value={form.wsMode}
          options={[
            { value: 'datazen_v1', label: t('newConn.wsModeDatazen') },
            { value: 'raw_binary', label: t('newConn.wsModeRaw') },
          ]}
          onChange={(v) => form.setWsMode(v === 'raw_binary' ? 'raw_binary' : 'datazen_v1')}
          data-testid="new-conn-ws-mode"
        />
      </div>
      <div>
        <Label>{t('newConn.wsTimeout')}</Label>
        <Input
          value={form.wsTimeout}
          onChange={(e) => form.setWsTimeout(e.target.value)}
          placeholder="30"
          data-testid="new-conn-ws-timeout"
        />
      </div>
      <div className="md:col-span-2">
        <Label>{t('newConn.wsAuthToken')}</Label>
        <Input
          type="password"
          value={form.wsAuthToken}
          onChange={(e) => form.setWsAuthToken(e.target.value)}
          data-testid="new-conn-ws-token"
        />
      </div>
      <div className="md:col-span-2 text-[11px] text-fg-muted">{t('newConn.wsHint')}</div>
    </div>
  );
}
