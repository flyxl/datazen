/**
 * §4.3 Snippet dropdown.
 *
 * Reuses the app-wide Web Context Menu host (mounted in `App.tsx`) instead of
 * introducing a second popover implementation, so click-away and keyboard
 * dismissal behave identically to every other menu.
 */
import { useCallback, type MouseEvent, type MutableRefObject } from 'react';
import { Code2 } from 'lucide-react';
import { ToolbarButton } from '../../../../components/ui/ToolbarButton';
import { showNativeContextMenu } from '../../../../lib/nativeContextMenu';
import {
  BUILTIN_SQL_SNIPPETS,
  type SqlSnippetItem,
} from '../../../../components/sql-editor/snippets';
import { useI18n } from '../../../../hooks/useI18n';
import { useSettingsStore } from '../../../../stores/settingsStore';
import { tid } from '../../../../lib/tid';
import type { I18nKey } from '../../../../locales';
import type { SqlEditorHandle } from '../../../../components/SqlEditor';

export interface SnippetMenuButtonProps {
  editorRef: MutableRefObject<SqlEditorHandle | null>;
  compact?: boolean;
  disabled?: boolean;
}

export function SnippetMenuButton({ editorRef, compact, disabled }: SnippetMenuButtonProps) {
  const { t } = useI18n();
  const userSnippets = useSettingsStore((s) => s.settings.sqlSnippets);

  const openMenu = useCallback(
    (event: MouseEvent<HTMLButtonElement>) => {
      const snippets: SqlSnippetItem[] = [...BUILTIN_SQL_SNIPPETS, ...(userSnippets ?? [])];
      const rect = event.currentTarget.getBoundingClientRect();

      showNativeContextMenu(
        snippets.map((item) => ({
          kind: 'item' as const,
          id: `snippet-${item.id}`,
          label: t(item.descriptionKey as I18nKey),
          // The prefix doubles as the "shortcut" column so users learn to type it.
          shortcut: item.prefix,
          action: () => editorRef.current?.insertSnippet?.(item.template),
        })),
        // Anchor under the button; the menu host clamps to the viewport itself.
        { x: rect.left, y: rect.bottom },
      );
    },
    [editorRef, t, userSnippets],
  );

  return (
    <ToolbarButton
      compact={compact}
      variant="ghost"
      label={t('query.snippets')}
      title={t('query.snippetsTitle')}
      icon={<Code2 className="h-3.5 w-3.5" />}
      onClick={openMenu}
      disabled={disabled}
      {...tid('editor-snippets-button')}
    />
  );
}
