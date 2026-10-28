import { open } from '@tauri-apps/plugin-dialog';
import type { PathPicker } from '@datazen/ui';

/**
 * The host's {@link PathPicker}: the single place in the app that opens a
 * native path dialog.
 *
 * `@datazen/ui` is bundled by drivers, extensions and window runtimes that have
 * no Tauri webview, so the design system cannot own this call — it asks for one
 * through the `onBrowse` prop instead, and the host hands it this function.
 * The `{ multiple: false, ...options }` default matches what `PathInput` did
 * when the call still lived inside the package.
 */
export const pickPath: PathPicker = async (options) => {
  const selected = await open({ multiple: false, ...options });
  return typeof selected === 'string' ? selected : null;
};
