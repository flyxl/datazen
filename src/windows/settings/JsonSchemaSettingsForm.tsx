import { useMemo } from 'react';
import {
  applySchemaDefaults,
  listSchemaPropertyEntries,
  readBooleanField,
} from '../../plugin-sdk/settings';

function ToggleRow({
  label,
  description,
  checked,
  onChange,
}: {
  label: string;
  description?: string;
  checked: boolean;
  onChange: (v: boolean) => void;
}) {
  return (
    <div className="flex items-start justify-between gap-4">
      <div className="min-w-0">
        <div className="text-sm text-fg-secondary">{label}</div>
        {description && <div className="mt-0.5 text-[11px] text-fg-muted">{description}</div>}
      </div>
      <button
        type="button"
        role="switch"
        aria-checked={checked}
        onClick={() => onChange(!checked)}
        className={`relative inline-flex h-5 w-9 shrink-0 cursor-pointer items-center rounded-full transition-colors ${
          checked ? 'bg-accent' : 'bg-edge'
        }`}
      >
        <span
          className={`inline-block h-3.5 w-3.5 rounded-full bg-white shadow transition-transform ${
            checked ? 'translate-x-[18px]' : 'translate-x-[3px]'
          }`}
        />
      </button>
    </div>
  );
}

export function JsonSchemaSettingsForm({
  schema,
  value,
  onChange,
}: {
  schema: object;
  value: unknown;
  onChange: (next: unknown) => void;
}) {
  const current = useMemo(() => {
    const raw = value && typeof value === 'object' && !Array.isArray(value)
      ? (value as Record<string, unknown>)
      : {};
    return applySchemaDefaults(schema, raw);
  }, [schema, value]);

  const fields = useMemo(() => listSchemaPropertyEntries(schema), [schema]);

  const handleFieldChange = (key: string, nextVal: boolean) => {
    onChange({ ...current, [key]: nextVal });
  };

  if (fields.length === 0) {
    return null;
  }

  return (
    <div className="space-y-3">
      {fields.map(({ key, prop }) => {
        const defaultVal = typeof prop.default === 'boolean' ? prop.default : false;
        const checked = readBooleanField(current, key, defaultVal);
        return (
          <ToggleRow
            key={key}
            label={prop.title ?? key}
            description={prop.description}
            checked={checked}
            onChange={(v) => handleFieldChange(key, v)}
          />
        );
      })}
    </div>
  );
}
