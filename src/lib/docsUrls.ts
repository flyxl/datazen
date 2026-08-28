/**
 * Official GitHub Pages help docs (F6 — in-app help opens the User Manual).
 * The former "docs.html" deep-dive page was removed; the manual is the
 * single documentation destination now.
 */
export const DOCS_BASE_EN = 'https://flyxl.github.io/datazen/manual.html';
export const DOCS_BASE_ZH = 'https://flyxl.github.io/datazen/zh/manual.html';

/**
 * Legacy section ids (from the removed docs.html) mapped to their closest
 * manual.html anchors, so existing deep links keep landing somewhere useful.
 */
const SECTION_REMAP: Record<string, string> = {
  overview: 'ui',
  features: 'charts',
  ai: 'ai',
  context: 'ai',
  workflows: 'workflow',
  opsDashboard: 'dashboard',
  dataSync: 'data-sync',
  dataTransfer: 'data-transfer',
  schemaDiff: 'schema-diff',
};

export type DocsSectionId = keyof typeof SECTION_REMAP;

export function isChineseUiLanguage(language: string): boolean {
  return language.startsWith('zh');
}

/** Map app settings language + optional section to the official docs URL. */
export function buildDocsUrl(language: string, section?: string): string {
  const base = isChineseUiLanguage(language) ? DOCS_BASE_ZH : DOCS_BASE_EN;
  const id = section?.trim();
  if (id && id in SECTION_REMAP) {
    return `${base}#${SECTION_REMAP[id]}`;
  }
  return base;
}
