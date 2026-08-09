import { describe, expect, it, vi, afterEach, beforeEach } from 'vitest';
import { render, cleanup, fireEvent, waitFor } from '@testing-library/react';
import { AiInput } from '../AiInput';
import type { ContextItem } from '../../../types';

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

vi.mock('../../../hooks/useAiKeyboard', () => ({
  useAiKeyboard: (onSubmit: () => void) => ({
    onKeyDown: (e: KeyboardEvent) => {
      if (e.key === 'Enter' && !e.shiftKey) {
        e.preventDefault();
        onSubmit();
      }
    },
    onCompositionStart: vi.fn(),
    onCompositionEnd: vi.fn(),
  }),
}));

vi.mock('../../../commands/context', () => ({
  contextCommands: {
    listFiles: vi.fn().mockResolvedValue([]),
  },
}));

afterEach(cleanup);

beforeEach(() => {
  vi.clearAllMocks();
});

describe('AiInput', () => {
  it('renders textarea with placeholder', () => {
    const { container } = render(
      <AiInput
        value=""
        onChange={vi.fn()}
        onSubmit={vi.fn()}
        placeholder="Type here"
      />,
    );
    const textarea = container.querySelector('textarea')!;
    expect(textarea).toBeInTheDocument();
    expect(textarea.placeholder).toBe('Type here');
  });

  it('calls onChange when typing', () => {
    const onChange = vi.fn();
    const { container } = render(
      <AiInput value="" onChange={onChange} onSubmit={vi.fn()} />,
    );
    const textarea = container.querySelector('textarea')!;
    fireEvent.change(textarea, { target: { value: 'hello' } });
    expect(onChange).toHaveBeenCalledWith('hello');
  });

  it('disables send button when value is empty', () => {
    const { container } = render(
      <AiInput value="" onChange={vi.fn()} onSubmit={vi.fn()} />,
    );
    const sendBtn = container.querySelectorAll('button')[0];
    expect(sendBtn).toBeDisabled();
  });

  it('enables send button when value is non-empty', () => {
    const { container } = render(
      <AiInput value="hello" onChange={vi.fn()} onSubmit={vi.fn()} />,
    );
    const sendBtn = container.querySelectorAll('button')[0];
    expect(sendBtn).not.toBeDisabled();
  });

  it('shows stop button when isLoading with onStop', () => {
    const onStop = vi.fn();
    const { container } = render(
      <AiInput
        value=""
        onChange={vi.fn()}
        onSubmit={vi.fn()}
        isLoading={true}
        onStop={onStop}
      />,
    );
    const stopBtn = container.querySelector('button')!;
    fireEvent.click(stopBtn);
    expect(onStop).toHaveBeenCalledTimes(1);
  });

  describe('context items', () => {
    const sampleItems: ContextItem[] = [
      { kind: 'file', id: 'schema.sql', name: 'schema.sql', path: 'schema.sql' },
      { kind: 'table', id: 'users', name: 'users', database: 'mydb' },
    ];

    it('renders inline tokens without remove buttons', () => {
      const { getAllByTestId } = render(
        <AiInput
          value=""
          onChange={vi.fn()}
          onSubmit={vi.fn()}
          contextItems={sampleItems}
          onContextItemsChange={vi.fn()}
        />,
      );
      const tokens = getAllByTestId('context-token');
      expect(tokens).toHaveLength(2);
      expect(tokens[0]).toHaveAttribute('data-kind', 'file');
      expect(tokens[0]).toHaveAttribute('data-id', 'schema.sql');
      expect(tokens[1]).toHaveAttribute('data-kind', 'table');
      expect(tokens[1]).toHaveAttribute('data-id', 'users');
      for (const token of tokens) {
        expect(token.querySelector('button')).toBeNull();
      }
    });

    it('Backspace at start removes last token', () => {
      const onContextItemsChange = vi.fn();
      const { container } = render(
        <AiInput
          value=""
          onChange={vi.fn()}
          onSubmit={vi.fn()}
          contextItems={sampleItems}
          onContextItemsChange={onContextItemsChange}
        />,
      );
      const textarea = container.querySelector('textarea')!;
      Object.defineProperty(textarea, 'selectionStart', { configurable: true, get: () => 0 });
      Object.defineProperty(textarea, 'selectionEnd', { configurable: true, get: () => 0 });
      fireEvent.keyDown(textarea, { key: 'Backspace' });
      expect(onContextItemsChange).toHaveBeenCalledWith([sampleItems[0]]);
    });

    it('does not render tokens when contextItems is undefined', () => {
      const { queryAllByTestId } = render(
        <AiInput value="" onChange={vi.fn()} onSubmit={vi.fn()} />,
      );
      expect(queryAllByTestId('context-token')).toHaveLength(0);
    });

    it('uses context.placeholder when context is enabled and placeholder omitted', () => {
      const { container } = render(
        <AiInput
          value=""
          onChange={vi.fn()}
          onSubmit={vi.fn()}
          contextItems={[]}
          onContextItemsChange={vi.fn()}
        />,
      );
      const textarea = container.querySelector('textarea')!;
      expect(textarea.placeholder).toBe('context.placeholder');
    });

    it('prefers explicit placeholder over context default', () => {
      const { container } = render(
        <AiInput
          value=""
          onChange={vi.fn()}
          onSubmit={vi.fn()}
          contextItems={[]}
          onContextItemsChange={vi.fn()}
          placeholder="regular placeholder"
        />,
      );
      const textarea = container.querySelector('textarea')!;
      expect(textarea.placeholder).toBe('regular placeholder');
    });

    it('detects @ in input and opens picker', async () => {
      const onChange = vi.fn();
      const { container } = render(
        <AiInput
          value=""
          onChange={onChange}
          onSubmit={vi.fn()}
          contextItems={[]}
          onContextItemsChange={vi.fn()}
        />,
      );
      const textarea = container.querySelector('textarea')!;

      Object.defineProperty(textarea, 'selectionStart', {
        configurable: true,
        get: () => textarea.value.length,
      });
      Object.defineProperty(textarea, 'selectionEnd', {
        configurable: true,
        get: () => textarea.value.length,
      });
      fireEvent.change(textarea, { target: { value: '@' } });

      await waitFor(() => {
        const picker = container.querySelector('.absolute.bottom-full');
        expect(picker).toBeInTheDocument();
      });
    });
  });
});
