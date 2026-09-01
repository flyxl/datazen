interface TauriEventPluginInternals {
  unregisterListener?: (event: string, eventId: number) => void;
  __datazenSafeUnlistenInstalled?: boolean;
}

interface TauriEventGlobal {
  __TAURI_EVENT_PLUGIN_INTERNALS__?: TauriEventPluginInternals;
}

/**
 * Work around the Tauri 2.11 event-unlisten registration race.
 *
 * Tauri's public unlisten function first calls this internal hook and only
 * then sends `plugin:event|unlisten`. During a fast React StrictMode cleanup,
 * the backend may have returned the event id before its webview registry eval
 * lands. The internal hook then throws while reading the missing entry, which
 * prevents the backend cleanup IPC from running at all.
 *
 * Swallowing only that TypeError lets the public API continue to its backend
 * cleanup. Remove this compatibility shim once the upstream guard ships in the
 * Rust `tauri` crate used by this project.
 */
export function installTauriEventUnlistenRaceWorkaround(): void {
  const internals = (globalThis as typeof globalThis & TauriEventGlobal)
    .__TAURI_EVENT_PLUGIN_INTERNALS__;
  if (!internals?.unregisterListener || internals.__datazenSafeUnlistenInstalled) {
    return;
  }

  const unregisterListener = internals.unregisterListener.bind(internals);
  internals.unregisterListener = (event, eventId) => {
    try {
      unregisterListener(event, eventId);
    } catch (error) {
      const isMissingListenerEntry =
        error instanceof TypeError && error.message.includes('handlerId');
      if (!isMissingListenerEntry) throw error;
    }
  };
  internals.__datazenSafeUnlistenInstalled = true;
}
