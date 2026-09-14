import { useState } from 'react';
import { Plus, Trash2 } from 'lucide-react';
import { Button } from '@datazen/ui';
import { Input } from '@datazen/ui';
import { useI18n } from '../../../../src/hooks/useI18n';
import type { KeyDetail } from '../../../../src/types';
import {
  invokeSetAdd,
  invokeSetRemove,
} from './keyEditorsInvokes';

function setMembers(detail: KeyDetail): string[] {
  const v = detail.value as Record<string, unknown>;
  const members = v?.members ?? v?.items;
  return Array.isArray(members) ? (members as string[]) : [];
}

export function SetEditor({
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
  const members = setMembers(detail);
  const [newMember, setNewMember] = useState('');

  return (
    <div className="space-y-2">
      <table className="w-full border-collapse">
        <thead>
          <tr className="border-b border-edge bg-surface-alt text-left">
            <th className="px-2 py-1.5 font-medium text-fg-muted">{t('redis.member')}</th>
            <th className="w-12 px-2 py-1.5" />
          </tr>
        </thead>
        <tbody>
          {members.map((member) => (
            <tr key={member} className="border-b border-edge">
              <td className="px-2 py-1.5 font-mono text-fg-secondary">{member}</td>
              <td className="px-2 py-1.5">
                <button
                  type="button"
                  className="rounded p-1 text-danger hover:bg-danger/10"
                  onClick={() =>
                    void invokeSetRemove(dbSessionId, dbIndex, detail.key, [member]).then(onChanged)
                  }
                >
                  <Trash2 className="h-3.5 w-3.5" />
                </button>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
      <div className="flex flex-wrap items-end gap-2">
        <Input
          value={newMember}
          onChange={(e) => setNewMember(e.target.value)}
          placeholder={t('redis.member')}
          className="h-7 flex-1 font-mono text-xs"
        />
        <Button
          variant="secondary"
          className="h-7 gap-1 px-2 text-xs"
          disabled={!newMember.trim()}
          onClick={() =>
            void invokeSetAdd(dbSessionId, dbIndex, detail.key, [newMember.trim()]).then(() => {
              setNewMember('');
              onChanged();
            })
          }
        >
          <Plus className="h-3 w-3" />
          {t('redis.add')}
        </Button>
      </div>
    </div>
  );
}
