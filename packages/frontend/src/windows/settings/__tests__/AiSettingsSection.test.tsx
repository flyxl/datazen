import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, fireEvent, cleanup, waitFor } from '@testing-library/react';
import { setBackendClient } from '@datazen/backend-client';
import { AiSettingsSection } from '../AiSettingsSection';
import { useAiStore } from '../../../stores/aiStore';
import { getDefaultSafetyGate } from '../../../lib/aiSafetyPresets';
import { LOCAL_CLI_PROVIDER_TYPES, PROVIDER_PRESETS } from '../../../lib/aiProviders';

// Form-split R6/F6-prep: the context commands ride the legacy invoke rail,
// which resolves the bound backend facade — the section's dir effect fires on
// mount. Bind the test transport so the mount does not leave an unhandled
// unbound-client rejection behind (vitest exits 1 on those).
setBackendClient('test', {
  transport: {
    call: async (method: string) => {
      if (method === 'context_get_dir') return '/tmp/datazen-context';
      return undefined;
    },
  },
} as never);

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
        globalInstructions: '',
        promptTemplates: [],
        memory: {
          enabled: false,
          autoLearn: false,
          cloudPolicy: 'normal_only',
          privacyAcknowledged: false,
        },
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

    fireEvent.click(screen.getByText('settings.ai.tabSafety'));
    await waitFor(() => {
      expect(screen.getByText(/settings\.ai\.maxToolRounds/)).toBeDefined();
      expect(screen.getByText(/settings\.ai\.apiRetries/)).toBeDefined();
      expect(screen.getByText(/settings\.ai\.maxToolTokens/)).toBeDefined();
      expect(screen.getByText(/settings\.ai\.maxToolResultBytes/)).toBeDefined();
      expect(screen.getByText(/settings\.ai\.maxToolDurationSecs/)).toBeDefined();
    });
  });

  it('renders global instructions with a byte counter and blocks over-limit saves', () => {
    render(<AiSettingsSection />);

    const input = screen.getByTestId('ai-global-instructions') as HTMLTextAreaElement;
    fireEvent.change(input, { target: { value: 'Use UTC.' } });
    expect(screen.getByTestId('ai-global-instructions-count').textContent).toContain('8 / 4096');

    fireEvent.change(input, { target: { value: 'a'.repeat(4097) } });
    expect(screen.getByTestId('ai-global-instructions-count').textContent).toContain('4097 / 4096');
    expect(screen.getByTestId('ai-global-instructions-count').textContent).toContain(
      'settings.ai.overLimitHint',
    );
    expect(screen.getByTestId('ai-context-save')).toBeDisabled();
  });

  it('saves instructions and templates together with the existing profiles', async () => {
    const saveSettingsConfig = vi.fn(async () => true);
    useAiStore.setState({ saveSettingsConfig });
    render(<AiSettingsSection />);

    fireEvent.change(screen.getByTestId('ai-global-instructions'), {
      target: { value: 'Use UTC.' },
    });
    fireEvent.click(screen.getByText('settings.ai.addTemplate'));
    fireEvent.change(screen.getByTestId('ai-template-name'), {
      target: { value: 'Default' },
    });
    fireEvent.change(screen.getByTestId('ai-template-content'), {
      target: { value: 'Be concise.' },
    });
    fireEvent.click(screen.getByText('common.confirm'));
    fireEvent.click(screen.getByTestId('ai-context-save'));

    await waitFor(() =>
      expect(saveSettingsConfig).toHaveBeenCalledWith(
        expect.objectContaining({
          activeProfileId: 'profile-1',
          globalInstructions: 'Use UTC.',
          promptTemplates: [
            expect.objectContaining({
              name: 'Default',
              content: 'Be concise.',
            }),
          ],
        }),
      ),
    );
  });

  it('creates a prompt template and tracks the 8KB/16KB limits', () => {
    render(<AiSettingsSection />);

    fireEvent.click(screen.getByText('settings.ai.addTemplate'));
    fireEvent.change(screen.getByTestId('ai-template-name'), {
      target: { value: 'Postgres style' },
    });
    fireEvent.change(screen.getByTestId('ai-template-database-types'), {
      target: { value: 'postgresql, mysql' },
    });
    fireEvent.change(screen.getByTestId('ai-template-content'), {
      target: { value: 'Prefer explicit JOINs.' },
    });
    expect(screen.getByTestId('ai-template-content-count').textContent).toContain('22 / 8192');
    fireEvent.click(screen.getByText('common.confirm'));

    expect(screen.getByText('Postgres style')).toBeDefined();
    expect(screen.getByText(/postgresql, mysql/)).toBeDefined();
    expect(screen.getByTestId('ai-templates-total-count').textContent).toContain('22 / 16384');
  });

  it('selects a local CLI agent, exposes path/workspace fields, and validates it', async () => {
    const validateConfig = vi.fn(async () => true);
    useAiStore.setState({
      providers: LOCAL_CLI_PROVIDER_TYPES.map((type) => PROVIDER_PRESETS[type]),
      validateConfig,
    });
    render(<AiSettingsSection />);

    fireEvent.click(screen.getByText('settings.ai.addModel'));
    await waitFor(() => expect(screen.getByTestId('ai-provider-select')).toBeDefined());
    fireEvent.click(screen.getByTestId('ai-provider-select'));
    fireEvent.mouseDown(await screen.findByText('Claude Code'));

    expect(screen.getByTestId('ai-local-cli-path')).toBeDefined();
    expect(screen.getByTestId('ai-local-cli-workspace')).toBeDefined();
    expect(screen.queryByText('settings.ai.apiKey')).toBeNull();

    fireEvent.change(screen.getByTestId('ai-local-cli-path'), {
      target: { value: '/usr/local/bin/claude' },
    });
    fireEvent.change(screen.getByTestId('ai-local-cli-workspace'), {
      target: { value: '/tmp/datazen-project' },
    });
    fireEvent.click(screen.getByTestId('ai-profile-validate'));

    await waitFor(() =>
      expect(validateConfig).toHaveBeenCalledWith(
        expect.objectContaining({
          providerType: 'claude_code',
          extra: {
            executablePath: '/usr/local/bin/claude',
            workspaceDir: '/tmp/datazen-project',
          },
        }),
      ),
    );
  });

  it('renders empty state when no profiles exist', () => {
    useAiStore.setState({
      settingsConfig: {
        activeProfileId: '',
        profiles: [],
        globalInstructions: '',
        promptTemplates: [],
        memory: {
          enabled: false,
          autoLearn: false,
          cloudPolicy: 'normal_only',
          privacyAcknowledged: false,
        },
      },
    });

    render(<AiSettingsSection />);

    expect(screen.getByText('settings.ai.noModels')).toBeDefined();
  });
});
