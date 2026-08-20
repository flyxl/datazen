import { ConnectionPage } from '../connection/ConnectionPage';

/**
 * Main window now directly hosts the unified connection workspace.
 * Legacy launcher behaviors were migrated into `ConnectionPage`.
 */
export function MainPage() {
  return <ConnectionPage />;
}
