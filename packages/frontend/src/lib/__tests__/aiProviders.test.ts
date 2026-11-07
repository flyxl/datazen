import { describe, expect, it } from 'vitest';
import {
  DEEP_SEEK_PROVIDER,
  OLLAMA_PROVIDER,
  PROVIDER_ORDER,
  PROVIDER_PRESETS,
  isKnownProviderType,
  normalizeAiProviders,
} from '../aiProviders';
import type { AiModelProfile, ProviderListItem } from '../../types';

describe('isKnownProviderType', () => {
  it('accepts every PRD provider preset, including the legacy anthropic key', () => {
    for (const type of PROVIDER_ORDER) expect(isKnownProviderType(type)).toBe(true);
    expect(isKnownProviderType('anthropic')).toBe(true);
  });

  it('rejects an unknown provider', () => {
    expect(isKnownProviderType('not_a_provider')).toBe(false);
  });
});

describe('provider presets', () => {
  it('exposes ten API presets plus eight local CLI agents in stable order', () => {
    expect(PROVIDER_ORDER).toEqual([
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
      'claude_code',
      'codex',
      'cursor',
      'opencode',
      'grok',
      'qoder',
      'codebuddy',
      'pi',
    ]);
    expect(Object.keys(PROVIDER_PRESETS)).toHaveLength(18);
    for (const type of [
      'claude_code',
      'codex',
      'cursor',
      'opencode',
      'grok',
      'qoder',
      'codebuddy',
      'pi',
    ] as const) {
      expect(PROVIDER_PRESETS[type].defaultProtocol).toBe('local_cli');
      expect(PROVIDER_PRESETS[type].supportsTools).toBe(false);
      expect(PROVIDER_PRESETS[type].defaultEndpoint).toBe('');
    }
  });

  it('uses the real compatible endpoints and protocol for each cloud preset', () => {
    expect(PROVIDER_PRESETS.gemini.defaultEndpoint).toBe(
      'https://generativelanguage.googleapis.com/v1beta/openai/',
    );
    expect(PROVIDER_PRESETS.gemini.defaultProtocol).toBe('open_ai_compatible');
    expect(PROVIDER_PRESETS.qwen.defaultEndpoint).toBe(
      'https://dashscope.aliyuncs.com/compatible-mode/v1',
    );
    expect(PROVIDER_PRESETS.mini_max.defaultEndpoint).toBe('https://api.minimax.io/v1');
    expect(PROVIDER_PRESETS.anthropic.defaultProtocol).toBe('anthropic_compatible');
    expect(PROVIDER_PRESETS.anthropic_compatible.defaultProtocol).toBe('anthropic_compatible');
    expect(PROVIDER_PRESETS.open_ai_compatible.defaultProtocol).toBe('open_ai_compatible');
    for (const preset of Object.values(PROVIDER_PRESETS)) {
      if (preset.providerType === 'custom') expect(preset.defaultEndpoint).toBe('');
    }
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

  const anthropic: ProviderListItem = {
    providerType: 'anthropic',
    displayName: 'Anthropic (Claude)',
    supportsStreaming: true,
    supportsTools: true,
    defaultEndpoint: 'https://api.anthropic.com',
    defaultProtocol: 'anthropic_compatible',
  };

  const custom: ProviderListItem = {
    providerType: 'custom',
    displayName: 'Custom',
    supportsStreaming: true,
    supportsTools: true,
    defaultEndpoint: '',
    defaultProtocol: 'open_ai_compatible',
  };

  it('keeps legacy anthropic visible and fills every missing preset', () => {
    const result = normalizeAiProviders([openAi, anthropic, custom]);

    expect(result.map((p) => p.providerType)).toEqual(PROVIDER_ORDER);
    expect(result.find((p) => p.providerType === 'anthropic')?.displayName).toBe(
      'Anthropic (Claude)',
    );
    expect(result.find((p) => p.providerType === 'gemini')?.defaultModel).toBe('gemini-2.5-pro');
  });

  it('applies frontend DeepSeek/Ollama defaults over old backend entries', () => {
    const backendDeepSeek: ProviderListItem = {
      providerType: 'deep_seek',
      displayName: 'DeepSeek Backend',
      supportsStreaming: false,
      supportsTools: false,
      defaultEndpoint: 'https://old.example.com',
      defaultProtocol: 'open_ai_compatible',
    };
    const backendOllama: ProviderListItem = {
      providerType: 'ollama',
      displayName: 'Old Ollama',
      supportsStreaming: false,
      supportsTools: false,
      defaultEndpoint: 'http://old.example.com',
      defaultProtocol: 'open_ai_compatible',
    };

    const result = normalizeAiProviders([openAi, backendDeepSeek, backendOllama, custom]);
    expect(result.find((p) => p.providerType === 'deep_seek')).toEqual(DEEP_SEEK_PROVIDER);
    expect(result.find((p) => p.providerType === 'ollama')).toEqual(OLLAMA_PROVIDER);
  });

  it('serializes old and new provider ids without changing the JSON shape', () => {
    const legacy: AiModelProfile = {
      id: 'legacy',
      name: 'Legacy Claude',
      providerType: 'anthropic',
      apiKey: 'secret',
      endpoint: 'https://api.anthropic.com',
      model: 'claude-sonnet-4-20250514',
      safetyGate: {
        redactCredentials: true,
        dataEgressLevel: 'strict',
        maxSampleRows: 3,
        dbToolPolicy: 'read_only',
        mcpToolPolicy: 'read_only',
        requireSqlConfirm: true,
        maxContextBytes: 32000,
        maxToolRounds: 30,
        maxToolTokens: 48000,
        maxToolResultBytes: 8192,
        maxToolDurationSecs: 300,
        apiRetries: 2,
      },
    };
    const restored = JSON.parse(JSON.stringify(legacy)) as AiModelProfile;
    expect(restored.providerType).toBe('anthropic');
    expect(restored.apiKey).toBe('secret');
    const gemini = { ...legacy, providerType: 'gemini' as const, model: 'gemini-2.5-pro' };
    expect(JSON.parse(JSON.stringify(gemini)).providerType).toBe('gemini');
  });
});
