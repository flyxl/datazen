import { useI18n } from '../../hooks/useI18n';
import type { SqlParam } from '../../lib/sqlBindParams';
import { Input } from '../ui/Input';

interface BindParamPanelProps {
  params: SqlParam[];
  values: Record<string, string>;
  onChange: (name: string, value: string) => void;
}

export function BindParamPanel({ params, values, onChange }: BindParamPanelProps) {
  const { t } = useI18n();
  if (params.length === 0) return null;

  return (
    <div className="border-b border-edge bg-surface px-3 py-2">
      <div className="mb-1.5 text-[11px] font-semibold uppercase tracking-wider text-fg-muted">
        {t('query.params')}
      </div>
      <div className="flex flex-wrap gap-2">
        {params.map((p) => (
          <label key={`${p.kind}:${p.name}`} className="flex items-center gap-1.5 text-xs text-fg-secondary">
            <span className="font-mono text-fg">{p.kind === 'named' ? `:${p.name}` : `$${p.name}`}</span>
            <Input
              value={values[p.name] ?? ''}
              onChange={(e) => onChange(p.name, e.target.value)}
              placeholder={t('query.paramValue')}
              className="h-7 w-36 text-xs"
            />
          </label>
        ))}
      </div>
    </div>
  );
}
