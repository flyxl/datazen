import { Trash2, X } from 'lucide-react';
import { Input } from '../../../components/ui/Input';
import { Select } from '../../../components/ui/Select';
import { capEnabled, controlDisabledKey } from '../../../lib/structureEditor/controlHints';
import type {
  StructureCapabilities,
  StructureIndexDraft,
} from '../../../lib/structureEditor/types';
import { useI18n } from '../../../hooks/useI18n';

interface StructureIndexTableProps {
  caps: StructureCapabilities | null;
  indexMethods: string[];
  columnNames: string[];
  tableName?: string;
  indexes: StructureIndexDraft[];
  onUpdate: (id: string, patch: Partial<StructureIndexDraft>) => void;
  onRemove: (id: string) => void;
}

export function suggestedIndexName(tableName: string, columns: string[]): string {
  const base = tableName.trim() || 'table';
  if (columns.length === 0) return `idx_${base}`;
  return `idx_${base}_${columns.join('_')}`;
}

function IndexColumnPicker({
  columnNames,
  selected,
  disabled,
  title,
  onChange,
}: {
  columnNames: string[];
  selected: string[];
  disabled?: boolean;
  title?: string;
  onChange: (columns: string[]) => void;
}) {
  const { t } = useI18n();
  const available = columnNames.filter((name) => !selected.includes(name));

  return (
    <div className="flex min-w-[12rem] flex-wrap items-center gap-1" title={title}>
      {selected.map((col, order) => (
        <span
          key={col}
          className="inline-flex max-w-full items-center gap-0.5 rounded-md border border-accent/25 bg-accent/10 py-0.5 pl-1.5 pr-0.5 text-xs text-accent"
        >
          <span className="text-[10px] text-accent/70">#{order + 1}</span>
          <span className="max-w-[7rem] truncate font-mono">{col}</span>
          {!disabled && (
            <button
              type="button"
              className="rounded p-0.5 hover:bg-accent/20"
              onClick={() => onChange(selected.filter((c) => c !== col))}
              aria-label={t('filter.remove')}
            >
              <X className="h-3 w-3" />
            </button>
          )}
        </span>
      ))}
      {!disabled && available.length > 0 && (
        <Select
          value=""
          placeholder={t('indexes.addColumn')}
          options={available.map((name) => ({ value: name, label: name }))}
          onChange={(v) => onChange([...selected, v])}
          className="!h-7 w-[7.5rem] shrink-0 !text-xs"
        />
      )}
      {!disabled && available.length === 0 && selected.length === 0 && (
        <span className="text-xs text-fg-muted">{t('indexes.selectColumns')}</span>
      )}
    </div>
  );
}

export function StructureIndexTable({
  caps,
  indexMethods,
  columnNames,
  tableName = '',
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
    return <p className="px-4 py-2 text-xs text-fg-muted">{t('structEditor.noIndexes')}</p>;
  }

  return (
    <table className="w-full border-collapse text-[13px]">
      <thead>
        <tr className="bg-surface-alt text-left text-xs font-medium text-fg-secondary">
          <th className="min-w-[140px] border-b border-edge px-2 py-2 font-medium">
            {t('indexes.indexName')}
          </th>
          <th className="min-w-[220px] border-b border-edge px-2 py-2 font-medium">
            {t('indexes.columns')}
          </th>
          <th className="w-[60px] border-b border-edge px-2 py-2 text-center font-medium">
            {t('indexes.unique')}
          </th>
          {capEnabled(caps, 'indexType') && (
            <th className="min-w-[100px] border-b border-edge px-2 py-2 font-medium">
              {t('indexes.indexType')}
            </th>
          )}
          <th className="w-10 border-b border-edge px-1 py-2" />
        </tr>
      </thead>
      <tbody>
        {indexes.map((idx) => {
          const canEdit = capEnabled(caps, 'createIndex');
          const canDrop = capEnabled(caps, 'dropIndex');
          const suggested = suggestedIndexName(tableName, idx.columns);

          return (
            <tr key={idx.id} className="border-b border-edge bg-surface">
              <td className="px-2 py-1.5 align-top">
                <Input
                  value={idx.name}
                  disabled={!canEdit}
                  title={!canEdit ? disabledTitle('createIndex') : undefined}
                  onChange={(e) => onUpdate(idx.id, { name: e.target.value })}
                  placeholder={suggested || 'idx_name'}
                  className="h-7 text-xs"
                />
              </td>
              <td className="px-2 py-1.5 align-top">
                <IndexColumnPicker
                  columnNames={columnNames}
                  selected={idx.columns}
                  disabled={!canEdit}
                  title={!canEdit ? disabledTitle('createIndex') : undefined}
                  onChange={(columns) => {
                    const prevSuggested = suggestedIndexName(tableName, idx.columns);
                    const nextSuggested = suggestedIndexName(tableName, columns);
                    const patch: Partial<StructureIndexDraft> = { columns };
                    // Keep auto-updating the name until the user customizes it.
                    if (!idx.name.trim() || idx.name === prevSuggested) {
                      patch.name = nextSuggested;
                    }
                    onUpdate(idx.id, patch);
                  }}
                />
              </td>
              <td className="px-2 py-1.5 text-center align-top">
                <input
                  type="checkbox"
                  checked={idx.isUnique}
                  disabled={!canEdit}
                  title={!canEdit ? disabledTitle('createIndex') : undefined}
                  onChange={(e) => onUpdate(idx.id, { isUnique: e.target.checked })}
                  className="mt-1.5 h-3.5 w-3.5 accent-accent disabled:opacity-40"
                />
              </td>
              {capEnabled(caps, 'indexType') && (
                <td className="px-2 py-1.5 align-top">
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
              <td className="px-1 py-1.5 text-center align-top">
                <button
                  type="button"
                  className="mt-0.5 rounded p-1 text-fg-muted hover:bg-danger/10 hover:text-danger disabled:opacity-40"
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
