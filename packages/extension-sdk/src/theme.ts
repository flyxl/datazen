/**
 * Theme token application for UI plugin pages (PRD §4.4 / §5).
 *
 * React-free core: consumes `theme.apply` snapshots pushed by the host,
 * writes the contract tokens (`--c-*` / `--dt-*`) onto
 * `document.documentElement`, toggles the `dark` class and re-dispatches the
 * same DOM CustomEvent host pages already use. React components can read the
 * identical state through the optional `useTheme` hook (`./react`, subpath
 * export) — plain-JS pages use {@link subscribeTheme} instead.
 */
import { BRIDGE_CHANNEL } from './bridge';

/** Wire shape of `theme.apply` payloads and the `host.ready` token snapshot. */
export interface ThemeSnapshot {
  /** Protocol version of this snapshot shape (host: THEME_SNAPSHOT_VERSION). */
  v: number;
  dark: boolean;
  /** CSS custom property name → resolved value, e.g. `"--c-accent": "#6366f1"`. */
  tokens: Record<string, string>;
}

/** State exposed to subscribers and the `useTheme` hook. */
export type ThemeState = ThemeSnapshot;

/** DOM CustomEvent fired on `document.documentElement` after every applied snapshot. */
export const THEME_CHANGED_EVENT = 'datazen:theme-pack-changed';

/**
 * Fallback palette merged under every incoming snapshot (PRD R5): a host or
 * snapshot that omits contract tokens still leaves plugin widgets styled.
 */
export const DEFAULT_THEME_TOKENS: Record<string, string> = {
  '--c-surface': '#18181b',
  '--c-surface-alt': '#1f1f23',
  '--c-surface-raised': '#27272a',
  '--c-surface-inset': '#101012',
  '--c-edge': '#3f3f46',
  '--c-fg': '#fafafa',
  '--c-fg-secondary': '#d4d4d8',
  '--c-fg-muted': '#a1a1aa',
  '--c-accent': '#6366f1',
  '--c-success': '#22c55e',
  '--c-warning': '#eab308',
  '--c-danger': '#ef4444',
  '--c-query-run': '#38bdf8',
  '--c-titlebar': '#18181b',
  '--c-titlebar-fg': '#fafafa',
  '--c-titlebar-fg-muted': '#a1a1aa',
  '--c-titlebar-hover': '#27272a',
  '--dt-null': '#71717a',
  '--dt-bool': '#fb923c',
  '--dt-number': '#38bdf8',
  '--dt-datetime': '#a78bfa',
  '--dt-json': '#facc15',
  '--dt-text': '#e4e4e7',
  '--dt-binary': '#fb7185',
};

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

/** Merge a wire snapshot over the built-in fallbacks; drop malformed entries. */
function normalize(snapshot: Partial<ThemeSnapshot> | null | undefined): ThemeState {
  const tokens: Record<string, string> = {};
  if (isRecord(snapshot?.tokens)) {
    for (const [name, value] of Object.entries(snapshot.tokens)) {
      if (name.startsWith('--') && typeof value === 'string' && value !== '') {
        tokens[name] = value;
      }
    }
  }
  return {
    v: typeof snapshot?.v === 'number' && Number.isFinite(snapshot.v) ? snapshot.v : 0,
    dark: snapshot?.dark === true,
    tokens: { ...DEFAULT_THEME_TOKENS, ...tokens },
  };
}

let current: ThemeState = { v: 0, dark: false, tokens: { ...DEFAULT_THEME_TOKENS } };

const listeners = new Set<(state: ThemeState) => void>();

/** Latest applied theme state (defaults before the first snapshot arrives). */
export function getThemeState(): ThemeState {
  return current;
}

/** Observe every applied snapshot. Returns an unsubscribe function. */
export function subscribeTheme(listener: (state: ThemeState) => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

function notifyListeners(): void {
  // Copy: a listener may subscribe/unsubscribe re-entrantly while we iterate.
  for (const listener of [...listeners]) {
    try {
      listener(current);
    } catch {
      // A broken subscriber must not break theming or other subscribers.
    }
  }
}

/**
 * Apply a host snapshot to the document root: write every token as an inline
 * CSS custom property on `document.documentElement`, toggle the `dark` class,
 * dispatch `datazen:theme-pack-changed` on the root element and notify
 * subscribers. Applying the same snapshot repeatedly is idempotent at the DOM
 * level (same properties, same class); subscribers are notified each time so
 * re-pushes stay observable.
 *
 * Writes go to `element.style` rather than reading back computed styles —
 * the iframe body stays transparent and inherits only what plugins consume.
 */
export function applyThemeSnapshot(
  snapshot: Partial<ThemeSnapshot> | null | undefined,
): ThemeState {
  const next = normalize(snapshot);
  current = next;
  if (typeof document !== 'undefined') {
    const root = document.documentElement;
    for (const [name, value] of Object.entries(next.tokens)) {
      root.style.setProperty(name, value);
    }
    root.classList.toggle('dark', next.dark);
    root.dispatchEvent(new CustomEvent<ThemeState>(THEME_CHANGED_EVENT, { detail: next }));
  }
  notifyListeners();
  return next;
}

export interface StartThemeListenerOptions {
  /** Override for tests; defaults to `window.parent` per the sandbox contract. */
  parentWindow?: Window | null;
}

/**
 * Listen for host-pushed `theme.apply` envelopes ({@link BRIDGE_CHANNEL}) and
 * apply each payload via {@link applyThemeSnapshot}. Only messages whose
 * `event.source` is the parent window are trusted — anything else could be a
 * spoofing attempt from inside the page. Returns a detach function.
 */
export function startThemeListener(options: StartThemeListenerOptions = {}): () => void {
  const parent =
    options.parentWindow !== undefined
      ? options.parentWindow
      : typeof window === 'undefined'
        ? null
        : window.parent;

  if (typeof window === 'undefined' || !parent) {
    return () => undefined;
  }

  function onMessage(event: MessageEvent): void {
    if (event.source !== parent) return;
    const data: unknown = event.data;
    if (!isRecord(data) || data.ch !== BRIDGE_CHANNEL || data.type !== 'theme.apply') {
      return;
    }
    applyThemeSnapshot(data.payload as Partial<ThemeSnapshot>);
  }

  window.addEventListener('message', onMessage);
  return () => {
    window.removeEventListener('message', onMessage);
  };
}
