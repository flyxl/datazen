import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { cleanup, render } from '@testing-library/react';
import { useLocaleDomains } from '../useLocaleDomains';
import { useSettingsStore } from '../../stores/settingsStore';
import { ensureLocaleDomains } from '../../locales';

const ensureMock = vi.fn().mockResolvedValue(undefined);

vi.mock('../../locales', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../../locales')>();
  return {
    ...actual,
    ensureLocaleDomains: (...args: Parameters<typeof ensureLocaleDomains>) => ensureMock(...args),
  };
});

function Probe({ domains, rerender }: { domains: readonly string[]; rerender: number }) {
  const ready = useLocaleDomains(domains as never);
  void rerender;
  return ready ? <span data-testid="ready" /> : <span data-testid="loading" />;
}

describe('useLocaleDomains', () => {
  beforeEach(() => {
    ensureMock.mockClear();
    ensureMock.mockResolvedValue(undefined);
    useSettingsStore.setState({ settings: { ...useSettingsStore.getState().settings, language: 'en' } });
  });

  afterEach(() => {
    cleanup();
  });

  it('does not re-ensure when a new array with the same keys is passed each render', async () => {
    const { rerender } = render(<Probe domains={['sync', 'transfer']} rerender={1} />);
    await vi.waitFor(() => expect(ensureMock).toHaveBeenCalledTimes(1));

    // A fresh array literal with the same (reordered) keys must NOT re-trigger.
    rerender(<Probe domains={['transfer', 'sync']} rerender={2} />);
    await vi.waitFor(() => expect(ensureMock).toHaveBeenCalledTimes(1));
  });

  it('re-ensures when the language changes', async () => {
    const { rerender } = render(<Probe domains={['sync']} rerender={1} />);
    await vi.waitFor(() => expect(ensureMock).toHaveBeenCalledTimes(1));

    useSettingsStore.setState({ settings: { ...useSettingsStore.getState().settings, language: 'zh-CN' } });
    rerender(<Probe domains={['sync']} rerender={2} />);
    await vi.waitFor(() => expect(ensureMock).toHaveBeenCalledTimes(2));
    // The language passed through is the new one.
    expect(ensureMock).toHaveBeenLastCalledWith('zh-CN', ['sync']);
  });

  it('passes the stable domain list to ensureLocaleDomains', async () => {
    render(<Probe domains={['transfer', 'sync']} rerender={1} />);
    await vi.waitFor(() => expect(ensureMock).toHaveBeenCalledTimes(1));
    expect(ensureMock).toHaveBeenLastCalledWith('en', ['sync', 'transfer']);
  });
});