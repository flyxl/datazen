/** Summary of an installed runtime theme pack (from manifest.json). */
export interface ThemePackSummary {
  id: string;
  name: string;
  version: string;
  apiVersion: number;
  modes: string[];
  author?: string;
  description?: string;
}

/** Alias for settings UI and theme service. */
export type InstalledThemePack = ThemePackSummary;
