import type { AiProviderType, ProviderListItem } from '../types';

const supportsBoth = { supportsStreaming: true, supportsTools: true } as const;

/** Canonical metadata for the ten API presets in PRD order. */
export const PROVIDER_PRESETS: Record<AiProviderType, ProviderListItem> = {
  anthropic: {
    providerType: 'anthropic',
    displayName: 'Anthropic (Claude)',
    ...supportsBoth,
    defaultEndpoint: 'https://api.anthropic.com',
    defaultProtocol: 'anthropic_compatible',
    defaultModel: 'claude-sonnet-4-20250514',
  },
  open_ai: {
    providerType: 'open_ai',
    displayName: 'OpenAI',
    ...supportsBoth,
    defaultEndpoint: 'https://api.openai.com/v1',
    defaultProtocol: 'open_ai_compatible',
    defaultModel: 'gpt-4o',
  },
  gemini: {
    providerType: 'gemini',
    displayName: 'Google Gemini',
    ...supportsBoth,
    defaultEndpoint: 'https://generativelanguage.googleapis.com/v1beta/openai/',
    defaultProtocol: 'open_ai_compatible',
    defaultModel: 'gemini-2.5-pro',
  },
  deep_seek: {
    providerType: 'deep_seek',
    displayName: 'DeepSeek',
    ...supportsBoth,
    defaultEndpoint: 'https://api.deepseek.com',
    defaultProtocol: 'open_ai_responses',
    defaultModel: 'deepseek-chat',
  },
  qwen: {
    providerType: 'qwen',
    displayName: 'Qwen',
    ...supportsBoth,
    defaultEndpoint: 'https://dashscope.aliyuncs.com/compatible-mode/v1',
    defaultProtocol: 'open_ai_compatible',
    defaultModel: 'qwen-plus',
  },
  mini_max: {
    providerType: 'mini_max',
    displayName: 'MiniMax',
    ...supportsBoth,
    defaultEndpoint: 'https://api.minimax.io/v1',
    defaultProtocol: 'open_ai_compatible',
    defaultModel: 'MiniMax-M3.1-Flash',
  },
  ollama: {
    providerType: 'ollama',
    displayName: 'Ollama',
    ...supportsBoth,
    defaultEndpoint: 'http://127.0.0.1:11434/v1',
    defaultProtocol: 'open_ai_compatible',
    defaultModel: 'llama3.2',
  },
  open_ai_compatible: {
    providerType: 'open_ai_compatible',
    displayName: 'OpenAI Compatible',
    ...supportsBoth,
    defaultEndpoint: '',
    defaultProtocol: 'open_ai_compatible',
    defaultModel: '',
  },
  anthropic_compatible: {
    providerType: 'anthropic_compatible',
    displayName: 'Anthropic Compatible',
    ...supportsBoth,
    defaultEndpoint: '',
    defaultProtocol: 'anthropic_compatible',
    defaultModel: '',
  },
  custom: {
    providerType: 'custom',
    displayName: 'Custom',
    ...supportsBoth,
    defaultEndpoint: '',
    defaultProtocol: 'open_ai_compatible',
    defaultModel: '',
  },
  claude_code: {
    providerType: 'claude_code',
    displayName: 'Claude Code',
    supportsStreaming: true,
    supportsTools: false,
    defaultEndpoint: '',
    defaultProtocol: 'local_cli',
    defaultModel: '',
  },
  codex: {
    providerType: 'codex',
    displayName: 'Codex',
    supportsStreaming: true,
    supportsTools: false,
    defaultEndpoint: '',
    defaultProtocol: 'local_cli',
    defaultModel: '',
  },
  cursor: {
    providerType: 'cursor',
    displayName: 'Cursor',
    supportsStreaming: true,
    supportsTools: false,
    defaultEndpoint: '',
    defaultProtocol: 'local_cli',
    defaultModel: '',
  },
  opencode: {
    providerType: 'opencode',
    displayName: 'OpenCode',
    supportsStreaming: true,
    supportsTools: false,
    defaultEndpoint: '',
    defaultProtocol: 'local_cli',
    defaultModel: '',
  },
  grok: {
    providerType: 'grok',
    displayName: 'Grok',
    supportsStreaming: true,
    supportsTools: false,
    defaultEndpoint: '',
    defaultProtocol: 'local_cli',
    defaultModel: '',
  },
  qoder: {
    providerType: 'qoder',
    displayName: 'Qoder',
    supportsStreaming: true,
    supportsTools: false,
    defaultEndpoint: '',
    defaultProtocol: 'local_cli',
    defaultModel: '',
  },
  codebuddy: {
    providerType: 'codebuddy',
    displayName: 'CodeBuddy',
    supportsStreaming: true,
    supportsTools: false,
    defaultEndpoint: '',
    defaultProtocol: 'local_cli',
    defaultModel: '',
  },
  pi: {
    providerType: 'pi',
    displayName: 'Pi',
    supportsStreaming: true,
    supportsTools: false,
    defaultEndpoint: '',
    defaultProtocol: 'local_cli',
    defaultModel: '',
  },
};

/** Local CLI agents are advertised after the ten API presets. */
export const LOCAL_CLI_PROVIDER_TYPES: AiProviderType[] = [
  'claude_code',
  'codex',
  'cursor',
  'opencode',
  'grok',
  'qoder',
  'codebuddy',
  'pi',
];

export function isLocalCliProvider(value: AiProviderType | string): boolean {
  return (LOCAL_CLI_PROVIDER_TYPES as string[]).includes(value);
}

export const PROVIDER_ORDER: AiProviderType[] = [
  'anthropic',
  'open_ai',
  'gemini',
  'deep_seek',
  'qwen',
  'mini_max',
  'ollama',
  'open_ai_compatible',
  'anthropic_compatible',
  'custom',
  ...LOCAL_CLI_PROVIDER_TYPES,
];

export const DEEP_SEEK_PROVIDER = PROVIDER_PRESETS.deep_seek;
export const OLLAMA_PROVIDER = PROVIDER_PRESETS.ollama;

export function isKnownProviderType(value: string): value is AiProviderType {
  return (PROVIDER_ORDER as string[]).includes(value);
}

function mergeProvider(provider: ProviderListItem, preset: ProviderListItem): ProviderListItem {
  return {
    ...preset,
    ...provider,
    providerType: preset.providerType,
    displayName: provider.displayName || preset.displayName,
    defaultEndpoint: provider.defaultEndpoint || preset.defaultEndpoint,
    defaultProtocol: provider.defaultProtocol || preset.defaultProtocol,
    defaultModel: provider.defaultModel || preset.defaultModel,
  };
}

/**
 * Normalize backend provider metadata into the ten canonical presets. Old
 * backends only return five entries; new presets are filled in locally so the
 * form never silently loses a preset. Legacy `anthropic` is now a first-class
 * Claude preset instead of being hidden.
 */
export function normalizeAiProviders(providers: ProviderListItem[]): ProviderListItem[] {
  const byType = new Map<AiProviderType, ProviderListItem>();

  for (const provider of providers) {
    if (!isKnownProviderType(provider.providerType)) continue;
    const preset = PROVIDER_PRESETS[provider.providerType];
    // DeepSeek/Ollama historically shipped stale backend defaults; keep the
    // frontend canonical endpoints so existing profiles migrate predictably.
    if (provider.providerType === 'deep_seek' || provider.providerType === 'ollama') {
      byType.set(provider.providerType, preset);
      continue;
    }
    byType.set(provider.providerType, mergeProvider(provider, preset));
  }

  for (const type of PROVIDER_ORDER) {
    if (!byType.has(type)) byType.set(type, PROVIDER_PRESETS[type]);
  }

  return PROVIDER_ORDER.map((type) => byType.get(type)!);
}
