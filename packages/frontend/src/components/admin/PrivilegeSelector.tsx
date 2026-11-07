import { allPrivileges, privilegeGroups } from '../../lib/commandSchema';
import type { DriverCommandDefinition } from '../../types';

interface PrivilegeSelectorProps {
  definition?: DriverCommandDefinition;
  selected: Set<string>;
  onToggle: (privilege: string) => void;
  className?: string;
  itemClassName?: string;
}

export function PrivilegeSelector({
  definition,
  selected,
  onToggle,
  className = 'grid grid-cols-2 gap-2',
  itemClassName = 'flex items-center gap-2 text-sm text-fg cursor-pointer',
}: PrivilegeSelectorProps) {
  const groups = privilegeGroups(definition);

  if (groups.length === 0) return null;

  return (
    <>
      {groups.map((group) => (
        <div key={group.label || 'default'} className={group.label ? 'mb-3' : ''}>
          {group.label && (
            <div className="text-[11px] font-medium text-fg-muted mb-1.5 uppercase tracking-wide">
              {group.label}
            </div>
          )}
          <div className={className}>
            {group.privileges.map((privilege) => (
              <label key={privilege} className={itemClassName}>
                <input
                  type="checkbox"
                  checked={selected.has(privilege)}
                  onChange={() => onToggle(privilege)}
                  className="rounded border-edge"
                />
                {privilege}
              </label>
            ))}
          </div>
        </div>
      ))}
    </>
  );
}

export function usePrivilegeOptions(definition?: DriverCommandDefinition) {
  const groups = privilegeGroups(definition);
  const all = allPrivileges(definition);
  return { groups, all };
}
