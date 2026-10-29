/**
 * Connection view contracts now live in @datazen/driver-sdk (types-to-sdk
 * track). This module re-exports them so existing host imports
 * (`src/lib/connectionViews/types`) keep working unchanged.
 */
export type {
  NodeContextMenuPayload,
  ConnectionOpenTarget,
  ConnectionViewActions,
  ConnectionViewProps,
} from '@datazen/driver-sdk';
