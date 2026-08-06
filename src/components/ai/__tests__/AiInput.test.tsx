import { describe, expect, it, vi, afterEach, beforeEach } from 'vitest';
import { render, cleanup, fireEvent, waitFor } from '@testing-library/react';
import { AiInput } from '../AiInput';
import type { ContextEntry } from '../../../types';

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

  describe('context files', () => {
    const sampleFiles: ContextEntry[] = [
      { name: 'schema.sql', path: 'schema.sql', isDir: false, size: 100 },
      { name: 'notes.md', path: 'notes.md', isDir: false, size: 200 },
    ];

    it('renders context file chips', () => {
      const { getByText } = render(
        <AiInput
          value=""
          onChange={vi.fn()}
          onSubmit={vi.fn()}
          contextFiles={sampleFiles}
          onContextFilesChange={vi.fn()}
        />,
      );
      expect(getByText('@schema.sql')).toBeInTheDocument();
      expect(getByText('@notes.md')).toBeInTheDocument();
    });

    it('removes context file when chip X is clicked', () => {
      const onContextFilesChange = vi.fn();
      const { container } = render(
        <AiInput
          value=""
          onChange={vi.fn()}
          onSubmit={vi.fn()}
          contextFiles={sampleFiles}
          onContextFilesChange={onContextFilesChange}
        />,
      );
      const removeButtons = container.querySelectorAll('span button');
      fireEvent.click(removeButtons[0]);
      expect(onContextFilesChange).toHaveBeenCalledWith([sampleFiles[1]]);
    });

    it('does not render chips when contextFiles is undefined', () => {
      const { container } = render(
        <AiInput value="" onChange={vi.fn()} onSubmit={vi.fn()} />,
      );
      expect(container.querySelectorAll('span').length).toBe(0);
    });

    it('uses context.placeholder when context is enabled', () => {
      const { container } = render(
        <AiInput
          value=""
          onChange={vi.fn()}
          onSubmit={vi.fn()}
          contextFiles={[]}
          onContextFilesChange={vi.fn()}
          placeholder="regular placeholder"
        />,
      );
      const textarea = container.querySelector('textarea')!;
      expect(textarea.placeholder).toBe('context.placeholder');
    });

    it('detects @ in input and opens picker', async () => {
      const onChange = vi.fn();
      const { container } = render(
        <AiInput
          value=""
          onChange={onChange}
          onSubmit={vi.fn()}
          contextFiles={[]}
          onContextFilesChange={vi.fn()}
        />,
      );
      const textarea = container.querySelector('textarea')!;

      Object.defineProperty(textarea, 'selectionStart', {
        value: 1,
        writable: true,
      });
      fireEvent.change(textarea, { target: { value: '@', selectionStart: 1 } });

      await waitFor(() => {
        const picker = container.querySelector('[class*="absolute bottom-full"]');
        expect(picker).toBeInTheDocument();
      });
    });
  });
});
