import { useState } from 'react';
import { FolderPlus } from 'lucide-react';
import { cn } from '../../lib/cn';
import { Input } from '../../components/ui/Input';
import { useI18n } from '../../hooks/useI18n';

export interface GroupPanelProps {
  groups: string[];
  selectedGroup: string | null;
  onSelectGroup: (group: string | null) => void;
  onAddGroup: (name: string) => void;
}

export function GroupPanel({ groups, selectedGroup, onSelectGroup, onAddGroup }: GroupPanelProps) {
  const { t } = useI18n();
  const [adding, setAdding] = useState(false);
  const [newName, setNewName] = useState('');

  function handleSubmit() {
    const trimmed = newName.trim();
    if (trimmed) {
      onAddGroup(trimmed);
      setNewName('');
    }
    setAdding(false);
  }

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex items-center justify-between border-b border-edge px-3 py-3">
        <div className="text-[11px] font-semibold uppercase tracking-wider text-fg-muted">{t('newConn.group')}</div>
        <button
          type="button"
          title={t('main.ctx.newGroup')}
          onClick={() => setAdding(true)}
          className="rounded p-1 text-fg-muted hover:bg-surface hover:text-fg"
        >
          <FolderPlus className="h-3.5 w-3.5" />
        </button>
      </div>
      <div className="flex-1 overflow-auto p-2">
        <div className="flex flex-col gap-1">
          <button
            type="button"
            onClick={() => onSelectGroup(null)}
            className={cn(
              'w-full rounded-md px-3 py-2 text-left text-sm transition-colors',
              selectedGroup === null
                ? 'bg-surface text-fg ring-1 ring-edge'
                : 'text-fg-secondary hover:bg-surface/50 hover:text-fg',
            )}
          >
            {t('main.allGroups')}
          </button>
          {groups.map((g) => (
            <button
              key={g}
              type="button"
              onClick={() => onSelectGroup(g)}
              className={cn(
                'w-full rounded-md px-3 py-2 text-left text-sm transition-colors',
                selectedGroup === g
                  ? 'bg-surface text-fg ring-1 ring-edge'
                  : 'text-fg-secondary hover:bg-surface/50 hover:text-fg',
              )}
            >
              {g}
            </button>
          ))}

          {adding && (
            <div className="mt-1 px-1">
              <Input
                autoFocus
                value={newName}
                onChange={(e) => setNewName(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === 'Enter') handleSubmit();
                  if (e.key === 'Escape') { setAdding(false); setNewName(''); }
                }}
                onBlur={handleSubmit}
                placeholder={t('main.groupNamePlaceholder')}
                className="h-8 text-xs"
              />
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
