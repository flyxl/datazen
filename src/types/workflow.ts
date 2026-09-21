/** Workflow definition and execution types. */

// ── Workflow types ──

export interface WorkflowVariable {
  name: string;
  type: string; // 'string' | 'number' | 'connection'
  description: string;
  required?: boolean;
  default?: unknown;
}

export type CommandCategory = 'query' | 'mutate' | 'admin' | 'observe' | 'pubSub' | 'stream' | 'io';

export type CommandAccessLevel = 'read' | 'write' | 'highRisk';

/**
 * Declarative native save dialog attached to a command (host thin shell).
 * The command returns `{ fileNameField, dataBase64Field }`; an interactive
 * `execute_driver_command` call pops the native save dialog, writes the bytes
 * and replaces the result with `{ resultPathField: savedPath | null }`.
 */
export interface DriverSaveDialogSpec {
  fileNameField: string;
  dataBase64Field: string;
  filterName: string;
  extensions: string[];
  resultPathField: string;
}

export interface DriverCommandMetadata {
  category: CommandCategory;
  risk?: CommandAccessLevel | null;
  workflow: boolean;
  ui: boolean;
  deprecated: boolean;
  replacedBy?: string | null;
  requiresConnection: boolean;
  saveDialog?: DriverSaveDialogSpec | null;
}

export interface DriverCommandDefinition {
  id: string;
  name: string;
  description?: string | null;
  inputSchema: Record<string, unknown>;
  outputSchema?: Record<string, unknown> | null;
  permissions: string[];
  metadata: DriverCommandMetadata;
}

export type WorkflowStepType =
  | 'query'
  | 'command'
  | 'ai'
  | 'condition'
  | 'foreach'
  | 'merge'
  | 'transform';

export interface ErrorHandlingConfig {
  strategy: 'abort' | 'skip' | 'fallback';
  fallbackSteps?: WorkflowStep[];
}

export interface WorkflowStep {
  type: WorkflowStepType;
  id: string;
  sql?: string;
  connection?: string;
  database?: string;
  command?: string;
  input?: Record<string, unknown>;
  prompt?: string;
  timeoutSecs?: number;
  onError?: ErrorHandlingConfig;
  if?: string;
  thenSteps?: WorkflowStep[];
  elseSteps?: WorkflowStep[];
  items?: string;
  asVar?: string;
  steps?: WorkflowStep[];
  maxIterations?: number;
  // merge step
  sources?: MergeSource[];
  columns?: string[];
  // transform step
  from?: string;
  addColumns?: TransformColumn[];
  filter?: string;
  sortBy?: string;
  offset?: number;
  limit?: number;
}

export interface MergeSource {
  source: string;
  columns?: Record<string, string>;
  add?: Record<string, unknown>;
}

export interface TransformColumn {
  name: string;
  expr: string;
}

export interface WorkflowOutput {
  format: string;
  template?: string;
}

export interface WorkflowDefinition {
  id: string;
  name: string;
  description: string;
  version?: string;
  author?: string;
  variables: WorkflowVariable[];
  /** Default connection inherited by data-operation steps. */
  connection?: string;
  /** Default database inherited by data-operation steps (multi-db connections). */
  database?: string;
  steps: WorkflowStep[];
  output?: WorkflowOutput;
  timeoutSecs?: number;
  errorHandling?: ErrorHandlingConfig;
  schedule?: WorkflowSchedule;
  /** `user` | `dashboardHidden` — hidden workflows are dashboard-owned SQL bindings. */
  visibility?: 'user' | 'dashboardHidden';
}

export interface WorkflowSchedule {
  enabled: boolean;
  interval_secs?: number;
  intervalSecs?: number;
}

export interface WorkflowListItem {
  id: string;
  name: string;
  description: string;
  variables: WorkflowVariable[];
  scheduled?: boolean;
}

export type StepStatus = 'success' | 'failed' | 'skipped' | 'timed_out';

export interface StepExecutionResult {
  stepId: string;
  stepType: string;
  status: StepStatus;
  result?: Record<string, unknown>;
  executionTimeMs: number;
  error?: string;
  connectionName?: string;
  sqlExecuted?: string;
}

export interface WorkflowExecutionResult {
  success: boolean;
  finalOutput: string;
  steps: StepExecutionResult[];
  totalTimeMs: number;
  error?: string;
}

export interface HistoryListItem {
  id: string;
  workflowId: string;
  workflowName: string;
  success: boolean;
  totalTimeMs: number;
  createdAt: string;
}

export interface HistoryEntry {
  id: string;
  workflowId: string;
  workflowName: string;
  variables: Record<string, unknown>;
  result: WorkflowExecutionResult;
  createdAt: string;
}
