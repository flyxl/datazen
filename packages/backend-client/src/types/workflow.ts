/**
 * Workflow wire vocabulary (contract §6.14): definitions, the synchronous
 * execution receipt, and the recorded run history.
 *
 * The shapes are the shared Rust vocabulary (`datazen::workflow::domain` —
 * `WorkflowDefinition`, `WorkflowListItem`, `WorkflowExecutionResult`,
 * `WorkflowRunListItem`, `WorkflowRunRecord`) serialized camelCase; both the
 * desktop IPC command rail and the server HTTP surface answer with this same
 * shape, which is what lets one client type serve both. These types used to
 * live as frontend-local definitions (`packages/frontend/src/types`); the
 * W8 刀3c web 收口 lifted them here wholesale so the domain vocabulary has one
 * source. No tagged-enum folding is needed on this surface — the Rust step
 * enum serializes with a `type` tag that already matches the flat TS shape.
 *
 * Name mapping against the frontend's pre-lift local names (the frontend keeps
 * re-export aliases, so existing import paths are unaffected):
 * - `WorkflowRunListItem` was `HistoryListItem`
 * - `WorkflowRunRecord` was `HistoryEntry`
 *
 * Two scalar responses ride wrapped on the HTTP surface and are unwrapped by
 * the web transport's decoder before they reach the domain client, so the
 * domain shape stays the desktop IPC's bare scalar:
 * - `getWorkflowYaml` answers `{ yaml }` over HTTP, `string` here;
 * - `clearWorkflowRuns` answers `{ cleared }` over HTTP, `number` here.
 *
 * `getWorkflowDir` stays OFF this surface by contract: it is desktop-only
 * filesystem semantics (the workflow directory path hands off to the local
 * file manager), and the server stores rows, not directories — the same
 * ruling as queryAssets' `get_favorites_root`.
 */

// ── definitions ─────────────────────────────────────────────────────────────

export interface WorkflowVariable {
  name: string;
  type: string; // 'string' | 'number' | 'connection'
  description: string;
  required?: boolean;
  default?: unknown;
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

export type WorkflowBlockType = 'sessionBlock' | 'transactionBlock';

/**
 * Block-level retry. The unit is always the whole block — a block never
 * resumes from the middle — so there is deliberately no per-step count.
 */
export interface WorkflowBlockRetry {
  block: number;
}

export interface WorkflowBlock {
  id: string;
  type: WorkflowBlockType;
  /** Block target, level 2 of the step > block > workflow > profile chain. */
  connection?: string;
  /** Block target (multi-db connections). */
  database?: string;
  /** Run this block concurrently with *other* blocks, never with itself. */
  parallel?: boolean;
  steps: WorkflowStep[];
  /** transactionBlock only. */
  retry?: WorkflowBlockRetry;
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
  /** v2 resource blocks; each block holds its own steps and target. */
  blocks?: WorkflowBlock[];
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

// ── list projection & execution receipt ─────────────────────────────────────

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

// ── recorded run history (contract names; frontend pre-lift names above) ────

/** One recorded workflow run, without the payload. Was `HistoryListItem`. */
export interface WorkflowRunListItem {
  id: string;
  workflowId: string;
  workflowName: string;
  success: boolean;
  totalTimeMs: number;
  createdAt: string;
}

/** One recorded workflow run, with the variables and the full result. Was `HistoryEntry`. */
export interface WorkflowRunRecord {
  id: string;
  workflowId: string;
  workflowName: string;
  variables: Record<string, unknown>;
  result: WorkflowExecutionResult;
  createdAt: string;
}

// ── requests ────────────────────────────────────────────────────────────────

/**
 * `getWorkflow` / `getWorkflowYaml` / `deleteWorkflow`: which definition. The
 * id field name is the desktop IPC parameter bag's, kept verbatim.
 */
export interface WorkflowIdRequest {
  workflowId: string;
}

/** `saveWorkflowYaml`: the YAML text to parse-and-persist. */
export interface WorkflowYamlSaveRequest {
  yaml: string;
}

/**
 * `saveWorkflow`: the definition rides inside the `{ workflow }` bag — the
 * desktop IPC parameter shape (and the dashboard `{ dashboard }` / schemaDiff
 * `{ profile }` domain precedent). The web route unfolds it into the flat
 * `WorkflowDefinition & SubmitRequest` PUT body on its side of the port.
 */
export interface WorkflowSaveRequest {
  workflow: WorkflowDefinition;
}

/**
 * `executeWorkflow`: one synchronous run of a persisted definition. Execution
 * is a single await — a long-running workflow past the transport timeout is
 * the caller's retry policy, not a job here (§6.14 ruling).
 */
export interface WorkflowExecuteRequest {
  workflowId: string;
  variables: Record<string, unknown>;
  /** Overrides the definition's default connection for this run. */
  connectionId?: string;
}

/**
 * `listWorkflowRuns` / `clearWorkflowRuns`: present = one workflow's runs,
 * absent = the whole recorded history. `null` and omission mean the same
 * thing (the desktop rail historically sent an explicit `null`).
 */
export interface WorkflowRunsListRequest {
  workflowId?: string | null;
}

/**
 * `getWorkflowRun`: which recorded run. The field name is the desktop IPC
 * parameter bag's `historyId`, kept verbatim — the contract renamed the noun
 * (run, not history) but not the parameter.
 */
export interface WorkflowRunGetRequest {
  historyId: string;
}
