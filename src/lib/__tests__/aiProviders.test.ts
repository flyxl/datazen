import { describe, expect, it } from 'vitest';
import { DEEP_SEEK_PROVIDER, isKnownProviderType, normalizeAiProviders } from '../aiProviders';
import type { ProviderListItem } from '../../types';

describe('isKnownProviderType', () => {
  it('accepts supported provider types', () => {
    expect(isKnownProviderType('open_ai')).toBe(true);
    expect(isKnownProviderType('deep_seek')).toBe(true);
    expect(isKnownProviderType('custom')).toBe(true);
  });

  it('rejects legacy anthropic', () => {
    expect(isKnownProviderType('anthropic')).toBe(false);
  });
});

describe('normalizeAiProviders', () => {
  const openAi: ProviderListItem = {
    providerType: 'open_ai',
    displayName: 'OpenAI',
    supportsStreaming: true,
    supportsTools: true,
    defaultEndpoint: 'https://api.openai.com/v1',
    defaultProtocol: 'open_ai_compatible',
  };

  const anthropic = {
    providerType: 'anthropic',
    displayName: 'Anthropic (Claude)',
    supportsStreaming: true,
    supportsTools: true,
    defaultEndpoint: 'https://api.anthropic.com',
    defaultProtocol: 'anthropic_compatible',
  } as ProviderListItem;

  const custom: ProviderListItem = {
    providerType: 'custom',
    displayName: 'Custom',
    supportsStreaming: true,
    supportsTools: true,
    defaultEndpoint: '',
    defaultProtocol: 'open_ai_compatible',
  };

  it('removes anthropic and adds deep_seek with defaults', () => {
    const result = normalizeAiProviders([openAi, anthropic, custom]);

    expect(result.map((p) => p.providerType)).toEqual(['open_ai', 'deep_seek', 'custom']);
    expect(result.find((p) => p.providerType === 'deep_seek')).toEqual(DEEP_SEEK_PROVIDER);
  });

  it('applies frontend deep_seek defaults over backend entry', () => {
    const backendDeepSeek: ProviderListItem = {
      providerType: 'deep_seek',
      displayName: 'DeepSeek Backend',
      supportsStreaming: false,
      supportsTools: false,
      defaultEndpoint: 'https://old.example.com',
      defaultProtocol: 'open_ai_compatible',
    };

    const result = normalizeAiProviders([openAi, backendDeepSeek, custom]);
    expect(result.find((p) => p.providerType === 'deep_seek')).toEqual(DEEP_SEEK_PROVIDER);
  });
});
