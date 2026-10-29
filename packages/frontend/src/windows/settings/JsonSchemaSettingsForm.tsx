import { useMemo } from 'react';
import {
  applySchemaDefaults,
  listSchemaPropertyEntries,
  readBooleanField,
} from '@datazen/driver-sdk';

import { ToggleRow } from './settingsUi';

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
    const raw =
      value && typeof value === 'object' && !Array.isArray(value)
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
            hint={prop.description}
            checked={checked}
            onChange={(v) => handleFieldChange(key, v)}
          />
        );
      })}
    </div>
  );
}
