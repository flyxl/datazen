import { useState } from 'react';
import { Plus } from 'lucide-react';
import { Button } from '@datazen/ui';
import { Input } from '@datazen/ui';
import { useI18n } from '../../../../src/hooks/useI18n';
import type { KeyDetail } from '../../../../src/types';
import {
  invokeListPop,
  invokeListPush,
  invokeListSet,
} from './keyEditorsInvokes';

function listItems(detail: KeyDetail): string[] {
  const v = detail.value as Record<string, unknown>;
  const items = v?.items ?? v?.members;
  return Array.isArray(items) ? (items as string[]) : [];
}

export function ListEditor({
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
  const items = listItems(detail);
  const [newValue, setNewValue] = useState('');
  const [editValues, setEditValues] = useState<Record<number, string>>({});

  const getValue = (index: number, original: string) =>
    editValues[index] !== undefined ? editValues[index] : original;

  return (
    <div className="space-y-2">
      <table className="w-full border-collapse">
        <thead>
          <tr className="border-b border-edge bg-surface-alt text-left">
            <th className="w-12 px-2 py-1.5 font-medium text-fg-muted">#</th>
            <th className="px-2 py-1.5 font-medium text-fg-muted">{t('redis.value')}</th>
            <th className="w-24 px-2 py-1.5" />
          </tr>
        </thead>
        <tbody>
          {items.map((item, index) => (
            <tr key={index} className="border-b border-edge">
              <td className="px-2 py-1.5 font-mono text-fg-muted">{index}</td>
              <td className="px-2 py-1.5">
                <Input
                  value={getValue(index, item)}
                  onChange={(e) =>
                    setEditValues((prev) => ({ ...prev, [index]: e.target.value }))
                  }
                  className="h-7 font-mono text-xs"
                />
              </td>
              <td className="px-2 py-1.5">
                <Button
                  variant="secondary"
                  className="h-6 px-1.5 text-[10px]"
                  onClick={() =>
                    void invokeListSet(
                      dbSessionId,
                      dbIndex,
                      detail.key,
                      index,
                      getValue(index, item),
                    ).then(onChanged)
                  }
                >
                  {t('common.save')}
                </Button>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
      <div className="flex flex-wrap items-end gap-2">
        <Input
          value={newValue}
          onChange={(e) => setNewValue(e.target.value)}
          placeholder={t('redis.value')}
          className="h-7 flex-1 font-mono text-xs"
        />
        <Button
          variant="secondary"
          className="h-7 gap-1 px-2 text-xs"
          disabled={!newValue}
          onClick={() =>
            void invokeListPush(dbSessionId, dbIndex, detail.key, 'left', [newValue]).then(() => {
              setNewValue('');
              onChanged();
            })
          }
        >
          <Plus className="h-3 w-3" />
          {t('redis.pushLeft')}
        </Button>
        <Button
          variant="secondary"
          className="h-7 gap-1 px-2 text-xs"
          disabled={!newValue}
          onClick={() =>
            void invokeListPush(dbSessionId, dbIndex, detail.key, 'right', [newValue]).then(() => {
              setNewValue('');
              onChanged();
            })
          }
        >
          <Plus className="h-3 w-3" />
          {t('redis.pushRight')}
        </Button>
        <Button
          variant="secondary"
          className="h-7 px-2 text-xs"
          onClick={() =>
            void invokeListPop(dbSessionId, dbIndex, detail.key, 'left').then(onChanged)
          }
        >
          {t('redis.popLeft')}
        </Button>
        <Button
          variant="secondary"
          className="h-7 px-2 text-xs"
          onClick={() =>
            void invokeListPop(dbSessionId, dbIndex, detail.key, 'right').then(onChanged)
          }
        >
          {t('redis.popRight')}
        </Button>
      </div>
    </div>
  );
}
