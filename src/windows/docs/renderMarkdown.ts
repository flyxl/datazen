/**
 * Render Workflow guide Markdown for the in-app Docs window.
 */
import { marked, Renderer, type Tokens } from 'marked';

/** GitHub-ish slug so in-doc anchors like `#32-最小可运行示例` resolve. */
export function githubSlug(text: string): string {
  return text
    .trim()
    .toLowerCase()
    .replace(/[^\p{L}\p{N}\s-]/gu, '')
    .replace(/\s+/g, '-');
}

const renderer = new Renderer();
renderer.heading = function heading({ tokens, depth, text }: Tokens.Heading): string {
  const inner = this.parser.parseInline(tokens);
  const plain = inner.replace(/<[^>]+>/g, '') || text;
  const id = githubSlug(plain);
  return `<h${depth} id="${id}">${inner}</h${depth}>\n`;
};

marked.use({ gfm: true, breaks: false, renderer });

/** Strip the leading H1 — DocsWindow already shows the section title. */
export function stripLeadingH1(md: string): string {
  return md.replace(/^#\s+[^\n]+\n+/, '');
}

export function renderWorkflowMarkdown(md: string): string {
  return marked.parse(stripLeadingH1(md), { async: false }) as string;
}
