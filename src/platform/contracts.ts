/**
 * Platform port contracts (form-split plan §3.3).
 *
 * Shared code sees platforms ONLY through these narrow ports plus
 * `configurePlatform`. It must never import a concrete platform entity from
 * `platform/desktop/` (gate R7) — the desktop implementations are app-owned
 * and are injected here at bootstrap time by the assembly root.
 *
 * Naming: `Platform` is already taken by `hooks/usePlatform` for the *OS*
 * dimension (macos/windows/linux — orthogonal to the form factor, both forms
 * need it). The form-factor host port is `PlatformServices` (backend-client
 * §7.1, extended with `openExternal`); the desktop-only overlay is
 * `DesktopHostServices`.
 */

import type { PlatformServices } from '@datazen/backend-client';
import type { SettingsSection } from '../windows/settings/settingsSections';
import type { AppSettings } from '../types';
import type {
  ExportProgressEvent as TablesExportProgressEvent,
  ExportTablesRequest as TablesExportRequest,
  ExportTablesResult as TablesExportResult,
} from '../commands/file';
export type {
  ExportProgressEvent as TablesExportProgressEvent,
  ExportTablesRequest as TablesExportRequest,
  ExportTablesResult as TablesExportResult,
} from '../commands/file';

// ── Port 2: window routing (absorbs src/lib/windowManager.ts) ────────────────

export interface OpenWindowOptions {
  params?: Record<string, string>;
  width?: number;
  height?: number;
  minWidth?: number;
  minHeight?: number;
  title?: string;
  center?: boolean;
}

/** Delivered to `onCloseRequested` handlers so they can veto the close. */
export interface WindowCloseEvent {
  preventDefault(): void;
}

/**
 * Window-level chrome: fullscreen queries/events, background color and the
 * close lifecycle. A web implementation degrades (F11 semantics / no-op) —
 * that is the port's contract, not a runtime probe.
 */
export interface WindowChromePort {
  isFullscreen(): Promise<boolean>;
  onFullscreenChanged(cb: (fs: boolean) => void): Promise<() => void>;
  setBackgroundRootColor(hex: string): Promise<void>;
  onCloseRequested(cb: (event: WindowCloseEvent) => void): Promise<() => void>;
  minimizeCurrent(): Promise<void>;
  closeCurrent(): Promise<void>;
}

export interface WindowRouter {
  openSingletonWindow(label: string, options: OpenWindowOptions): void;
  openMultiInstanceWindow(label: string, options: OpenWindowOptions): void;
  focusMainWindow(): Promise<void>;
  hasOpenChildWindows(): Promise<boolean>;
  readonly windowChrome: WindowChromePort;
}

// ── Port 3: event channel (absorbs src/lib/crossWindowBus.ts) ────────────────

/**
 * Host broadcast events (`menu:*`, `datazen:open-connection`, `ai:stream-*`,
 * `wapps:changed` …). Kernel data-plane events are a different rail and stay
 * on `BackendTransport.subscribe` (`EventStreamSource`); the two coexist by
 * design and must not be merged.
 */
export interface EventChannel {
  emitCrossWindow(event: string, payload?: unknown): Promise<void>;
  listenCrossWindow(event: string, handler: (payload?: unknown) => void): Promise<() => void>;
}

// ── Port 4: desktop host overlay (desktop-only, app-exported) ────────────────

/**
 * Pure-desktop host services: the host settings pair, menu rebuild and the
 * host directory openers. This *type* lives in contracts so the assembly root
 * can reference it, but the implementation is only exported from the desktop
 * zone — a web build has no name for it at all (ruling #7).
 */
export interface DesktopHostServices {
  getHostSettings(): Promise<AppSettings>;
  saveHostSettings(s: AppSettings): Promise<void>;
  rebuildMenu(language: string): Promise<void>;
  getLogPath(): Promise<string>;
  getAppExecutablePath(): Promise<string>;
  /** @deprecated E2E-only; prefer openLogDir / openWorkflowsDir / openContextDir (one IPC: open_known_dir). */
  openPath(path: string): Promise<void>;
  openLogDir(): Promise<void>;
  openWorkflowsDir(): Promise<void>;
  openContextDir(): Promise<void>;
  checkUpdatesOnStartup(enabled: boolean): void;
  /**
   * The host app's own version string (the desktop app bundle's). Added by
   * the form-split R6 ledger burn — the About surfaces and the onboarding
   * brand panel used to import `@tauri-apps/api/app` directly.
   */
  getAppVersion(): Promise<string>;
  /**
   * Persist the resolved surface background for the next window's init-script
   * bake (host file, desktop-only paint continuity concern). Best-effort fire
   * and forget. Added by the form-split R6 ledger burn (surfaceBgCache).
   */
  persistSurfaceBackground(hex: string, dark: boolean): void;
}

// ── Port 5: host settings (form-split F4) ────────────────────────────────────

/**
 * The host half of the settings surface, as shared code sees it.
 *
 * The settings surface is split in two by the form-split plan: the shared half
 * rides the kernel track (`BackendClient.getSettings()` / `saveSettings()` —
 * nested `Settings` wire, same signature in every form factor); the host half
 * (paths, process, updater and grid-appearance fields the desktop settings
 * file carries beyond the shared contract) rides this port. Shared code never
 * learns which form factor it is in — it calls `requireHostSettings()`
 * unconditionally, and every assembly binds *something*: the desktop binds
 * the projecting `DesktopSettingsClient`, a web build binds the inert port
 * (extras `{}`, writes no-op — the kernel save is the whole write).
 */
export interface HostSettingsPort {
  /**
   * The host-only extras. Web form: `{}` — the shared contract is all there
   * is, so call sites merge it under the shared half and stay branch-free.
   */
  readHostExtras(): Promise<Partial<AppSettings>>;
  /**
   * Persist the full flat settings blob. Desktop: the host write normalizes
   * the shared half inside and fires the monitor/tray side effects. Web: a
   * no-op — the kernel save that follows in `settingsCommands.saveSettings`
   * is the whole write.
   */
  writeHostSettings(settings: AppSettings): Promise<void>;
  /** Rebuild the native menu after a language switch. Web: no-op. */
  rebuildMenu(language: string): Promise<void>;
  getLogPath(): Promise<string>;
  getAppExecutablePath(): Promise<string>;
  /** @deprecated E2E-only; prefer openLogDir / openWorkflowsDir / openContextDir (one IPC: open_known_dir). */
  openPath(path: string): Promise<void>;
  openLogDir(): Promise<void>;
  openWorkflowsDir(): Promise<void>;
  openContextDir(): Promise<void>;
  /** Kick the updater's startup check when the user enables it. Web: no-op. */
  checkUpdatesOnStartup(enabled: boolean): void;
  /**
   * The host app's version string. Rejects on forms without a host app —
   * callers gate on `capabilities.versionSource === 'build-constant'` and
   * keep their own fallback. Added by the form-split R6 ledger burn.
   */
  getAppVersion(): Promise<string>;
  /**
   * Persist the surface background for the next window's first-paint bake.
   * Inert on forms without a host store. Added by the R6 ledger burn.
   */
  persistSurfaceBackground(hex: string, dark: boolean): void;
}

/** The inert, web-shaped binding: extras are empty and every write is a no-op. */
export function inertHostSettingsPort(): HostSettingsPort {
  return {
    async readHostExtras() {
      return {};
    },
    async writeHostSettings() {
      // The kernel save is the whole write on a form factor without a host.
    },
    async rebuildMenu() {
      // No native menu outside the desktop host.
    },
    async getLogPath() {
      return '';
    },
    async getAppExecutablePath() {
      return '';
    },
    async openPath() {
      // No host opener.
    },
    async openLogDir() {
      // No host directories.
    },
    async openWorkflowsDir() {
      // No host directories.
    },
    async openContextDir() {
      // No host directories.
    },
    checkUpdatesOnStartup() {
      // No updater outside the desktop host.
    },
    async getAppVersion() {
      // A form without a host app has no bundle version; callers gate on
      // `capabilities.versionSource` and keep their own fallback.
      throw new Error('getAppVersion: no host app on this form factor.');
    },
    persistSurfaceBackground() {
      // No host file to bake into the next window's first paint.
    },
  };
}

// ── Port 6: tables exporter (form-split F4 dual-flow demonstration) ──────────

/**
 * The shared export signature (plan §3.3): `exportTables(request)` — the
 * desktop implementation streams through the save-dialog token writer (path
 * never returns to JS); a web implementation assembles the bytes and Blob-
 * downloads them. Both answer the same signature, so the export UI never
 * learns which form factor it is in.
 */
export interface TablesExporterPort {
  exportTables(request: TablesExportRequest): Promise<TablesExportResult | null>;
  /**
   * Writer progress while streaming (row counts per table). Optional: a form
   * without a streaming writer simply has no progress to report.
   */
  onExportProgress?(handler: (event: TablesExportProgressEvent) => void): Promise<() => void>;
}

// ── Capability table (static, compile-time; full rollout in F5) ──────────────

export interface PlatformCapabilities {
  /** Sections the settings page may show; replaces `settingsSectionsForBackend(boolean)`. */
  readonly settingsSections: ReadonlySet<SettingsSection>;
  readonly updater: boolean;
  readonly versionSource: 'host-app' | 'build-constant' | 'http';
}

// ── Injection (bootstrap, registry style — no constructor threading) ─────────

export interface PlatformBindings {
  services: PlatformServices;
  windows: WindowRouter;
  events: EventChannel;
  /** The host half of the settings surface (form-split F4). The desktop
   *  assembly binds the projecting `DesktopSettingsClient`; a binding without
   *  one (tests, a web build until F5's assembly) resolves the inert port via
   *  `requireHostSettings()`. */
  hostSettings?: HostSettingsPort;
  /** The dual-flow export demonstration port (form-split F4, plan §3.3). */
  tablesExporter?: TablesExporterPort;
  /** F5 makes this required; until then a binding without it runs with
   *  `capabilitiesUnavailable()` semantics. */
  capabilities?: PlatformCapabilities;
}

let bindings: PlatformBindings | null = null;

/**
 * Install the platform ports. Called by the app assembly root before the
 * first React render. A second call replaces the binding (tests rebind).
 */
export function configurePlatform(next: PlatformBindings): void {
  bindings = next;
}

/** Drop the binding. Exists so tests can exercise the unconfigured path. */
export function clearPlatform(): void {
  bindings = null;
}

export function isPlatformConfigured(): boolean {
  return bindings !== null;
}

/**
 * Resolve the platform bindings or fail loudly (§7.4 semantics): an
 * unconfigured call must throw a locatable error, never silently degrade to a
 * direct host connection.
 */
export function requirePlatform(): PlatformBindings {
  if (!bindings) {
    throw new Error(
      'Platform ports have not been configured; check that the app assembly root ran configurePlatform().',
    );
  }
  return bindings;
}

/** Null-tolerant accessor for call sites that legitimately degrade by form. */
export function getPlatformOrNull(): PlatformBindings | null {
  return bindings;
}

/**
 * The export port, or an explicit failure when no form bound one.
 *
 * Export is a user-triggered action, not an ambient read: an unbound exporter
 * is a wiring gap worth failing loudly (§7.4 semantics) rather than degrade
 * into a silently missing download.
 */
export function requireTablesExporter(): TablesExporterPort {
  const exporter = bindings?.tablesExporter;
  if (!exporter) {
    throw new Error(
      'No tables exporter has been bound; check that the app assembly root ran configurePlatform().',
    );
  }
  return exporter;
}

/**
 * The host settings port, or the inert default when this binding predates F4.
 *
 * Deliberately NOT a throw: `settingsCommands` calls this unconditionally, and
 * a binding assembled before F4 (or a unit test that never touches settings)
 * must keep working with the web-shaped behavior rather than failing loud at
 * a distance from the missing wiring. Assemblies that care bind it explicitly.
 */
export function requireHostSettings(): HostSettingsPort {
  return bindings?.hostSettings ?? inertHostSettingsPort();
}

/**
 * Capability lookup. Throws while F5 has not landed the static table yet;
 * callers that must degrade before F5 should use `getPlatformOrNull()`.
 */
export function requireCapabilities(): PlatformCapabilities {
  const caps = requirePlatform().capabilities;
  if (!caps) {
    throw new Error(
      'Platform capabilities have not been provided yet (form-split F5); the consuming call site must not be reachable in this build.',
    );
  }
  return caps;
}
