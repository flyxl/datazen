import { describe, expect, it } from 'vitest';
import { buildDocsUrl, DOCS_BASE_EN, DOCS_BASE_ZH } from '../docsUrls';

describe('buildDocsUrl', () => {
  it('uses English base for non-Chinese languages', () => {
    expect(buildDocsUrl('en')).toBe(DOCS_BASE_EN);
    expect(buildDocsUrl('de', 'ai')).toBe(`${DOCS_BASE_EN}#ai`);
  });

  it('uses Chinese base for zh-CN and zh-TW', () => {
    expect(buildDocsUrl('zh-CN')).toBe(DOCS_BASE_ZH);
    expect(buildDocsUrl('zh-TW', 'workflows')).toBe(`${DOCS_BASE_ZH}#workflows`);
  });

  it('ignores unknown section ids', () => {
    expect(buildDocsUrl('en', 'getting-started')).toBe(DOCS_BASE_EN);
    expect(buildDocsUrl('en', '  ')).toBe(DOCS_BASE_EN);
  });

  it('maps all documented section anchors', () => {
    for (const id of [
      'overview',
      'features',
      'ai',
      'context',
      'workflows',
      'opsDashboard',
      'schemaDiff',
    ] as const) {
      expect(buildDocsUrl('en', id)).toBe(`${DOCS_BASE_EN}#${id}`);
      expect(buildDocsUrl('zh-CN', id)).toBe(`${DOCS_BASE_ZH}#${id}`);
    }
  });
});
