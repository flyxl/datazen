import { GripVertical, Trash2 } from 'lucide-react';
import { Input } from '../../../components/ui/Input';
import { Select } from '../../../components/ui/Select';
import { cn } from '../../../lib/cn';
import { capEnabled, controlDisabledKey } from '../../../lib/structureEditor/controlHints';
import type {
  StructureCapabilities,
  StructureColumnDraft,
  StructureEditorUiConfig,
} from '../../../lib/structureEditor/types';
import { useI18n } from '../../../hooks/useI18n';

interface StructureColumnTableProps {
  mode: 'create' | 'alter';
  caps: StructureCapabilities | null;
  uiConfig: StructureEditorUiConfig;
  columns: StructureColumnDraft[];
  originalById: Map<string, StructureColumnDraft>;
  reorderEnabled: boolean;
  dragIdx: number | null;
  onDragStart: (idx: number) => void;
  onDragOver: (e: React.DragEvent, targetIdx: number) => void;
  onDragEnd: () => void;
  onUpdate: (id: string, patch: Partial<StructureColumnDraft>) => void;
  onRemove: (id: string) => void;
}

export function StructureColumnTable({
  mode,
  caps,
  uiConfig,
  columns,
  originalById,
  reorderEnabled,
  dragIdx,
  onDragStart,
  onDragOver,
  onDragEnd,
  onUpdate,
  onRemove,
}: StructureColumnTableProps) {
  const { t } = useI18n();
  const showComment = uiConfig.fields.comment === true;
  const showColumnUnique = mode === 'create';
  const columnTypes = uiConfig.columnTypes;

  const disabledTitle = (control: Parameters<typeof capEnabled>[1]) => {
    if (capEnabled(caps, control)) return undefined;
    return t(controlDisabledKey(control));
  };

  return (
    <table className="w-full border-collapse text-[13px]">
      <thead className="sticky top-0 z-10">
        <tr className="bg-surface-alt text-left text-xs font-medium text-fg-secondary">
          <th className="w-8 border-b border-edge px-1 py-2.5" />
          <th className="min-w-[140px] border-b border-edge px-2 py-2.5 font-medium">{t('structView.fieldName')}</th>
          <th className="min-w-[160px] border-b border-edge px-2 py-2.5 font-medium">{t('structView.type')}</th>
          <th className="w-[60px] border-b border-edge px-2 py-2.5 text-center font-medium">{t('structView.nullable')}</th>
          <th className="w-[60px] border-b border-edge px-2 py-2.5 text-center font-medium">{t('structView.primaryKey')}</th>
          {showColumnUnique && (
            <th className="w-[60px] border-b border-edge px-2 py-2.5 text-center font-medium">{t('structView.unique')}</th>
          )}
          <th className="min-w-[120px] border-b border-edge px-2 py-2.5 font-medium">{t('structView.defaultValue')}</th>
          {showComment && (
            <th className="min-w-[120px] border-b border-edge px-2 py-2.5 font-medium">{t('structView.comment')}</th>
          )}
          <th className="w-10 border-b border-edge px-1 py-2.5" />
        </tr>
      </thead>
      <tbody>
        {columns.map((col, idx) => {
          const isNew = !originalById.has(col.id);
          const canRename = isNew || capEnabled(caps, 'renameColumn');
          const canAlterType = isNew
            ? mode === 'create'
              ? capEnabled(caps, 'createTable')
              : capEnabled(caps, 'addColumn')
            : capEnabled(caps, 'alterType');
          const canAlterNull = isNew
            ? mode === 'create'
              ? capEnabled(caps, 'createTable')
              : capEnabled(caps, 'addColumn')
            : capEnabled(caps, 'alterNullability');
          const canAlterDefault = isNew
            ? mode === 'create'
              ? capEnabled(caps, 'createTable')
              : capEnabled(caps, 'addColumn')
            : capEnabled(caps, 'alterDefault');
          const canAlterPk = capEnabled(caps, 'alterPrimaryKey');
          const canAlterUnique = capEnabled(caps, 'createIndex');
          const canComment = capEnabled(caps, 'comment') && showComment;
          const canDrop = capEnabled(caps, 'dropColumn');

          return (
            <tr
              key={col.id}
              draggable={reorderEnabled}
              onDragStart={() => reorderEnabled && onDragStart(idx)}
              onDragOver={(e) => reorderEnabled && onDragOver(e, idx)}
              onDragEnd={onDragEnd}
              className={cn(
                'border-b border-edge bg-surface transition-colors hover:bg-surface-alt/50',
                dragIdx === idx && 'opacity-50',
              )}
            >
              <td className="px-1 py-1.5 text-center">
                <GripVertical
                  className={cn(
                    'mx-auto h-3.5 w-3.5 text-fg-muted',
                    reorderEnabled ? 'cursor-grab' : 'cursor-not-allowed opacity-40',
                  )}
                  title={reorderEnabled ? undefined : disabledTitle('reorderColumn')}
                />
              </td>
              <td className="px-2 py-1.5">
                <Input
                  value={col.name}
                  disabled={!canRename}
                  title={!canRename ? disabledTitle(isNew ? 'addColumn' : 'renameColumn') : undefined}
                  onChange={(e) => onUpdate(col.id, { name: e.target.value })}
                  placeholder="column_name"
                  className="h-7 text-xs"
                />
              </td>
              <td className="px-2 py-1.5">
                <Select
                  value={col.dataType}
                  options={columnTypes}
                  disabled={!canAlterType}
                  title={!canAlterType ? disabledTitle(isNew ? 'addColumn' : 'alterType') : undefined}
                  onChange={(v) => onUpdate(col.id, { dataType: v })}
                  className="h-7 text-xs"
                />
              </td>
              <td className="px-2 py-1.5 text-center">
                <input
                  type="checkbox"
                  checked={col.nullable}
                  disabled={!canAlterNull}
                  title={!canAlterNull ? disabledTitle(isNew ? 'addColumn' : 'alterNullability') : undefined}
                  onChange={(e) => onUpdate(col.id, { nullable: e.target.checked })}
                  className="h-3.5 w-3.5 accent-accent disabled:opacity-40"
                />
              </td>
              <td className="px-2 py-1.5 text-center">
                <input
                  type="checkbox"
                  checked={col.isPrimaryKey ?? false}
                  disabled={!canAlterPk}
                  title={!canAlterPk ? disabledTitle('alterPrimaryKey') : undefined}
                  onChange={(e) =>
                    onUpdate(col.id, {
                      isPrimaryKey: e.target.checked,
                      nullable: e.target.checked ? false : col.nullable,
                    })
                  }
                  className="h-3.5 w-3.5 accent-accent disabled:opacity-40"
                />
              </td>
              {showColumnUnique && (
                <td className="px-2 py-1.5 text-center">
                  <input
                    type="checkbox"
                    checked={col.isUnique ?? false}
                    disabled={!canAlterUnique}
                    title={!canAlterUnique ? disabledTitle('createIndex') : undefined}
                    onChange={(e) => onUpdate(col.id, { isUnique: e.target.checked })}
                    className="h-3.5 w-3.5 accent-accent disabled:opacity-40"
                  />
                </td>
              )}
              <td className="px-2 py-1.5">
                <Input
                  value={col.defaultValue ?? ''}
                  disabled={!canAlterDefault}
                  title={!canAlterDefault ? disabledTitle(isNew ? 'addColumn' : 'alterDefault') : undefined}
                  onChange={(e) => onUpdate(col.id, { defaultValue: e.target.value || null })}
                  placeholder="NULL"
                  className="h-7 text-xs"
                />
              </td>
              {showComment && (
                <td className="px-2 py-1.5">
                  <Input
                    value={col.comment ?? ''}
                    disabled={!canComment}
                    title={!canComment ? disabledTitle('comment') : undefined}
                    onChange={(e) => onUpdate(col.id, { comment: e.target.value || null })}
                    placeholder=""
                    className="h-7 text-xs"
                  />
                </td>
              )}
              <td className="px-1 py-1.5 text-center">
                <button
                  type="button"
                  className="rounded p-1 text-fg-muted hover:bg-danger/10 hover:text-danger disabled:opacity-40"
                  disabled={!canDrop && !isNew}
                  title={!canDrop && !isNew ? disabledTitle('dropColumn') : t('structEditor.deleteColumn')}
                  onClick={() => onRemove(col.id)}
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
