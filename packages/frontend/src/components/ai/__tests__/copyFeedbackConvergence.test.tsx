/**
 * Copy-feedback convergence for the AI affordances (track: copy-feedback-converge).
 *
 * `AiCodeBlock`, `Nl2SqlPanel` and `WorkflowChatPanel` each hand-rolled a
 * "click -> setState(true) -> setTimeout(... setState(false))" pair. These tests
 * pin the converged behaviour that replacing them with `useCopyFeedback`
 * produced, and -- for `AiCodeBlock` specifically -- the regression test for the
 * timer that used to outlive the component.
 *
 * Assertions are runtime observations only (DOM, clipboard call arguments, and
 * which timer handles reached `clearTimeout`). Nothing here inspects the
 * source.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import {
  advanceBy,
  installControlledClipboard,
  installResolvedClipboard,
  spyOnWindowTimers,
} from '../../../test/copyFeedbackHarness';
import { AiCodeBlock } from '../AiCodeBlock';
import { Nl2SqlPanel } from '../Nl2SqlPanel';
import { WorkflowChatPanel } from '../WorkflowChatPanel';

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

vi.mock('../../../lib/windowManager', () => ({
  openSettingsWindow: vi.fn(),
}));

vi.mock('../SqlCodeBlock', () => ({
  SqlCodeBlock: ({ code }: { code: string }) => <pre data-testid="sql-code">{code}</pre>,
}));

vi.mock('../AiInput', () => ({
  AiInput: () => <div data-testid="ai-input" />,
}));

vi.mock('../AiEgressNotice', () => ({
  AiEgressNotice: () => null,
}));

vi.mock('../../../commands/ai', () => ({
  aiCommands: { workflowSave: vi.fn().mockResolvedValue(undefined) },
}));

const aiState = vi.hoisted(() => ({
  isConfigured: true,
  nl2sql: { input: '', isGenerating: false, generatedSql: '' },
  nl2sqlError: null as string | null,
  setNl2SqlInput: vi.fn(),
  generateSql: vi.fn().mockResolvedValue(undefined),
  clearNl2Sql: vi.fn(),
  workflowChat: {
    messages: [] as { role: 'user' | 'assistant'; content: string }[],
    isStreaming: false,
    streamContent: '',
    streamReasoning: '',
    requestId: null as string | null,
  },
  initWorkflowChat: vi.fn(),
  sendWorkflowChatMessage: vi.fn().mockResolvedValue(undefined),
  clearWorkflowChat: vi.fn(),
}));

vi.mock('../../../stores/aiStore', () => ({
  useAiStore: (sel: (s: typeof aiState) => unknown) => sel(aiState as never),
}));

/** The `useCopyFeedback` windows these three affordances pass in. */
const AI_CODE_FEEDBACK_MS = 1500;
const NL2SQL_FEEDBACK_MS = 2000;

/** lucide renders `Check` with a `.lucide-check` class; the idle icon is `Copy`. */
function showsCheck(element: HTMLElement): boolean {
  return element.querySelector('.lucide-check') !== null;
}

beforeEach(() => {
  vi.useFakeTimers();
  // jsdom has no layout engine; the panel scrolls the transcript into view on
  // every message change. Same stub as WorkflowChatPanel.test.tsx.
  Element.prototype.scrollIntoView = vi.fn();
  aiState.nl2sql = { input: '', isGenerating: false, generatedSql: '' };
  aiState.workflowChat = {
    messages: [],
    isStreaming: false,
    streamContent: '',
    streamReasoning: '',
    requestId: null,
  };
});

afterEach(() => {
  cleanup();
  vi.useRealTimers();
  vi.restoreAllMocks();
  vi.clearAllMocks();
});

describe('AiCodeBlock copy feedback', () => {
  it('keeps the confirmation for the full 1500ms window and reverts after it', () => {
    const writeText = installResolvedClipboard();
    render(<AiCodeBlock code="SELECT 1" language="sql" />);

    fireEvent.click(screen.getByTestId('ai-code-copy'));
    expect(writeText).toHaveBeenCalledWith('SELECT 1');
    expect(showsCheck(screen.getByTestId('ai-code-copy'))).toBe(true);

    // Edge-precise: one millisecond early is still confirmed.
    advanceBy(AI_CODE_FEEDBACK_MS - 1);
    expect(showsCheck(screen.getByTestId('ai-code-copy'))).toBe(true);

    advanceBy(1);
    expect(showsCheck(screen.getByTestId('ai-code-copy'))).toBe(false);
  });

  it('rolls back to the copy icon when the clipboard write rejects', async () => {
    const clipboard = installControlledClipboard();
    render(<AiCodeBlock code="SELECT 1" language="sql" />);

    fireEvent.click(screen.getByTestId('ai-code-copy'));
    // Optimistic: confirmed before the write has settled.
    expect(showsCheck(screen.getByTestId('ai-code-copy'))).toBe(true);

    await clipboard.settle(0, 'reject');
    expect(showsCheck(screen.getByTestId('ai-code-copy'))).toBe(false);
  });

  /**
   * The regression test for the leaked timer. Before convergence this file
   * armed a bare `setTimeout(..., 1500)` whose handle was never stored, so the
   * callback stayed in the timer queue after the component disappeared. React 18
   * no longer warns about a setState on an unmounted fiber, so a leak is
   * otherwise invisible.
   *
   * The witness is the *handle*, not the timer count. Counting live timers
   * across the unmount reads too low even for correct code — mounting queues
   * timers of its own and the unmount legitimately clears those too — so a count
   * difference is evidence about the wrong thing. Naming the handle the click
   * armed and asserting that exact handle reaches `clearTimeout` is unambiguous.
   */
  it('clears the feedback timer when unmounted inside the window', () => {
    installResolvedClipboard();
    const timers = spyOnWindowTimers();
    const { unmount } = render(<AiCodeBlock code="SELECT 1" language="sql" />);

    fireEvent.click(screen.getByTestId('ai-code-copy'));
    // The window is armed, and this is the handle it armed.
    const windowHandle = timers.lastArmedHandle();
    expect(timers.clearedHandles()).not.toContain(windowHandle);

    unmount();

    // Before convergence this handle was never cleared: the bare
    // `setTimeout(..., 1500)` had no owner to cancel it.
    expect(timers.clearedHandles()).toContain(windowHandle);
  });

  it('gives every click a full window instead of stacking timers', () => {
    installResolvedClipboard();
    render(<AiCodeBlock code="SELECT 1" language="sql" />);

    fireEvent.click(screen.getByTestId('ai-code-copy'));
    advanceBy(AI_CODE_FEEDBACK_MS - 100);
    fireEvent.click(screen.getByTestId('ai-code-copy'));

    // The second click restarted the window rather than leaving the first
    // timer to fire early.
    advanceBy(100);
    expect(showsCheck(screen.getByTestId('ai-code-copy'))).toBe(true);

    advanceBy(AI_CODE_FEEDBACK_MS - 100);
    expect(showsCheck(screen.getByTestId('ai-code-copy'))).toBe(false);
  });

  it('a late rejection from an earlier click does not erase a newer confirmation', async () => {
    const clipboard = installControlledClipboard();
    render(<AiCodeBlock code="SELECT 1" language="sql" />);

    fireEvent.click(screen.getByTestId('ai-code-copy'));
    fireEvent.click(screen.getByTestId('ai-code-copy'));

    // The first write fails only now, well after the second click.
    await clipboard.settle(0, 'reject');
    expect(showsCheck(screen.getByTestId('ai-code-copy'))).toBe(true);

    // ...and it must not have killed the second click's timer either.
    advanceBy(AI_CODE_FEEDBACK_MS);
    expect(showsCheck(screen.getByTestId('ai-code-copy'))).toBe(false);
  });
});

describe('Nl2SqlPanel copy feedback', () => {
  function renderPanel() {
    aiState.nl2sql = { input: '', isGenerating: false, generatedSql: 'SELECT 1' };
    return render(<Nl2SqlPanel dbSessionId="s1" database="db" onSqlChange={vi.fn()} />);
  }

  const copyButton = () => screen.getByTitle('common.copy');

  it('confirms immediately, holds for 2000ms, then reverts', () => {
    const writeText = installResolvedClipboard();
    renderPanel();

    fireEvent.click(copyButton());
    // Optimistic: the icon flips without waiting for the write to settle.
    expect(writeText).toHaveBeenCalledWith('SELECT 1');
    expect(showsCheck(copyButton())).toBe(true);

    advanceBy(NL2SQL_FEEDBACK_MS - 1);
    expect(showsCheck(copyButton())).toBe(true);

    advanceBy(1);
    expect(showsCheck(copyButton())).toBe(false);
  });

  it('rolls back when the clipboard write rejects', async () => {
    const clipboard = installControlledClipboard();
    renderPanel();

    fireEvent.click(copyButton());
    expect(showsCheck(copyButton())).toBe(true);

    await clipboard.settle(0, 'reject');
    expect(showsCheck(copyButton())).toBe(false);
  });

  it('clears the feedback timer when unmounted inside the window', () => {
    installResolvedClipboard();
    const timers = spyOnWindowTimers();
    const { unmount } = renderPanel();

    fireEvent.click(copyButton());
    const windowHandle = timers.lastArmedHandle();
    expect(timers.clearedHandles()).not.toContain(windowHandle);

    unmount();

    expect(timers.clearedHandles()).toContain(windowHandle);
  });

  it('a late rejection from an earlier click does not erase a newer confirmation', async () => {
    const clipboard = installControlledClipboard();
    renderPanel();

    fireEvent.click(copyButton());
    fireEvent.click(copyButton());

    await clipboard.settle(0, 'reject');
    expect(showsCheck(copyButton())).toBe(true);

    advanceBy(NL2SQL_FEEDBACK_MS);
    expect(showsCheck(copyButton())).toBe(false);
  });
});

describe('WorkflowChatPanel copy feedback', () => {
  /** Two workflow YAML blocks so the per-block marker routing is observable. */
  function renderTwoBlocks() {
    aiState.workflowChat = {
      messages: [
        {
          role: 'assistant',
          content:
            '```yaml\nname: first\nsteps:\n  - id: one\n```\n\n```yaml\nname: second\nsteps:\n  - id: two\n```\n',
        },
      ],
      isStreaming: false,
      streamContent: '',
      streamReasoning: '',
      requestId: null,
    };
    return render(<WorkflowChatPanel connections={[]} />);
  }

  /**
   * Each block renders `[copy][preview]`; the copy button is icon-only, so it is
   * reached through its labelled sibling.
   */
  function blockCopyButtons(): HTMLElement[] {
    return screen
      .getAllByText('workflows.aiCreate.preview')
      .map((preview) => preview.closest('button')?.previousElementSibling as HTMLElement);
  }

  it('marks only the block whose YAML was copied', () => {
    const writeText = installResolvedClipboard();
    renderTwoBlocks();

    const buttons = blockCopyButtons();
    expect(buttons).toHaveLength(2);

    fireEvent.click(buttons[0]);
    expect(writeText.mock.calls[0]?.[0]).toContain('name: first');
    expect(showsCheck(buttons[0])).toBe(true);
    expect(showsCheck(buttons[1])).toBe(false);

    // Copying the other block moves the marker rather than lighting both.
    fireEvent.click(buttons[1]);
    expect(writeText.mock.calls[1]?.[0]).toContain('name: second');
    expect(showsCheck(buttons[0])).toBe(false);
    expect(showsCheck(buttons[1])).toBe(true);
  });

  it('holds the marker for the full 1500ms window', () => {
    installResolvedClipboard();
    renderTwoBlocks();
    const buttons = blockCopyButtons();

    fireEvent.click(buttons[0]);

    advanceBy(AI_CODE_FEEDBACK_MS - 1);
    expect(showsCheck(buttons[0])).toBe(true);

    advanceBy(1);
    expect(showsCheck(buttons[0])).toBe(false);
  });

  it('clears the feedback timer when unmounted inside the window', () => {
    installResolvedClipboard();
    const timers = spyOnWindowTimers();
    const { unmount } = renderTwoBlocks();
    const buttons = blockCopyButtons();

    fireEvent.click(buttons[0]);
    const windowHandle = timers.lastArmedHandle();
    expect(timers.clearedHandles()).not.toContain(windowHandle);

    unmount();

    expect(timers.clearedHandles()).toContain(windowHandle);
  });

  it('a late rejection from an earlier click does not erase a newer confirmation', async () => {
    const clipboard = installControlledClipboard();
    renderTwoBlocks();
    const buttons = blockCopyButtons();

    fireEvent.click(buttons[0]);
    fireEvent.click(buttons[1]);

    await clipboard.settle(0, 'reject');
    // The stale failure must not move the marker back to block #0.
    expect(showsCheck(buttons[0])).toBe(false);
    expect(showsCheck(buttons[1])).toBe(true);

    advanceBy(AI_CODE_FEEDBACK_MS);
    expect(showsCheck(buttons[1])).toBe(false);
  });
});
