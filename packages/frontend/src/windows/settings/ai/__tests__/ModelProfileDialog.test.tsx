import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, fireEvent, cleanup, waitFor } from '@testing-library/react';
import { ModelProfileDialog } from '../ModelProfileDialog';
import { useAiStore } from '../../../../stores/aiStore';
import { getDefaultSafetyGate } from '../../../../lib/aiSafetyPresets';

vi.mock('../../../../hooks/useI18n', () => ({
  useI18n: () => ({
    t: (key: string) => key,
  }),
}));

describe('ModelProfileDialog', () => {
  beforeEach(() => {
    useAiStore.setState({
      providers: [
        {
          providerType: 'open_ai',
          displayName: 'OpenAI',
          supportsStreaming: true,
          supportsTools: true,
          defaultEndpoint: 'https://api.openai.com/v1',
          defaultProtocol: 'open_ai_compatible',
        },
        {
          providerType: 'ollama',
          displayName: 'Ollama',
          supportsStreaming: true,
          supportsTools: false,
          defaultEndpoint: 'http://localhost:11434',
          defaultProtocol: 'open_ai_compatible',
        },
      ],
      remoteModels: [],
    });
  });

  afterEach(() => {
    cleanup();
  });

  it('renders form inputs and handles tab switching', () => {
    render(
      <ModelProfileDialog
        open={true}
        onClose={vi.fn()}
        profile={null}
        onSave={vi.fn().mockResolvedValue(true)}
      />,
    );

    expect(screen.getByText('settings.ai.addModel')).toBeDefined();
    expect(screen.getByText('settings.ai.tabBasic')).toBeDefined();
    expect(screen.getByText('settings.ai.tabSafety')).toBeDefined();

    // Switch to safety tab
    fireEvent.click(screen.getByText('settings.ai.tabSafety'));
    expect(screen.getByText('settings.ai.safetyPreset')).toBeDefined();
    expect(screen.getByText('settings.ai.presetCloudStrict')).toBeDefined();
    expect(screen.getByText('settings.ai.presetLocalTrust')).toBeDefined();
  });

  it('saves basic profile with onSave callback', async () => {
    const onSave = vi.fn().mockResolvedValue(true);
    const onClose = vi.fn();

    render(<ModelProfileDialog open={true} onClose={onClose} profile={null} onSave={onSave} />);

    // Enter model name
    const modelInput = screen.getByPlaceholderText('e.g. gpt-4o, claude-3-7-sonnet, qwen2.5-coder');
    fireEvent.change(modelInput, { target: { value: 'gpt-4o' } });

    // Save
    fireEvent.click(screen.getByText('common.save'));

    await waitFor(() => {
      expect(onSave).toHaveBeenCalled();
      expect(onClose).toHaveBeenCalled();
    });
  });

  it('warns on unrestricted egress with remote endpoint', async () => {
    const onSave = vi.fn().mockResolvedValue(true);
    const onClose = vi.fn();

    render(
      <ModelProfileDialog
        open={true}
        onClose={onClose}
        profile={{
          id: 'test-1',
          name: 'Remote Dangerous',
          providerType: 'open_ai',
          apiKey: 'sk-123',
          endpoint: 'https://api.remote.com',
          model: 'gpt-4o',
          maxTokens: 200000,
          safetyGate: getDefaultSafetyGate('local_trust'), // unrestricted
          isDefault: false,
        }}
        onSave={onSave}
      />,
    );

    // Attempt to save
    fireEvent.click(screen.getByText('common.save'));

    // High risk warning dialog should appear
    await waitFor(() => {
      expect(screen.getByText('settings.ai.unrestrictedWarningTitle')).toBeDefined();
    });

    // Confirm risk
    fireEvent.click(screen.getByText('common.confirm'));

    await waitFor(() => {
      expect(onSave).toHaveBeenCalled();
      expect(onClose).toHaveBeenCalled();
    });
  });
});
