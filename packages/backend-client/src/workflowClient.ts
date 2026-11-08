/**
 * The Workflow domain client (W8 刀3c web 收口, contract §6.14).
 *
 * Definition management, synchronous execution and the recorded run history.
 * `getWorkflowDir` is deliberately not a method here — it is desktop-only
 * filesystem semantics (the workflow directory path hands off to the local
 * file manager); the server's definitions are rows, not directories, and the
 * web form reports the gap as `CapabilityUnsupported` through
 * `WEB_CAPABILITIES`. Same ruling as queryAssets' `get_favorites_root`.
 *
 * Request DTOs are the desktop IPC parameter bags, field names verbatim
 * (`workflowId`, `historyId`). Write faces that carry a body (save /
 * save-yaml / execute) fold a `SubmissionToken` in the factory
 * (`{ ...request, submissionToken }`, the queryAssets precedent); the
 * bodyless DELETE faces (delete / clear-runs) carry no token, matching the
 * §6.14 ruling that keeps them on the queryAssets / schemaDiff DELETE
 * precedent.
 *
 * `saveWorkflow`'s domain request is the IPC rail's `{ workflow }` bag (the
 * dashboard `{ dashboard }` / schemaDiff `{ profile }` precedent); the web
 * route's body constructor unfolds it into the contract's flat
 * `WorkflowDefinition & SubmitRequest` PUT body.
 */

import type { BackendGuard } from './guard';
import type { BackendTransport } from './transport';
import type {
  WorkflowDefinition,
  WorkflowExecuteRequest,
  WorkflowExecutionResult,
  WorkflowIdRequest,
  WorkflowListItem,
  WorkflowRunGetRequest,
  WorkflowRunListItem,
  WorkflowRunRecord,
  WorkflowRunsListRequest,
  WorkflowSaveRequest,
  WorkflowYamlSaveRequest,
} from './types/workflow';
import type { SubmissionToken, SubmitRequest } from './types/session';

/** The Workflow slice of `MethodMap`. */
export interface WorkflowMethodMap {
  listWorkflows: { request: Record<string, never>; response: WorkflowListItem[] };
  getWorkflow: { request: WorkflowIdRequest; response: WorkflowDefinition };
  /**
   * HTTP answers `{ yaml }` (scalar-wrap family); the web transport's decoder
   * unwraps it, so the domain shape is the desktop IPC's bare string.
   */
  getWorkflowYaml: { request: WorkflowIdRequest; response: string };
  /** The definition rides inside the `{ workflow }` bag (IPC-shaped domain form). */
  saveWorkflow: { request: WorkflowSaveRequest & SubmitRequest; response: void };
  saveWorkflowYaml: {
    request: WorkflowYamlSaveRequest & SubmitRequest;
    response: WorkflowDefinition;
  };
  /** Unknown ids are an idempotent no-op (HTTP 204 either way). */
  deleteWorkflow: { request: WorkflowIdRequest; response: void };
  executeWorkflow: {
    request: WorkflowExecuteRequest & SubmitRequest;
    response: WorkflowExecutionResult;
  };
  /** Library backend: no-op with identical semantics on the desktop. */
  reloadWorkflows: { request: Record<string, never>; response: void };
  /** Absent `workflowId` = the whole recorded history. */
  listWorkflowRuns: { request: WorkflowRunsListRequest; response: WorkflowRunListItem[] };
  getWorkflowRun: { request: WorkflowRunGetRequest; response: WorkflowRunRecord };
  /**
   * Rows removed. HTTP answers `{ cleared }`; the web transport's decoder
   * unwraps it, so the domain shape is the desktop IPC's bare number.
   */
  clearWorkflowRuns: { request: WorkflowRunsListRequest; response: number };
}

/** The Workflow domain client surface. */
export interface WorkflowClient {
  listWorkflows(): Promise<WorkflowListItem[]>;
  getWorkflow(request: WorkflowIdRequest): Promise<WorkflowDefinition>;
  getWorkflowYaml(request: WorkflowIdRequest): Promise<string>;
  saveWorkflow(request: WorkflowSaveRequest, token: SubmissionToken): Promise<void>;
  saveWorkflowYaml(
    request: WorkflowYamlSaveRequest,
    token: SubmissionToken,
  ): Promise<WorkflowDefinition>;
  deleteWorkflow(request: WorkflowIdRequest): Promise<void>;
  executeWorkflow(
    request: WorkflowExecuteRequest,
    token: SubmissionToken,
  ): Promise<WorkflowExecutionResult>;
  reloadWorkflows(): Promise<void>;
  listWorkflowRuns(request?: WorkflowRunsListRequest): Promise<WorkflowRunListItem[]>;
  getWorkflowRun(request: WorkflowRunGetRequest): Promise<WorkflowRunRecord>;
  clearWorkflowRuns(request?: WorkflowRunsListRequest): Promise<number>;
}

/** Build the Workflow domain client over a transport. */
export function createWorkflowMethods(
  transport: BackendTransport,
  guard: BackendGuard,
): WorkflowClient {
  return {
    listWorkflows: () => guard('listWorkflows', () => transport.call('listWorkflows', {})),

    getWorkflow: (request) => guard('getWorkflow', () => transport.call('getWorkflow', request)),

    getWorkflowYaml: (request) =>
      guard('getWorkflowYaml', () => transport.call('getWorkflowYaml', request)),

    saveWorkflow: (request, submissionToken) =>
      guard('saveWorkflow', () => transport.call('saveWorkflow', { ...request, submissionToken })),

    saveWorkflowYaml: (request, submissionToken) =>
      guard('saveWorkflowYaml', () =>
        transport.call('saveWorkflowYaml', { ...request, submissionToken }),
      ),

    deleteWorkflow: (request) =>
      guard('deleteWorkflow', () => transport.call('deleteWorkflow', request)),

    executeWorkflow: (request, submissionToken) =>
      guard('executeWorkflow', () =>
        transport.call('executeWorkflow', { ...request, submissionToken }),
      ),

    reloadWorkflows: () => guard('reloadWorkflows', () => transport.call('reloadWorkflows', {})),

    listWorkflowRuns: (request) =>
      guard('listWorkflowRuns', () => transport.call('listWorkflowRuns', request ?? {})),

    getWorkflowRun: (request) =>
      guard('getWorkflowRun', () => transport.call('getWorkflowRun', request)),

    clearWorkflowRuns: (request) =>
      guard('clearWorkflowRuns', () => transport.call('clearWorkflowRuns', request ?? {})),
  };
}
