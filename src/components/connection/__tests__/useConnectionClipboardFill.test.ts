import { afterEach, describe, expect, it, vi } from 'vitest';
import { act, fireEvent, renderHook, waitFor } from '@testing-library/react';
import { useConnectionClipboardFill } from '../useConnectionClipboardFill';
import type { ConnectionFormState } from '../useConnectionForm';
import { DB_REGISTRY } from '../../../lib/databaseTypes';

function stubForm(overrides: Partial<ConnectionFormState> = {}): ConnectionFormState {
  return {
    name: '',
    databaseType: 'postgresql',
    host: '127.0.0.1',
    port: '5432',
    database: 'postgres',
    username: 'postgres',
    password: '',
    options: {},
    setName: vi.fn(),
    setHost: vi.fn(),
    setPort: vi.fn(),
    setDatabase: vi.fn(),
    setUsername: vi.fn(),
    setPassword: vi.fn(),
    setSchema: vi.fn(),
    setSslMode: vi.fn(),
    setOptions: vi.fn(),
    setShowAdvanced: vi.fn(),
    handleDatabaseTypeChange: vi.fn(),
    ...overrides,
  } as ConnectionFormState;
}

function mockClipboard(text: string) {
  Object.defineProperty(navigator, 'clipboard', {
    configurable: true,
    value: { readText: vi.fn().mockResolvedValue(text) },
  });
}

describe('useConnectionClipboardFill', () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('switches to redis and expands advanced for a TLS redis URL', async () => {
    if (!DB_REGISTRY.redis) return;
    mockClipboard('rediss://alice:s3cret@cache.internal:6380/2');
    const form = stubForm();
    const onApplied = vi.fn();
    renderHook(() => useConnectionClipboardFill(form, { enabled: true, onApplied }));

    await waitFor(() => {
      expect(form.handleDatabaseTypeChange).toHaveBeenCalledWith('redis');
    });
    expect(form.setHost).toHaveBeenCalledWith('cache.internal');
    expect(form.setShowAdvanced).toHaveBeenCalledWith(true);
    expect(onApplied).toHaveBeenCalledWith('redis');
  });

  it('does not auto-fill when editing an existing connection', async () => {
    mockClipboard('redis://cache:6379');
    const form = stubForm({ name: 'prod' });
    renderHook(() => useConnectionClipboardFill(form, { enabled: false }));
    await act(async () => {
      await Promise.resolve();
    });
    expect(form.handleDatabaseTypeChange).not.toHaveBeenCalled();
    expect(form.setHost).not.toHaveBeenCalled();
  });

  it('skips clipboard content that is not a connection string', async () => {
    mockClipboard('hello world');
    const form = stubForm();
    renderHook(() => useConnectionClipboardFill(form, { enabled: true }));
    await act(async () => {
      await Promise.resolve();
    });
    expect(form.setHost).not.toHaveBeenCalled();
  });

  it('does not overwrite a form that already has a name or password', async () => {
    mockClipboard('redis://cache:6379');
    const named = stubForm({ name: 'prod' });
    renderHook(() => useConnectionClipboardFill(named, { enabled: true }));
    await waitFor(() => {
      expect(navigator.clipboard.readText).toHaveBeenCalled();
    });
    expect(named.setHost).not.toHaveBeenCalled();

    const secret = stubForm({ password: 'secret' });
    renderHook(() => useConnectionClipboardFill(secret, { enabled: true }));
    await waitFor(() => {
      expect(navigator.clipboard.readText).toHaveBeenCalled();
    });
    expect(secret.setHost).not.toHaveBeenCalled();
  });

  it('fills from a paste event and ignores a second paste', async () => {
    Object.defineProperty(navigator, 'clipboard', {
      configurable: true,
      value: { readText: vi.fn().mockRejectedValue(new Error('denied')) },
    });
    const form = stubForm();
    renderHook(() => useConnectionClipboardFill(form, { enabled: true }));

    await act(async () => {
      await Promise.resolve();
    });

    fireEvent.paste(window, {
      clipboardData: { getData: () => 'mysql://root@db.internal:3306/app' },
    });
    expect(form.handleDatabaseTypeChange).toHaveBeenCalledWith('mysql');
    expect(form.setHost).toHaveBeenCalledWith('db.internal');

    vi.mocked(form.handleDatabaseTypeChange).mockClear();
    fireEvent.paste(window, {
      clipboardData: { getData: () => 'redis://cache:6379' },
    });
    expect(form.handleDatabaseTypeChange).not.toHaveBeenCalled();
  });

  it('does not treat a password-field paste without a URL scheme as a connection string', async () => {
    Object.defineProperty(navigator, 'clipboard', {
      configurable: true,
      value: { readText: vi.fn().mockRejectedValue(new Error('denied')) },
    });
    const form = stubForm();
    renderHook(() => useConnectionClipboardFill(form, { enabled: true }));
    await act(async () => {
      await Promise.resolve();
    });

    const password = document.createElement('input');
    password.type = 'password';
    document.body.appendChild(password);
    fireEvent.paste(password, {
      clipboardData: { getData: () => '10.0.0.8:8812' },
    });
    expect(form.setHost).not.toHaveBeenCalled();
    password.remove();
  });

  it('cancels an in-flight clipboard read on unmount', async () => {
    let resolveRead: (value: string) => void = () => undefined;
    Object.defineProperty(navigator, 'clipboard', {
      configurable: true,
      value: {
        readText: vi.fn(
          () =>
            new Promise<string>((resolve) => {
              resolveRead = resolve;
            }),
        ),
      },
    });
    const form = stubForm();
    const { unmount } = renderHook(() => useConnectionClipboardFill(form, { enabled: true }));
    unmount();
    await act(async () => {
      resolveRead('redis://cache:6379');
    });
    expect(form.setHost).not.toHaveBeenCalled();
  });
});
