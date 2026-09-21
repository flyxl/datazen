/**
 * Settings store bridge — driver-SDK capability injection.
 *
 * Drivers never import the host `settingsStore` module directly. The host
 * binds its real zustand store at module load (same pattern as
 * `schemaStoreBridge.bindSchemaStore`); drivers consume it through
 * `useBoundSettingsStore`.
 */

/** Subset of the host settings state consumed by driver UI. */
export type SettingsBridgeState = {
  settings: {
    safeMode: boolean;
    editorFontFamily: string;
    driverSettings: Record<string, unknown>;
  };
};

export type BoundSettingsStore = {
  <T>(selector: (state: SettingsBridgeState) => T): T;
  getState(): SettingsBridgeState;
  setState: (partial: Record<string, unknown>) => void;
};

let boundStore: BoundSettingsStore | null = null;

export function bindSettingsStore(store: BoundSettingsStore): void {
  boundStore = store;
}

function getStore(): BoundSettingsStore {
  if (!boundStore) {
    throw new Error('SettingsStore has not been bound to driver-sdk yet.');
  }
  return boundStore;
}

export type UseBoundSettingsStore = {
  <T>(selector: (state: SettingsBridgeState) => T): T;
  getState(): SettingsBridgeState;
  setState(
    partial:
      | Partial<SettingsBridgeState>
      | ((state: SettingsBridgeState) => Partial<SettingsBridgeState>),
  ): void;
};

/** Zustand-like accessor: hook selector form + imperative `getState`/`setState`. */
export const useBoundSettingsStore: UseBoundSettingsStore = Object.assign(
  <T>(selector: (state: SettingsBridgeState) => T): T => getStore()(selector),
  {
    getState: (): SettingsBridgeState => getStore().getState(),
    setState: (
      partial:
        | Partial<SettingsBridgeState>
        | ((state: SettingsBridgeState) => Partial<SettingsBridgeState>),
    ): void => {
      // Forwarded verbatim to the bound zustand store (partial object or
      // updater function, exactly like `store.setState`).
      getStore().setState(partial as unknown as Record<string, unknown>);
    },
  },
);
