/** Official GitHub Pages help docs (F6 — in-app docs open here). */
export const DOCS_BASE_EN = 'https://flyxl.github.io/datazen/docs.html';
export const DOCS_BASE_ZH = 'https://flyxl.github.io/datazen/zh/docs.html';

export type DocsSectionId =
  | 'overview'
  | 'features'
  | 'ai'
  | 'context'
  | 'workflows'
  | 'opsDashboard'
  | 'schemaDiff';

const VALID_SECTIONS = new Set<DocsSectionId>([
  'overview',
  'features',
  'ai',
  'context',
  'workflows',
  'opsDashboard',
  'schemaDiff',
]);

export function isChineseUiLanguage(language: string): boolean {
  return language.startsWith('zh');
}

/** Map app settings language + optional section to the official docs URL. */
export function buildDocsUrl(language: string, section?: string): string {
  const base = isChineseUiLanguage(language) ? DOCS_BASE_ZH : DOCS_BASE_EN;
  const id = section?.trim();
  if (id && VALID_SECTIONS.has(id as DocsSectionId)) {
    return `${base}#${id}`;
  }
  return base;
}
