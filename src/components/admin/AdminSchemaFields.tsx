import { Input } from '../ui/Input';
import {
  fieldLabel,
  fieldPlaceholder,
  getSchemaField,
  hasSchemaField,
  schemaProperties,
  type SchemaField,
} from '../../lib/commandSchema';
import type { DriverCommandDefinition } from '../../types';

interface AdminSchemaFieldsProps {
  definition?: DriverCommandDefinition;
  values: Record<string, string>;
  onChange: (name: string, value: string) => void;
  exclude?: string[];
  labels?: Record<string, string>;
}

function renderField(
  field: SchemaField,
  value: string,
  onChange: (name: string, value: string) => void,
  labelOverride?: string,
) {
  if (field.schema.type !== 'string' && field.schema.type != null) return null;

  return (
    <div key={field.name}>
      <label className="block text-xs text-fg-muted mb-1">
        {labelOverride ?? fieldLabel(field)}
      </label>
      <Input
        value={value}
        onChange={(e) => onChange(field.name, e.target.value)}
        placeholder={fieldPlaceholder(field)}
      />
    </div>
  );
}

export function AdminSchemaFields({
  definition,
  values,
  onChange,
  exclude = [],
  labels,
}: AdminSchemaFieldsProps) {
  if (!definition) return null;

  const excluded = new Set(exclude);
  const fields = schemaProperties(definition).filter((field) => !excluded.has(field.name));

  return (
    <>
      {fields.map((field) =>
        renderField(field, values[field.name] ?? '', onChange, labels?.[field.name]),
      )}
    </>
  );
}

export function adminSchemaHasField(
  definition: DriverCommandDefinition | undefined,
  name: string,
): boolean {
  return hasSchemaField(definition, name);
}

export function adminSchemaField(
  definition: DriverCommandDefinition | undefined,
  name: string,
): SchemaField | undefined {
  return getSchemaField(definition, name);
}
