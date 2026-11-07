import { useCallback, useEffect, useMemo, useState } from 'react';
import { useAiStore } from '../../../stores/aiStore';
import {
  AI_SAFETY_PRESETS,
  detectSafetyPreset,
  getDefaultSafetyGate,
  isLocalEndpoint,
  type AiSafetyPresetType,
} from '../../../lib/aiSafetyPresets';
import {
  isLocalCliProvider,
  normalizeAiProviders,
  PROVIDER_PRESETS,
} from '../../../lib/aiProviders';
import type {
  AiModelProfile,
  AiProviderType,
  AiSafetyGateConfig,
  ProviderListItem,
} from '../../../types';

/** Accept the pre-T16 custom protocol value while saving canonical values. */
export function normalizeCustomProtocol(value: unknown): string {
  if (value === 'anthropic') return 'anthropic_compatible';
  if (
    value === 'open_ai_compatible' ||
    value === 'open_ai_responses' ||
    value === 'anthropic_compatible'
  ) {
    return value;
  }
  return 'open_ai_compatible';
}

/** A brand-new profile, seeded from the provider metadata when available. */
export function createEmptyProfile(providers: ProviderListItem[] = []): AiModelProfile {
  const defaultProvider = providers.find((p) => p.providerType === 'open_ai');
  return {
    id: crypto.randomUUID(),
    name: '',
    providerType: 'open_ai',
    apiKey: '',
    endpoint: defaultProvider?.defaultEndpoint ?? '',
    model: defaultProvider?.defaultModel ?? '',
    maxTokens: 200000,
    safetyGate: getDefaultSafetyGate('cloud_strict'),
    isDefault: false,
  };
}

export type ModelProfileSaveOutcome =
  /** Profile persisted (or handed to the caller's save that resolved true). */
  | 'saved'
  /** Nothing saved yet: the high-risk egress confirmation is open. */
  | 'confirming'
  /** Persisting failed — the store's `configError` carries the reason. */
  | 'failed';

interface UseModelProfileDraftOptions {
  /** Profile being edited, `null` when creating a new one. */
  profile: AiModelProfile | null;
  /**
   * Re-seed the draft whenever this flips to true (dialogs pass their `open`
   * flag). Inline hosts (the first-run journey) leave it at `true`.
   */
  open?: boolean;
  onSave: (profile: AiModelProfile) => Promise<boolean>;
  /** Called after a successful save — dialogs close, the journey advances. */
  onSaved?: (profile: AiModelProfile) => void;
}

/**
 * Draft state + actions of an AI model profile — the single implementation
 * behind the settings' `ModelProfileDialog` and the first-run journey's AI
 * step. Extracted so the journey reuses the exact settings form instead of a
 * parallel copy.
 */
export function useModelProfileDraft({
  profile,
  open = true,
  onSave,
  onSaved,
}: UseModelProfileDraftOptions) {
  const {
    providers,
    remoteModels,
    fetchingRemoteModels,
    fetchRemoteModels,
    validateConfig,
    validating,
    configError,
    clearError,
  } = useAiStore();

  const [tab, setTab] = useState<'basic' | 'safety'>('basic');
  const [draft, setDraft] = useState<AiModelProfile>(() => profile ?? createEmptyProfile());
  const [customProtocol, setCustomProtocol] = useState<string>('open_ai_compatible');
  const [manualModelInput, setManualModelInput] = useState(false);
  const [validateOk, setValidateOk] = useState(false);
  const [showHighRiskModal, setShowHighRiskModal] = useState(false);
  const [savingState, setSavingState] = useState(false);

  const providerDefaults = useMemo(() => normalizeAiProviders(providers), [providers]);
  useEffect(() => {
    if (profile) {
      setDraft(profile);
      if (profile.providerType === 'custom' && profile.extra?.protocol) {
        setCustomProtocol(normalizeCustomProtocol(profile.extra.protocol));
      }
    } else {
      setDraft(createEmptyProfile(providerDefaults));
    }
    setTab('basic');
    setValidateOk(false);
    clearError();
    // Re-seed only when the target profile changes or the host re-opens the form.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [profile, open, clearError]);

  const isCustom = draft.providerType === 'custom';
  const isOllama = draft.providerType === 'ollama';
  const isLocalCli = isLocalCliProvider(draft.providerType);
  const localCliPath =
    typeof draft.extra?.executablePath === 'string' ? draft.extra.executablePath : '';
  const localCliWorkspace =
    typeof draft.extra?.workspaceDir === 'string' ? draft.extra.workspaceDir : '';
  const selectedProvider = providerDefaults.find((p) => p.providerType === draft.providerType);

  // Provider metadata loads asynchronously: fill the default endpoint once it
  // arrives so the field is never left blank when the user starts typing a key.
  useEffect(() => {
    const provider = providerDefaults.find((p) => p.providerType === draft.providerType);
    if (!provider) return;
    setDraft((d) => ({
      ...d,
      endpoint: d.endpoint?.trim() ? d.endpoint : (provider.defaultEndpoint ?? ''),
      model: d.model?.trim() ? d.model : (provider.defaultModel ?? ''),
    }));
  }, [draft.providerType, providerDefaults]);

  /** Provider metadata for the picker (falls back to the draft's own type). */
  const providerOptions = useMemo(
    () => providerDefaults.map((p) => ({ value: p.providerType, label: p.displayName })),
    [providerDefaults],
  );

  /** Custom-protocol choices; labels stay in the view layer (i18n keys). */
  const protocolOptions = useMemo(
    () =>
      [
        { value: 'open_ai_compatible', labelKey: 'settings.ai.protocolOpenAiChat' },
        { value: 'open_ai_responses', labelKey: 'settings.ai.protocolOpenAiResponses' },
        { value: 'anthropic_compatible', labelKey: 'settings.ai.protocolAnthropic' },
      ] as const,
    [],
  );

  const modelOptions = useMemo(
    () => remoteModels.map((m) => ({ value: m.id, label: m.displayName || m.id })),
    [remoteModels],
  );

  const updateDraft = useCallback((patch: Partial<AiModelProfile>) => {
    setDraft((d) => ({ ...d, ...patch }));
  }, []);

  const handleProviderChange = useCallback(
    (val: string) => {
      const providerType = val as AiProviderType;
      const provider =
        providerDefaults.find((p) => p.providerType === providerType) ??
        PROVIDER_PRESETS[providerType];
      const isOllama = providerType === 'ollama';
      const isCli = isLocalCliProvider(providerType);
      const protocol = normalizeCustomProtocol(provider?.defaultProtocol);
      setDraft((d) => ({
        ...d,
        providerType,
        apiKey: isOllama ? 'ollama' : '',
        endpoint: isCli ? '' : (provider?.defaultEndpoint ?? ''),
        model: provider?.defaultModel ?? '',
        // A local CLI is a client of a vendor model, not a trusted local
        // endpoint; keep the same strict egress default as cloud API presets.
        safetyGate: getDefaultSafetyGate(isOllama ? 'local_trust' : 'cloud_strict'),
        extra: isCli
          ? { executablePath: '', workspaceDir: '' }
          : providerType === 'custom'
            ? { protocol }
            : undefined,
      }));
      setCustomProtocol(protocol);
      setManualModelInput(false);
      setValidateOk(false);
    },
    [customProtocol, providerDefaults],
  );

  const handleProtocolChange = useCallback((value: string) => {
    setCustomProtocol(value);
    setDraft((d) => ({ ...d, extra: { protocol: value } }));
  }, []);

  const handleLocalCliPathChange = useCallback(
    (key: 'executablePath' | 'workspaceDir', value: string) => {
      setDraft((d) => ({
        ...d,
        extra: { ...(d.extra ?? {}), [key]: value },
      }));
      setValidateOk(false);
    },
    [],
  );

  const handleFetchModels = useCallback(async () => {
    const endpoint = draft.endpoint?.trim();
    const apiKey = draft.apiKey?.trim() || (isOllama ? 'ollama' : '');
    if (!endpoint || (!apiKey && !isOllama)) return;
    const protocol = isCustom
      ? customProtocol
      : normalizeCustomProtocol(selectedProvider?.defaultProtocol);
    await fetchRemoteModels(protocol, endpoint, apiKey || 'ollama');
  }, [
    customProtocol,
    draft.apiKey,
    draft.endpoint,
    fetchRemoteModels,
    isCustom,
    isOllama,
    selectedProvider,
  ]);

  const handleValidate = useCallback(async () => {
    setValidateOk(false);
    const ok = await validateConfig({
      providerType: draft.providerType,
      apiKey: draft.apiKey,
      endpoint: draft.endpoint,
      model: draft.model,
      maxTokens: draft.maxTokens,
      extra: isCustom ? { protocol: customProtocol } : draft.extra,
    });
    if (ok) {
      setValidateOk(true);
      setTimeout(() => setValidateOk(false), 3000);
    }
  }, [customProtocol, draft, isCustom, validateConfig]);

  const applyPreset = useCallback((presetKey: AiSafetyPresetType) => {
    setDraft((d) => ({ ...d, safetyGate: { ...AI_SAFETY_PRESETS[presetKey] } }));
  }, []);

  const updateGate = useCallback(
    <K extends keyof AiSafetyGateConfig>(key: K, val: AiSafetyGateConfig[K]) => {
      setDraft((d) => ({ ...d, safetyGate: { ...d.safetyGate, [key]: val } }));
    },
    [],
  );

  const performSave = useCallback(async (): Promise<boolean> => {
    setSavingState(true);
    const profileToSave: AiModelProfile = {
      ...draft,
      name: draft.name.trim() || draft.model || selectedProvider?.displayName || draft.providerType,
      extra: isCustom ? { protocol: customProtocol } : draft.extra,
    };
    const success = await onSave(profileToSave);
    setSavingState(false);
    if (success) onSaved?.(profileToSave);
    return success;
  }, [customProtocol, draft, isCustom, onSave, onSaved, selectedProvider]);

  /**
   * Save entry point: unrestricted egress on a remote endpoint asks for an
   * explicit confirmation first (`ModelProfileForm` renders that dialog).
   */
  const requestSave = useCallback(async (): Promise<ModelProfileSaveOutcome> => {
    const isEgressUnrestricted = draft.safetyGate.dataEgressLevel === 'unrestricted';
    const isRemote = !isLocalEndpoint(draft.endpoint);
    if (isEgressUnrestricted && isRemote) {
      setShowHighRiskModal(true);
      return 'confirming';
    }
    return (await performSave()) ? 'saved' : 'failed';
  }, [draft.endpoint, draft.safetyGate.dataEgressLevel, performSave]);

  const confirmHighRisk = useCallback(async () => {
    setShowHighRiskModal(false);
    await performSave();
  }, [performSave]);

  const currentPreset = detectSafetyPreset(draft.safetyGate);

  /** Enough input to talk to a provider (Ollama needs no key). */
  const hasCredentials = isOllama || isLocalCli || Boolean(draft.apiKey?.trim());

  return {
    // state
    tab,
    setTab,
    draft,
    updateDraft,
    customProtocol,
    manualModelInput,
    setManualModelInput,
    validateOk,
    showHighRiskModal,
    setShowHighRiskModal,
    savingState,
    validating,
    configError,
    providers,
    providerOptions,
    protocolOptions,
    modelOptions,
    fetchingRemoteModels,
    remoteModels,
    // derived
    isCustom,
    isOllama,
    isLocalCli,
    localCliPath,
    localCliWorkspace,
    selectedProvider,
    currentPreset,
    hasCredentials,
    // actions
    handleProviderChange,
    handleProtocolChange,
    handleLocalCliPathChange,
    handleFetchModels,
    handleValidate,
    applyPreset,
    updateGate,
    requestSave,
    confirmHighRisk,
    performSave,
  };
}

export type ModelProfileDraft = ReturnType<typeof useModelProfileDraft>;
