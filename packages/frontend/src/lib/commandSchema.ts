import type { DriverCommandDefinition } from '../types';

export interface SchemaField {
  name: string;
  schema: Record<string, unknown>;
  required: boolean;
}

export interface PrivilegeGroup {
  label: string;
  privileges: string[];
}

export function schemaProperties(definition?: DriverCommandDefinition): SchemaField[] {
  const schema = definition?.inputSchema;
  const properties = schema?.properties;
  if (!properties || typeof properties !== 'object' || Array.isArray(properties)) return [];
  return Object.entries(properties as Record<string, unknown>).map(([name, raw]) => ({
    name,
    schema: (raw && typeof raw === 'object' ? raw : {}) as Record<string, unknown>,
    required: Array.isArray(schema?.required) && schema.required.includes(name),
  }));
}

export function hasSchemaField(
  definition: DriverCommandDefinition | undefined,
  name: string,
): boolean {
  return schemaProperties(definition).some((field) => field.name === name);
}

export function getSchemaField(
  definition: DriverCommandDefinition | undefined,
  name: string,
): SchemaField | undefined {
  return schemaProperties(definition).find((field) => field.name === name);
}

export function fieldPlaceholder(field: SchemaField): string | undefined {
  const examples = field.schema.examples;
  if (Array.isArray(examples) && typeof examples[0] === 'string') {
    return examples[0];
  }
  const example = field.schema.example;
  if (typeof example === 'string') return example;
  return undefined;
}

export function fieldLabel(field: SchemaField): string {
  if (typeof field.schema.title === 'string' && field.schema.title) return field.schema.title;
  if (typeof field.schema.description === 'string' && field.schema.description) {
    return field.schema.description;
  }
  return field.name;
}

function readDatazenExtension(
  definition?: DriverCommandDefinition,
): Record<string, unknown> | undefined {
  const ext = definition?.inputSchema?.['x-datazen'];
  if (!ext || typeof ext !== 'object' || Array.isArray(ext)) return undefined;
  return ext as Record<string, unknown>;
}

export function privilegeGroups(definition?: DriverCommandDefinition): PrivilegeGroup[] {
  const ext = readDatazenExtension(definition);
  const raw = ext?.privilegeGroups;
  if (!Array.isArray(raw)) return [];

  const groups: PrivilegeGroup[] = [];
  for (const item of raw) {
    if (!item || typeof item !== 'object' || Array.isArray(item)) continue;
    const record = item as Record<string, unknown>;
    const privileges = Array.isArray(record.privileges)
      ? record.privileges.filter((p): p is string => typeof p === 'string')
      : [];
    if (privileges.length === 0) continue;
    groups.push({
      label: typeof record.label === 'string' ? record.label : '',
      privileges,
    });
  }
  return groups;
}

export function allPrivileges(definition?: DriverCommandDefinition): string[] {
  const grouped = privilegeGroups(definition);
  if (grouped.length === 0) return [];
  return grouped.flatMap((group) => group.privileges);
}

export function hasCommand(
  definitions: DriverCommandDefinition[] | undefined,
  commandId: string,
): boolean {
  return definitions?.some((definition) => definition.id === commandId) ?? false;
}
