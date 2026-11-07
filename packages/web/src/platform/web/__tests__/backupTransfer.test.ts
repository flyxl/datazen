/**
 * WR-F6 web 备份/恢复流测试——`backupTransfer.ts` + §6.5 两条路由的 wire 行为：
 *
 * 1. **上传信封形状**：`exportBackup` → `POST /api/v1/artifacts/uploads`，
 *    请求体逐字段对照 wave1 server envelope
 *    `{ executionId, targetFingerprint, ttlSeconds?, payloadBase64,
 *    expectedSha256? }`（多余键零容忍），201 → `{ artifactId }`；
 * 2. **hash 传递**：显式 `expectedSha256` 原文透传；缺省路径对聚合载荷
 *    计算 SHA-256（node:crypto 独立复算对账）；
 * 3. **下载三取法分发**：`restoreFromArtifact` 先 `?metadata=1` 规划、再逐
 *    `?chunkIndex=N` 重装，字节与 `publishedByteSize` 对账；
 * 4. **越权错误传播**：uploads 的 403 `PermissionDenied` 与读取侧 §11 的
 *    有意 404（跨组织 = NotFound）以 `ApiError` 原码穿透整条装配链；
 * 5. **Blob 触发**：`saveLocalCopy` 走注入的 `deliver`（缺省即
 *    `downloadBlob` 的 anchor 半边）——文件名、字节、MIME 逐项断言；
 * 6. **FileReader 导入**：`importBackupFile` 走注入读半边取回字节
 *    （缺省 FileReader 路径在无 FileReader 环境按 `CapabilityUnsupported`
 *    显式拒绝）。
 *
 * fetch 用 `vi.stubGlobal` 假体（与 domainContractTables.test.ts 同款）；
 * 客户端栈走真件：`createWebBackendTransport` → guard → artifact 域客户端。
 */
import { createHash } from 'node:crypto';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { ApiError, createBackendClient } from '@datazen/backend-client';

import {
  createWebBackupTransfer,
  type WebBackupTransfer,
  type WebBackupTransferOptions,
} from '../backupTransfer';
import { createWebBackendTransport } from '../backendTransport';

/* ------------------------------------------------------------------ */
/* 假 fetch 基建。                                                      */
/* ------------------------------------------------------------------ */

interface RecordedRequest {
  url: RequestInfo | URL;
  init: RequestInit;
}

const calls: RecordedRequest[] = [];

function stubFetchWith(respond: (request: RecordedRequest, index: number) => Response): void {
  calls.length = 0;
  vi.stubGlobal(
    'fetch',
    vi.fn(async (url: RequestInfo | URL, init?: RequestInit): Promise<Response> => {
      const request: RecordedRequest = { url, init: init ?? {} };
      const index = calls.length;
      calls.push(request);
      return respond(request, index);
    }),
  );
}

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'content-type': 'application/json' },
  });
}

function errorResponse(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'content-type': 'application/json' },
  });
}

function rawResponse(bytes: Uint8Array, headers: Record<string, string>): Response {
  return new Response(bytes as BodyInit, {
    status: 200,
    headers: { 'content-type': 'application/octet-stream', ...headers },
  });
}

function newClient(): ReturnType<typeof createBackendClient> {
  const transport = createWebBackendTransport({ csrfTokenProvider: () => 'csrf-token' });
  return createBackendClient('web-test', transport);
}

function newBackupTransfer(options?: WebBackupTransferOptions): WebBackupTransfer {
  return createWebBackupTransfer(newClient(), options);
}

async function rejectionOf(run: () => Promise<unknown>): Promise<ApiError> {
  try {
    await run();
  } catch (error: unknown) {
    return error as ApiError;
  }
  throw new Error('the call was expected to be rejected.');
}

afterEach(() => {
  vi.unstubAllGlobals();
});

/* ------------------------------------------------------------------ */
/* fixtures。                                                           */
/* ------------------------------------------------------------------ */

const PAYLOAD = new TextEncoder().encode('{"kind":"datazen-backup","tables":["orders"]}');
const PAYLOAD_B64 = Buffer.from(PAYLOAD).toString('base64');
const PAYLOAD_SHA256 = createHash('sha256').update(PAYLOAD).digest('hex');

/* ------------------------------------------------------------------ */
/* 1+2：上传信封形状 + hash 传递。                                       */
/* ------------------------------------------------------------------ */

describe('exportBackup → POST /api/v1/artifacts/uploads（§6.5 单请求 envelope）', () => {
  it('envelope 逐字段对照 wave1 server 形状，201 回执铸造 artifactId', async () => {
    stubFetchWith(() => jsonResponse({ artifactId: 'art-9' }, 201));
    const backup = newBackupTransfer();

    const receipt = await backup.exportBackup({
      executionId: 'exec-1',
      targetFingerprint: 'fp-orders-1',
      ttlSeconds: 120,
      payload: PAYLOAD,
      fileName: 'orders-backup.dat',
    });

    expect(calls).toHaveLength(1);
    expect(String(calls[0]!.url)).toBe('/api/v1/artifacts/uploads');
    expect(calls[0]!.init.method).toBe('POST');
    expect(calls[0]!.init.credentials).toBe('include');
    expect((calls[0]!.init.headers as Headers).get('x-dz-csrf-token')).toBe('csrf-token');

    const body = JSON.parse(String(calls[0]!.init.body)) as Record<string, unknown>;
    expect(Object.keys(body).sort()).toEqual([
      'executionId',
      'expectedSha256',
      'payloadBase64',
      'targetFingerprint',
      'ttlSeconds',
    ]);
    expect(body['executionId']).toBe('exec-1');
    expect(body['targetFingerprint']).toBe('fp-orders-1');
    expect(body['ttlSeconds']).toBe(120);
    expect(body['payloadBase64']).toBe(PAYLOAD_B64);
    // 缺省路径：客户端对聚合载荷自算 SHA-256（node:crypto 独立复算对账）。
    expect(body['expectedSha256']).toBe(PAYLOAD_SHA256);
    // 回执 = 服务端铸造的产物 id + 客户端记账（WebBackupExportResult 全形）。
    expect(receipt.artifactId).toBe('art-9');
    expect(receipt.sha256).toBe(PAYLOAD_SHA256);
    expect(receipt.byteLength).toBe(PAYLOAD.byteLength);
    expect(receipt.chunkCount).toBe(1);
    expect(receipt.savedLocally).toBe(false);
  });

  it('显式 expectedSha256 原文透传（服务端按声明摘要复核）', async () => {
    stubFetchWith(() => jsonResponse({ artifactId: 'art-10' }, 201));
    const backup = newBackupTransfer();

    await backup.exportBackup({
      executionId: 'exec-1',
      targetFingerprint: 'fp-orders-1',
      payload: PAYLOAD,
      expectedSha256: 'deadbeef',
    });

    const body = JSON.parse(String(calls[0]!.init.body)) as Record<string, unknown>;
    expect(body['expectedSha256']).toBe('deadbeef');
  });

  it('空载荷在出站前拒绝（InvalidArgument，fetch 零调用）', async () => {
    stubFetchWith(() => jsonResponse({ artifactId: 'art-x' }, 201));
    const backup = newBackupTransfer();

    const error = await rejectionOf(() =>
      backup.exportBackup({
        executionId: 'exec-1',
        targetFingerprint: 'fp-1',
        payload: new Uint8Array(0),
      }),
    );
    expect(error).toBeInstanceOf(ApiError);
    expect(error.code).toBe('InvalidArgument');
    expect(calls).toHaveLength(0);
  });
});

/* ------------------------------------------------------------------ */
/* 3：下载三取法分发（metadata 规划 + 逐 chunkIndex 重装）。              */
/* ------------------------------------------------------------------ */

describe('restoreFromArtifact → GET /api/v1/artifacts/{id} 三取法', () => {
  it('先 ?metadata=1 规划，再逐 ?chunkIndex= 重装并与 publishedByteSize 对账', async () => {
    const half = Math.floor(PAYLOAD.length / 2);
    stubFetchWith((request, index) => {
      const url = String(request.url);
      if (index === 0) {
        expect(url).toBe('/api/v1/artifacts/art-1?metadata=1');
        return jsonResponse({
          targetFingerprint: 'fp-orders-1',
          artifactId: 'art-1',
          lifecycle: 'complete',
          publishedChunkCount: 2,
          publishedByteSize: PAYLOAD.length,
          totalChunks: 2,
          resultCompleteness: 'complete',
          truncationReason: null,
        });
      }
      const chunkIndex = Number(new URL(url, 'https://x.test').searchParams.get('chunkIndex'));
      const bytes = chunkIndex === 0 ? PAYLOAD.subarray(0, half) : PAYLOAD.subarray(half);
      return rawResponse(bytes, {
        'x-dz-total-chunks': '2',
        'x-dz-published-chunk-count': '2',
        'x-dz-published-byte-length': String(PAYLOAD.length),
        'x-dz-result-completeness': 'complete',
      });
    });
    const backup = newBackupTransfer();

    const restored = await backup.restoreFromArtifact({ artifactId: 'art-1' });

    expect([...restored]).toEqual([...PAYLOAD]);
    expect(String(calls[1]!.url)).toBe('/api/v1/artifacts/art-1?chunkIndex=0');
    expect(String(calls[2]!.url)).toBe('/api/v1/artifacts/art-1?chunkIndex=1');
  });

  it('truncated 产物拒绝恢复（不宣称完整），fetch 只到 metadata 一跳', async () => {
    stubFetchWith(() =>
      jsonResponse({
        artifactId: 'art-2',
        lifecycle: 'truncated',
        publishedChunkCount: 1,
        publishedByteSize: 3,
        totalChunks: 4,
        resultCompleteness: 'truncated',
        truncationReason: 'consumerLimitExceeded',
      }),
    );
    const backup = newBackupTransfer();

    const error = await rejectionOf(() => backup.restoreFromArtifact({ artifactId: 'art-2' }));
    expect(error).toBeInstanceOf(ApiError);
    expect(error.code).toBe('InvalidArgument');
    expect(calls).toHaveLength(1);
  });
});

/* ------------------------------------------------------------------ */
/* 4：越权错误传播。                                                     */
/* ------------------------------------------------------------------ */

describe('越权拒绝断言（验收项）：ApiError 原码穿透装配链', () => {
  it('uploads 403 PermissionDenied（ACL 双重检查不过）→ exportBackup 原码拒绝', async () => {
    stubFetchWith(() =>
      errorResponse(403, {
        code: 'PermissionDenied',
        message: 'Artifact ownership must match the authenticated organization and principal.',
        requestId: 'req-403',
        retryDisposition: 'never',
      }),
    );
    const backup = newBackupTransfer();

    const error = await rejectionOf(() =>
      backup.exportBackup({
        executionId: 'exec-1',
        targetFingerprint: 'fp-1',
        payload: PAYLOAD,
      }),
    );
    expect(error).toBeInstanceOf(ApiError);
    expect(error.code).toBe('PermissionDenied');
    expect(error.requestId).toBe('req-403');
  });

  it('跨组织读取 = §11 有意 404（NotFound，不可区分语义）→ restoreFromArtifact 原码拒绝', async () => {
    stubFetchWith(() =>
      errorResponse(404, {
        code: 'NotFound',
        message: 'Artifact is not visible to this organization or has expired.',
        requestId: 'req-404',
        retryDisposition: 'never',
      }),
    );
    const backup = newBackupTransfer();

    const error = await rejectionOf(() =>
      backup.restoreFromArtifact({ artifactId: 'art-other-org' }),
    );
    expect(error).toBeInstanceOf(ApiError);
    expect(error.code).toBe('NotFound');
    expect(error.requestId).toBe('req-404');
  });
});

/* ------------------------------------------------------------------ */
/* 5：Blob 触发（下载=用户保存）。                                       */
/* ------------------------------------------------------------------ */

describe('浏览器 Blob 语义', () => {
  it('saveLocalCopy 走 deliver 半边：文件名/字节/MIME 逐项断言', async () => {
    stubFetchWith(() => jsonResponse({ artifactId: 'art-11' }, 201));
    const deliver = vi.fn<(contents: BlobPart, fileName: string, mimeType: string) => void>();
    const backup = newBackupTransfer({ deliver });

    const receipt = await backup.exportBackup({
      executionId: 'exec-1',
      targetFingerprint: 'fp-1',
      payload: PAYLOAD,
      fileName: 'orders-backup.dat',
      saveLocalCopy: true,
    });

    expect(deliver).toHaveBeenCalledTimes(1);
    const [contents, fileName, mimeType] = deliver.mock.calls[0]!;
    expect(new Uint8Array(contents as ArrayBuffer)).toEqual(PAYLOAD);
    expect(fileName).toBe('orders-backup.dat');
    expect(mimeType).toBe('application/octet-stream');
    expect(receipt.savedLocally).toBe(true);
  });

  it('缺省（saveLocalCopy 缺席）不触发本地保存', async () => {
    stubFetchWith(() => jsonResponse({ artifactId: 'art-12' }, 201));
    const deliver = vi.fn<(contents: BlobPart, fileName: string, mimeType: string) => void>();
    const backup = newBackupTransfer({ deliver });

    const receipt = await backup.exportBackup({
      executionId: 'exec-1',
      targetFingerprint: 'fp-1',
      payload: PAYLOAD,
    });

    expect(deliver).not.toHaveBeenCalled();
    expect(receipt.savedLocally).toBe(false);
  });
});

/* ------------------------------------------------------------------ */
/* 6：FileReader 导入（导入=读入后可上传）。                             */
/* ------------------------------------------------------------------ */

describe('importBackupFile', () => {
  it('注入读半边取回文件字节（随后可交 exportBackup 上传）', async () => {
    const readFile = vi.fn<(file: File) => Promise<Uint8Array>>((file) => {
      expect(file.name).toBe('orders-backup.dat');
      return Promise.resolve(PAYLOAD);
    });
    const backup = newBackupTransfer({ readFile });

    const bytes = await backup.importBackupFile({
      file: { name: 'orders-backup.dat' } as unknown as File,
    });

    expect([...bytes]).toEqual([...PAYLOAD]);
    expect(readFile).toHaveBeenCalledTimes(1);
  });

  it('无 FileReader 的环境按 CapabilityUnsupported 显式拒绝（缺省读半边，不出站）', async () => {
    // node 环境本就无 FileReader；stubGlobal 钉住缺位，防未来 node 版本带上它。
    vi.stubGlobal('FileReader', undefined);
    // calls 只由 stubFetchWith 清空；本用例不出站，也走同一基建并钉住
    // 「胆敢触传输即炸」——缺省读半边的拒绝必须发生在任何 fetch 之前。
    stubFetchWith(() => {
      throw new Error('importBackupFile must not touch the transport');
    });
    const backup = newBackupTransfer();
    const error = await rejectionOf(() =>
      backup.importBackupFile({ file: { name: 'x' } as unknown as File }),
    );
    expect(error).toBeInstanceOf(ApiError);
    expect(error.code).toBe('CapabilityUnsupported');
    expect(calls).toHaveLength(0);
  });
});
