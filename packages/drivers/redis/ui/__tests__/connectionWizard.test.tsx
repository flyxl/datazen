import { afterEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { RedisConnectionWizard, RedisTlsFields } from '../ConnectionWizard';
import type { ConnectionFormState } from '../../../../../src/components/connection/useConnectionForm';

vi.mock('../../../../../src/hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

function stubForm(overrides: Partial<ConnectionFormState> = {}): ConnectionFormState {
  return {
    name: '',
    host: '127.0.0.1',
    port: '6379',
    database: '0',
    username: '',
    password: '',
    options: { topology: 'standalone' },
    validationErrors: {},
    setName: vi.fn(),
    setHost: vi.fn(),
    setPort: vi.fn(),
    setDatabase: vi.fn(),
    setUsername: vi.fn(),
    setPassword: vi.fn(),
    setOptions: vi.fn(),
    setSslMode: vi.fn(),
    setShowAdvanced: vi.fn(),
    ...overrides,
  } as ConnectionFormState;
}

function mockClipboard(text: string | Promise<string> | Error) {
  const readText =
    text instanceof Error
      ? vi.fn().mockRejectedValue(text)
      : vi.fn().mockResolvedValue(text);
  Object.defineProperty(navigator, 'clipboard', {
    configurable: true,
    value: { readText },
  });
  return readText;
}

describe('RedisConnectionWizard', () => {
  afterEach(() => {
    cleanup();
    vi.useRealTimers();
  });

  it('shows topology select and host fields without tab chrome', () => {
    const form = stubForm({
      validationErrors: { host: 'need-host', port: 'need-port' },
    });
    render(<RedisConnectionWizard form={form} />);

    expect(screen.getByTestId('redis-topology')).toBeTruthy();
    expect(screen.getByText('newConn.host')).toBeTruthy();
    expect(screen.getByText('need-host')).toBeTruthy();
    expect(screen.getByText('need-port')).toBeTruthy();
    expect(screen.queryByText('redis.wizard.endpoints')).toBeNull();
    expect(screen.queryByText('redis.wizard.tls')).toBeNull();
    fireEvent.change(screen.getByPlaceholderText('127.0.0.1'), { target: { value: '10.0.0.1' } });
    fireEvent.change(screen.getByDisplayValue('6379'), { target: { value: '6380' } });
    fireEvent.change(screen.getByPlaceholderText('0'), { target: { value: '1' } });
    expect(form.setHost).toHaveBeenCalledWith('10.0.0.1');
    expect(form.setPort).toHaveBeenCalledWith('6380');
  });

  it('shows cluster node list when topology is cluster', () => {
    render(
      <RedisConnectionWizard
        form={stubForm({ options: { topology: 'cluster', clusterNodes: ['10.0.0.1:7000'] } })}
      />,
    );

    expect(screen.getByText('redis.wizard.clusterNodes')).toBeTruthy();
    expect(screen.queryByText('newConn.host')).toBeNull();
  });

  it('fills the form from the clipboard button', async () => {
    mockClipboard('redis://alice:s3cret@cache.internal:6380/2');
    const form = stubForm();
    render(<RedisConnectionWizard form={form} />);

    fireEvent.click(screen.getByTestId('redis-fill-clipboard'));

    await waitFor(() => {
      expect(form.setHost).toHaveBeenCalledWith('cache.internal');
    });
    expect(form.setPort).toHaveBeenCalledWith('6380');
    expect(form.setUsername).toHaveBeenCalledWith('alice');
    expect(form.setPassword).toHaveBeenCalledWith('s3cret');
    expect(form.setDatabase).toHaveBeenCalledWith('2');
    expect(screen.getByTestId('redis-clipboard-status').textContent).toBe(
      'redis.wizard.pastedFromClipboard',
    );
  });

  it('shows an empty banner when clipboard is not a redis endpoint', async () => {
    mockClipboard('not a redis url');
    render(<RedisConnectionWizard form={stubForm()} />);

    fireEvent.click(screen.getByTestId('redis-fill-clipboard'));

    await waitFor(() => {
      expect(screen.getByTestId('redis-clipboard-status').textContent).toBe(
        'redis.wizard.clipboardEmpty',
      );
    });
  });

  it('auto-fills a pristine form from the clipboard on mount', async () => {
    mockClipboard('redis://prod.internal:6379');
    const form = stubForm();
    render(<RedisConnectionWizard form={form} />);

    await waitFor(() => {
      expect(form.setHost).toHaveBeenCalledWith('prod.internal');
    });
    expect(form.setPort).toHaveBeenCalledWith('6379');
  });

  it('does not auto-fill when the form already has a name', async () => {
    const readText = mockClipboard('redis://prod.internal:6379');
    const form = stubForm({ name: 'existing' });
    render(<RedisConnectionWizard form={form} />);

    await waitFor(() => {
      expect(screen.getByTestId('redis-fill-clipboard')).toBeTruthy();
    });
    expect(readText).not.toHaveBeenCalled();
    expect(form.setHost).not.toHaveBeenCalled();
  });

  it('fills from a paste event containing a redis URL', () => {
    mockClipboard(new Error('no clipboard'));
    const form = stubForm({ name: 'existing' });
    render(<RedisConnectionWizard form={form} />);

    fireEvent.paste(screen.getByTestId('redis-fill-clipboard'), {
      clipboardData: { getData: () => 'redis://paste.me:6380' },
    });

    expect(form.setHost).toHaveBeenCalledWith('paste.me');
    expect(form.setPort).toHaveBeenCalledWith('6380');
  });

  it('does not intercept a password-field paste that is not a redis URL', () => {
    mockClipboard(new Error('no clipboard'));
    const form = stubForm({ name: 'existing' });
    render(<RedisConnectionWizard form={form} />);
    const password = document.querySelector('input[type="password"]') as HTMLInputElement;
    fireEvent.paste(password, {
      clipboardData: { getData: () => '10.0.0.8:6380' },
    });
    expect(form.setHost).not.toHaveBeenCalled();
  });

  it('retries auto-fill when the window is focused', async () => {
    const readText = mockClipboard('redis://focus.me:6379');
    const form = stubForm();
    render(<RedisConnectionWizard form={form} />);
    await waitFor(() => {
      expect(form.setHost).toHaveBeenCalledWith('focus.me');
    });
    expect(readText).toHaveBeenCalled();
  });

  it('shows an empty banner when reading the clipboard fails from the button', async () => {
    mockClipboard(new Error('denied'));
    render(<RedisConnectionWizard form={stubForm({ name: 'existing' })} />);
    fireEvent.click(screen.getByTestId('redis-fill-clipboard'));
    await waitFor(() => {
      expect(screen.getByTestId('redis-clipboard-status').textContent).toBe(
        'redis.wizard.clipboardEmpty',
      );
    });
  });

  it('clears the clipboard status banner after a timeout', async () => {
    vi.useFakeTimers();
    mockClipboard('not redis');
    render(<RedisConnectionWizard form={stubForm({ name: 'existing' })} />);
    fireEvent.click(screen.getByTestId('redis-fill-clipboard'));
    await act(async () => {
      await Promise.resolve();
      await Promise.resolve();
    });
    expect(screen.getByTestId('redis-clipboard-status')).toBeTruthy();
    act(() => {
      vi.advanceTimersByTime(3000);
    });
    expect(screen.queryByTestId('redis-clipboard-status')).toBeNull();
    vi.useRealTimers();
  });

  it('switches topology to cluster and standalone', () => {
    const form = stubForm({ host: '', port: '' });
    render(<RedisConnectionWizard form={form} />);
    fireEvent.click(screen.getByTitle('redis.wizard.topology'));
    fireEvent.mouseDown(screen.getByText('redis.wizard.topologyCluster'));
    expect(form.setOptions).toHaveBeenCalledWith(expect.objectContaining({ topology: 'cluster' }));

    cleanup();
    const clusterForm = stubForm({ host: '', port: '', options: { topology: 'cluster' } });
    render(<RedisConnectionWizard form={clusterForm} />);
    fireEvent.click(screen.getByTitle('redis.wizard.topology'));
    fireEvent.mouseDown(screen.getByText('redis.wizard.topologyStandalone'));
    expect(clusterForm.setHost).toHaveBeenCalledWith('127.0.0.1');
    expect(clusterForm.setPort).toHaveBeenCalledWith('6379');
  });

  it('renders sentinel fields and validation errors', () => {
    const form = stubForm({
      options: { topology: 'sentinel', sentinelMasterName: '', sentinelNodes: [] },
      validationErrors: {
        sentinelMasterName: 'required',
        sentinelNodes: 'nodes',
      },
    });
    render(<RedisConnectionWizard form={form} />);
    expect(screen.getByText('redis.wizard.sentinelMasterName')).toBeTruthy();
    expect(screen.getByText('required')).toBeTruthy();
    expect(screen.getByText('nodes')).toBeTruthy();
    fireEvent.change(screen.getByPlaceholderText('mymaster'), { target: { value: 'mymaster' } });
    fireEvent.change(document.querySelector('textarea') as HTMLTextAreaElement, {
      target: { value: '127.0.0.1:26379' },
    });
    const passwordInputs = document.querySelectorAll('input[type="password"]');
    fireEvent.change(passwordInputs[0], { target: { value: 'sent' } });
    fireEvent.change(passwordInputs[1], { target: { value: 'redis' } });
    fireEvent.change(screen.getByPlaceholderText('0'), { target: { value: '2' } });
    const username = Array.from(document.querySelectorAll('input')).find(
      (el) => el.type !== 'password' && el.type !== 'number' && el.placeholder !== 'mymaster',
    );
    fireEvent.change(username as HTMLInputElement, { target: { value: 'alice' } });
    expect(form.setOptions).toHaveBeenCalled();
    expect(form.setDatabase).toHaveBeenCalledWith('2');
    expect(form.setPassword).toHaveBeenCalledWith('redis');
    expect(form.setUsername).toHaveBeenCalledWith('alice');
  });

  it('renders cluster validation errors and node edits', () => {
    const form = stubForm({
      options: { topology: 'cluster', clusterNodes: ['10.0.0.1:7000'] },
      validationErrors: { clusterNodes: 'bad-node' },
    });
    render(<RedisConnectionWizard form={form} />);
    expect(screen.getByText('bad-node')).toBeTruthy();
    fireEvent.change(document.querySelector('textarea') as HTMLTextAreaElement, {
      target: { value: '10.0.0.1:7000\n10.0.0.2:7000' },
    });
    expect(form.setOptions).toHaveBeenCalled();
  });
});

describe('RedisTlsFields', () => {
  afterEach(() => {
    cleanup();
  });

  it('toggles TLS options and shows the sentinel mTLS warning', () => {
    const form = stubForm({
      options: {
        topology: 'sentinel',
        tls: { enabled: true, caPath: '/ca.pem', certPath: '/c.crt' },
      },
    });
    render(<RedisTlsFields form={form} />);
    expect(screen.getByText('redis.wizard.sentinelMtlsLimitation')).toBeTruthy();
    const checkboxes = screen.getAllByRole('checkbox');
    fireEvent.click(checkboxes[0]);
    fireEvent.click(checkboxes[1]);
    expect(form.setOptions).toHaveBeenCalled();
    fireEvent.change(screen.getByPlaceholderText('/path/to/ca.pem'), {
      target: { value: '/tmp/ca.pem' },
    });
    fireEvent.change(screen.getByPlaceholderText('/path/to/client.crt'), {
      target: { value: '/tmp/c.crt' },
    });
    fireEvent.change(screen.getByPlaceholderText('/path/to/client.key'), {
      target: { value: '/tmp/k.key' },
    });
    fireEvent.change(document.querySelector('input[type="password"]') as HTMLInputElement, {
      target: { value: 'phrase' },
    });
  });
});
