import { getUpdateChannel } from './updater';
import { useUpdateReminderStore } from '../stores/updateReminderStore';

/** Start once per main window; elapsed wall time includes system sleep. */
export function startUpdateReminderSchedule(): () => void {
  if (getUpdateChannel() !== 'auto') return () => {};

  const checkIfDue = () => {
    void useUpdateReminderStore.getState().checkForUpdatesIfDue();
  };
  const onVisibilityChange = () => {
    if (document.visibilityState === 'visible') checkIfDue();
  };

  checkIfDue();
  window.addEventListener('focus', checkIfDue);
  window.addEventListener('online', checkIfDue);
  document.addEventListener('visibilitychange', onVisibilityChange);
  // Resume need not produce a focus/visibility event. This only checks the
  // local deadline; network requests remain limited to once per 24 hours.
  const intervalId = window.setInterval(checkIfDue, 60_000);

  return () => {
    window.clearInterval(intervalId);
    window.removeEventListener('focus', checkIfDue);
    window.removeEventListener('online', checkIfDue);
    document.removeEventListener('visibilitychange', onVisibilityChange);
  };
}
