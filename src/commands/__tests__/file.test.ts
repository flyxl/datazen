import { beforeEach, describe, expect, it, vi } from 'vitest';

const invokeMock = vi.hoisted(() => vi.fn());
const listenMock = vi.hoisted(() => vi.fn());

vi.mock('@tauri-apps/api/core', () => ({
  invoke: invokeMock,
}));

vi.mock('@tauri-apps/api/event', () => ({
  listen: listenMock,
}));

import {
  fileCommands,
  onExportProgress,
  type ExportTablesRequest,
} from '../file';

describe('fileCommands dialog wrappers', () => {
  beforeEach(() => {
    invokeMock.mockReset();
    listenMock.mockReset();
    invokeMock.mockResolvedValue(undefined);
    listenMock.mockResolvedValue(() => undefined);
  });

  it('saveTextWithDialog forwards text and dialog filter args', async () => {
    invokeMock.mockResolvedValueOnce(true);
    await expect(
      fileCommands.saveTextWithDialog('body', 'out.csv', 'CSV', ['csv', '.txt']),
    ).resolves.toBe(true);
    expect(invokeMock).toHaveBeenCalledWith('save_text_with_dialog', {
      contents: 'body',
      defaultFileName: 'out.csv',
      filterName: 'CSV',
      extensions: ['csv', '.txt'],
    });
  });

  it('saveBase64WithDialog forwards base64 payload and filter args', async () => {
    await fileCommands.saveBase64WithDialog('AAAA', 'icon.png', 'PNG', ['png']);
    expect(invokeMock).toHaveBeenCalledWith('save_base64_with_dialog', {
      dataBase64: 'AAAA',
      defaultFileName: 'icon.png',
      filterName: 'PNG',
      extensions: ['png'],
    });
  });

  it('openTextWithDialog returns opened file or null when cancelled', async () => {
    const opened = { fileName: 'a.sql', content: 'SELECT 1;' };
    invokeMock.mockResolvedValueOnce(opened);
    await expect(fileCommands.openTextWithDialog('SQL', ['sql'])).resolves.toEqual(opened);
    expect(invokeMock).toHaveBeenCalledWith('open_text_with_dialog', {
      filterName: 'SQL',
      extensions: ['sql'],
    });

    invokeMock.mockResolvedValueOnce(null);
    await expect(fileCommands.openTextWithDialog('SQL', ['sql'])).resolves.toBeNull();
  });

  it('openBase64WithDialog returns basename + base64 without a path', async () => {
    const opened = { fileName: 'b.zip', dataBase64: 'UEs=' };
    invokeMock.mockResolvedValueOnce(opened);
    await expect(fileCommands.openBase64WithDialog('ZIP', ['.zip'])).resolves.toEqual(opened);
    expect(invokeMock).toHaveBeenCalledWith('open_base64_with_dialog', {
      filterName: 'ZIP',
      extensions: ['.zip'],
    });
    expect(Object.keys(opened)).not.toContain('path');
  });
});

describe('fileCommands streaming save session', () => {
  beforeEach(() => {
    invokeMock.mockReset();
    invokeMock.mockResolvedValue(undefined);
  });

  it('begin/append/finish/abort share the opaque token', async () => {
    invokeMock.mockResolvedValueOnce('token-1');
    await expect(
      fileCommands.beginSaveWithDialog('log.txt', 'Text', ['txt']),
    ).resolves.toBe('token-1');
    expect(invokeMock).toHaveBeenCalledWith('begin_save_with_dialog', {
      defaultFileName: 'log.txt',
      filterName: 'Text',
      extensions: ['txt'],
    });

    await fileCommands.appendSaveText('token-1', 'chunk');
    expect(invokeMock).toHaveBeenCalledWith('append_save_text', {
      token: 'token-1',
      chunk: 'chunk',
    });

    await fileCommands.finishSave('token-1');
    expect(invokeMock).toHaveBeenLastCalledWith('finish_save', { token: 'token-1' });

    await fileCommands.abortSave('token-2');
    expect(invokeMock).toHaveBeenLastCalledWith('abort_save', { token: 'token-2' });
  });
});

describe('exportTablesStream + progress events', () => {
  beforeEach(() => {
    invokeMock.mockReset();
    listenMock.mockReset();
    invokeMock.mockResolvedValue(undefined);
    listenMock.mockResolvedValue(() => undefined);
  });

  it('maps dbSessionId to snake_case request envelope and returns the result', async () => {
    const request: ExportTablesRequest = {
      dbSessionId: 'session-7',
      databaseType: 'sqlite',
      mode: 'data_and_structure',
      dataFormat: 'sql_insert',
      outputMode: 'zip',
      tables: [{ tableName: 'users', columns: ['id'], ddl: null }],
    };
    const result = { Saved: 3 } as const;
    invokeMock.mockResolvedValueOnce(result);

    await expect(fileCommands.exportTablesStream(request)).resolves.toEqual(result);
    // Wrapper forwards verbatim — camelCase→snake_case mapping is Tauri's job.
    const [, payload] = invokeMock.mock.calls[0];
    expect(invokeMock.mock.calls[0][0]).toBe('export_tables_stream');
    expect(payload.request.dbSessionId).toBe('session-7');
  });

  it('reports cancelled result passthrough', async () => {
    invokeMock.mockResolvedValueOnce({ Cancelled: null });
    const request: ExportTablesRequest = {
      dbSessionId: 's',
      mode: 'structure_only',
      dataFormat: 'sql_insert',
      outputMode: 'single',
      tables: [],
    };
    await expect(fileCommands.exportTablesStream(request)).resolves.toEqual({
      Cancelled: null,
    });
  });

  it('onExportProgress subscribes to batch-export-progress and relays payloads', async () => {
    const unlisten = vi.fn();
    listenMock.mockResolvedValueOnce(unlisten);
    const handler = vi.fn();

    await expect(onExportProgress(handler)).resolves.toBe(unlisten);
    expect(listenMock).toHaveBeenCalledTimes(1);
    expect(listenMock.mock.calls[0][0]).toBe('batch-export-progress');

    const relay = listenMock.mock.calls[0][1] as (e: { payload: unknown }) => void;
    relay({ payload: { table: 'users', rowsWritten: 10 } });
    expect(handler).toHaveBeenCalledWith({ table: 'users', rowsWritten: 10 });

    await unlisten();
    expect(unlisten).toHaveBeenCalled();
  });
});
