import {
  Braces,
  Eye,
  Hash,
  Shapes,
  Table2,
  Zap,
  type LucideIcon,
} from 'lucide-react';
import { DB_REGISTRY } from '../../../lib/databaseTypes';

export interface SchemaTreeCategoryDef {
  id: string;
  labelKey: string;
  icon: LucideIcon;
  color: string;
}

export const BASE_CATEGORIES: SchemaTreeCategoryDef[] = [
  { id: 'tables', labelKey: 'schemaTree.tables', icon: Table2, color: 'text-blue-400' },
  { id: 'views', labelKey: 'schemaTree.views', icon: Eye, color: 'text-purple-400' },
];

export const OBJECT_KIND_CATEGORIES: Record<string, SchemaTreeCategoryDef> = {
  function: {
    id: 'function',
    labelKey: 'schemaTree.functions',
    icon: Braces,
    color: 'text-orange-400',
  },
  procedure: {
    id: 'procedure',
    labelKey: 'schemaTree.procedures',
    icon: Braces,
    color: 'text-emerald-400',
  },
  trigger: { id: 'trigger', labelKey: 'schemaTree.triggers', icon: Zap, color: 'text-amber-400' },
  sequence: {
    id: 'sequence',
    labelKey: 'schemaTree.sequences',
    icon: Hash,
    color: 'text-cyan-400',
  },
  type: { id: 'type', labelKey: 'schemaTree.types', icon: Shapes, color: 'text-pink-400' },
};

export const KV_CATEGORIES: SchemaTreeCategoryDef[] = [
  { id: 'tables', labelKey: 'schemaTree.keys', icon: Table2, color: 'text-blue-400' },
];

export const LEAF_KIND_ICON: Record<
  string,
  { icon: LucideIcon; color: string }
> = {
  table: { icon: Table2, color: 'text-blue-400' },
  view: { icon: Eye, color: 'text-purple-400' },
  materializedView: { icon: Eye, color: 'text-purple-400' },
  systemTable: { icon: Table2, color: 'text-gray-400' },
  function: { icon: Braces, color: 'text-orange-400' },
  procedure: { icon: Braces, color: 'text-emerald-400' },
  trigger: { icon: Zap, color: 'text-amber-400' },
  sequence: { icon: Hash, color: 'text-cyan-400' },
  type: { icon: Shapes, color: 'text-pink-400' },
};

/** Categories for a driver type (tables/views + supported object kinds). */
export function getCategoriesForDriver(databaseType: string): SchemaTreeCategoryDef[] {
  const meta = DB_REGISTRY[databaseType as keyof typeof DB_REGISTRY];
  const objectKinds = meta?.supportedObjectKinds;
  if (!objectKinds || objectKinds.length === 0) return BASE_CATEGORIES;
  const objectCats = objectKinds.map((kind) => OBJECT_KIND_CATEGORIES[kind]).filter(Boolean);
  return [...BASE_CATEGORIES, ...objectCats];
}

/** Effective categories for UnifiedSchemaTree (KV stores use keys label). */
export function getEffectiveCategories(
  databaseType: string,
  isKeyValue = false,
): SchemaTreeCategoryDef[] {
  if (isKeyValue) return KV_CATEGORIES;
  return getCategoriesForDriver(databaseType);
}
