import type { WappPermission } from '../../types/wapp';

/** Human-readable explanations shown as permission badge tooltips. */
export const PERMISSION_LABELS: Record<WappPermission, string> = {
  'context:connections': 'Read the connection list (names and types only, never credentials)',
  'command:invoke': 'Run database commands through the host Driver Command API',
  'storage:local': 'Keep a small private key-value store on this machine',
  'ui:notify': 'Show notifications via the host (rate limited)',
};
