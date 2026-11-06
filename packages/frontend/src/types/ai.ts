/** AI provider, chat, and analysis types. */

// ── AI Types ──

export type AiProviderType =
  | 'open_ai'
  | 'anthropic'
  | 'deep_seek'
  | 'ollama'
  | 'custom'
  | 'gemini'
  | 'qwen'
  | 'mini_max'
  | 'open_ai_compatible'
  | 'anthropic_compatible'
  | 'claude_code'
  | 'codex'
  | 'cursor'
  | 'opencode'
  | 'grok'
  | 'qoder'
  | 'codebuddy'
  | 'pi';

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

export type AiMemoryCloudPolicy = 'normal_only' | 'allow_internal' | 'disable_cloud';

export interface AiMemorySettings {
  /** Opt-in switch. Older configs omit this and therefore stay disabled. */
  enabled: boolean;
  autoLearn: boolean;
  cloudPolicy: AiMemoryCloudPolicy;
  /** True after the first-use privacy explanation was acknowledged. */
  privacyAcknowledged: boolean;
}

export const defaultAiMemorySettings: AiMemorySettings = {
  enabled: false,
  autoLearn: false,
  cloudPolicy: 'normal_only',
  privacyAcknowledged: false,
};

export interface AiSkillsSettings {
  /** Master switch; skills remain invisible and untoolable when false. */
  enabled: boolean;
  /** One optional directory in addition to `~/.agents/skills`. */
  customDir: string | null;
  /** Off by default: no skill listing/body is injected automatically. */
  autoUse: boolean;
  /** Explicitly selected stable skill ids used while autoUse is false. */
  selected: string[];
}

export const defaultAiSkillsSettings: AiSkillsSettings = {
  enabled: false,
  customDir: null,
  autoUse: false,
  selected: [],
};

export interface AiSkillSummary {
  id: string;
  name: string;
  description: string;
  source: 'user' | 'custom';
  path: string;
  version?: string;
}

export interface AiSkillContent extends AiSkillSummary {
  content: string;
  resources: string[];
  metadata: Record<string, string>;
}

export interface AiSettingsConfig {
  activeProfileId: string;
  profiles: AiModelProfile[];
  globalInstructions: string;
  promptTemplates: PromptTemplate[];
  memory: AiMemorySettings;
  /** Optional for settings returned by pre-T17 hosts. */
  skills?: AiSkillsSettings;
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
  /** Model preselected by the preset; empty for compatible/custom endpoints. */
  defaultModel?: string;
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

export type AiMemoryScopeKind =
  | 'org'
  | 'organization'
  | 'user'
  | 'connection'
  | 'database'
  | 'schema'
  | 'table'
  | 'column';

export type AiMemoryKind =
  | 'preference'
  | 'glossary'
  | 'relationship'
  | 'pattern'
  | 'correction'
  | 'environment'
  | 'negative'
  | 'schema_note';

export type AiMemoryStatus = 'candidate' | 'active' | 'rejected' | 'archived' | 'stale';

export type AiMemorySensitivity = 'normal' | 'internal' | 'secret';

export type AiMemorySource =
  | 'explicit'
  | 'correction'
  | 'usage'
  | 'feedback'
  | 'schema_note'
  | 'import'
  | 'org';

export interface AiMemoryEntry {
  id: string;
  scopeType: AiMemoryScopeKind;
  scopeId: string;
  connectionId: string | null;
  databaseName: string | null;
  schemaName: string | null;
  tableName: string | null;
  columnName: string | null;
  kind: AiMemoryKind;
  subject: string;
  content: string;
  structuredJson: Record<string, unknown> | null;
  source: AiMemorySource;
  sourceRef: string | null;
  confidence: number;
  sensitivity: AiMemorySensitivity;
  status: AiMemoryStatus;
  pinned: boolean;
  /** True when a learned candidate collides with an active slot. */
  conflict?: boolean;
  useCount: number;
  lastUsedAt: string | null;
  expiresAt: string | null;
  createdAt: string;
  updatedAt: string;
}

export interface AiMemoryPage {
  entries: AiMemoryEntry[];
  total: number;
  limit: number;
  offset: number;
}

export interface AiMemoryStats {
  total: number;
  byStatus: Record<string, number>;
  byKind: Record<string, number>;
  byScope: Record<string, number>;
}

export interface AiMemoryCreateInput {
  id?: string;
  scopeType: AiMemoryScopeKind;
  scopeId?: string;
  connectionId?: string;
  databaseName?: string;
  schemaName?: string;
  tableName?: string;
  columnName?: string;
  kind: AiMemoryKind;
  subject: string;
  content: string;
  structuredJson?: Record<string, unknown>;
  source?: AiMemorySource;
  sourceRef?: string;
  confidence?: number;
  sensitivity?: AiMemorySensitivity;
  status?: AiMemoryStatus;
  pinned?: boolean;
  expiresAt?: string;
}

export interface AiMemoryCorrectionInput {
  originalSql: string;
  newSql: string;
  connectionId?: string;
  database?: string;
}

export interface AiMemorySqlSample {
  sql: string;
  observedAt: string;
}

export interface AiMemoryExtractionInput {
  sql: string;
  connectionId?: string;
  database?: string;
  history?: AiMemorySqlSample[];
  windowDays?: number;
  minOccurrences?: number;
  modelOutputJson?: string;
}

export interface AiMemoryStaleInput {
  connectionId?: string;
  database?: string;
  schema?: string;
  table?: string;
  column?: string;
}
export interface AiMemoryUpdateInput {
  subject?: string;
  content?: string;
  kind?: AiMemoryKind;
  structuredJson?: Record<string, unknown>;
  sourceRef?: string;
  confidence?: number;
  sensitivity?: AiMemorySensitivity;
  status?: AiMemoryStatus;
  pinned?: boolean;
  expiresAt?: string;
}

export interface AiMemoryListParams {
  scopeType?: AiMemoryScopeKind;
  scopeId?: string;
  status?: AiMemoryStatus;
  kind?: AiMemoryKind;
  query?: string;
  connectionId?: string;
  limit?: number;
  offset?: number;
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
  /** Persisted terminal state; cancellation/failure survives a reload. */
  status?: AiMessageStatus;
  /** Stable creation time so AppDb replace updates keep message timestamps. */
  createdAt?: string;
  /** Active memories injected for this assistant turn, if any. */
  memoryUsed?: AiMemoryReference[];
}

export interface AiLoadedSkill {
  id?: string;
  name: string;
  source?: string;
  loadedAt: string;
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

export interface AiMemoryCandidatesEvent {
  requestId: string;
  candidates: AiMemoryReference[];
}

export type AiMessageStatus = 'complete' | 'cancelled' | 'failed';

export interface AiSessionMessage {
  id: string;
  role: 'system' | 'user' | 'assistant' | 'tool';
  content: string;
  status: AiMessageStatus;
  metadata?: Record<string, unknown> | null;
  createdAt: string;
}

export interface AiSessionSummary {
  id: string;
  sessionKey: string;
  connectionId?: string | null;
  database?: string | null;
  title: string;
  metadata?: Record<string, unknown> | null;
  messageCount: number;
  preview: string;
  createdAt: string;
  updatedAt: string;
  lastAccessAt: string;
}

export interface AiSessionDetail extends AiSessionSummary {
  messages: AiSessionMessage[];
}

export interface AiSessionMessageInput {
  id?: string;
  role: 'system' | 'user' | 'assistant' | 'tool';
  content: string;
  status?: AiMessageStatus;
  createdAt?: string;
  metadata?: Record<string, unknown>;
}

export interface AiSessionUpsertInput {
  id?: string;
  sessionKey: string;
  connectionId?: string;
  database?: string;
  title?: string;
  metadata?: Record<string, unknown>;
  lastAccessAt?: string;
  messages: AiSessionMessageInput[];
  /** False (default) replaces all messages; true appends and de-duplicates ids. */
  append?: boolean;
}

export interface AiChatSession {
  id: string;
  /** Session isolation key used for localStorage persistence. */
  sessionKey: string;
  /** Connection/database scope used by the backend session list. */
  connectionId?: string | null;
  database?: string | null;
  title?: string;
  /** Prompt template / memory policy metadata retained with the session. */
  metadata?: Record<string, unknown> | null;
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
  /** Skills whose bodies were loaded in this session (session-scoped). */
  loadedSkills?: AiLoadedSkill[];
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
