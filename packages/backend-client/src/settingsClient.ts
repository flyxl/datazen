/**
 * The Settings domain client (form-split F3).
 *
 * Two methods, one contract: shared, form-factor-independent settings. The
 * desktop projection (`DesktopSettings extends SharedSettings`) is the F4
 * demonstration domain — until then this client delegates 1:1, same as every
 * other domain.
 *
 * A domain client is independently importable
 * (`import { createSettingsMethods } from '@datazen/backend-client'`), composes
 * into the facade through `clients.ts`, and owns exactly its slice of
 * `MethodMap` — the slice is declared here and intersected into the map in
 * `transport.ts`, so this file is the only place a Settings method can be
 * added without the port noticing.
 */

import type { BackendGuard } from './guard';
import type { BackendTransport } from './transport';
import type { SharedSettings } from './settings';

/** The Settings slice of `MethodMap`. */
export interface SettingsMethodMap {
  getSettings: { request: undefined; response: SharedSettings };
  saveSettings: { request: { settings: SharedSettings }; response: SharedSettings };
}

/** The Settings domain client surface. */
export interface SettingsClient {
  getSettings(): Promise<SharedSettings>;
  saveSettings(settings: SharedSettings): Promise<SharedSettings>;
}

/** Build the Settings domain client over a transport. */
export function createSettingsMethods(
  transport: BackendTransport,
  guard: BackendGuard,
): SettingsClient {
  return {
    getSettings: () => guard('getSettings', () => transport.call('getSettings', undefined)),
    saveSettings: (settings) =>
      guard('saveSettings', () => transport.call('saveSettings', { settings })),
  };
}

/** Compile-time: every Settings map key is served by this domain client. */
type Assert<T extends true> = T;
export type SettingsClientCoversMap = Assert<
  keyof SettingsMethodMap extends keyof SettingsClient ? true : false
>;
