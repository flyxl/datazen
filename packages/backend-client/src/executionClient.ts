/**
 * The Execution domain client (form-split F3).
 *
 * Everything that *runs* SQL and observes the run: execute inside a session,
 * execute target-bound, read an execution view, and the one and only cancel
 * path. `cancelExecution` reuses the Job domain's receipt decoder because the
 * wire shape is shared — a cancel receipt is a cancel receipt whichever
 * resource it addresses.
 */

import { ApiError } from './errors';
import type { BackendGuard } from './guard';
import { parseCancelReceipt } from './jobClient';
import type { BackendTransport } from './transport';
import type {
  CancelReceipt,
  ExecuteAtTargetRequest,
  ExecuteInSessionRequest,
  ExecutionReceipt,
  ExecutionView,
  Id,
  SubmitRequest,
  SubmissionToken,
} from './types';

/** The Execution slice of `MethodMap`. */
export interface ExecutionMethodMap {
  executeInSession: {
    request: ExecuteInSessionRequest & SubmitRequest;
    response: ExecutionReceipt;
  };
  executeAtTarget: {
    request: ExecuteAtTargetRequest & SubmitRequest;
    response: ExecutionReceipt;
  };
  getExecution: { request: { executionId: string }; response: ExecutionView };
  cancelExecution: { request: SubmitRequest & { executionId: string }; response: CancelReceipt };
}

/** The Execution domain client surface. */
export interface ExecutionClient {
  executeInSession(
    request: ExecuteInSessionRequest,
    token: SubmissionToken,
  ): Promise<ExecutionReceipt>;
  executeAtTarget(
    request: ExecuteAtTargetRequest,
    token: SubmissionToken,
  ): Promise<ExecutionReceipt>;
  getExecution(executionId: Id): Promise<ExecutionView>;
  /**
   * Ask the backend to cancel an execution. This is the *only* path that may
   * cancel SQL: ending the event subscription is not cancelling, and keeping
   * them in separate types is what keeps that true.
   */
  cancelExecution(executionId: Id, token: SubmissionToken): Promise<CancelReceipt>;
}

/** Build the Execution domain client over a transport. */
export function createExecutionMethods(
  transport: BackendTransport,
  guard: BackendGuard,
): ExecutionClient {
  return {
    executeInSession: (request, submissionToken) =>
      guard('executeInSession', () =>
        transport.call('executeInSession', { ...request, submissionToken }),
      ),

    executeAtTarget: (request, submissionToken) =>
      guard('executeAtTarget', () =>
        transport.call('executeAtTarget', { ...request, submissionToken }),
      ),

    getExecution: (executionId) =>
      guard('getExecution', () => transport.call('getExecution', { executionId })),

    cancelExecution: async (executionId, submissionToken) => {
      const raw = await guard('cancelExecution', () =>
        transport.call('cancelExecution', { executionId, submissionToken }),
      );
      const receipt = parseCancelReceipt(raw);
      if (receipt.executionId !== executionId) {
        throw new ApiError('ServiceUnavailable', 'Cancel receipt belongs to another execution.');
      }
      return receipt;
    },
  };
}

/** Compile-time: every Execution map key is served by this domain client. */
type Assert<T extends true> = T;
export type ExecutionClientCoversMap = Assert<
  keyof ExecutionMethodMap extends keyof ExecutionClient ? true : false
>;
