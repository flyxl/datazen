import type { ComponentType } from 'react';
import type { AppSettings } from '../types';

export type PluginSettingsContribution = {
  pluginId: string;
  label: string;
  SettingsSection?: ComponentType<{ value: unknown; onChange: (next: unknown) => void }>;
  schema?: object;
};

export function mergePluginSettings(
  all: AppSettings['pluginSettings'],
  pluginId: string,
  next: unknown,
): AppSettings['pluginSettings'] {
  return { ...all, [pluginId]: next };
}

export function readBooleanField(
  obj: Record<string, unknown>,
  key: string,
  defaultValue: boolean,
): boolean {
  const val = obj[key];
  if (typeof val === 'boolean') return val;
  return defaultValue;
}

type JsonSchemaProperty = {
  type?: string;
  title?: string;
  description?: string;
  default?: unknown;
};

type JsonSchemaObject = {
  type?: string;
  properties?: Record<string, JsonSchemaProperty>;
};

export function applySchemaDefaults(
  schema: object,
  value: Record<string, unknown>,
): Record<string, unknown> {
  const s = schema as JsonSchemaObject;
  if (s.type !== 'object' || !s.properties) return { ...value };

  const out = { ...value };
  for (const [key, prop] of Object.entries(s.properties)) {
    if (out[key] === undefined && prop.default !== undefined) {
      out[key] = prop.default;
    }
  }
  return out;
}

export function listBooleanSchemaFields(schema: object): JsonSchemaProperty[] {
  const s = schema as JsonSchemaObject;
  if (s.type !== 'object' || !s.properties) return [];
  return Object.values(s.properties).filter((p) => p.type === 'boolean');
}

export function listSchemaPropertyEntries(
  schema: object,
): { key: string; prop: JsonSchemaProperty }[] {
  const s = schema as JsonSchemaObject;
  if (s.type !== 'object' || !s.properties) return [];
  return Object.entries(s.properties)
    .filter(([, prop]) => prop.type === 'boolean')
    .map(([key, prop]) => ({ key, prop }));
}
