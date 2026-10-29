export type { StructureEditorUiConfig } from '@datazen/extension-points';

/** Mirrors `packages/driver-api` structure editor types (serde camelCase JSON). */

export type AlterStrategy = 'none' | 'direct' | 'sqliteRebuild';

export type StructureChangeMode = 'create' | 'alter';

export type StatementRisk = 'additive' | 'destructive' | 'rewrite';

export interface StructureCapabilities {
  createTable: boolean;
  addColumn: boolean;
  dropColumn: boolean;
  renameColumn: boolean;
  alterType: boolean;
  alterNullability: boolean;
  alterDefault: boolean;
  alterPrimaryKey: boolean;
  reorderColumn: boolean;
  comment: boolean;
  createIndex: boolean;
  dropIndex: boolean;
  rebuildIndex: boolean;
  indexType: boolean;
  indexInclude: boolean;
  indexFilter: boolean;
  indexComment: boolean;
  alterStrategy: AlterStrategy;
  dialectId: string;
  indexMethods: string[];
}

export interface StructureColumnDraft {
  id: string;
  name: string;
  dataType: string;
  nullable: boolean;
  defaultValue?: string | null;
  comment?: string | null;
  isPrimaryKey?: boolean;
  isAutoIncrement?: boolean;
  isUnique?: boolean;
}

export interface StructureIndexDraft {
  id: string;
  name: string;
  columns: string[];
  isUnique: boolean;
  isPrimary?: boolean;
  indexType?: string;
  includeColumns?: string[];
  filter?: string | null;
  comment?: string | null;
}

export interface StructureChangeRequest {
  mode: StructureChangeMode;
  schema?: string | null;
  table: string;
  originalColumns?: StructureColumnDraft[];
  currentColumns: StructureColumnDraft[];
  originalIndexes?: StructureIndexDraft[];
  currentIndexes?: StructureIndexDraft[];
}

export interface PlanStatement {
  sql: string;
  summary: string;
  risk: StatementRisk;
}

export interface StructureChangePlan {
  statements: PlanStatement[];
  warnings?: string[];
}

export type StructureCapabilityFlag = {
  [K in keyof StructureCapabilities]: StructureCapabilities[K] extends boolean ? K : never;
}[keyof StructureCapabilities];
