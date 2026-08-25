import { Channel, invoke } from '@tauri-apps/api/core';
import type { DriverCommandDefinition, QueryStreamEvent } from '../types';

export interface ExecuteDriverCommandRequest {
  /** Runtime db session id. `resolve_session` also accepts a persisted
   * connection id (dual-mode; e.g. the extension bridge falls back to the
   * raw id when it cannot resolve a live session). */
  dbSessionId?: string;
  driverType?: string;
  command: string;
  input: Record<string, unknown>;
}

export interface ExecuteDriverCommandStreamRequest {
  dbSessionId: string;
  command: string;
  input: Record<string, unknown>;
  onEvent: (event: QueryStreamEvent) => void;
  applyResultLimit?: boolean;
  recordHistory?: boolean;
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

  executeStream: async (request: ExecuteDriverCommandStreamRequest) => {
    const onEventChannel = new Channel<QueryStreamEvent>();
    onEventChannel.onmessage = request.onEvent;
    await invoke<void>('execute_driver_command_stream', {
      request: {
        dbSessionId: request.dbSessionId,
        command: request.command,
        input: request.input,
      },
      onEvent: onEventChannel,
      applyResultLimit: request.applyResultLimit,
      recordHistory: request.recordHistory,
    });
  },
};
