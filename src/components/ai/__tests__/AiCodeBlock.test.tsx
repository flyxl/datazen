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
});
