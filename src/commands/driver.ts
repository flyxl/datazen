import { invoke } from '@tauri-apps/api/core';
import type { DriverCommandDefinition } from '../types';

export interface ExecuteDriverCommandRequest {
  connectionId?: string;
  driverType?: string;
  command: string;
  input: Record<string, unknown>;
}

export interface CommandResult {
  data: unknown;
}

export const driverCommands = {
  getConnectionCommands: (connectionId: string) =>
    invoke<DriverCommandDefinition[]>('get_connection_commands', { connectionId }),

  getDriverCommands: (driverType: string) =>
    invoke<DriverCommandDefinition[]>('get_driver_commands', { driverType }),

  execute: (request: ExecuteDriverCommandRequest) =>
    invoke<CommandResult>('execute_driver_command', { request }),
};
