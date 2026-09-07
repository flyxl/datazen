import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, fireEvent, cleanup, waitFor } from '@testing-library/react';
import { AiSettingsSection } from '../AiSettingsSection';
import { useAiStore } from '../../../stores/aiStore';
import { getDefaultSafetyGate } from '../../../lib/aiSafetyPresets';

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({
    t: (key: string) => key,
  }),
}));

vi.mock('../../../hooks/useConfirmDialog', () => ({
  useConfirmDialog: () => [vi.fn().mockResolvedValue(true), null],
}));

describe('AiSettingsSection', () => {
  beforeEach(() => {
    useAiStore.setState({
      settingsConfig: {
        activeProfileId: 'profile-1',
        profiles: [
          {
            id: 'profile-1',
            name: 'Ollama Qwen',
            providerType: 'ollama',
            endpoint: 'http://localhost:11434',
            model: 'qwen2.5-coder',
            maxTokens: 32000,
            safetyGate: getDefaultSafetyGate('local_trust'),
            isDefault: true,
          },
        ],
      },
      providers: [],
    });
  });

  afterEach(() => {
    cleanup();
  });

  it('renders model profiles and add button', () => {
    render(<AiSettingsSection />);

    expect(screen.getByText('settings.ai.modelsTitle')).toBeDefined();
    expect(screen.getByText('Ollama Qwen')).toBeDefined();
    expect(screen.getByText('qwen2.5-coder')).toBeDefined();
    expect(screen.getByText('settings.ai.addModel')).toBeDefined();
  });

  it('opens add model dialog when clicking add button', async () => {
    render(<AiSettingsSection />);

    fireEvent.click(screen.getByText('settings.ai.addModel'));

    await waitFor(() => {
      expect(screen.getByText('settings.ai.tabBasic')).toBeDefined();
      expect(screen.getByText('settings.ai.tabSafety')).toBeDefined();
    });
  });

  it('renders empty state when no profiles exist', () => {
    useAiStore.setState({
      settingsConfig: {
        activeProfileId: '',
        profiles: [],
      },
    });

    render(<AiSettingsSection />);

    expect(screen.getByText('settings.ai.noModels')).toBeDefined();
  });
});
