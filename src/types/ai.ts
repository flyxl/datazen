/** AI provider, chat, and analysis types. */

// ── AI Types ──

export type AiProviderType = 'open_ai' | 'deep_seek' | 'ollama' | 'custom';

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

export interface AiSettingsConfig {
  activeProfileId: string;
  profiles: AiModelProfile[];
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

export interface AiChatMessage {
  /** Unique message identifier for stable React keys. */
  id?: string;
  role: 'system' | 'user' | 'assistant' | 'tool';
  content: string;
  reasoning?: string;
  questions?: AiQuestion[];
  toolCalls?: AiToolCall[];
  toolCallId?: string;
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
  requestId: string | null;
}

export interface StreamChunkPayload {
  requestId: string;
  content: string;
  reasoning?: string;
  done: boolean;
  usage?: { promptTokens: number; completionTokens: number; totalTokens: number };
  toolCalls?: AiToolCall[];
}

export interface StreamErrorPayload {
  requestId: string;
  error: string;
}
