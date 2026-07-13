import { Input } from '../ui/Input';
import { useI18n } from '../../hooks/useI18n';
import { Label } from './shared';
import type { ConnectionFormState } from './useConnectionForm';

export function IndexConnectionFields({ form }: { form: ConnectionFormState }) {
  const { t } = useI18n();
  return (
    <div className="md:col-span-2">
      <Label>{t('newConn.databaseIndex')}</Label>
      <Input
        type="number"
        min={0}
        max={15}
        value={form.database}
        onChange={(e) => {
          const v = e.target.value;
          if (v === '') {
            form.setDatabase('');
            return;
          }
          form.setDatabase(String(Math.min(15, Math.max(0, parseInt(v, 10) || 0))));
        }}
        onBlur={() => {
          if (form.database.trim() === '') form.setDatabase('0');
        }}
        placeholder="0"
      />
    </div>
  );
}
