/**
 * The Artifact domain client (form-split F3).
 *
 * One method: the chunked read of a job or execution artifact. It stands alone
 * because artifacts are a resource with their own addressing (`artifactId` +
 * byte range) rather than a member of whichever job produced them — a
 * downloaded export outlives the job that wrote it.
 */

import type { BackendGuard } from './guard';
import type { BackendTransport } from './transport';
import type { ArtifactChunk, ArtifactReadRequest } from './types';

/** The Artifact slice of `MethodMap`. */
export interface ArtifactMethodMap {
  readArtifact: { request: ArtifactReadRequest; response: ArtifactChunk };
}

/** The Artifact domain client surface. */
export interface ArtifactClient {
  readArtifact(request: ArtifactReadRequest): Promise<ArtifactChunk>;
}

/** Build the Artifact domain client over a transport. */
export function createArtifactMethods(
  transport: BackendTransport,
  guard: BackendGuard,
): ArtifactClient {
  return {
    readArtifact: (request) => guard('readArtifact', () => transport.call('readArtifact', request)),
  };
}

/** Compile-time: every Artifact map key is served by this domain client. */
type Assert<T extends true> = T;
export type ArtifactClientCoversMap = Assert<
  keyof ArtifactMethodMap extends keyof ArtifactClient ? true : false
>;
