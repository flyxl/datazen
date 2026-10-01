import { Channel } from '@tauri-apps/api/core';
import type { DriverCommandDefinition, QueryStreamEvent } from '../../../../src/types';
import { transitionalTransport } from './desktopBinding';

export interface ExecuteDriverCommandRequest {
  /** Runtime db session id (required for session-bound commands). */
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

/**
 * Thin re-export over the bound `BackendTransport` (§7.2 薄再导出过渡).
 *
 * Same exported names, same signatures, same arguments — the payload handed to
 * the backend is byte-identical to what the previous direct `invoke` sent. Only
 * the routing moved: the transport is what a non-desktop build replaces, and
 * the `Channel` wiring below is the desktop half of that seam, kept here where
 * the push callback is still part of this signature.
 */
export const driverCommands = {
  getConnectionCommands: (dbSessionId: string) =>
    transitionalTransport().call('get_connection_commands', { dbSessionId }) as Promise<
      DriverCommandDefinition[]
    >,

  getDriverCommands: (driverType: string) =>
    transitionalTransport().call('get_driver_commands', { driverType }) as Promise<DriverCommandDefinition[]>,

  execute: (request: ExecuteDriverCommandRequest) =>
    transitionalTransport().call('execute_driver_command', { request }) as Promise<CommandResult>,

  executeStream: async (request: ExecuteDriverCommandStreamRequest) => {
    const onEventChannel = new Channel<QueryStreamEvent>();
    onEventChannel.onmessage = request.onEvent;
    await transitionalTransport().call('execute_driver_command_stream', {
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