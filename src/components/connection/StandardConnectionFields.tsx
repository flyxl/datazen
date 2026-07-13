import { Input } from '../ui/Input';
import { useI18n } from '../../hooks/useI18n';
import { Label } from './shared';
import type { ConnectionFormState } from './useConnectionForm';

export interface StandardConnectionFieldsProps {
  form: ConnectionFormState;
  databaseField?: React.ReactNode;
  hostPlaceholder?: string;
  databasePlaceholder?: string;
}

export function StandardConnectionFields({
  form,
  databaseField,
  hostPlaceholder = '127.0.0.1',
  databasePlaceholder,
}: StandardConnectionFieldsProps) {
  const { t } = useI18n();
  return (
    <>
      <div>
        <Label required>{t('newConn.host')}</Label>
        <Input
          value={form.host}
          onChange={(e) => form.setHost(e.target.value)}
          placeholder={hostPlaceholder}
        />
      </div>
      <div>
        <Label required>{t('newConn.port')}</Label>
        <Input value={form.port} onChange={(e) => form.setPort(e.target.value)} />
      </div>
      {databaseField ?? (
        <div className="md:col-span-2">
          <Label>{t('newConn.database')}</Label>
          <Input
            value={form.database}
            onChange={(e) => form.setDatabase(e.target.value)}
            placeholder={databasePlaceholder}
          />
        </div>
      )}
      {form.hasUsername && (
        <div>
          <Label>{t('newConn.username')}</Label>
          <Input
            value={form.username}
            onChange={(e) => form.setUsername(e.target.value)}
            placeholder={databasePlaceholder ? 'postgres' : undefined}
          />
        </div>
      )}
      <div className={form.hasUsername ? '' : 'md:col-span-2'}>
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
