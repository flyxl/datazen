/**
 * 产物镜像：`ArtifactChunk` / `ArtifactSummary` / `ArtifactProvenance`。
 *
 * 与 `packages/platform-api/src/dto/artifact.rs` 逐字对应（camelCase wire）。
 *
 * **线缆编码不在契约层**：`bytes` 是进程内的原始字节；base64 等传输编码由宿主适配器
 * 负责。因此本包用 `Uint8Array` 表达字节——它不是 JSON 类型，落到 IPC 时由适配器决定
 * 编码（`Uint8Array` → Rust `Vec<u8>`）。
 *
 * 端口层的 `ChunkPayload` / `ByteRange` 是私有词汇，**不出现在客户端契约里**。
 */

import type { Counter, Id, Timestamp } from './ids';
import type { ResultCompleteness, StatementResultSource } from './execution';

/** 结果块。`totalChunks` 在长度未知时为 `null`。 */
export interface ArtifactChunk {
  readonly artifactId: Id;
  /** 块序号（wire 为十进制字符串 Counter）。 */
  readonly chunkIndex: Counter;
  readonly totalChunks: Counter | null;
  /** 本块在产物中的起始偏移（wire 为十进制字符串 Counter）。 */
  readonly offset: Counter;
  readonly bytes: Uint8Array;
  readonly resultCompleteness: ResultCompleteness;
}

/** 产物摘要。**不含**内容，也不含任何可还原凭据的材料。 */
export interface ArtifactSummary {
  readonly artifactId: Id;
  readonly organizationId: Id;
  readonly executionId: Id | null;
  readonly sizeBytes: Counter;
  readonly chunkCount: Counter | null;
  readonly resultCompleteness: ResultCompleteness;
  readonly createdAt: Timestamp;
  /** 过期时刻。过期后端口层返回错误，HTTP 映射由宿主负责。 */
  readonly expiresAt: Timestamp | null;
}

/** 产物与其来源的绑定：前端据此把结果块还原到正确的语句/上下文上。 */
export interface ArtifactProvenance {
  readonly artifactId: Id;
  readonly executionId: Id;
  readonly statementIndex: number;
  readonly source: StatementResultSource;
}

/** 本块结束偏移（offset + 字节长度），与 Rust `ArtifactChunk::end_offset()` 同构。 */
export function chunkEndOffset(chunk: ArtifactChunk): Counter {
  const start = Number.parseInt(chunk.offset, 10);
  const from = Number.isSafeInteger(start) && start > 0 ? start : 0;
  return String(from + chunk.bytes.length);
}

/** 块大小（字节数）。 */
export function chunkByteLength(chunk: ArtifactChunk): number {
  return chunk.bytes.length;
}

/** 读取块的某一段（纯内存切片，不发请求）。偏移越界返回空视图。 */
export function sliceChunk(chunk: ArtifactChunk, from: number, to: number): Uint8Array {
  const start = Math.max(0, from);
  const end = Math.min(chunk.bytes.length, Math.max(start, to));
  return chunk.bytes.subarray(start, end);
}

/** 读取块的文本解码结果。需要指定编码的场景由调用方传入 `TextDecoder`。 */
export function decodeChunk(chunk: ArtifactChunk, decoder?: TextDecoder): string {
  return (decoder ?? new TextDecoder()).decode(chunk.bytes);
}
