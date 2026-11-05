import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { SkillsSection } from '../SkillsSection';
import type { AiSettingsConfig } from '../../../types';

const commands = vi.hoisted(() => ({
  listSkills: vi.fn(),
}));
const store = vi.hoisted(() => ({
  saveSettingsConfig: vi.fn().mockResolvedValue(true),
}));

vi.mock('../../../commands/ai', () => ({ aiCommands: commands }));
vi.mock('../../../stores/aiStore', () => ({
  useAiStore: (selector: (state: typeof store) => unknown) => selector(store),
}));
vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));
vi.mock('@datazen/ui', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@datazen/ui')>()),
  PathInput: ({
    value,
    onChange,
    inputTestId,
  }: {
    value: string;
    onChange: (path: string) => void;
    inputTestId?: string;
  }) => (
    <input
      data-testid={inputTestId}
      value={value}
      onChange={(event) => onChange(event.target.value)}
    />
  ),
}));

function configWith(overrides: Partial<AiSettingsConfig['skills']> = {}): AiSettingsConfig {
  return {
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
    skills: {
      enabled: true,
      customDir: null,
      autoUse: false,
      ...overrides,
    },
  };
}

describe('SkillsSection', () => {
  beforeEach(() => {
    commands.listSkills.mockResolvedValue([
      {
        name: 'sql-review',
        description: 'Review SQL safely',
        source: 'user',
      },
    ]);
  });
  afterEach(() => {
    cleanup();
    vi.clearAllMocks();
  });

  it('defaults auto-use off and persists it only when explicitly enabled', async () => {
    render(<SkillsSection config={configWith()} />);
    await waitFor(() => expect(commands.listSkills).toHaveBeenCalled());

    const autoUse = screen.getByTestId('ai-skills-auto-use');
    expect(autoUse).toHaveAttribute('aria-checked', 'false');
    fireEvent.click(autoUse);
    await waitFor(() =>
      expect(store.saveSettingsConfig).toHaveBeenCalledWith(
        expect.objectContaining({
          skills: expect.objectContaining({ autoUse: true }),
        }),
      ),
    );
  });

  it('renders the name/description listing without loading bodies', async () => {
    render(<SkillsSection config={configWith()} />);
    await waitFor(() => expect(screen.getByTestId('ai-skills-row-sql-review')).toBeInTheDocument());
    expect(screen.getByText('Review SQL safely')).toBeInTheDocument();
    expect(commands.listSkills).toHaveBeenCalledTimes(1);
  });

  it('does not scan when skills are disabled and explains why', async () => {
    render(<SkillsSection config={configWith({ enabled: false })} />);
    expect(screen.getByTestId('ai-skills-disabled')).toBeInTheDocument();
    expect(commands.listSkills).not.toHaveBeenCalled();
  });

  it('saves one optional custom directory', async () => {
    render(<SkillsSection config={configWith()} />);
    await waitFor(() => expect(commands.listSkills).toHaveBeenCalled());
    const input = screen.getByTestId('ai-skills-dir');
    fireEvent.change(input, { target: { value: '/tmp/team-skills' } });
    const saveButtons = screen.getAllByRole('button', { name: 'settings.ai.skills.saveDir' });
    fireEvent.click(saveButtons[0]);
    await waitFor(() =>
      expect(store.saveSettingsConfig).toHaveBeenCalledWith(
        expect.objectContaining({
          skills: expect.objectContaining({ customDir: '/tmp/team-skills' }),
        }),
      ),
    );
  });
});
