import { useState } from 'react';
import { Plus, Trash2 } from 'lucide-react';
import { Button } from '@datazen/ui';
import { Input } from '@datazen/ui';
import { useI18n } from '../../../../src/hooks/useI18n';
import type { KeyDetail } from '../../../../src/types';
import {
  invokeHashDel,
  invokeHashSet,
} from './keyEditorsInvokes';

export function HashEditor({
  dbSessionId,
  dbIndex,
  detail,
  onChanged,
}: {
  dbSessionId: string;
  dbIndex: number;
  detail: KeyDetail;
  onChanged: () => void;
}) {
  const { t } = useI18n();
  const raw =
    typeof detail.value === 'object' && detail.value !== null
      ? ((detail.value as Record<string, Record<string, string>>).fields ??
        (detail.value as Record<string, string>))
      : {};
  const fields = Object.entries(raw);
  const [newField, setNewField] = useState('');
  const [newValue, setNewValue] = useState('');
  const [editValues, setEditValues] = useState<Record<string, string>>({});

  const getValue = (field: string, original: string) => editValues[field] ?? original;

  return (
    <div className="space-y-2">
      <table className="w-full border-collapse">
        <thead>
          <tr className="border-b border-edge bg-surface-alt text-left">
            <th className="px-2 py-1.5 font-medium text-fg-muted">{t('redis.field')}</th>
            <th className="px-2 py-1.5 font-medium text-fg-muted">{t('redis.value')}</th>
            <th className="w-20 px-2 py-1.5" />
          </tr>
        </thead>
        <tbody>
          {fields.map(([field, val]) => (
            <tr key={field} className="border-b border-edge">
              <td className="px-2 py-1.5 font-mono text-fg-secondary">{field}</td>
              <td className="px-2 py-1.5">
                <Input
                  value={getValue(field, String(val))}
                  onChange={(e) => setEditValues((prev) => ({ ...prev, [field]: e.target.value }))}
                  className="h-7 font-mono text-xs"
                />
              </td>
              <td className="px-2 py-1.5">
                <div className="flex gap-1">
                  <Button
                    variant="secondary"
                    className="h-6 px-1.5 text-[10px]"
                    onClick={() =>
                      void invokeHashSet(
                        dbSessionId,
                        dbIndex,
                        detail.key,
                        field,
                        getValue(field, String(val)),
                      ).then(onChanged)
                    }
                  >
                    {t('common.save')}
                  </Button>
                  <Button
                    variant="ghost"
                    className="h-6 px-1.5 text-[10px] text-danger"
                    onClick={() =>
                      void invokeHashDel(dbSessionId, dbIndex, detail.key, [field]).then(onChanged)
                    }
                  >
                    <Trash2 className="h-3 w-3" />
                  </Button>
                </div>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
      <div className="flex flex-wrap items-end gap-2">
        <Input
          value={newField}
          onChange={(e) => setNewField(e.target.value)}
          placeholder={t('redis.field')}
          className="h-7 flex-1 font-mono text-xs"
        />
        <Input
          value={newValue}
          onChange={(e) => setNewValue(e.target.value)}
          placeholder={t('redis.value')}
          className="h-7 flex-1 font-mono text-xs"
        />
        <Button
          variant="secondary"
          className="h-7 gap-1 px-2 text-xs"
          disabled={!newField.trim()}
          onClick={() =>
            void invokeHashSet(dbSessionId, dbIndex, detail.key, newField.trim(), newValue).then(
              () => {
                setNewField('');
                setNewValue('');
                onChanged();
              },
            )
          }
        >
          <Plus className="h-3 w-3" />
          {t('redis.add')}
        </Button>
      </div>
    </div>
  );
}
