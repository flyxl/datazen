import { invokeBackend } from './helpers.js';
import { browser } from '@wdio/globals';
import type { ExecutionReceipt, ExecutionView, SubmissionToken } from '@datazen/backend-client';

export interface CatalogMetadata {
  database: string;
  schemas: string[];
  relations: Array<{
    ref: { database: string; schema: string | null; name: string };
    kind: string;
    rowCount: number | null;
  }>;
}

export async function listDatabases(target: { dbSessionId: string }): Promise<string[]> {
  const result = await invokeBackend<{ data: { databases: string[] } }>('execute_driver_command', {
    request: { dbSessionId: target.dbSessionId, command: 'list_databases', input: {} },
  });
  return result.data.databases;
}

export async function readCatalog(target: {
  dbSessionId: string;
  database: string;
  schema?: string | null;
}): Promise<CatalogMetadata> {
  const result = await invokeBackend<{ data: CatalogMetadata }>('execute_driver_command', {
    request: {
      dbSessionId: target.dbSessionId,
      command: 'list_catalog',
      input: { database: target.database, schema: target.schema ?? null },
    },
  });
  return result.data;
}

const SAFE_API_ERROR_CODES = new Set([
  'InvalidArgument',
  'TargetRequired',
  'TargetConflict',
  'TargetUnsupported',
  'SessionNotFound',
  'SessionLost',
  'RuntimeEpochMismatch',
  'ContextConflict',
  'PermissionDenied',
  'ResourceBusy',
  'QueueFull',
  'TransactionResolutionRequired',
  'CapabilityUnsupported',
  'UnsupportedPlan',
  'EndpointOverlap',
  'SessionQuotaExceeded',
  'IdempotencyExpired',
  'RollbackFailed',
  'CleanupFailed',
  'OutcomeUnknown',
  'PlanStale',
  'SourceChanged',
  'TargetConflictRows',
  'IdempotencyConflict',
  'Unauthenticated',
  'NotFound',
  'PayloadTooLarge',
  'QuotaExceeded',
  'RateLimited',
  'ServiceUnavailable',
  'ConfigRevisionMismatch',
]);

export class RuntimeMetadataProbeError extends Error {
  constructor(
    readonly phase: string,
    readonly reason: string,
  ) {
    super(`Runtime metadata probe failed at ${phase}: ${reason}`);
  }
}

function safeApiErrorCode(error: unknown): string {
  let code: string | undefined;
  if (error && typeof error === 'object' && 'code' in error) {
    const candidate = (error as { code?: unknown }).code;
    if (typeof candidate === 'string') code = candidate;
  }
  if (!code && error instanceof Error) {
    // invokeBackend converts Tauri's already-sanitized public error payload to
    // "<code>: <message>". Extract only the closed-set code; never emit message text.
    code = /^([A-Za-z][A-Za-z0-9]*):/.exec(error.message)?.[1];
  }
  if (!code) return 'IPC_ERROR';
  if (SAFE_API_ERROR_CODES.has(code)) return code;
  const wireCode = [...SAFE_API_ERROR_CODES].find(
    (known) => `${known[0].toLowerCase()}${known.slice(1)}` === code,
  );
  return wireCode ?? 'IPC_ERROR';
}

async function probeCall<T>(phase: string, operation: () => Promise<T>): Promise<T> {
  try {
    return await operation();
  } catch (error) {
    if (error instanceof RuntimeMetadataProbeError) throw error;
    throw new RuntimeMetadataProbeError(phase, safeApiErrorCode(error));
  }
}

async function readRuntimeCommand<T>(
  connectionId: string,
  database: string,
  schema: string | null,
  command: string,
  input: Record<string, unknown>,
): Promise<T> {
  const profiles = await probeCall('profiles', () =>
    invokeBackend<
      Array<{ connectionId: string; configRevision: number | string; enabled: boolean }>
    >('platform_list_profiles', { request: {} }),
  );
  const profile = profiles.find((entry) => entry.connectionId === connectionId && entry.enabled);
  if (!profile) throw new RuntimeMetadataProbeError('profiles', 'profileUnavailable');
  const token = await probeCall('token', () =>
    invokeBackend<SubmissionToken>('issue_submission_token', {
      request: { operation: 'executeAtTarget', handle: null },
    }),
  );
  const request = {
    target: {
      connectionId,
      namespace: { database, schema, catalog: null, path: [] },
      object: null,
    },
    expectedConfigRevision: profile.configRevision,
    call: { command, input },
    idempotencyKey: token.idempotencyKey,
  };
  const admissionDeadline = Date.now() + 10_000;
  let receipt: ExecutionReceipt;
  for (;;) {
    try {
      receipt = await probeCall('submit', () =>
        invokeBackend<ExecutionReceipt>('execute_at_target', { request }),
      );
      break;
    } catch (error) {
      if (
        !(error instanceof RuntimeMetadataProbeError) ||
        error.reason !== 'ResourceBusy' ||
        Date.now() >= admissionDeadline
      )
        throw error;
      // Match the product caller's bounded admission retry: no receipt exists,
      // so retry only ResourceBusy while reusing the same request and token.
      await new Promise<void>((resolve) => setTimeout(resolve, 100));
    }
  }
  await probeCall('poll', () =>
    browser.waitUntil(
      async () => {
        const view = await probeCall('poll', () =>
          invokeBackend<ExecutionView>('get_execution', {
            request: { executionId: receipt.executionId },
          }),
        );
        return ['succeeded', 'failed', 'cancelled'].includes(view.state);
      },
      { timeout: 20000, timeoutMsg: 'Runtime metadata completion unavailable' },
    ),
  );
  // Fetch once after polling so TypeScript sees the resolved view explicitly.
  const completed = await probeCall('result', () =>
    invokeBackend<ExecutionView>('get_execution', {
      request: { executionId: receipt.executionId },
    }),
  );
  if (completed.state !== 'succeeded') {
    throw new RuntimeMetadataProbeError(
      'result',
      `${completed.state}/${completed.errorCode ?? 'noErrorCode'}`,
    );
  }
  for (const artifactId of completed.artifactIds) {
    const chunk = await probeCall('artifactRead', () =>
      invokeBackend<{ bytes: number[] }>('read_artifact_chunk', {
        request: { artifactId, chunkIndex: '0' },
      }),
    );
    let payload: { output?: { kind?: string; data?: unknown } };
    try {
      payload = JSON.parse(new TextDecoder().decode(new Uint8Array(chunk.bytes))) as typeof payload;
    } catch {
      throw new RuntimeMetadataProbeError('artifactDecode', 'invalidPayload');
    }
    if (payload.output?.kind === 'commandResult' && payload.output.data !== undefined)
      return payload.output.data as T;
  }
  throw new RuntimeMetadataProbeError('artifactResult', 'typedCommandResultUnavailable');
}

/** Exercise the navigator's runtime target/metadata-resource route directly. */
export async function readRuntimeCatalog(
  connectionId: string,
  database: string,
): Promise<CatalogMetadata> {
  return readRuntimeCommand<CatalogMetadata>(connectionId, database, null, 'list_catalog', {
    database,
    schema: null,
  });
}

export interface RuntimeRelationSchema {
  definition: {
    columns: Array<{ name: string }>;
  };
}

/** Read a relation through the same runtime metadata route used by schema views. */
export async function readRuntimeRelationSchema(
  connectionId: string,
  relation: { database: string; schema: string | null; name: string },
): Promise<RuntimeRelationSchema> {
  const result = await readRuntimeCommand<{ value: RuntimeRelationSchema }>(
    connectionId,
    relation.database,
    relation.schema,
    'read_relation_schema',
    { relation },
  );
  if (!result.value || !Array.isArray(result.value.definition?.columns)) {
    throw new RuntimeMetadataProbeError('schemaDecode', 'typedRelationSchemaUnavailable');
  }
  return result.value;
}
