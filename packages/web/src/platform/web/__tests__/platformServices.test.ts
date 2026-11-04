/**
 * WR-E3 PlatformServices web 半边测试 —— node 环境、全手写假体（无第三方
 * mock 库，风格对齐 domainContractTables / eventStreamSource）：
 *
 * - 保存类对话框 → Blob + `<a download>`：saveText 原文、saveBinary
 *   base64 解码为字节，锚 download 文件名 + object URL revoke，返回
 *   `true`（浏览器的「保存」= 触发下载；下载器的取消不归对话框端口）；
 * - 打开文本 → `<input type="file">` + FileReader：`change` 带文件 →
 *   `{ fileName, content }`；picker `cancel` 事件 → `null`（取消半边）；
 * - 剪贴板 → async Clipboard API 优先；API 缺席/拒绝 → execCommand 兜底；
 *   双失败 → `CapabilityUnsupported`（§13 词汇，绝无静默 no-op）；
 *   readClipboard 无历史读半边 → API 缺席即如实抛；
 * - web 装配（WR-E3 绑定面）：宿主叠加能力注入空缺省
 *   （`requireHostSettings().readHostExtras() == {}`，即
 *   `platform.host.config.get()` 的 web 半边）；导出绑定共享 Blob 半边，
 *   经分页 `getTableData` 假 fetch 全链路驱动：满页续读 + afterKey 原样
 *   回传 + 短页收束 → 单个 Blob（CSV 形状）+ 锚下载 + `{ Saved: n }`。
 */
import { afterEach, describe, expect, it, vi } from 'vitest';
import { ApiError, clearPlatformServices } from '@datazen/backend-client';
import {
  clearPlatform,
  requireHostSettings,
  requireTablesExporter,
  type TablesExportRequest,
} from '@datazen/frontend';

import { WEB_EXPORT_PAGE_SIZE, assembleWebPlatform } from '../assembly';
import { createWebPlatformServices } from '../platformServices';

/* ------------------------------------------------------------------ */
/* 手写 DOM 假体：document / element / FileReader / URL object URL。     */
/* ------------------------------------------------------------------ */

interface FakeElement {
  tag: string;
  type: string;
  accept: string;
  download: string;
  href: string;
  value: string;
  files: Array<{ name: string; text: string }> | null;
  style: Record<string, string>;
  clickCount: number;
  handlers: Map<string, () => void>;
  addEventListener(type: string, handler: () => void): void;
  click(): void;
  remove(): void;
  select(): void;
  setAttribute(name: string, value: string): void;
}

function createElementSpy(tag: string): FakeElement {
  const element: FakeElement = {
    tag,
    type: '',
    accept: '',
    download: '',
    href: '',
    value: '',
    files: null,
    style: {},
    clickCount: 0,
    handlers: new Map(),
    addEventListener(type: string, handler: () => void) {
      element.handlers.set(type, handler);
    },
    click() {
      element.clickCount += 1;
    },
    remove() {
      // 分离节点 remove 是无操作。
    },
    select() {
      // execCommand 前的选区；假体记录即可。
    },
    setAttribute() {
      // 假体不需要属性账目。
    },
  };
  return element;
}

const createdElements: FakeElement[] = [];
let execCommandResult = true;
const execCommandCalls: string[] = [];

function installDocument(): void {
  createdElements.length = 0;
  execCommandResult = true;
  execCommandCalls.length = 0;
  vi.stubGlobal('document', {
    cookie: '',
    fullscreenElement: null,
    createElement: (tag: string) => {
      const element = createElementSpy(tag);
      createdElements.push(element);
      return element;
    },
    body: {
      appendChild: () => undefined,
      removeChild: () => undefined,
    },
    execCommand: (command: string) => {
      execCommandCalls.push(command);
      return execCommandResult;
    },
  });
}

/** 最小 FileReader 假体：readAsText 后微任务里回调 onload/onerror。 */
class FakeFileReader {
  static mode: 'ok' | 'error' = 'ok';
  result: string | null = null;
  error: Error | null = null;
  onload: (() => void) | null = null;
  onerror: (() => void) | null = null;

  readAsText(file: { text: string }): void {
    if (FakeFileReader.mode === 'error') {
      this.error = new Error('disk gone');
      queueMicrotask(() => this.onerror?.());
      return;
    }
    this.result = file.text;
    queueMicrotask(() => this.onload?.());
  }
}

const blobs: Blob[] = [];
const objectUrls: string[] = [];
const revokedUrls: string[] = [];
const originalCreateObjectURL = URL.createObjectURL;
const originalRevokeObjectURL = URL.revokeObjectURL;

function installObjectUrls(): void {
  blobs.length = 0;
  objectUrls.length = 0;
  revokedUrls.length = 0;
  (URL as unknown as { createObjectURL: (blob: Blob) => string }).createObjectURL = (
    blob: Blob,
  ) => {
    blobs.push(blob);
    const url = `blob:fake-${objectUrls.length}`;
    objectUrls.push(url);
    return url;
  };
  (URL as unknown as { revokeObjectURL: (url: string) => void }).revokeObjectURL = (
    url: string,
  ) => {
    revokedUrls.push(url);
  };
}

afterEach(() => {
  URL.createObjectURL = originalCreateObjectURL;
  URL.revokeObjectURL = originalRevokeObjectURL;
  FakeFileReader.mode = 'ok';
  clearPlatform();
  clearPlatformServices();
  vi.unstubAllGlobals();
});

/* ------------------------------------------------------------------ */
/* 保存类对话框：Blob + <a download>（浏览器的「保存」语义）。             */
/* ------------------------------------------------------------------ */

describe('保存类对话框 → Blob + <a download>', () => {
  it('saveTextWithDialog：文本 Blob + 锚 download 文件名 + revoke，返回 true', async () => {
    installDocument();
    installObjectUrls();
    const services = createWebPlatformServices();

    await expect(
      services.saveTextWithDialog({
        contents: 'SELECT 1;',
        defaultFileName: 'query.sql',
        filterName: 'SQL',
        extensions: ['.sql'],
      }),
    ).resolves.toBe(true);

    const anchor = createdElements.find((element) => element.tag === 'a');
    expect(anchor).toBeDefined();
    expect(anchor!.download).toBe('query.sql');
    expect(anchor!.href).toBe('blob:fake-0');
    expect(anchor!.clickCount).toBe(1);
    expect(revokedUrls).toEqual(['blob:fake-0']);
    expect(blobs).toHaveLength(1);
    expect(blobs[0]!.type).toContain('text/plain');
    await expect(blobs[0]!.text()).resolves.toBe('SELECT 1;');
  });

  it('saveBinaryWithDialog：base64 解码为字节 Blob，走同一锚下载，返回 true', async () => {
    installDocument();
    installObjectUrls();
    const services = createWebPlatformServices();

    // 'hi' 的 base64。
    await expect(
      services.saveBinaryWithDialog({
        dataBase64: 'aGk=',
        defaultFileName: 'diagram.svg',
        filterName: 'SVG',
        extensions: ['.svg'],
      }),
    ).resolves.toBe(true);

    const bytes = new Uint8Array(await blobs[0]!.arrayBuffer());
    expect(Array.from(bytes)).toEqual([104, 105]); // 'h', 'i'
    expect(createdElements.find((element) => element.tag === 'a')!.download).toBe('diagram.svg');
  });
});

/* ------------------------------------------------------------------ */
/* 打开文件：<input type="file"> + FileReader。                          */
/* ------------------------------------------------------------------ */

describe('openTextWithDialog → <input type="file">', () => {
  it('change 带文件 → FileReader 读出 { fileName, content }；accept 归一为点号扩展', async () => {
    installDocument();
    vi.stubGlobal('FileReader', FakeFileReader);
    const services = createWebPlatformServices();

    const pending = services.openTextWithDialog({ filterName: 'SQL', extensions: ['sql'] });
    const picker = createdElements.find((element) => element.tag === 'input');
    expect(picker).toBeDefined();
    expect(picker!.type).toBe('file');
    expect(picker!.accept).toBe('.sql');

    picker!.files = [{ name: 'q.sql', text: 'SELECT 1;' }];
    picker!.handlers.get('change')?.();

    await expect(pending).resolves.toEqual({ fileName: 'q.sql', content: 'SELECT 1;' });
  });

  it('picker cancel 事件 → null（端口的取消半边，resolve 而非 reject）', async () => {
    installDocument();
    vi.stubGlobal('FileReader', FakeFileReader);
    const services = createWebPlatformServices();

    const pending = services.openTextWithDialog({ filterName: 'SQL', extensions: ['.sql'] });
    const picker = createdElements.find((element) => element.tag === 'input')!;
    picker.handlers.get('cancel')?.();

    await expect(pending).resolves.toBeNull();
  });

  it('change 无文件（无取消事件的浏览器）→ 按 null 收束，不悬挂', async () => {
    installDocument();
    vi.stubGlobal('FileReader', FakeFileReader);
    const services = createWebPlatformServices();

    const pending = services.openTextWithDialog({ filterName: 'SQL', extensions: ['.sql'] });
    const picker = createdElements.find((element) => element.tag === 'input')!;
    picker!.files = [];
    picker!.handlers.get('change')?.();

    await expect(pending).resolves.toBeNull();
  });
});

/* ------------------------------------------------------------------ */
/* 剪贴板：async API → execCommand 兜底 → 如实 CapabilityUnsupported。    */
/* ------------------------------------------------------------------ */

describe('剪贴板（async API / execCommand 兜底 / 如实失败）', () => {
  it('writeClipboard：async Clipboard API 可用 → 直写，不碰兜底', async () => {
    installDocument();
    const written: string[] = [];
    vi.stubGlobal('navigator', {
      clipboard: { writeText: async (text: string) => void written.push(text) },
    });
    await expect(createWebPlatformServices().writeClipboard('hello')).resolves.toBeUndefined();
    expect(written).toEqual(['hello']);
    expect(execCommandCalls).toEqual([]);
  });

  it('writeClipboard：API 缺席 → execCommand("copy") 兜底成功', async () => {
    installDocument();
    vi.stubGlobal('navigator', {});
    await expect(createWebPlatformServices().writeClipboard('legacy')).resolves.toBeUndefined();
    expect(execCommandCalls).toEqual(['copy']);
  });

  it('writeClipboard：双失败 → CapabilityUnsupported（绝无静默 no-op）', async () => {
    installDocument();
    execCommandResult = false;
    vi.stubGlobal('navigator', {});
    const rejection = await createWebPlatformServices()
      .writeClipboard('nope')
      .then(
        () => null,
        (error: unknown) => error,
      );
    expect(rejection).toBeInstanceOf(ApiError);
    expect((rejection as ApiError).code).toBe('CapabilityUnsupported');
  });

  it('readClipboard：API 缺席 → CapabilityUnsupported（读无历史半边）', async () => {
    installDocument();
    vi.stubGlobal('navigator', {});
    await expect(createWebPlatformServices().readClipboard()).rejects.toMatchObject({
      code: 'CapabilityUnsupported',
    });
  });

  it('readClipboard：API 可用 → readText 原样回传', async () => {
    installDocument();
    vi.stubGlobal('navigator', {
      clipboard: { readText: async () => 'from clipboard' },
    });
    await expect(createWebPlatformServices().readClipboard()).resolves.toBe('from clipboard');
  });
});

/* ------------------------------------------------------------------ */
/* web 装配（WR-E3 绑定面）：host 空缺省 + 导出 Blob 半边全链路。          */
/* ------------------------------------------------------------------ */

const EXPORT_REQUEST: TablesExportRequest = {
  dbSessionId: 'sess-9',
  mode: 'data_only',
  dataFormat: 'csv',
  outputMode: 'single',
  tables: [{ tableName: 'users', columns: ['id', 'name'] }],
};

describe('web 装配 WR-E3 绑定面', () => {
  it('宿主叠加能力注入空缺省：readHostExtras() == {}（platform.host.config.get 的 web 半边）', async () => {
    installDocument();
    assembleWebPlatform();
    await expect(requireHostSettings().readHostExtras()).resolves.toEqual({});
    await expect(requireHostSettings().writeHostSettings({} as never)).resolves.toBeUndefined();
    await expect(requireHostSettings().getLogPath()).resolves.toBe('');
  });

  it('导出绑定 Blob 半边：分页 getTableData → 单个 CSV Blob + <a download> + { Saved: n }', async () => {
    installDocument();
    installObjectUrls();
    const calls: Array<{ url: string; body: Record<string, unknown> }> = [];
    vi.stubGlobal(
      'fetch',
      vi.fn(async (input: RequestInfo | URL, init?: RequestInit): Promise<Response> => {
        const body = JSON.parse(String(init?.body ?? '{}')) as Record<string, unknown>;
        calls.push({ url: String(input), body });
        const page = body['page'] as number;
        // 第 1 页满页（续读），第 2 页短页（收束）。
        const rows =
          page === 1
            ? Array.from({ length: WEB_EXPORT_PAGE_SIZE }, (_u, index) => [index, `row-${index}`])
            : [[WEB_EXPORT_PAGE_SIZE, 'last']];
        return new Response(
          JSON.stringify({
            columns: [
              { name: 'id', dataType: 'int', nullable: false },
              { name: 'name', dataType: 'text', nullable: true },
            ],
            rows,
            page,
            pageSize: WEB_EXPORT_PAGE_SIZE,
            paging:
              page === 1
                ? {
                    mode: 'keyset',
                    orderColumns: [],
                    driftSuspected: false,
                    nextAfterKey: [{ column: 'id', value: WEB_EXPORT_PAGE_SIZE - 1 }],
                  }
                : null,
          }),
          { status: 200, headers: { 'content-type': 'application/json' } },
        );
      }),
    );

    assembleWebPlatform();
    await expect(requireTablesExporter().exportTables(EXPORT_REQUEST)).resolves.toEqual({
      Saved: 1,
    });

    // 两页读：page 1 满页续读；page 2 携带后端回传的 afterKey（原样）。
    expect(calls).toHaveLength(2);
    expect(calls[0]!.url).toBe('/api/v1/table-data/reads');
    expect(calls[0]!.body).toMatchObject({
      page: 1,
      pageSize: WEB_EXPORT_PAGE_SIZE,
      skipCount: true,
    });
    expect(calls[0]!.body['context']).toMatchObject({
      dbSessionId: 'sess-9',
      table: 'users',
      connectionId: null,
    });
    expect(calls[1]!.body['page']).toBe(2);
    expect(calls[1]!.body['afterKey']).toEqual([{ column: 'id', value: WEB_EXPORT_PAGE_SIZE - 1 }]);

    // Blob 下载形状：单个 object URL、CSV 表头 + 两页行、锚 download、revoke。
    expect(blobs).toHaveLength(1);
    const text = await blobs[0]!.text();
    expect(text).toContain('id,name');
    expect(text).toContain('0,row-0');
    expect(text).toContain(`${WEB_EXPORT_PAGE_SIZE},last`);
    const anchor = createdElements.find((element) => element.tag === 'a');
    expect(anchor!.download).toBe('users.csv');
    expect(anchor!.href).toBe('blob:fake-0');
    expect(revokedUrls).toContain('blob:fake-0');
  });

  it('导出读失败 → 拒绝上抛（无 Blob、无下载，不谎报 Saved）', async () => {
    installDocument();
    installObjectUrls();
    vi.stubGlobal(
      'fetch',
      vi.fn(async (): Promise<Response> => {
        throw new TypeError('fetch failed');
      }),
    );
    assembleWebPlatform();
    await expect(requireTablesExporter().exportTables(EXPORT_REQUEST)).rejects.toBeInstanceOf(
      ApiError,
    );
    expect(blobs).toHaveLength(0);
    expect(createdElements.filter((element) => element.tag === 'a')).toHaveLength(0);
  });
});
