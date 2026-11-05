/** AI provider, chat, and analysis types. */

// ── AI Types ──

export type AiProviderType = 'open_ai' | 'deep_seek' | 'ollama' | 'custom';

export type AiAgentMode = 'ask' | 'agent';

export type AiDataEgressLevel = 'strict' | 'sample_masked' | 'unrestricted';

export type AiToolPermissionPolicy = 'disabled' | 'read_only' | 'require_confirm' | 'unrestricted';

export interface AiSafetyGateConfig {
  redactCredentials: boolean;
  dataEgressLevel: AiDataEgressLevel;
  maxSampleRows: number;
  dbToolPolicy: AiToolPermissionPolicy;
  mcpToolPolicy: AiToolPermissionPolicy;
  requireSqlConfirm: boolean;
  maxContextBytes: number;
  maxToolRounds: number;
  maxToolTokens: number;
  maxToolResultBytes: number;
  maxToolDurationSecs: number;
  apiRetries: number;
}

export interface AiModelProfile {
  id: string;
  name: string;
  providerType: AiProviderType;
  apiKey?: string;
  endpoint?: string;
  model: string;
  maxTokens?: number;
  extra?: Record<string, unknown>;
  safetyGate: AiSafetyGateConfig;
  isDefault?: boolean;
}

export interface PromptTemplate {
  id: string;
  name: string;
  content: string;
  databaseTypes: string[];
  isDefault: boolean;
}

export interface AiSettingsConfig {
  activeProfileId: string;
  profiles: AiModelProfile[];
  globalInstructions: string;
  promptTemplates: PromptTemplate[];
}

export interface AiProviderConfig {
  providerType: AiProviderType;
  apiKey?: string;
  endpoint?: string;
  model: string;
  maxTokens?: number;
  extra?: Record<string, unknown>;
}

export interface ModelInfo {
  id: string;
  displayName: string;
  contextWindow: number;
  supportsStreaming: boolean;
  supportsTools: boolean;
}

export interface ProviderListItem {
  providerType: AiProviderType;
  displayName: string;
  supportsStreaming: boolean;
  supportsTools: boolean;
  defaultEndpoint: string;
  defaultProtocol: string;
}

export interface DiagnosisResult {
  explanation: string;
  suggestedSql: string | null;
  changes: string[];
}

export interface ExplainAnalysis {
  summary: string;
  bottlenecks: Bottleneck[];
  suggestions: ExplainSuggestion[];
}

export interface Bottleneck {
  node: string;
  description: string;
  severity: 'high' | 'medium' | 'low';
}

export interface ExplainSuggestion {
  description: string;
  sql: string | null;
  impact: string;
}

export interface AiQuestionOption {
  id: string;
  label: string;
}

export interface AiQuestion {
  id: string;
  prompt: string;
  options: AiQuestionOption[];
  allowMultiple?: boolean;
}

export interface AiToolCall {
  id: string;
  name: string;
  arguments: string;
}

export interface AiToolResult {
  toolCallId: string;
  content: string;
}

/** Result excerpt frozen into a chat turn; the Rust side re-caps it. */
export interface AiChatResultPreview {
  columns: string[];
  rows: (string | number | boolean | null)[][];
  rowCount?: number;
  elapsedMs?: number;
}

/** Auto-attached editor/result facts, frozen when the user sends. */
export interface AiChatContextSnapshot {
  editorSql?: string;
  selection?: string;
  lastError?: string;
  resultPreview?: AiChatResultPreview;
}

/** Non-secret citation metadata for one memory injected into a chat run. */
export interface AiMemoryReference {
  /** Stable answer reference (`M1`, `M2`, ...). */
  reference: string;
  id: string;
  subject: string;
  kind: string;
  scope: string;
  score: number;
}

export interface AiChatMessage {
  /** Unique message identifier for stable React keys. */
  id?: string;
  role: 'system' | 'user' | 'assistant' | 'tool';
  content: string;
  reasoning?: string;
  questions?: AiQuestion[];
  toolCalls?: AiToolCall[];
  toolCallId?: string;
  /** Terminal marker: the run ended because the user cancelled it. */
  cancelled?: boolean;
  /** Active memories injected for this assistant turn, if any. */
  memoryUsed?: AiMemoryReference[];
}

export interface AiChatInjectionSnapshot {
  templateId?: string | null;
  templateName?: string | null;
  templateHash?: string | null;
  hasConnectionNotes: boolean;
  hasGlobalInstructions: boolean;
}

export interface AiChatInjectionEvent {
  requestId: string;
  snapshot: AiChatInjectionSnapshot;
}

export interface AiChatSession {
  id: string;
  /** Session isolation key used for localStorage persistence. */
  sessionKey: string;
  messages: AiChatMessage[];
  isStreaming: boolean;
  streamContent: string;
  streamReasoning: string;
  /** Qualified MCP tool name (mcp/server/tool) shown during streaming tool execution. */
  streamMcpToolName: string | null;
  /** Host-provided egress scope from the first chunk of this run. */
  egressSummary?: string | null;
  /** Accumulated memory references for the in-flight run. */
  streamMemoryUsed?: AiMemoryReference[] | null;
  /** Latest run's memory references; reset on send/clear/new session. */
  memoryUsed?: AiMemoryReference[] | null;
  /** Host-provided prompt material snapshot for this run's chips. */
  injection?: AiChatInjectionSnapshot | null;
  requestId: string | null;
  /**
   * Non-secret attribution of the last run started from this session: which
   * target it claimed, and (for a shared run) the grant id. Never the runtime
   * handle itself.
   */
  attribution?:
    | { kind: 'independent' }
    | { kind: 'editorGrant'; grantId: string; editorSessionId: string }
    | null;
}

export interface StreamChunkPayload {
  requestId: string;
  content: string;
  reasoning?: string;
  done: boolean;
  /** `done && cancelled` is the terminal chunk of a cancelled run. */
  cancelled?: boolean;
  usage?: { promptTokens: number; completionTokens: number; totalTokens: number };
  toolCalls?: AiToolCall[];
  /** FR-13 egress scope; the host sends it on the first chunk only. */
  egressSummary?: string;
  /** Active memories injected into this run; the host sends it once up front. */
  memoryUsed?: AiMemoryReference[];
}

export interface StreamErrorPayload {
  requestId: string;
  error: string;
}
