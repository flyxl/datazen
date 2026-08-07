/** @vitest-environment node */
import { describe, expect, it } from 'vitest';
import {
  githubSlug,
  renderWorkflowMarkdown,
  stripLeadingH1,
} from './renderMarkdown';
import workflowGuideZh from '../../../docs/workflow-guide.md?raw';
import workflowGuideEn from '../../../docs/workflow-guide.en.md?raw';

describe('renderMarkdown', () => {
  it('slugs headings like GFM (drop punctuation)', () => {
    expect(githubSlug('3.2 最小可运行示例')).toBe('32-最小可运行示例');
    expect(githubSlug('7. Condition expressions')).toBe('7-condition-expressions');
  });

  it('strips leading H1', () => {
    expect(stripLeadingH1('# Title\n\nBody')).toBe('Body');
  });

  it('renders Chinese workflow guide with anchors and tables', () => {
    const html = renderWorkflowMarkdown(workflowGuideZh);
    expect(html).toContain('id="7-条件表达式"');
    expect(html).toContain('<table');
    expect(html).not.toMatch(/^<h1/);
  });

  it('renders English workflow guide', () => {
    const html = renderWorkflowMarkdown(workflowGuideEn);
    expect(html).toContain('id="7-condition-expressions"');
    expect(html).toContain('snake_case');
  });
});
