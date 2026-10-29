import type { NativeMenuPredefined } from './nativeContextMenu';

const COMMANDS: Record<Exclude<NativeMenuPredefined, 'Separator'>, string> = {
  Cut: 'cut',
  Copy: 'copy',
  Paste: 'paste',
  SelectAll: 'selectAll',
  Undo: 'undo',
  Redo: 'redo',
};

/** Map OS predefined edit items onto document.execCommand for Web menus. */
export function runPredefinedEdit(item: NativeMenuPredefined): void {
  if (item === 'Separator') return;
  const command = COMMANDS[item];
  if (!command) return;
  document.execCommand(command);
}
