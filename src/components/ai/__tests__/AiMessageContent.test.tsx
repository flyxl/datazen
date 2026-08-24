import { describe, expect, it, vi } from 'vitest';
import { render } from '@testing-library/react';
import { AiMessageContent } from '../AiMessageContent';

vi.mock('../../SqlCodeBlock', () => ({
  SqlCodeBlock: ({ code }: { code: string }) => <div data-testid="sql-code-block">{code}</div>,
}));

describe('AiMessageContent', () => {
  it('renders plain text without code block wrapper', () => {
    const { getByText, queryByTestId } = render(<AiMessageContent content="Hello world" />);
    expect(getByText('Hello world')).toBeInTheDocument();
    expect(queryByTestId('ai-code-block')).toBeNull();
  });

  it('renders text and inline code block segments', () => {
    const { getByText, getByTestId, queryByText } = render(
      <AiMessageContent content={'Before\n```sql\nSELECT 1\n```\nAfter'} onInsertSql={vi.fn()} />,
    );
    expect(getByText('Before')).toBeInTheDocument();
    expect(getByText('After')).toBeInTheDocument();
    expect(queryByText('```sql')).toBeNull();
    expect(getByTestId('ai-code-block')).toBeInTheDocument();
    expect(getByTestId('sql-code-block')).toHaveTextContent('SELECT 1');
  });
});
