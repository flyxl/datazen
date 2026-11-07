/**
 * The legacy driver-command gateway domain client (form-split F3).
 *
 * **Transitional escape hatch.** These five commands predate the kernel
 * surface: `packages/driver-sdk` speaks them directly over
 * `client.transport`, and they are declared in `MethodMap` only so the SDK
 * could be migrated without changing a single call site. This domain client
 * gives them a first-class, guard-wrapped surface for that migration; the
 * whole domain — map slice, client, and the host commands behind it — is
 * deleted once the kernel implements those methods (plan §5.3).
 *
 * Method names stay snake_case on purpose: they are the *existing* IPC command
 * names, not future kernel names, and pretending otherwise would invite
 * renaming churn on code whose destiny is deletion.
 */

import type { BackendGuard } from './guard';
import type { BackendTransport, CommandResult } from './transport';

/** The legacy gateway slice of `MethodMap`. */
export interface DriverGatewayMethodMap {
  get_driver_commands: { request: { driverType: string }; response: unknown[] };
  get_connection_commands: { request: { dbSessionId: string }; response: unknown[] };
  execute_driver_command: { request: unknown; response: CommandResult };
  /**
   * The streaming gateway. It takes a push callback rather than a subscription
   * handle because that is what the current backend offers, and this contract
   * must describe what exists: the `AsyncIterable` form belongs on
   * `BackendTransport.subscribe`, which a backend implements once it can push
   * an ordered event stream instead of a bare callback.
   */
  execute_driver_command_stream: {
    request: {
      /** The nested driver-command request, forwarded untouched. */
      request: unknown;
      /**
       * The adapter's own push handle — a channel on the desktop, a WebSocket
       * on the web — passed through opaquely. Typing it concretely here would
       * name a transport, and this package names none.
       */
      onEvent: unknown;
      applyResultLimit?: boolean;
      recordHistory?: boolean;
    };
    response: undefined;
  };
  /**
   * Binary open has no `PlatformServices` counterpart: §7.1 declares
   * text-open and directory-open only. The legacy command therefore stays on
   * the transport until the spec adds an open-binary capability.
   */
  open_base64_with_dialog: { request: Record<string, unknown>; response: unknown };
}

/** The legacy gateway domain client surface. */
export interface DriverGatewayClient {
  /** The command catalogue of one driver type. */
  get_driver_commands(driverType: string): Promise<unknown[]>;
  /** The command catalogue of one live connection. */
  get_connection_commands(dbSessionId: string): Promise<unknown[]>;
  /** Execute one catalogue command; the `{ data }` envelope is a public contract. */
  execute_driver_command(request: unknown): Promise<CommandResult>;
  /**
   * Execute one catalogue command with streamed deliveries. The push handle is
   * passed through opaquely for exactly the same reason as on the map.
   */
  execute_driver_command_stream(request: {
    request: unknown;
    onEvent: unknown;
    applyResultLimit?: boolean;
    recordHistory?: boolean;
  }): Promise<void>;
  /** Open a binary file through the host dialog; base64 comes back. */
  open_base64_with_dialog(request: Record<string, unknown>): Promise<unknown>;
}

/** Build the legacy gateway domain client over a transport. */
export function createDriverGatewayMethods(
  transport: BackendTransport,
  guard: BackendGuard,
): DriverGatewayClient {
  return {
    get_driver_commands: (driverType) =>
      guard('get_driver_commands', () => transport.call('get_driver_commands', { driverType })),

    get_connection_commands: (dbSessionId) =>
      guard('get_connection_commands', () =>
        transport.call('get_connection_commands', { dbSessionId }),
      ),

    execute_driver_command: (request) =>
      guard('execute_driver_command', () => transport.call('execute_driver_command', request)),

    execute_driver_command_stream: (request) =>
      guard('execute_driver_command_stream', () =>
        transport.call('execute_driver_command_stream', request),
      ),

    open_base64_with_dialog: (request) =>
      guard('open_base64_with_dialog', () => transport.call('open_base64_with_dialog', request)),
  };
}

/** Compile-time: every legacy gateway map key is served by this domain client. */
type Assert<T extends true> = T;
export type DriverGatewayClientCoversMap = Assert<
  keyof DriverGatewayMethodMap extends keyof DriverGatewayClient ? true : false
>;
