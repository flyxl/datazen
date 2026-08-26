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
  /** F1: optional explicit database pin — the host switches the session to
   * this logical database before executing (session-bound commands only). */
  database?: string | null;
  /** F7: optional target schema (PG-family). Rewrite-capable drivers inline
   * it as a qualified name; others ignore it. */
  schema?: string | null;
}

export interface ExecuteDriverCommandStreamRequest {
  dbSessionId: string;
  command: string;
  input: Record<string, unknown>;
  onEvent: (event: QueryStreamEvent) => void;
  /** F1: optional explicit database pin, applied before streaming. */
  database?: string | null;
  /** F7: optional target schema (PG-family), forwarded with the stream
   * request like `database`. */
  schema?: string | null;
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
        database: request.database ?? null,
        schema: request.schema ?? null,
      },
      onEvent: onEventChannel,
      applyResultLimit: request.applyResultLimit,
      recordHistory: request.recordHistory,
    });
  },
};
