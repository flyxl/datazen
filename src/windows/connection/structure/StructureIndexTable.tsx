import { Trash2 } from 'lucide-react';
import { Input } from '../../../components/ui/Input';
import { Select } from '../../../components/ui/Select';
import { capEnabled, controlDisabledKey } from '../../../lib/structureEditor/controlHints';
import type {
  StructureCapabilities,
  StructureColumnDraft,
  StructureIndexDraft,
} from '../../../lib/structureEditor/types';
import { useI18n } from '../../../hooks/useI18n';

interface StructureIndexTableProps {
  caps: StructureCapabilities | null;
  indexMethods: string[];
  columnNames: string[];
  indexes: StructureIndexDraft[];
  onUpdate: (id: string, patch: Partial<StructureIndexDraft>) => void;
  onRemove: (id: string) => void;
}

export function StructureIndexTable({
  caps,
  indexMethods,
  columnNames,
  indexes,
  onUpdate,
  onRemove,
}: StructureIndexTableProps) {
  const { t } = useI18n();

  const disabledTitle = (control: Parameters<typeof capEnabled>[1]) => {
    if (capEnabled(caps, control)) return undefined;
    return t(controlDisabledKey(control));
  };

  const methodOptions = indexMethods.map((m) => ({ value: m, label: m }));

  if (indexes.length === 0) {
    return (
      <p className="px-4 py-2 text-xs text-fg-muted">{t('structEditor.noIndexes')}</p>
    );
  }

  return (
    <table className="w-full border-collapse text-[13px]">
      <thead>
        <tr className="bg-surface-alt text-left text-xs font-medium text-fg-secondary">
          <th className="min-w-[140px] border-b border-edge px-2 py-2 font-medium">{t('indexes.indexName')}</th>
          <th className="min-w-[180px] border-b border-edge px-2 py-2 font-medium">{t('indexes.selectColumns')}</th>
          <th className="w-[60px] border-b border-edge px-2 py-2 text-center font-medium">{t('indexes.unique')}</th>
          {capEnabled(caps, 'indexType') && (
            <th className="min-w-[100px] border-b border-edge px-2 py-2 font-medium">{t('indexes.indexType')}</th>
          )}
          <th className="w-10 border-b border-edge px-1 py-2" />
        </tr>
      </thead>
      <tbody>
        {indexes.map((idx) => {
          const canEdit = capEnabled(caps, 'createIndex');
          const canDrop = capEnabled(caps, 'dropIndex');

          return (
            <tr key={idx.id} className="border-b border-edge bg-surface">
              <td className="px-2 py-1.5">
                <Input
                  value={idx.name}
                  disabled={!canEdit}
                  title={!canEdit ? disabledTitle('createIndex') : undefined}
                  onChange={(e) => onUpdate(idx.id, { name: e.target.value })}
                  placeholder="idx_name"
                  className="h-7 text-xs"
                />
              </td>
              <td className="px-2 py-1.5">
                <select
                  multiple
                  value={idx.columns}
                  disabled={!canEdit}
                  title={!canEdit ? disabledTitle('createIndex') : undefined}
                  onChange={(e) => {
                    const selected = Array.from(e.target.selectedOptions).map((o) => o.value);
                    onUpdate(idx.id, { columns: selected });
                  }}
                  className="min-h-[4.5rem] w-full rounded border border-edge bg-surface-alt px-2 py-1 text-xs text-fg disabled:opacity-40"
                >
                  {columnNames.map((name) => (
                    <option key={name} value={name}>
                      {name}
                    </option>
                  ))}
                </select>
              </td>
              <td className="px-2 py-1.5 text-center">
                <input
                  type="checkbox"
                  checked={idx.isUnique}
                  disabled={!canEdit}
                  title={!canEdit ? disabledTitle('createIndex') : undefined}
                  onChange={(e) => onUpdate(idx.id, { isUnique: e.target.checked })}
                  className="h-3.5 w-3.5 accent-accent disabled:opacity-40"
                />
              </td>
              {capEnabled(caps, 'indexType') && (
                <td className="px-2 py-1.5">
                  <Select
                    value={idx.indexType || indexMethods[0] || 'btree'}
                    options={methodOptions}
                    disabled={!canEdit}
                    title={!canEdit ? disabledTitle('indexType') : undefined}
                    onChange={(v) => onUpdate(idx.id, { indexType: v })}
                    className="h-7 text-xs"
                  />
                </td>
              )}
              <td className="px-1 py-1.5 text-center">
                <button
                  type="button"
                  className="rounded p-1 text-fg-muted hover:bg-danger/10 hover:text-danger disabled:opacity-40"
                  disabled={!canDrop}
                  title={!canDrop ? disabledTitle('dropIndex') : t('structEditor.deleteIndex')}
                  onClick={() => onRemove(idx.id)}
                >
                  <Trash2 className="h-3.5 w-3.5" />
                </button>
              </td>
            </tr>
          );
        })}
      </tbody>
    </table>
  );
}
