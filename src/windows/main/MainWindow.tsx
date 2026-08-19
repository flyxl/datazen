import { ConnectionWindow } from '../connection/ConnectionWindow';

/**
 * Main window now directly hosts the unified connection workspace.
 * Legacy launcher behaviors were migrated into `ConnectionWindow`.
 */
export function MainWindow() {
  return <ConnectionWindow />;
}
