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
 * Pure-desktop host services: the legacy settings pair, menu rebuild and the
 * host directory openers. This *type* lives in contracts so the assembly root
 * can reference it, but the implementation is only exported from the desktop
 * zone — a web build has no name for it at all (ruling #7).
 */
export interface DesktopHostServices {
  getHostSettings(): Promise<AppSettings>;
  saveHostSettings(s: AppSettings): Promise<void>;
  rebuildMenu(language: string): Promise<void>;
  getLogPath(): Promise<string>;
  openLogDir(): Promise<void>;
  openWorkflowsDir(): Promise<void>;
  openContextDir(): Promise<void>;
  checkUpdatesOnStartup(enabled: boolean): void;
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
