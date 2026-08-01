import { useEffect, useState } from 'react';
import { Loader2, Play, Wand2 } from 'lucide-react';
import { useI18n } from '../../hooks/useI18n';
import { useAiStore } from '../../stores/aiStore';
import type { SkillListItem } from '../../types';

interface SkillsPanelProps {
  connectionId?: string;
}

export function SkillsPanel({ connectionId }: SkillsPanelProps) {
  const { t } = useI18n();
  const skills = useAiStore((s) => s.skills);
  const skillsLoading = useAiStore((s) => s.skillsLoading);
  const loadSkills = useAiStore((s) => s.loadSkills);
  const executeSkill = useAiStore((s) => s.executeSkill);
  const result = useAiStore((s) => s.skillExecutionResult);
  const isExecuting = useAiStore((s) => s.isExecutingSkill);
  const skillError = useAiStore((s) => s.skillError);
  const clearSkillResult = useAiStore((s) => s.clearSkillResult);

  const [selectedSkill, setSelectedSkill] = useState<SkillListItem | null>(null);
  const [variables, setVariables] = useState<Record<string, string>>({});

  useEffect(() => {
    void loadSkills();
  }, [loadSkills]);

  const handleSelect = (skill: SkillListItem) => {
    setSelectedSkill(skill);
    clearSkillResult();
    const defaults: Record<string, string> = {};
    for (const v of skill.variables) {
      defaults[v.name] = v.default != null ? String(v.default) : '';
    }
    setVariables(defaults);
  };

  const handleExecute = async () => {
    if (!selectedSkill) return;
    await executeSkill({
      skillId: selectedSkill.id,
      variables,
      connectionId,
    });
  };

  if (skillsLoading) {
    return (
      <div className="flex items-center justify-center py-4 text-fg-muted text-xs">
        <Loader2 className="h-4 w-4 animate-spin mr-1" />
        {t('skills.loading')}
      </div>
    );
  }

  if (skills.length === 0) {
    return (
      <div className="py-4 text-center text-xs text-fg-muted">
        {t('skills.empty')}
      </div>
    );
  }

  return (
    <div className="space-y-3">
      <h3 className="text-sm font-medium flex items-center gap-2">
        <Wand2 className="w-4 h-4" />
        {t('skills.title')}
      </h3>

      <div className="space-y-1">
        {skills.map((skill) => (
          <button
            key={skill.id}
            type="button"
            className={`w-full text-left px-3 py-2 rounded-md text-sm transition-colors ${
              selectedSkill?.id === skill.id
                ? 'bg-accent/10 text-accent'
                : 'hover:bg-muted/50 text-fg-secondary'
            }`}
            onClick={() => handleSelect(skill)}
          >
            <div className="font-medium">{skill.name}</div>
            <div className="text-[11px] text-fg-muted truncate">{skill.description}</div>
          </button>
        ))}
      </div>

      {selectedSkill && (
        <div className="space-y-2 border-t border-edge pt-3">
          {selectedSkill.variables.map((v) => (
            <div key={v.name}>
              <label className="text-[11px] text-fg-muted block mb-0.5">
                {v.description || v.name}
                {v.required && <span className="text-red-400 ml-0.5">*</span>}
              </label>
              <input
                type="text"
                value={variables[v.name] ?? ''}
                onChange={(e) =>
                  setVariables((prev) => ({ ...prev, [v.name]: e.target.value }))
                }
                className="w-full h-7 rounded border border-edge bg-surface px-2 text-xs text-fg outline-none focus:border-accent"
                placeholder={v.name}
                disabled={isExecuting}
              />
            </div>
          ))}

          <button
            type="button"
            className="flex items-center gap-1.5 px-3 py-1.5 text-xs bg-accent text-white rounded hover:bg-accent/90 disabled:opacity-50 transition-colors"
            onClick={() => void handleExecute()}
            disabled={isExecuting}
          >
            {isExecuting ? (
              <Loader2 className="h-3.5 w-3.5 animate-spin" />
            ) : (
              <Play className="h-3.5 w-3.5" />
            )}
            {isExecuting ? t('skills.running') : t('skills.run')}
          </button>
        </div>
      )}

      {skillError && (
        <div className="text-xs text-red-400 rounded bg-red-500/10 p-2">
          {skillError}
        </div>
      )}

      {result && (
        <div className="border border-edge rounded-md p-3 bg-surface">
          <div className="text-[11px] text-fg-muted mb-1">{t('skills.result')}</div>
          <pre className="text-xs text-fg-secondary whitespace-pre-wrap break-words max-h-60 overflow-auto">
            {result}
          </pre>
        </div>
      )}
    </div>
  );
}
