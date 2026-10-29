/**
 * Generic driver-command IPC gateway. The single implementation lives in
 * `@datazen/driver-sdk` (`ipc/driverCommands`); this thin host module keeps
 * existing `commands/driver` imports stable.
 */
export { driverCommands } from '@datazen/driver-sdk';
export type {
  ExecuteDriverCommandRequest,
  ExecuteDriverCommandStreamRequest,
  CommandResult,
} from '@datazen/driver-sdk';
