/** @vitest-environment node */
import { describe, expect, it } from 'vitest';
import {
  githubSlug,
  renderWorkflowMarkdown,
  stripLeadingH1,
} from './renderMarkdown';
import workflowGuideZh from '../../../docs/workflow-guide.md?raw';
import workflowGuideEn from '../../../docs/workflow-guide.en.md?raw';
import opsDashboardGuideZh from '../../../docs/ops-dashboard-guide.md?raw';
import opsDashboardGuideEn from '../../../docs/ops-dashboard-guide.en.md?raw';
import schemaDiffGuideZh from '../../../docs/schema-diff-guide.md?raw';
import schemaDiffGuideEn from '../../../docs/schema-diff-guide.en.md?raw';

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

  it('renders Chinese ops dashboard guide with anchors and tables', () => {
    const html = renderWorkflowMarkdown(opsDashboardGuideZh);
    expect(html).toContain('id="3-快速入门"');
    expect(html).toContain('<table');
    expect(html).toContain('MonitorEngine');
    expect(html).not.toMatch(/^<h1/);
  });

  it('renders English ops dashboard guide', () => {
    const html = renderWorkflowMarkdown(opsDashboardGuideEn);
    expect(html).toContain('id="3-quick-start"');
    expect(html).toContain('configId');
  });

  it('renders Chinese schema diff guide with anchors and tables', () => {
    const html = renderWorkflowMarkdown(schemaDiffGuideZh);
    expect(html).toContain('id="2-快速入门"');
    expect(html).toContain('<table');
    expect(html).toContain('DEPLOY');
    expect(html).not.toMatch(/^<h1/);
  });

  it('renders English schema diff guide', () => {
    const html = renderWorkflowMarkdown(schemaDiffGuideEn);
    expect(html).toContain('id="2-quick-start"');
    expect(html).toContain('additive-only');
  });
});
