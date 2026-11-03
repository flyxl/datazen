/**
 * Web 备份/恢复装配层（WR-F6）——数据备份的受控传输双半边，落在 web 形态
 * 的域客户端装配点上（与 `assembly.ts` 的 `tablesExporter` 同层）：
 *
 * - **备份导出（上行）**：`exportBackup` 把备份载荷经 artifactClient 的
 *   chunked writer（`createUploadWriter`/`appendUploadChunk`/`finalizeUpload`）
 *   聚合后，以 §6.5 在账端点 `POST /api/v1/artifacts/uploads` 的 v1 envelope
 *   **单请求**落入服务端产物库——SHA-256 客户端随算随传（`expectedSha256`），
 *   服务端 finalize 时复核，不一致即 abort（`InvalidArgument`）；
 * - **恢复导入（下行）**：`restoreFromArtifact` 走三取法读回——先
 *   `?metadata=1` 取发布元数据（规划读取），再逐 `?chunkIndex=` 重装字节，
 *   与 `publishedByteSize` 对账；`truncated`/`pending` 产物拒绝恢复；
 * - **浏览器 Blob 语义**：下载=用户保存（`downloadBlob` 的 anchor 半边，
 *   与 tablesExporter 同款路径无关保存）；导入=FileReader 读入后可再经
 *   `exportBackup` 上传回产物库（`importBackupFile` 只负责读，上传与导出
 *   共用同一条受控管道，产物库的 ACL/TTL 语义不因来源分叉）。
 *
 * 本层不新增 wire 面：三个动作全部骑在 §6.5 的两条既有路由上
 * （uploads / {id} 三取法），越权（跨组织读、ACL 不符的写）由服务端逐请求
 * 判定并以 `ApiError` 原码传播——本层只做参数整形与完整性对账。
 */

import { ApiError, toCounter, toId, sha256Hex, UPLOAD_CHUNK_BYTES } from '@datazen/backend-client';
import type { ArtifactClient } from '@datazen/backend-client';
import { downloadBlob } from '@datazen/frontend';

/** 备份导出请求。载荷可为字节或 UTF-8 文本（JSON 快照常态）。 */
export interface WebBackupExportRequest {
  /** 上传信封的 executionId（服务端按认证主体复核 ACL，此字段不授权）。 */
  executionId: string;
  /** 目标指纹（信封必填半边；服务端原样入库）。 */
  targetFingerprint: string;
  /** 产物 TTL 秒数；缺省 = 服务端 3600。 */
  ttlSeconds?: number;
  /** 备份载荷；字符串按 UTF-8 编码。 */
  payload: Uint8Array | string;
  /** 本地保存半边的文件名；缺省 `datazen-backup-<epoch>.dat`。 */
  fileName?: string;
  /** 同时触发浏览器 Blob 下载（下载=用户保存）。 */
  saveLocalCopy?: boolean;
  /** 声明摘要透传；缺省由客户端对聚合载荷计算 SHA-256。 */
  expectedSha256?: string;
}

/** 备份导出回执：服务端铸造的产物 id + 客户端记账。 */
export interface WebBackupExportResult {
  artifactId: string;
  /** 随信封声明的摘要（透传或自算的小写 hex）。 */
  sha256: string;
  byteLength: number;
  chunkCount: number;
  savedLocally: boolean;
}

/** 备份/恢复端口（web 形态装配产物）。 */
export interface WebBackupTransfer {
  /** 上行：聚合 → §6.5 uploads 单请求落服务端产物库（+可选本地保存）。 */
  exportBackup(request: WebBackupExportRequest): Promise<WebBackupExportResult>;
  /** 下行：metadata 规划 + 逐 chunk 读回重装，字节总数与元数据对账。 */
  restoreFromArtifact(request: { artifactId: string }): Promise<Uint8Array>;
  /** 本地半边：FileReader 读入用户文件（随后可交 `exportBackup` 上传）。 */
  importBackupFile(input: { file: File }): Promise<Uint8Array>;
}

/** 可注入半边（测试与宿主覆盖）。 */
export interface WebBackupTransferOptions {
  /** Blob 下载半边；缺省 `downloadBlob`（anchor + objectURL）。 */
  deliver?: (contents: BlobPart, fileName: string, mimeType: string) => void;
  /** 文件读入半边；缺省 FileReader `readAsArrayBuffer`。 */
  readFile?: (file: File) => Promise<Uint8Array>;
  /** 摘要函数；缺省 backend-client 的 WebCrypto `sha256Hex`。 */
  sha256?: (bytes: Uint8Array) => Promise<string>;
}

/** 缺省文件读入：FileReader → ArrayBuffer → Uint8Array（导入=FileReader 读入）。 */
function readFileWithFileReader(file: File): Promise<Uint8Array> {
  if (typeof FileReader === 'undefined') {
    throw new ApiError(
      'CapabilityUnsupported',
      'Backup file import requires a browser FileReader; this environment has none.',
    );
  }
  return new Promise<Uint8Array>((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(new Uint8Array(reader.result as ArrayBuffer));
    reader.onerror = () =>
      reject(new ApiError('InvalidArgument', `Backup import failed to read '${file.name}'.`));
    reader.readAsArrayBuffer(file);
  });
}

/** 把 `payload` 归一为字节并切成 ≤ {@link UPLOAD_CHUNK_BYTES} 的聚合片。 */
function sliceIntoChunks(payload: Uint8Array): Uint8Array[] {
  const chunks: Uint8Array[] = [];
  for (let offset = 0; offset < payload.length; offset += UPLOAD_CHUNK_BYTES) {
    chunks.push(payload.subarray(offset, offset + UPLOAD_CHUNK_BYTES));
  }
  return chunks;
}

/** 组装备份/恢复端口。`client` 通常是装配好的 `BackendClient.artifact` 半边。 */
export function createWebBackupTransfer(
  client: ArtifactClient,
  options: WebBackupTransferOptions = {},
): WebBackupTransfer {
  const deliver = options.deliver ?? downloadBlob;
  const readFile = options.readFile ?? readFileWithFileReader;
  const digest = options.sha256 ?? sha256Hex;

  return {
    async exportBackup(request: WebBackupExportRequest): Promise<WebBackupExportResult> {
      const executionId = toId(request.executionId);
      if (executionId === undefined) {
        throw new ApiError('InvalidArgument', 'Backup export requires a non-empty executionId.');
      }
      if (request.targetFingerprint.length === 0) {
        throw new ApiError(
          'InvalidArgument',
          'Backup export requires a non-empty targetFingerprint.',
        );
      }
      const bytes =
        typeof request.payload === 'string'
          ? new TextEncoder().encode(request.payload)
          : request.payload;
      if (bytes.length === 0) {
        throw new ApiError('InvalidArgument', 'Backup export refuses an empty payload.');
      }

      const chunks = sliceIntoChunks(bytes);
      const writer = client.createUploadWriter({
        executionId,
        targetFingerprint: request.targetFingerprint,
        ...(request.ttlSeconds !== undefined ? { ttlSeconds: request.ttlSeconds } : {}),
        // begin+首块：第一片随 begin 走，其余片 append（纯客户端聚合）。
        ...(chunks.length > 0 ? { firstChunk: chunks[0] } : {}),
      });
      for (const chunk of chunks.slice(1)) {
        client.appendUploadChunk(writer, chunk);
      }
      const sha256 = request.expectedSha256 ?? (await digest(bytes));
      const receipt = await client.finalizeUpload(writer, { expectedSha256: sha256 });

      const fileName = request.fileName ?? `datazen-backup-${Date.now()}.dat`;
      let savedLocally = false;
      if (request.saveLocalCopy) {
        // 下载=用户保存：浏览器自己的路径无关保存（与 tablesExporter 同款）。
        deliver(bytes as BlobPart, fileName, 'application/octet-stream');
        savedLocally = true;
      }
      return {
        artifactId: receipt.artifactId,
        sha256,
        byteLength: writer.byteLength,
        chunkCount: writer.chunkCount,
        savedLocally,
      };
    },

    async restoreFromArtifact(request: { artifactId: string }): Promise<Uint8Array> {
      const artifactId = toId(request.artifactId);
      if (artifactId === undefined) {
        throw new ApiError('InvalidArgument', 'Backup restore requires a non-empty artifactId.');
      }
      // 取法一：metadata 规划（服务端逐请求 ACL；revoked/expired 一律 404）。
      const metadata = await client.readArtifactMetadata({ artifactId, metadata: true });
      if (metadata.resultCompleteness !== 'complete') {
        throw new ApiError(
          'InvalidArgument',
          `Backup restore refuses a ${metadata.resultCompleteness} artifact; only complete payloads restore.`,
        );
      }
      // 取法二：逐 chunkIndex 重装（0..publishedChunkCount-1，每请求独立授权）。
      const parts: Uint8Array[] = [];
      let total = 0;
      for (let index = 0; index < metadata.publishedChunkCount; index += 1) {
        const chunkIndex = toCounter(index);
        if (chunkIndex === undefined) break;
        const chunk = await client.readArtifact({ artifactId, chunkIndex });
        parts.push(chunk.bytes);
        total += chunk.bytes.length;
      }
      if (total !== metadata.publishedByteSize) {
        throw new ApiError(
          'ServiceUnavailable',
          `Backup restore reassembled ${total} bytes but the artifact declares ${metadata.publishedByteSize}.`,
        );
      }
      const restored = new Uint8Array(total);
      let offset = 0;
      for (const part of parts) {
        restored.set(part, offset);
        offset += part.length;
      }
      return restored;
    },

    async importBackupFile(input: { file: File }): Promise<Uint8Array> {
      return readFile(input.file);
    },
  };
}
