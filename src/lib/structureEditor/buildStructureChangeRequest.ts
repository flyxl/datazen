import type {
  StructureChangeMode,
  StructureChangeRequest,
  StructureColumnDraft,
  StructureIndexDraft,
} from './types';

export interface BuildStructureChangeRequestParams {
  mode: StructureChangeMode;
  table: string;
  schema?: string | null;
  originalColumns: StructureColumnDraft[];
  currentColumns: StructureColumnDraft[];
  originalIndexes: StructureIndexDraft[];
  currentIndexes: StructureIndexDraft[];
}

export function buildStructureChangeRequest(
  params: BuildStructureChangeRequestParams,
): StructureChangeRequest {
  const namedColumns = params.currentColumns.filter((c) => c.name.trim());

  return {
    mode: params.mode,
    table: params.table.trim(),
    schema: params.schema ?? null,
    originalColumns: params.mode === 'alter' ? params.originalColumns : [],
    currentColumns: namedColumns,
    originalIndexes: params.mode === 'alter' ? params.originalIndexes : [],
    currentIndexes: params.currentIndexes,
  };
}
