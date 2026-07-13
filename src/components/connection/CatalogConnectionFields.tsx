import { Input } from '../ui/Input';
import { useI18n } from '../../hooks/useI18n';
import { Label } from './shared';
import type { ConnectionFormState } from './useConnectionForm';

export interface CatalogConnectionFieldsProps {
  form: ConnectionFormState;
  hostPlaceholder?: string;
}

export function CatalogConnectionFields({
  form,
  hostPlaceholder = '127.0.0.1',
}: CatalogConnectionFieldsProps) {
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
      <div>
        <Label required>{t('newConn.catalog')}</Label>
        <Input
          value={form.database}
          onChange={(e) => form.setDatabase(e.target.value)}
          placeholder="hive"
        />
      </div>
      <div>
        <Label>{t('newConn.schema')}</Label>
        <Input
          value={form.schema}
          onChange={(e) => form.setSchema(e.target.value)}
          placeholder="default"
        />
      </div>
      <div>
        <Label>{t('newConn.username')}</Label>
        <Input
          value={form.username}
          onChange={(e) => form.setUsername(e.target.value)}
          placeholder="user"
        />
      </div>
      <div className="md:col-span-2">
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
