import { newColumnId, newIndexId } from './draftIds';
import type {
  StructureColumnDraft,
  StructureEditorUiConfig,
  StructureIndexDraft,
} from './types';

export function emptyColumnDraft(defaultType: string): StructureColumnDraft {
  return {
    id: newColumnId(),
    name: '',
    dataType: defaultType,
    nullable: true,
    defaultValue: null,
    comment: null,
    isPrimaryKey: false,
    isAutoIncrement: false,
    isUnique: false,
  };
}

export function emptyIndexDraft(defaultMethod: string): StructureIndexDraft {
  return {
    id: newIndexId(),
    name: '',
    columns: [],
    isUnique: false,
    isPrimary: false,
    indexType: defaultMethod,
  };
}

export function defaultCreateColumns(uiConfig: StructureEditorUiConfig): StructureColumnDraft[] {
  return [
    {
      ...emptyColumnDraft(uiConfig.defaultColumnType),
      name: 'id',
      nullable: false,
      isPrimaryKey: true,
    },
    emptyColumnDraft(uiConfig.defaultColumnType),
  ];
}
