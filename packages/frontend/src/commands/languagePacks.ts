import { legacyInvoke as invoke } from '@datazen/backend-client';

/** Mirrors `LANGUAGE_PACKS_CHANGED_EVENT` in the desktop host commands. */
export const LANGUAGE_PACKS_CHANGED_EVENT = 'language-packs:changed';

/** A manifest.json inside a language pack ZIP (wire shape, camelCase). */
export interface LanguagePackManifest {
  packVersion: number;
  code: string;
  label: string;
  version: string;
  author?: string | null;
  description?: string | null;
}

/** One installed pack as listed by `list_language_packs`. */
export interface LanguagePackSummary {
  code: string;
  label: string;
  version: string;
  author?: string | null;
  description?: string | null;
  /** Total translated messages across all domain files. */
  messageCount: number;
  /** Install directory name (always equals `code`). */
  fileName: string;
}

/** Per-domain message counts shown in the install preview. */
export interface LanguagePackDomainStat {
  domain: string;
  messageCount: number;
}

/** Preview returned after a native pick + validate-only inspect (no install). */
export interface LanguagePackPreview {
  pickToken: string;
  packageLabel: string;
  manifest: LanguagePackManifest;
  messageCount: number;
  domains: LanguagePackDomainStat[];
}

/**
 * `read_language_pack_messages` response: `label` + full `messages` map —
 * everything `registerLocale` needs to make the locale selectable, in one IPC.
 */
export interface LanguagePackMessages {
  code: string;
  label: string;
  messages: Record<string, string>;
}

export const languagePackCommands = {
  listLanguagePacks: () => invoke<LanguagePackSummary[]>('list_language_packs'),

  readLanguagePack: (code: string) =>
    invoke<LanguagePackMessages>('read_language_pack_messages', { code }),

  /**
   * Native picker + validate-only inspect. Returns `null` when cancelled.
   * The wire-level `overridePath` escape hatch is webdriver/E2E-only.
   */
  inspectLanguagePackFileWithDialog: (overridePath?: string) =>
    invoke<LanguagePackPreview | null>('inspect_language_pack_file_with_dialog', {
      overridePath,
    }),

  /**
   * Install a pack picked via {@link inspectLanguagePackFileWithDialog}.
   * The wire-level `overridePath` escape hatch is webdriver/E2E-only.
   */
  installLanguagePack: (pickToken?: string, overridePath?: string) =>
    invoke<LanguagePackSummary>('install_language_pack', { pickToken, overridePath }),

  removeLanguagePack: (code: string) => invoke<void>('remove_language_pack', { code }),
};
