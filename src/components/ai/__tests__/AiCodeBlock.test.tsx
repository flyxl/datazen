import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { render, fireEvent, cleanup } from '@testing-library/react';
import { AiCodeBlock } from '../AiCodeBlock';

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

vi.mock('../../SqlCodeBlock', () => ({
  SqlCodeBlock: ({ code }: { code: string }) => <div data-testid="sql-code-block">{code}</div>,
}));

afterEach(cleanup);

beforeEach(() => {
  vi.clearAllMocks();
  Object.defineProperty(navigator, 'clipboard', {
    configurable: true,
    value: { writeText: vi.fn().mockResolvedValue(undefined) },
  });
});

describe('AiCodeBlock', () => {
  it('renders SQL block with language label and syntax block', () => {
    const { getByTestId, getByText } = render(<AiCodeBlock language="sql" code="SELECT 1" />);
    expect(getByTestId('ai-code-block')).toBeInTheDocument();
    expect(getByText('sql')).toBeInTheDocument();
    expect(getByTestId('sql-code-block')).toHaveTextContent('SELECT 1');
  });

  it('shows insert button for SQL and calls onInsertSql', () => {
    const onInsertSql = vi.fn();
    const { getByTestId } = render(
      <AiCodeBlock language="sql" code="SELECT 1" onInsertSql={onInsertSql} />,
    );
    fireEvent.click(getByTestId('ai-code-insert'));
    expect(onInsertSql).toHaveBeenCalledWith('SELECT 1');
  });

  it('hides insert button for non-SQL blocks', () => {
    const onInsertSql = vi.fn();
    const { queryByTestId, getByText } = render(
      <AiCodeBlock language="json" code='{"a":1}' onInsertSql={onInsertSql} />,
    );
    expect(queryByTestId('ai-code-insert')).toBeNull();
    expect(getByText('{"a":1}')).toBeInTheDocument();
  });

  it('copies code to clipboard', () => {
    const { getByTestId } = render(<AiCodeBlock language="sql" code="SELECT 2" />);
    fireEvent.click(getByTestId('ai-code-copy'));
    expect(navigator.clipboard.writeText).toHaveBeenCalledWith('SELECT 2');
  });

  it('hides actions while streaming', () => {
    const { queryByTestId } = render(
      <AiCodeBlock language="sql" code="SELECT 1" isStreaming onInsertSql={vi.fn()} />,
    );
    expect(queryByTestId('ai-code-copy')).toBeNull();
    expect(queryByTestId('ai-code-insert')).toBeNull();
  });

  it('shows run button for SQL when onRunCode provided', () => {
    const onRunCode = vi.fn();
    const { getByTestId } = render(
      <AiCodeBlock language="sql" code="SELECT 1" onRunCode={onRunCode} />,
    );
    fireEvent.click(getByTestId('ai-code-run'));
    expect(onRunCode).toHaveBeenCalledWith('SELECT 1', 'sql');
  });

  it('hides run button for non-SQL blocks', () => {
    const { queryByTestId } = render(
      <AiCodeBlock language="python" code="print('hi')" onRunCode={vi.fn()} />,
    );
    expect(queryByTestId('ai-code-run')).toBeNull();
  });

  it('shows new query button when onNewQuery provided', () => {
    const onNewQuery = vi.fn();
    const { getByTestId } = render(
      <AiCodeBlock language="sql" code="SELECT 1" onNewQuery={onNewQuery} />,
    );
    fireEvent.click(getByTestId('ai-code-new-query'));
    expect(onNewQuery).toHaveBeenCalledWith('SELECT 1');
  });

  it('shows dialect selector for SQL blocks', () => {
    const { getByTestId } = render(<AiCodeBlock language="sql" code="SELECT 1" />);
    expect(getByTestId('ai-code-dialect-select')).toBeInTheDocument();
  });

  it('hides dialect selector for non-SQL blocks', () => {
    const { queryByTestId } = render(<AiCodeBlock language="json" code="{}" />);
    expect(queryByTestId('ai-code-dialect-select')).toBeNull();
  });

  it('enters fullscreen mode on toggle', () => {
    const { getByTestId } = render(<AiCodeBlock language="sql" code="SELECT 1" />);
    fireEvent.click(getByTestId('ai-code-fullscreen-toggle'));
    expect(getByTestId('ai-code-fullscreen')).toBeInTheDocument();
  });

  it('exits fullscreen on second toggle', () => {
    const { getByTestId, queryByTestId } = render(<AiCodeBlock language="sql" code="SELECT 1" />);
    // Enter fullscreen
    fireEvent.click(getByTestId('ai-code-fullscreen-toggle'));
    expect(getByTestId('ai-code-fullscreen')).toBeInTheDocument();
    // Exit fullscreen
    fireEvent.click(getByTestId('ai-code-fullscreen-toggle'));
    expect(queryByTestId('ai-code-fullscreen')).toBeNull();
  });

  it('fullscreen shows copy button', () => {
    const { getByTestId } = render(<AiCodeBlock language="sql" code="SELECT 1" />);
    fireEvent.click(getByTestId('ai-code-fullscreen-toggle'));
    // The fullscreen copy button uses the same testid
    fireEvent.click(getByTestId('ai-code-copy'));
    expect(navigator.clipboard.writeText).toHaveBeenCalledWith('SELECT 1');
  });
});
