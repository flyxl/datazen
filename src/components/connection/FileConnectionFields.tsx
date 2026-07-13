import { Input } from '../ui/Input';
import { useI18n } from '../../hooks/useI18n';
import { Label } from './shared';
import type { ConnectionFormState } from './useConnectionForm';

export function FileConnectionFields({ form }: { form: ConnectionFormState }) {
  const { t } = useI18n();
  return (
    <div className="md:col-span-2">
      <Label required>{t('newConn.dbFilePath')}</Label>
      <Input
        value={form.database}
        onChange={(e) => form.setDatabase(e.target.value)}
        placeholder="/path/to/db.sqlite"
      />
    </div>
  );
}
