import type { NativeMenuItemDef } from './nativeContextMenu';
import { nativeEditMenuItems } from './nativeContextMenu';

export type SqlEditorContextMenuLabels = {
  addFavorite: string;
};

/**
 * Build native context-menu items for the SQL editor.
 * Always includes OS edit actions; favorite is enabled only when there is SQL text.
 */
export function buildSqlEditorContextMenuItems(
  labels: SqlEditorContextMenuLabels,
  sqlText: string,
  onAddFavorite: (sql: string) => void,
): NativeMenuItemDef[] {
  const trimmed = sqlText.trim();
  return [
    ...nativeEditMenuItems(),
    { kind: 'separator' },
    {
      kind: 'item',
      id: 'add-favorite',
      label: labels.addFavorite,
      enabled: trimmed.length > 0,
      action: () => {
        if (trimmed) onAddFavorite(trimmed);
      },
    },
  ];
}
