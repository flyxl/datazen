/**
 * Copy-feedback convergence for `McpSettingsSection`
 * (track: copy-feedback-converge).
 *
 * The section's config-snippet copy button used a bare `setTimeout` with no
 * unmount cleanup and no rollback. The converged hook makes the label optimistic
 * and request-bound, and the timer is owned by the component that armed it.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import {
  advanceBy,
  installControlledClipboard,
  installResolvedClipboard,
  spyOnWindowTimers,
} from '../../../test/copyFeedbackHarness';
import { McpSettingsSection } from '../McpSettingsSection';
import { settingsCommands } from '../../../commands/settings';
import { aiCommands } from '../../../commands/ai';
import { clearCachedAppExecutablePathForTest } from '../../../lib/mcpAgentConfig';
import { useSettingsStore } from '../../../stores/settingsStore';

vi.mock('../../../hooks/useLocaleDomains', () => ({
  useLocaleDomains: () => true,
}));

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

/** This section's `useCopyFeedback` window. */
const FEEDBACK_MS = 2000;

const snippetButton = () => screen.getByText('mcp.config.copy').closest('button') as HTMLElement;

beforeEach(() => {
  vi.useFakeTimers();
  clearCachedAppExecutablePathForTest();
  vi.spyOn(aiCommands, 'mcpListAllTools').mockResolvedValue([]);
  vi.spyOn(aiCommands, 'mcpGetStatus').mockResolvedValue({ running: false, transport: 'stdio' });
  vi.spyOn(settingsCommands, 'getAppExecutablePath').mockResolvedValue('/usr/bin/datazen');
});

afterEach(() => {
  cleanup();
  vi.useRealTimers();
  vi.restoreAllMocks();
  vi.clearAllMocks();
});

/** The section mounts several async effects; let them land before clicking. */
async function renderSection() {
  const utils = render(
    <McpSettingsSection
      settings={useSettingsStore.getState().settings}
      onSettingsChange={vi.fn()}
    />,
  );
  // Several async effects feed the snippet; flush them with microtasks rather
  // than `waitFor`, which cannot make progress under fake timers.
  await act(async () => {
    await Promise.resolve();
  });
  return { ...utils, copyButton: snippetButton() };
}

describe('McpSettingsSection copy feedback', () => {
  it('confirms on click and holds the label for the full 2000ms window', async () => {
    const writeText = installResolvedClipboard();
    const { copyButton } = await renderSection();

    fireEvent.click(copyButton);
    expect(writeText.mock.calls[0]?.[0]).toContain('datazen');
    expect(screen.getByText('mcp.config.copied')).toBeInTheDocument();

    advanceBy(FEEDBACK_MS - 1);
    expect(screen.getByText('mcp.config.copied')).toBeInTheDocument();

    advanceBy(1);
    expect(screen.getByText('mcp.config.copy')).toBeInTheDocument();
  });

  it('rolls back when the clipboard write rejects', async () => {
    const clipboard = installControlledClipboard();
    const { copyButton } = await renderSection();

    fireEvent.click(copyButton);
    expect(screen.getByText('mcp.config.copied')).toBeInTheDocument();

    await clipboard.settle(0, 'reject');
    expect(screen.getByText('mcp.config.copy')).toBeInTheDocument();
  });

  it('clears the feedback timer when unmounted inside the window', async () => {
    installResolvedClipboard();
    const timers = spyOnWindowTimers();
    const { unmount, copyButton } = await renderSection();

    fireEvent.click(copyButton);
    const windowHandle = timers.lastArmedHandle();
    expect(timers.clearedHandles()).not.toContain(windowHandle);

    unmount();

    expect(timers.clearedHandles()).toContain(windowHandle);
  });

  it('a late rejection from an earlier click does not erase a newer confirmation', async () => {
    const clipboard = installControlledClipboard();
    const { copyButton } = await renderSection();

    fireEvent.click(copyButton);
    fireEvent.click(copyButton);

    await clipboard.settle(0, 'reject');
    expect(screen.getByText('mcp.config.copied')).toBeInTheDocument();

    advanceBy(FEEDBACK_MS);
    expect(screen.getByText('mcp.config.copy')).toBeInTheDocument();
  });
});
