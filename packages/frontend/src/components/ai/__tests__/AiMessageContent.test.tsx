import { describe, expect, it, vi, afterEach } from 'vitest';
import { render, cleanup } from '@testing-library/react';
import { AiMessageContent } from '../AiMessageContent';

afterEach(cleanup);

vi.mock('../../SqlCodeBlock', () => ({
  SqlCodeBlock: ({ code }: { code: string }) => <div data-testid="sql-code-block">{code}</div>,
}));

describe('AiMessageContent', () => {
  it('renders plain text via markdown', () => {
    const { container } = render(<AiMessageContent content="Hello world" />);
    expect(container.textContent).toContain('Hello world');
  });

  it('renders markdown headings', () => {
    const { container } = render(<AiMessageContent content="# Title" />);
    expect(container.querySelector('h1')).toBeTruthy();
    expect(container.textContent).toContain('Title');
  });

  it('renders markdown lists', () => {
    const { container } = render(<AiMessageContent content="- Item 1\n- Item 2" />);
    expect(container.querySelector('ul')).toBeTruthy();
  });

  it('renders markdown bold and italic', () => {
    const { container } = render(<AiMessageContent content="**bold** and *italic*" />);
    expect(container.querySelector('strong')).toBeTruthy();
    expect(container.querySelector('em')).toBeTruthy();
  });

  it('renders fenced code block as AiCodeBlock', () => {
    const { getByTestId, queryByText } = render(
      <AiMessageContent content={'Before\n```sql\nSELECT 1\n```\nAfter'} onInsertSql={vi.fn()} />,
    );
    expect(getByTestId('ai-code-block')).toBeInTheDocument();
    expect(queryByText('```sql')).toBeNull();
  });

  it('renders text around code blocks', () => {
    const { getByText } = render(
      <AiMessageContent content={'Before\n```sql\nSELECT 1\n```\nAfter'} />,
    );
    expect(getByText('Before')).toBeInTheDocument();
    expect(getByText('After')).toBeInTheDocument();
  });

  it('applies animate-pulse when streaming', () => {
    const { container } = render(<AiMessageContent content="Hello" isStreaming />);
    expect(container.querySelector('.animate-pulse')).toBeTruthy();
  });

  it('renders multiple code blocks', () => {
    const { getAllByTestId } = render(
      <AiMessageContent content={'```sql\nSELECT 1\n```\n\n```sql\nSELECT 2\n```'} />,
    );
    expect(getAllByTestId('ai-code-block')).toHaveLength(2);
  });

  it('renders empty content as null', () => {
    const { container } = render(<AiMessageContent content="" />);
    expect(container.firstChild).toBeNull();
  });

  it('renders whitespace-only text segments as null', () => {
    const { container } = render(
      <AiMessageContent content={'   \n\n```sql\nSELECT 1\n```\n\n   '} />,
    );
    // The code block should render, whitespace-only text segments should be filtered
    expect(container.querySelector('[data-testid="ai-code-block"]')).toBeTruthy();
  });

  it('[tester] strips script tags from markdown output (XSS)', () => {
    const { container } = render(
      <AiMessageContent content='Hello <script>alert("xss")</script> world' />,
    );
    expect(container.innerHTML).not.toContain('<script');
    expect(container.textContent).toContain('Hello');
    expect(container.textContent).toContain('world');
  });

  it('[tester] strips iframe tags from markdown output (XSS)', () => {
    const { container } = render(
      <AiMessageContent content='Before <iframe src="evil.com"></iframe> After' />,
    );
    expect(container.innerHTML).not.toContain('<iframe');
    expect(container.textContent).toContain('Before');
    expect(container.textContent).toContain('After');
  });

  it('[tester] neuters javascript: href links (XSS)', () => {
    const { container } = render(<AiMessageContent content="[Click me](javascript:alert(1))" />);
    expect(container.innerHTML).not.toContain('javascript:');
    expect(container.textContent).toContain('Click me');
  });

  it('[tester] strips onclick event handlers (XSS)', () => {
    const { container } = render(
      <AiMessageContent content='<p onclick="alert(1)">Safe text</p>' />,
    );
    expect(container.innerHTML).not.toContain('onclick');
    expect(container.textContent).toContain('Safe text');
  });

  it('[tester] passes onRunCode and onNewQuery through to AiCodeBlock', () => {
    const onRunCode = vi.fn();
    const onNewQuery = vi.fn();
    const { getByTestId } = render(
      <AiMessageContent
        content={'```sql\nSELECT 1\n```'}
        onRunCode={onRunCode}
        onNewQuery={onNewQuery}
      />,
    );
    expect(getByTestId('ai-code-block')).toBeInTheDocument();
    // The props are passed to AiCodeBlock which renders the buttons
  });

  it('[tester] handles content with only code blocks (no text)', () => {
    const { queryByTestId, container } = render(
      <AiMessageContent content={'```sql\nSELECT 1\n```'} />,
    );
    expect(queryByTestId('ai-code-block')).toBeInTheDocument();
    // No text segments should be rendered
    expect(container.querySelector('.ai-markdown')).toBeNull();
  });

  it('[tester] handles mixed content with multiple code and text segments', () => {
    const content =
      'First paragraph\n\n```sql\nSELECT 1\n```\n\nSecond paragraph\n\n```python\nprint("hi")\n```\n\nThird paragraph';
    const { getAllByTestId, getByText } = render(<AiMessageContent content={content} />);
    expect(getAllByTestId('ai-code-block')).toHaveLength(2);
    expect(getByText('First paragraph')).toBeInTheDocument();
    expect(getByText('Second paragraph')).toBeInTheDocument();
    expect(getByText('Third paragraph')).toBeInTheDocument();
  });

  it('[tester] renders GFM tables', () => {
    const { container } = render(
      <AiMessageContent content={'| Name | Age |\n|------|-----|\n| Alice | 30 |\n| Bob | 25 |'} />,
    );
    expect(container.querySelector('table')).toBeTruthy();
  });

  it('[tester] renders blockquotes', () => {
    const { container } = render(<AiMessageContent content={'> This is a quote'} />);
    expect(container.querySelector('blockquote')).toBeTruthy();
  });

  it('[tester] renders inline code', () => {
    const { container } = render(<AiMessageContent content="Use `SELECT *` to get all columns" />);
    expect(container.querySelector('code')).toBeTruthy();
  });
});
