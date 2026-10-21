import type { ResultCompleteness } from '@datazen/backend-client';

/** Durable result references remain readable independently of the runtime session. */
export interface QueryResultArtifacts {
  executionId: string;
  artifactIds: readonly string[];
  resultCompleteness: ResultCompleteness;
}
