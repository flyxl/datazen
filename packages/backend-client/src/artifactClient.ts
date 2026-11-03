/**
 * The Artifact domain client (form-split F3; upload half added by WR-F6).
 *
 * Two halves, one wire shape each:
 *
 * - **Read** — `readArtifact`, the chunked read of a job or execution
 *   artifact. Artifacts are a resource with their own addressing
 *   (`artifactId` + byte range) rather than a member of whichever job
 *   produced them — a downloaded export outlives the job that wrote it. The
 *   three §6.5 read modes (metadata / chunkIndex / offset+limit) travel as
 *   the three variants of `ArtifactReadRequest`; `readArtifactMetadata` is
 *   the metadata-mode convenience that names the intent.
 * - **Write** — the controlled upload of
 *   `POST /api/v1/artifacts/uploads` (§6.5 在账端点). The server endpoint is
 *   a **single-request** begin+append+finalize: the v1 envelope
 *   `{ executionId, targetFingerprint, ttlSeconds?, payloadBase64,
 *   expectedSha256? }` goes in, `{ artifactId }` comes back. There is no
 *   per-chunk append route, so the chunked writer protocol lives entirely
 *   client-side: `createUploadWriter` / `appendUploadChunk` aggregate and
 *   digest in memory, and `finalizeUpload` lands the whole payload as the one
 *   envelope request (the L1 8 MiB body gate is the physical ceiling the
 *   aggregation must respect — `UPLOAD_CHUNK_BYTES` stays well under it to
 *   leave room for the base64/JSON envelope inflation).
 *
 * Wire contract (settled with the server wave1 handler, `router.rs`
 * `handler_artifact_upload`): success 201 → `{ artifactId }`; ACL mismatch →
 * `PermissionDenied`; declared-hash mismatch → `InvalidArgument` (the artifact
 * is aborted server-side); oversized aggregate → `PayloadTooLarge` /
 * `QuotaExceeded`. Cross-organization reads answer the deliberate §11 404 —
 * an artifact not visible to the caller's organization is "not found", never
 * "forbidden" (CM-05, no id enumeration).
 */

import { ApiError } from './errors';
import type { BackendGuard } from './guard';
import type { BackendTransport } from './transport';
import type {
  ArtifactChunk,
  ArtifactMetadata,
  ArtifactMetadataReadRequest,
  ArtifactReadRequest,
  ArtifactUploadEnvelope,
  ArtifactUploadReceipt,
  Id,
} from './types';

/** The Artifact slice of `MethodMap`. */
export interface ArtifactMethodMap {
  readArtifact: { request: ArtifactReadRequest; response: ArtifactChunk };
  /** The §6.5 uploads endpoint: one request, begin+append+finalize. */
  createArtifactUpload: { request: ArtifactUploadEnvelope; response: ArtifactUploadReceipt };
}

/** The Artifact domain client surface. */
export interface ArtifactClient {
  readArtifact(request: ArtifactReadRequest): Promise<ArtifactChunk>;
  /**
   * The metadata read mode (`?metadata=1`) by name. Same wire method as
   * `readArtifact` — this is the planning call a restore makes first, and it
   * is deliberately not a separate `MethodMap` key: the §6.5 route table has
   * exactly two artifact entries and the map stays 1:1 with them.
   */
  readArtifactMetadata(request: ArtifactMetadataReadRequest): Promise<ArtifactMetadata>;
  /** The one-shot upload: the whole payload already in hand. */
  createArtifactUpload(request: ArtifactUploadEnvelope): Promise<ArtifactUploadReceipt>;
  /** Begin a chunked upload writer (client-side aggregation; no wire traffic). */
  createUploadWriter(request: ArtifactUploadBeginRequest): ArtifactUploadWriter;
  /** Aggregate one more chunk into the writer (client-side; no wire traffic). */
  appendUploadChunk(writer: ArtifactUploadWriter, bytes: Uint8Array): void;
  /** Land the aggregated payload as the single §6.5 uploads request. */
  finalizeUpload(
    writer: ArtifactUploadWriter,
    options?: ArtifactUploadFinalizeOptions,
  ): Promise<ArtifactUploadReceipt>;
}

/* ------------------------------------------------------------------ */
/* Chunked writer envelope (client-side half of the uploads endpoint). */
/* ------------------------------------------------------------------ */

/**
 * Per-chunk cap for the client-side aggregation. The server's L1 body gate is
 * 8 MiB and the envelope inflates the payload (base64 ×4/3) plus JSON
 * framing; 4 MiB of raw payload is the conservative landing zone.
 */
export const UPLOAD_CHUNK_BYTES = 4 * 1024 * 1024;

/** Begin request of a chunked upload: the envelope's identity fields (+ first chunk). */
export interface ArtifactUploadBeginRequest {
  executionId: Id;
  targetFingerprint: string;
  /** Server-side TTL; omitted = the handler default (3600). */
  ttlSeconds?: number;
  /** Optional first chunk — `begin` + 首块 in one call. */
  firstChunk?: Uint8Array;
}

/**
 * A chunked upload in progress. The bytes never touched the wire yet: the
 * writer is the client-side stand-in for the server's `UploadSession`, and
 * `finalizeUpload` is the only step that produces a request.
 */
export interface ArtifactUploadWriter {
  readonly executionId: Id;
  readonly targetFingerprint: string;
  readonly ttlSeconds?: number;
  /** Chunks aggregated so far (each ≤ {@link UPLOAD_CHUNK_BYTES} raw bytes). */
  readonly chunkCount: number;
  /** Aggregated byte total. */
  readonly byteLength: number;
}

/** Options of `finalizeUpload`. */
export interface ArtifactUploadFinalizeOptions {
  /**
   * Declared content digest (lowercase hex). Absent = the client computes
   * SHA-256 over the aggregated bytes; present = passed through verbatim so
   * the server compares against the caller's own accounting (a hash the
   * client computed earlier, or one asserted by the backup manifest).
   */
  expectedSha256?: string;
  /** Digest override (tests / hosts without WebCrypto); default `sha256Hex`. */
  sha256?: (bytes: Uint8Array) => Promise<string>;
}

/**
 * Lowercase-hex SHA-256 via WebCrypto (`crypto.subtle`), the same digest the
 * server recomputes with `sha2`. Sliced copies only: `subtle` requires fresh
 * buffers, and the writer's internal chunk list must stay caller-immutible.
 */
export async function sha256Hex(bytes: Uint8Array): Promise<string> {
  const digest = await globalThis.crypto.subtle.digest('SHA-256', bytes.slice());
  return Array.from(new Uint8Array(digest), (b) => b.toString(16).padStart(2, '0')).join('');
}

/** Standard base64 of `bytes` (`btoa` in 32 KiB shots — no spread overflow). */
export function toBase64(bytes: Uint8Array): string {
  let binary = '';
  for (let offset = 0; offset < bytes.length; offset += 0x8000) {
    binary += String.fromCharCode(...bytes.subarray(offset, offset + 0x8000));
  }
  return btoa(binary);
}

/** Build the Artifact domain client over a transport. */
export function createArtifactMethods(
  transport: BackendTransport,
  guard: BackendGuard,
): ArtifactClient {
  return {
    readArtifact: (request) => guard('readArtifact', () => transport.call('readArtifact', request)),

    // Metadata is the same wire method (the §6.5 route table's `?metadata=1`
    // branch); the guard key stays `readArtifact`, so a backend that cannot
    // serve artifact reads answers `CapabilityUnsupported` here too.
    readArtifactMetadata: (request) =>
      guard('readArtifact', () =>
        transport.call('readArtifact', { artifactId: request.artifactId, metadata: true }),
      ) as unknown as Promise<ArtifactMetadata>,

    createArtifactUpload: (request) =>
      guard('createArtifactUpload', () => transport.call('createArtifactUpload', request)),

    createUploadWriter: (request) => {
      if (typeof request.executionId !== 'string' || request.executionId.length === 0) {
        throw new ApiError('InvalidArgument', 'createUploadWriter requires an executionId.');
      }
      if (typeof request.targetFingerprint !== 'string' || request.targetFingerprint.length === 0) {
        throw new ApiError('InvalidArgument', 'createUploadWriter requires a targetFingerprint.');
      }
      const chunks: Uint8Array[] = [];
      let byteLength = 0;
      const push = (bytes: Uint8Array): void => {
        if (bytes.length > UPLOAD_CHUNK_BYTES) {
          throw new ApiError(
            'InvalidArgument',
            `Upload chunk of ${bytes.length} bytes exceeds the ${UPLOAD_CHUNK_BYTES}-byte client aggregation cap.`,
          );
        }
        chunks.push(bytes);
        byteLength += bytes.length;
      };
      if (request.firstChunk !== undefined) push(request.firstChunk);
      const state: WriterState = {
        executionId: request.executionId,
        targetFingerprint: request.targetFingerprint,
        ...(request.ttlSeconds !== undefined ? { ttlSeconds: request.ttlSeconds } : {}),
        get chunkCount(): number {
          return chunks.length;
        },
        get byteLength(): number {
          return byteLength;
        },
        chunksRef: chunks,
        push,
      };
      return state;
    },

    appendUploadChunk: (writer, bytes) => {
      stateOf(writer).push(bytes);
    },

    finalizeUpload: async (writer, options) => {
      const state = stateOf(writer);
      if (state.chunksRef.length === 0) {
        throw new ApiError('InvalidArgument', 'finalizeUpload requires at least one chunk.');
      }
      const total = state.chunksRef.reduce((sum, chunk) => sum + chunk.length, 0);
      const payload = new Uint8Array(total);
      let offset = 0;
      for (const chunk of state.chunksRef) {
        payload.set(chunk, offset);
        offset += chunk.length;
      }
      const expectedSha256 = options?.expectedSha256 ?? (await (options?.sha256 ?? sha256Hex)(payload));
      const envelope: ArtifactUploadEnvelope = {
        executionId: state.executionId,
        targetFingerprint: state.targetFingerprint,
        ...(state.ttlSeconds !== undefined ? { ttlSeconds: state.ttlSeconds } : {}),
        payloadBase64: toBase64(payload),
        expectedSha256,
      };
      return guard('createArtifactUpload', () => transport.call('createArtifactUpload', envelope));
    },
  };
}

/** Internal writer state; the public {@link ArtifactUploadWriter} hides it. */
interface WriterState extends ArtifactUploadWriter {
  chunksRef: Uint8Array[];
  push(bytes: Uint8Array): void;
}

/** Recover the internal state a factory produced for `writer`. */
function stateOf(writer: ArtifactUploadWriter): WriterState {
  const state = writer as WriterState;
  if (typeof state.push !== 'function' || !Array.isArray(state.chunksRef)) {
    throw new ApiError(
      'InvalidArgument',
      'appendUploadChunk/finalizeUpload require a writer produced by createUploadWriter.',
    );
  }
  return state;
}

/** Compile-time: every Artifact map key is served by this domain client. */
type Assert<T extends true> = T;
export type ArtifactClientCoversMap = Assert<
  keyof ArtifactMethodMap extends keyof ArtifactClient ? true : false
>;
