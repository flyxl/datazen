/**
 * The two binding interfaces the host must satisfy, and the registry that
 * hands them out.
 *
 * `BackendTransport` and `PlatformServices` are reproduced from
 * `docs/architecture/platform/shared-boundaries-and-ports.md` §7.1. They
 * declare *method names and payload shapes only* — no transport module
 * specifier, no network primitive literal (architecture guard F-07 scans this
 * whole package and fails the build if one appears). An adapter supplies the
 * mechanism; nothing here knows how any backend is reached.
 *
 * The distinction the two interfaces encode is deliberate and not
 * interchangeable: `BackendTransport` reaches the *kernel* (connections,
 * sessions, executions, jobs, artifacts); `PlatformServices` reaches the *host*
 * (native file dialogs, clipboard). Only a desktop adapter can implement the
 * latter, and a browser build is expected to bind a narrower one.
 */

import type { UnsupportedMethodKind } from './errors';
import type { SettingsMethodMap } from './settingsClient';
import type { IdentityMethodMap } from './identityClient';
import type { ConnectionMethodMap } from './connectionClient';
import type { SessionMethodMap } from './sessionClient';
import type { ExecutionMethodMap } from './executionClient';
import type { JobMethodMap } from './jobClient';
import type { ArtifactMethodMap } from './artifactClient';
import type { TableDataMethodMap } from './tableDataClient';
import type { DriverGatewayMethodMap } from './driverGatewayClient';
import type { LegacyHostMethodMap } from './legacyHostCommands';
import type { ConnectionEvent, EventEnvelope, SubscribeEventsRequest } from './types';

/** Legacy gateway envelope. Kept because the `{ data }` unwrap is a public contract. */
export interface CommandResult {
  data: unknown;
}

/**
 * Every method a `BackendTransport` may be asked to perform, with its request
 * and response types.
 *
 * Form-split F3: the map is now the **intersection of the domain maps** — each
 * domain client file declares its own slice (`SettingsMethodMap`,
 * `SessionMethodMap`, ...) and this alias composes them. The composition is
 * itself the compile-time key-consistency assertion: a key added to a domain
 * slice appears in `MethodMap` by construction, a key removed disappears the
 * same way, and two domains can never silently declare the same key with
 * different shapes (the intersection would surface it at the first use site).
 *
 * Two groups, and the difference matters:
 *
 * - **Kernel surface** — keys are exactly the method names of `BackendClient`
 *   (`system-overview.md` §6.2). These are the platform's real API.
 * - **Legacy driver-command gateway** — keys are the *existing* desktop IPC
 *   command names (`execute_driver_command`, ...). They exist today because
 *   `packages/driver-sdk` predates the kernel and calls them directly; they are
 *   here only so the SDK can be migrated without changing a single call site,
 *   and they go away when the kernel implements those methods.
 */
export type MethodMap = SettingsMethodMap &
  IdentityMethodMap &
  ConnectionMethodMap &
  SessionMethodMap &
  ExecutionMethodMap &
  JobMethodMap &
  ArtifactMethodMap &
  TableDataMethodMap &
  DriverGatewayMethodMap &
  LegacyHostMethodMap;

/**
 * Bound by the host to exactly one backend.
 *
 * `call` is generic over `MethodMap`, so a method name that does not exist
 * cannot be compiled — including a `backendId` smuggled in as a business
 * parameter. The backend is chosen by holding a different instance, never by
 * passing an argument.
 */
export interface BackendTransport {
  call<K extends keyof MethodMap>(
    method: K,
    payload: MethodMap[K]['request'],
  ): Promise<MethodMap[K]['response']>;
  /** Stream transport adapted to AsyncIterable; ending it only cancels the
   * subscription. */
  subscribe?(
    method: 'subscribeEvents',
    payload: SubscribeEventsRequest,
  ): AsyncIterable<EventEnvelope<ConnectionEvent>>;
  cancel?(opaque: string, reason: string): Promise<void>;
  /**
   * The methods this backend cannot serve, and why.
   *
   * A capability that belongs to the *host* and not to the backend — the
   * desktop's persistent store, for instance — does not exist in every form
   * factor. Declaring the gap here is what keeps the port honest: the facade
   * turns a call to one of these methods into an `ApiError` with the §13 code
   * `CapabilityUnsupported` **before** the call leaves the process, instead of
   * letting it travel and come back as an opaque transport failure or — worse
   * — as a plausible-looking empty result.
   *
   * The value says *which kind* of gap it is, because the two call for
   * different reactions from a caller:
   *
   * - `capability` — this form factor has no way to serve the method, now or
   *   later. A browser build has no host store, so it can never serve the
   *   saved views. A caller hides the feature.
   * - `notImplemented` — the backend could serve the method and does not yet.
   *   The desktop has not registered the 07/08/09 commands. A caller may say
   *   "not available yet" instead of hiding the feature.
   *
   * Absent means "every method in `MethodMap` is served". It is a capability
   * statement, not a platform identity: a backend that *can* serve named views
   * says so by leaving the field out, whatever form factor it runs in.
   *
   * Note what this field is **not**: it is not the list of methods a backend
   * happens to lack today. Whether a method is *registered* in `MethodMap` is
   * a question of contract completeness — the port lists every method the
   * product has, so that a method landing later cannot slip past the port
   * unnoticed — while whether a particular backend *serves* one is declared
   * here, by that backend.
   */
  unsupportedMethods?: Partial<Record<keyof MethodMap, UnsupportedMethodKind>>;
}

/**
 * Host capabilities, injected separately from the backend.
 *
 * A `false`/`null` result means the *user cancelled*. That is a normal outcome
 * and resolves rather than rejects, so callers can `if (!saved)` without a
 * try/catch.
 */
export interface PlatformServices {
  saveTextWithDialog(input: SaveTextInput): Promise<boolean>;
  saveBinaryWithDialog(input: SaveBinaryInput): Promise<boolean>;
  openTextWithDialog(input: OpenTextInput): Promise<OpenedTextFile | null>;
  openDirectoryWithDialog(input: OpenDirectoryInput): Promise<OpenedDirectory | null>;
  writeClipboard(text: string): Promise<void>;
  /**
   * Rich-text clipboard write. The desktop host command bypasses browser
   * user-gesture restrictions; the web form degrades to the async clipboard
   * API's `ClipboardItem` when available and to plain text otherwise.
   * Added by the form-split R6 ledger burn (the fetchRelationDdl copy
   * helpers' native fallback leg).
   */
  writeClipboardHtml(input: WriteClipboardHtmlInput): Promise<void>;
  readClipboard(): Promise<string>;
  /**
   * External open. `url` goes to the system browser (web form degrades to
   * `window.open`); a local `path` only has desktop semantics. Added by the
   * form-split plan F2 as the 9th atomic capability (`openPath` / docs URLs /
   * opening host directories), mirroring §7.1.
   */
  openExternal(input: OpenExternalInput): Promise<void>;
  /**
   * Toggle the window's fullscreen state (Menu-bar action). Web form:
   * the document fullscreen API on the root element. Added by the form-split
   * R6 ledger burn (MenuBar's last chrome touchpoint outside the zones).
   */
  toggleFullscreen(): Promise<void>;
  /**
   * System notification (wapp sandbox bridge). Desktop rides the registered
   * notification plugin; web degrades to the `Notification` API. Added by the
   * form-split R6 ledger burn (wappBridge's plugin invoke).
   */
  showNotification(input: ShowNotificationInput): Promise<void>;
  /**
   * Native path dialog (the host's `PathPicker`). Desktop rides the dialog
   * plugin; the web form has no native picker and resolves `null` (cancel
   * semantics — callers disable the browse affordance by capability, not by
   * probing). Added by the form-split F6 prep (lib/pathPicker shim burn).
   */
  pickPath(input?: PickPathInput): Promise<string | null>;
  /**
   * The OS platform dimension (macos/windows/linux), orthogonal to the form
   * factor — both forms need it. Desktop probes plugin-os; the web form
   * sniffs the user agent. Added by the form-split F6 prep (hooks/usePlatform
   * shim burn).
   */
  getOsPlatform(): Promise<OsPlatform>;
}

/** The OS dimension, shared by both form factors. */
export type OsPlatform = 'macos' | 'windows' | 'linux' | 'unknown';

/** Dialog filter shape for {@link PlatformServices.pickPath}. */
export interface PickPathFileFilter {
  name: string;
  extensions: string[];
}

/**
 * Options for the native path dialog. Structurally identical to
 * `@datazen/ui`'s `PathPickerDialogOptions` so the host binding satisfies the
 * `PathPicker` signature without the design system depending on this package.
 */
export interface PickPathInput {
  directory?: boolean;
  multiple?: boolean;
  title?: string;
  defaultPath?: string;
  filters?: PickPathFileFilter[];
}

export interface WriteClipboardHtmlInput {
  html: string;
  /** Plain-text fallback the same clipboard entry carries. */
  altText: string;
}

export interface ShowNotificationInput {
  title: string;
  body?: string;
}

export interface OpenExternalInput {
  url?: string;
  path?: string;
}

export interface SaveTextInput {
  contents: string;
  defaultFileName: string;
  filterName: string;
  extensions: readonly string[];
}

export interface SaveBinaryInput {
  dataBase64: string;
  defaultFileName: string;
  filterName: string;
  extensions: readonly string[];
}

export interface OpenTextInput {
  filterName: string;
  extensions: readonly string[];
}

export interface OpenDirectoryInput {
  title?: string;
}

/** Result of a text open. Never carries a host path. */
export interface OpenedTextFile {
  fileName: string;
  content: string;
}

/**
 * The one place a host path legitimately exists.
 *
 * `PlatformServices` is the only boundary that may surface a path, because it
 * is the only one that already had one; every artifact read stays path-free by
 * design.
 */
export interface OpenedDirectory {
  path: string;
}

let boundPlatformServices: PlatformServices | null = null;

/**
 * Install host capabilities. Called by the app entry before first paint.
 *
 * Re-binding replaces the previous instance, which is what the desktop adapter
 * needs when it rebuilds itself across a backend switch.
 */
export function setPlatformServices(services: PlatformServices): void {
  boundPlatformServices = services;
}

/** Drop the binding. Exists so tests can exercise the unbound path. */
export function clearPlatformServices(): void {
  boundPlatformServices = null;
}

/** Whether host capabilities are currently bound. */
export function isPlatformServicesBound(): boolean {
  return boundPlatformServices !== null;
}

/**
 * Resolve the bound capabilities, or fail loudly.
 *
 * §7.4 is explicit: an unbound call must throw a locatable error and must
 * never silently fall back to a direct desktop connection. The message names
 * the missing binding, so the stack trace alone points at the adapter that
 * was supposed to run.
 */
export function requirePlatformServices(): PlatformServices {
  if (!boundPlatformServices) {
    throw new Error('PlatformServices has not been bound; check that the platform adapter ran.');
  }
  return boundPlatformServices;
}
