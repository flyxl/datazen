/**
 * The Schema-diff domain client (form-split F3, contract §6.13).
 *
 * Profiles, single-table comparisons, prepare/deploy job submission and the
 * job read-back family. The Rust side serializes plan requirements as tagged
 * enums; the client normalizes them into the discriminated union
 * `PlanRequirement` on the way in (`getJobDetails`, `verifyRecovery`) and
 * unfolds them on the way out (`executeDeploy` with a plan body), so callers
 * only ever see the normalized plan shape.
 *
 * `get_migration_run` / `list_migration_runs` are deliberately NOT methods
 * here: the contract keeps the migration-run read face in the sync domain
 * (§6.13 remark), where those commands converge with the data-sync family.
 */

import type { BackendGuard } from './guard';
import type { BackendTransport } from './transport';
import type {
  PlanRequirement,
  SchemaDiffCompareRequest,
  SchemaDiffDeployCancelRequest,
  SchemaDiffDeployRequest,
  SchemaDiffJobAccepted,
  SchemaDiffJobDetails,
  SchemaDiffJobDetailsRequest,
  SchemaDiffJobsListRequest,
  SchemaDiffPlan,
  SchemaDiffPrepareEnvelope,
  SchemaDiffPreparePlanRequest,
  SchemaDiffPrepareUnifiedRequest,
  SchemaDiffProfile,
  SchemaDiffProfileDeleteRequest,
  SchemaDiffProfileSaveRequest,
  SchemaDiffRecoveryVerificationRequest,
  TableSchemaDiff,
} from './types/schemaDiff';
import type { JobView } from './types/jobs';
import type { SubmissionToken, SubmitRequest } from './types/session';

// ── tagged-enum folding (wire ↔ normalized) ─────────────────────────────────

type PlanRequirementWire =
  | { backfill: { table: string; column: string; reason: string } }
  | { unsupported: { operation: string; reason: string } };

type SchemaDiffPlanWire = Omit<SchemaDiffPlan, 'requirements'> & {
  requirements?: PlanRequirementWire[];
};

type SchemaDiffPrepareEnvelopeWire = Omit<SchemaDiffPrepareEnvelope, 'plan'> & {
  plan: SchemaDiffPlanWire;
};

/**
 * The wire-level job details (tagged requirements). Both transports (desktop
 * IPC and server HTTP) answer with this shape; the domain client folds it.
 * Exported for hosts that bind their own facade (tests, tooling).
 */
export type SchemaDiffJobDetailsWire = Omit<SchemaDiffJobDetails, 'prepared'> & {
  prepared: SchemaDiffPrepareEnvelopeWire | null;
};

function normalizeRequirement(raw: PlanRequirementWire): PlanRequirement {
  if ('backfill' in raw) {
    const { table, column, reason } = raw.backfill;
    return { kind: 'Backfill', table, column, reason };
  }
  const { operation, reason } = raw.unsupported;
  const dot = operation.indexOf('.');
  if (dot > 0) {
    return {
      kind: 'Unsupported',
      table: operation.slice(0, dot),
      column: operation.slice(dot + 1),
      reason,
    };
  }
  return { kind: 'Unsupported', table: operation, column: '', reason };
}

function denormalizeRequirement(requirement: PlanRequirement): PlanRequirementWire {
  if (requirement.kind === 'Backfill') {
    return {
      backfill: {
        table: requirement.table,
        column: requirement.column,
        reason: requirement.reason,
      },
    };
  }
  const operation = requirement.column
    ? `${requirement.table}.${requirement.column}`
    : requirement.table;
  return { unsupported: { operation, reason: requirement.reason } };
}

function normalizePlan(plan: SchemaDiffPlanWire): SchemaDiffPlan {
  return {
    ...plan,
    requirements: (plan.requirements ?? []).map(normalizeRequirement),
  };
}

function denormalizePlan(plan: SchemaDiffPlan): SchemaDiffPlanWire {
  return {
    ...plan,
    requirements: (plan.requirements ?? []).map(denormalizeRequirement),
  };
}

function normalizeJobDetails(raw: SchemaDiffJobDetailsWire): SchemaDiffJobDetails {
  return {
    ...raw,
    prepared: raw.prepared ? { ...raw.prepared, plan: normalizePlan(raw.prepared.plan) } : null,
  };
}

/** Rollback counts derived from a plan's statement list and completeness report. */
export function rollbackCompletenessCounts(plan: SchemaDiffPlan): {
  complete: number;
  missing: number;
} {
  const total = plan.statements.length;
  const missing = plan.rollbackCompleteness.missing.length;
  return { complete: total - missing, missing };
}

// ── MethodMap slice ─────────────────────────────────────────────────────────

/** The Schema-diff slice of `MethodMap`. */
export interface SchemaDiffMethodMap {
  listSchemaDiffProfiles: { request: Record<string, never>; response: SchemaDiffProfile[] };
  saveSchemaDiffProfile: {
    request: SchemaDiffProfileSaveRequest & SubmitRequest;
    response: void;
  };
  deleteSchemaDiffProfile: {
    request: SchemaDiffProfileDeleteRequest & SubmitRequest;
    response: void;
  };
  compareTableSchemas: { request: SchemaDiffCompareRequest; response: TableSchemaDiff };
  prepareSchemaDiffPlan: {
    request: SchemaDiffPreparePlanRequest & SubmitRequest;
    response: SchemaDiffJobAccepted;
  };
  prepareSchemaUnifiedPlan: {
    request: SchemaDiffPrepareUnifiedRequest & SubmitRequest;
    response: SchemaDiffJobAccepted;
  };
  executeSchemaDiffDeploy: {
    request: SchemaDiffDeployRequest & SubmitRequest;
    response: SchemaDiffJobAccepted;
  };
  listSchemaDiffJobs: { request: SchemaDiffJobsListRequest; response: JobView[] };
  getSchemaDiffJobDetails: {
    request: SchemaDiffJobDetailsRequest;
    response: SchemaDiffJobDetails;
  };
  cancelSchemaDiffDeploy: {
    request: SchemaDiffDeployCancelRequest & SubmitRequest;
    response: boolean;
  };
  verifySchemaDiffJobRecovery: {
    request: SchemaDiffRecoveryVerificationRequest & SubmitRequest;
    response: SchemaDiffJobDetails;
  };
}

/** The Schema-diff domain client surface. */
export interface SchemaDiffClient {
  listSchemaDiffProfiles(): Promise<SchemaDiffProfile[]>;
  saveSchemaDiffProfile(
    request: SchemaDiffProfileSaveRequest,
    token: SubmissionToken,
  ): Promise<void>;
  deleteSchemaDiffProfile(
    request: SchemaDiffProfileDeleteRequest,
    token: SubmissionToken,
  ): Promise<void>;
  compareTableSchemas(request: SchemaDiffCompareRequest): Promise<TableSchemaDiff>;
  prepareSchemaDiffPlan(
    request: SchemaDiffPreparePlanRequest,
    token: SubmissionToken,
  ): Promise<SchemaDiffJobAccepted>;
  prepareSchemaUnifiedPlan(
    request: SchemaDiffPrepareUnifiedRequest,
    token: SubmissionToken,
  ): Promise<SchemaDiffJobAccepted>;
  executeSchemaDiffDeploy(
    request: SchemaDiffDeployRequest,
    token: SubmissionToken,
  ): Promise<SchemaDiffJobAccepted>;
  listSchemaDiffJobs(request: SchemaDiffJobsListRequest): Promise<JobView[]>;
  getSchemaDiffJobDetails(request: SchemaDiffJobDetailsRequest): Promise<SchemaDiffJobDetails>;
  cancelSchemaDiffDeploy(
    request: SchemaDiffDeployCancelRequest,
    token: SubmissionToken,
  ): Promise<boolean>;
  verifySchemaDiffJobRecovery(
    request: SchemaDiffRecoveryVerificationRequest,
    token: SubmissionToken,
  ): Promise<SchemaDiffJobDetails>;
}

// ── client factory ──────────────────────────────────────────────────────────

/**
 * Fold a wire-level job-details payload into the normalized shape. Both
 * transports (desktop IPC and server HTTP) answer with the tagged shape
 * (`SchemaDiffJobDetailsWire`, exported above); the domain client folds it —
 * hosts that bind their own facade (tests, tooling) can fold it through the
 * same function instead of copying the rule.
 */
export function normalizeSchemaDiffJobDetailsWire(raw: unknown): SchemaDiffJobDetails {
  return normalizeJobDetails(raw as SchemaDiffJobDetailsWire);
}

/** Build the Schema-diff domain client over a transport. */
export function createSchemaDiffMethods(
  transport: BackendTransport,
  guard: BackendGuard,
): SchemaDiffClient {
  return {
    listSchemaDiffProfiles: () =>
      guard('listSchemaDiffProfiles', () => transport.call('listSchemaDiffProfiles', {})),

    saveSchemaDiffProfile: (request, submissionToken) =>
      guard('saveSchemaDiffProfile', () =>
        transport.call('saveSchemaDiffProfile', { ...request, submissionToken }),
      ),

    deleteSchemaDiffProfile: (request, submissionToken) =>
      guard('deleteSchemaDiffProfile', () =>
        transport.call('deleteSchemaDiffProfile', { ...request, submissionToken }),
      ),

    compareTableSchemas: (request) =>
      guard('compareTableSchemas', () => transport.call('compareTableSchemas', request)),

    prepareSchemaDiffPlan: (request, submissionToken) =>
      guard('prepareSchemaDiffPlan', () =>
        transport.call('prepareSchemaDiffPlan', { ...request, submissionToken }),
      ),

    prepareSchemaUnifiedPlan: (request, submissionToken) =>
      guard('prepareSchemaUnifiedPlan', () =>
        transport.call('prepareSchemaUnifiedPlan', { ...request, submissionToken }),
      ),

    executeSchemaDiffDeploy: (request, submissionToken) => {
      // The plan body rides the wire with its requirements unfolded back into
      // the tagged enum; a planId/selectionRevision reference passes through
      // untouched. The unfold is a pure re-labelling of the same fields, so
      // the single cast below is the boundary where the wire shape takes over
      // from the normalized one.
      const wireRequest =
        request.plan === undefined
          ? { ...request, submissionToken }
          : ({
              ...request,
              plan: denormalizePlan(request.plan),
              submissionToken,
            } as SchemaDiffDeployRequest & SubmitRequest);
      return guard('executeSchemaDiffDeploy', () =>
        transport.call('executeSchemaDiffDeploy', wireRequest),
      );
    },

    listSchemaDiffJobs: (request) =>
      guard('listSchemaDiffJobs', () => transport.call('listSchemaDiffJobs', request)),

    getSchemaDiffJobDetails: (request) =>
      guard('getSchemaDiffJobDetails', async () =>
        normalizeSchemaDiffJobDetailsWire(await transport.call('getSchemaDiffJobDetails', request)),
      ),

    cancelSchemaDiffDeploy: (request, submissionToken) =>
      guard('cancelSchemaDiffDeploy', () =>
        transport.call('cancelSchemaDiffDeploy', { ...request, submissionToken }),
      ),

    verifySchemaDiffJobRecovery: (request, submissionToken) =>
      guard('verifySchemaDiffJobRecovery', async () =>
        normalizeSchemaDiffJobDetailsWire(
          await transport.call('verifySchemaDiffJobRecovery', { ...request, submissionToken }),
        ),
      ),
  };
}
