import { describe, expect, it, vi, afterEach } from 'vitest';
import { render, screen, fireEvent, cleanup } from '@testing-library/react';
import { ModelProfileCard } from '../ModelProfileCard';
import { getDefaultSafetyGate } from '../../../../lib/aiSafetyPresets';
import type { AiModelProfile } from '../../../../types';

vi.mock('../../../../hooks/useI18n', () => ({
  useI18n: () => ({
    t: (key: string) => key,
  }),
}));

describe('ModelProfileCard', () => {
  afterEach(() => {
    cleanup();
  });

  const mockProfile: AiModelProfile = {
    id: 'test-profile-1',
    name: 'Claude Sonnet 3.7',
    providerType: 'open_ai',
    apiKey: 'sk-test',
    endpoint: 'https://api.example.com',
    model: 'claude-3-7-sonnet',
    maxTokens: 200000,
    safetyGate: getDefaultSafetyGate('cloud_strict'),
    isDefault: false,
  };

  it('renders profile information and default badge', () => {
    render(
      <ModelProfileCard
        profile={mockProfile}
        isDefault={true}
        isActive={true}
        onSetDefault={vi.fn()}
        onEdit={vi.fn()}
        onDuplicate={vi.fn()}
        onDelete={vi.fn()}
      />,
    );

    expect(screen.getByText('Claude Sonnet 3.7')).toBeDefined();
    expect(screen.getByText('claude-3-7-sonnet')).toBeDefined();
    expect(screen.getByText('settings.ai.defaultBadge')).toBeDefined();
    expect(screen.getByText('settings.ai.presetCloudStrict')).toBeDefined();
  });

  it('calls action callbacks when buttons are clicked', () => {
    const onSetDefault = vi.fn();
    const onEdit = vi.fn();
    const onDuplicate = vi.fn();
    const onDelete = vi.fn();

    render(
      <ModelProfileCard
        profile={mockProfile}
        isDefault={false}
        isActive={false}
        onSetDefault={onSetDefault}
        onEdit={onEdit}
        onDuplicate={onDuplicate}
        onDelete={onDelete}
      />,
    );

    fireEvent.click(screen.getByTitle('settings.ai.setDefault'));
    expect(onSetDefault).toHaveBeenCalled();

    fireEvent.click(screen.getByTitle('settings.ai.editModel'));
    expect(onEdit).toHaveBeenCalled();

    fireEvent.click(screen.getByTitle('settings.ai.duplicate'));
    expect(onDuplicate).toHaveBeenCalled();

    fireEvent.click(screen.getByTitle('common.delete'));
    expect(onDelete).toHaveBeenCalled();
  });
});
