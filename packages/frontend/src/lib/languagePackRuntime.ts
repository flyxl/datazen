import { unregisterTranslations } from '@datazen/ui';
import { languagePackCommands } from '../commands/languagePacks';
import { registerLocale, unregisterLocale } from '../locales';

/**
 * Locale codes that always ride in the app bundle — their dictionaries are
 * compiled in, so "ensuring" them is a no-op.
 */
const BUILTIN_LOCALES: ReadonlySet<string> = new Set(['en', 'zh-CN']);

/** Locales whose pack messages have been registered in this module instance. */
const registered = new Set<string>();

/**
 * Ensure a locale is usable in the shared translation engine before it
 * becomes the active locale: installed pack → read messages → register via
 * the extension-locale channel (`registerLocale`), so the language dropdown
 * and `t()` both pick it up. Builtin locales and unknown codes are a no-op —
 * the engine's fallback chain (`locale ?? en ?? key`) stays authoritative.
 *
 * Called from `startLocaleSync` on boot and on every language change: pack
 * registration must be finished *before* `setLocale`, otherwise the first
 * render in the new language would flash English.
 */
export async function ensureLanguagePackMessages(locale: string): Promise<void> {
  if (!locale || BUILTIN_LOCALES.has(locale) || registered.has(locale)) return;
  let pack;
  try {
    pack = await languagePackCommands.readLanguagePack(locale);
  } catch {
    // Not installed (or read failed) — fall back to the engine's own chain.
    return;
  }
  registerLocale(pack.code, pack.label, pack.messages);
  registered.add(pack.code);
}

/**
 * Unload a pack's dictionaries after uninstall: drop the registry entry so
 * the language dropdown no longer offers it, and remove the translations so
 * a later reinstall cannot inherit stale keys (register merges with
 * `Object.assign` — leftover keys would otherwise survive re-installation).
 */
export function unloadLanguagePack(code: string): void {
  unregisterTranslations(code);
  unregisterLocale(code);
  registered.delete(code);
}

/** Test helper: forget every runtime registration. */
export function __resetLanguagePackRuntimeForTests(): void {
  registered.clear();
}
