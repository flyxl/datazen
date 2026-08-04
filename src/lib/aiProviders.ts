import type { AiProviderType, ProviderListItem } from '../types';

export const DEEP_SEEK_PROVIDER: ProviderListItem = {
  providerType: 'deep_seek',
  displayName: 'DeepSeek',
  supportsStreaming: true,
  supportsTools: true,
  defaultEndpoint: 'https://api.deepseek.com',
  defaultProtocol: 'open_ai_responses',
};

const PROVIDER_ORDER: AiProviderType[] = ['open_ai', 'deep_seek', 'custom'];

export function isKnownProviderType(value: string): value is AiProviderType {
  return value === 'open_ai' || value === 'deep_seek' || value === 'custom';
}

/** Hide legacy Anthropic built-in provider; ensure DeepSeek is available with frontend defaults. */
export function normalizeAiProviders(providers: ProviderListItem[]): ProviderListItem[] {
  const byType = new Map<AiProviderType, ProviderListItem>();

  for (const provider of providers) {
    if ((provider.providerType as string) === 'anthropic') continue;
    if (provider.providerType === 'deep_seek') {
      byType.set('deep_seek', { ...provider, ...DEEP_SEEK_PROVIDER });
      continue;
    }
    if (isKnownProviderType(provider.providerType)) {
      byType.set(provider.providerType, provider);
    }
  }

  if (!byType.has('deep_seek')) {
    byType.set('deep_seek', DEEP_SEEK_PROVIDER);
  }

  return PROVIDER_ORDER.filter((type) => byType.has(type)).map((type) => byType.get(type)!);
}
