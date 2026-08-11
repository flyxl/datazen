import { invoke } from '@tauri-apps/api/core';
import type { DriverCommandDefinition } from '../types';

export interface ExecuteDriverCommandRequest {
  connectionId: string;
  command: string;
  input: Record<string, unknown>;
}

export interface CommandResult {
  data: unknown;
}

export const driverCommands = {
  getConnectionCommands: (connectionId: string) =>
    invoke<DriverCommandDefinition[]>('get_connection_commands', { connectionId }),

  execute: (request: ExecuteDriverCommandRequest) =>
    invoke<CommandResult>('execute_driver_command', { request }),
};
