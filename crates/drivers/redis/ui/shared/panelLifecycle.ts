/**
 * Tab-lifetime hook for the driver's per-tab state.
 *
 * The host hands the view a `panelId` (an opaque per-tab identity) and one
 * reliable disposal signal: `onPanelClosed`. Nothing else. Switching tabs
 * *unmounts* the view — the host renders only the active tab — and the state
 * must survive that, so cleanup keyed on unmount would be wrong twice over:
 * it would wipe the user's console on every tab switch, and it would never fire
 * for a tab closed while inactive. Hence the close signal.
 *
 * ## Why the registration outlives the component
 *
 * A tab can be closed while its view is unmounted, and the view is not around to
 * hear about it. So the registration is deliberately held at module scope,
 * keyed by `panelId`, rather than in the effect's cleanup: the host keeps it
 * alive across unmounts, and `bound` stops a remount from stacking duplicates.
 */
import { resetRightTab } from './rightTabState';
import { resetTranscript } from '../console/consoleTranscript';

const bound = new Set<string>();

/**
 * Drop this tab's state when the host closes the tab. Idempotent per `panelId`,
 * so calling it on every mount is both safe and free.
 */
export function bindPanelClose(
  panelId: string,
  onPanelClosed: (handler: () => void) => () => void,
): void {
  if (bound.has(panelId)) return;
  bound.add(panelId);
  onPanelClosed(() => {
    bound.delete(panelId);
    resetRightTab(panelId);
    resetTranscript(panelId);
  });
}

/** Test-only: forget which panels are bound, so a case starts unregistered. */
export function resetPanelBindings(): void {
  bound.clear();
}
