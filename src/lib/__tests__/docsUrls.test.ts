import { describe, expect, it } from 'vitest';
import { buildDocsUrl, DOCS_BASE_EN, DOCS_BASE_ZH } from '../docsUrls';

describe('buildDocsUrl', () => {
  it('uses English base for non-Chinese languages', () => {
    expect(buildDocsUrl('en')).toBe(DOCS_BASE_EN);
    expect(buildDocsUrl('de', 'ai')).toBe(`${DOCS_BASE_EN}#ai`);
  });

  it('uses Chinese base for zh-CN and zh-TW', () => {
    expect(buildDocsUrl('zh-CN')).toBe(DOCS_BASE_ZH);
    expect(buildDocsUrl('zh-TW', 'workflows')).toBe(`${DOCS_BASE_ZH}#workflow`);
  });

  it('ignores unknown section ids', () => {
    expect(buildDocsUrl('en', 'getting-started')).toBe(DOCS_BASE_EN);
    expect(buildDocsUrl('en', '  ')).toBe(DOCS_BASE_EN);
  });

  it('remaps legacy docs.html sections to manual anchors', () => {
    const cases: Array<[string, string]> = [
      ['overview', 'ui'],
      ['features', 'charts'],
      ['ai', 'ai'],
      ['context', 'ai'],
      ['workflows', 'workflow'],
      ['opsDashboard', 'dashboard'],
      ['schemaDiff', 'sync'],
    ];
    for (const [id, anchor] of cases) {
      expect(buildDocsUrl('en', id)).toBe(`${DOCS_BASE_EN}#${anchor}`);
      expect(buildDocsUrl('zh-CN', id)).toBe(`${DOCS_BASE_ZH}#${anchor}`);
    }
  });
});
