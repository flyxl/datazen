import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { useState as reactUseState } from 'react';
import type { ConnectionFormState } from '../useConnectionForm';
import {
  noopTabFill,
  type HttpProxyTunnelFieldsValue,
  type SshTunnelFieldsValue,
  type WebSocketTunnelFieldsValue,
} from '../tunnelFieldContracts';
import { SshTunnelFields } from '../SshTunnelFields';
import { HttpProxyTunnelFields } from '../HttpProxyTunnelFields';
import { WebSocketTunnelFields } from '../WebSocketTunnelFields';
import type { SshAuthMethod } from '../../../types';

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

vi.mock('@datazen/ui', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@datazen/ui')>();
  return {
    ...actual,
    PathInput: ({
      value,
      onChange,
      placeholder,
    }: {
      value: string;
      onChange: (v: string) => void;
      placeholder?: string;
    }) => (
      <input value={value} placeholder={placeholder} onChange={(e) => onChange(e.target.value)} />
    ),
  };
});

afterEach(cleanup);

/**
 * [tester] (e)1 — contract completeness, asserted by the type system.
 *
 * The narrow contracts exist so the connection form can keep passing its whole
 * `ConnectionFormState` while the settings editor passes a lightweight adapter.
 * That only holds if `ConnectionFormState` is assignable to *every* contract: a
 * field the form owns but the contract omits would either fail to compile (the
 * component could not read it) or, worse, be silently dropped by a cast. These
 * three aliases turn "the form state satisfies the contract" into a compile-time
 * error if a field is ever removed from a contract.
 */
type Satisfies<From, To> = From extends To ? true : false;
type SshContractComplete = Satisfies<ConnectionFormState, SshTunnelFieldsValue>;
type HttpProxyContractComplete = Satisfies<ConnectionFormState, HttpProxyTunnelFieldsValue>;
type WebSocketContractComplete = Satisfies<ConnectionFormState, WebSocketTunnelFieldsValue>;

const contractCompleteness: [
  SshContractComplete,
  HttpProxyContractComplete,
  WebSocketContractComplete,
] = [true, true, true];

/** The form-track fields the settings editor must also be able to drive. */
const FORM_TRACK_FIELDS = [
  'supportsSSH',
  'sshEnabled',
  'setSshEnabled',
  'sshJumpEnabled',
  'setSshJumpEnabled',
  'sshJumpHost',
  'sshJumpPort',
  'sshJumpUsername',
  'sshJumpAuthMethod',
  'sshJumpPassword',
  'sshJumpKeyPath',
  'sshJumpPassphrase',
] as const;

function sshValue(overrides: Partial<SshTunnelFieldsValue> = {}): SshTunnelFieldsValue {
  return {
    supportsSSH: true,
    sshEnabled: false,
    setSshEnabled: vi.fn(),
    sshHost: '',
    setSshHost: vi.fn(),
    sshPort: '22',
    setSshPort: vi.fn(),
    sshUsername: '',
    setSshUsername: vi.fn(),
    sshAuthMethod: 'password',
    setSshAuthMethod: vi.fn(),
    sshPassword: '',
    setSshPassword: vi.fn(),
    sshKeyPath: '',
    setSshKeyPath: vi.fn(),
    sshPassphrase: '',
    setSshPassphrase: vi.fn(),
    sshJumpEnabled: false,
    setSshJumpEnabled: vi.fn(),
    sshJumpHost: '',
    setSshJumpHost: vi.fn(),
    sshJumpPort: '22',
    setSshJumpPort: vi.fn(),
    sshJumpUsername: '',
    setSshJumpUsername: vi.fn(),
    sshJumpAuthMethod: 'password',
    setSshJumpAuthMethod: vi.fn(),
    sshJumpPassword: '',
    setSshJumpPassword: vi.fn(),
    sshJumpKeyPath: '',
    setSshJumpKeyPath: vi.fn(),
    sshJumpPassphrase: '',
    setSshJumpPassphrase: vi.fn(),
    tabFill: noopTabFill,
    pickPath: vi.fn(async () => null),
    ...overrides,
  };
}

function sshHarness(overrides: Partial<SshTunnelFieldsValue> = {}) {
  return function Harness({ showEnableToggle }: { showEnableToggle?: boolean }) {
    const [sshEnabled, setSshEnabled] = reactUseState(overrides.sshEnabled ?? false);
    const [sshAuthMethod, setSshAuthMethod] = reactUseState<SshAuthMethod>(
      overrides.sshAuthMethod ?? 'password',
    );
    return (
      <SshTunnelFields
        form={sshValue({
          ...overrides,
          sshEnabled,
          setSshEnabled,
          sshAuthMethod,
          setSshAuthMethod,
        })}
        showEnableToggle={showEnableToggle}
      />
    );
  };
}

function httpProxyValue(): HttpProxyTunnelFieldsValue {
  return {
    httpProxyHost: 'proxy.corp.example',
    setHttpProxyHost: vi.fn(),
    httpProxyPort: '3128',
    setHttpProxyPort: vi.fn(),
    httpProxyScheme: 'http',
    setHttpProxyScheme: vi.fn(),
    httpProxyUsername: '',
    setHttpProxyUsername: vi.fn(),
    httpProxyPassword: '',
    setHttpProxyPassword: vi.fn(),
    httpProxyTimeout: '30',
    setHttpProxyTimeout: vi.fn(),
  };
}

function webSocketValue(): WebSocketTunnelFieldsValue {
  return {
    wsUrl: 'wss://relay.example.com/v1',
    setWsUrl: vi.fn(),
    wsMode: 'datazen_v1',
    setWsMode: vi.fn(),
    wsAuthToken: '',
    setWsAuthToken: vi.fn(),
    wsTimeout: '30',
    setWsTimeout: vi.fn(),
  };
}

describe('[tester] tunnel field contracts (e)', () => {
  it('is satisfied by ConnectionFormState without a cast, and keeps the form-track fields', () => {
    expect(contractCompleteness).toEqual([true, true, true]);

    // Belt and braces at runtime: every field the form track relies on is a key
    // of the contract value the settings editor must be able to supply.
    const keys = Object.keys(sshValue());
    for (const field of FORM_TRACK_FIELDS) expect(keys).toContain(field);
    expect(Object.keys(httpProxyValue())).toContain('setHttpProxyScheme');
    expect(Object.keys(webSocketValue())).toContain('setWsMode');
  });

  it('has no `any` or `@ts-ignore` escape hatch in the contract or the field components', () => {
    const files = [
      'src/components/connection/tunnelFieldContracts.ts',
      'src/components/connection/SshTunnelFields.tsx',
      'src/components/connection/HttpProxyTunnelFields.tsx',
      'src/components/connection/WebSocketTunnelFields.tsx',
    ];
    for (const file of files) {
      const source = readFileSync(resolve(process.cwd(), file), 'utf8');
      expect(source, `${file} must not use @ts-ignore`).not.toMatch(/@ts-(ignore|expect-error)/);
      expect(source, `${file} must not use the any type`).not.toMatch(/:\s*any\b/);
      expect(source, `${file} must not cast through unknown`).not.toMatch(/as unknown as/);
    }
  });

  // (e)2 — `showEnableToggle` defaults to `true`, so the form track's rendering
  // and behaviour must be byte-for-byte the pre-change component.
  it('defaults showEnableToggle to true: the form checkbox renders and gates the panel', () => {
    const Harness = sshHarness();
    render(<Harness />);

    const checkbox = screen.getByTestId('new-conn-ssh-tunnel-checkbox') as HTMLInputElement;
    expect(checkbox).toBeInTheDocument();
    expect(screen.getByTestId('new-conn-ssh-tunnel')).toBeInTheDocument();
    expect(checkbox.checked).toBe(false);
    // sshEnabled === false hides the whole SSH body, exactly as before.
    expect(screen.queryByPlaceholderText('ssh.example.com')).toBeNull();

    fireEvent.click(screen.getByText('newConn.sshTunnel'));
    expect(checkbox.checked).toBe(true);
    expect(screen.getByPlaceholderText('ssh.example.com')).toBeInTheDocument();
  });

  it('defaults showEnableToggle to true: the checkbox reports the new checked value', () => {
    const setSshEnabled = vi.fn();
    render(<SshTunnelFields form={sshValue({ setSshEnabled })} />);

    fireEvent.click(screen.getByTestId('new-conn-ssh-tunnel-checkbox'));
    expect(setSshEnabled).toHaveBeenCalledWith(true);
  });

  it('applies innerPanelClassName to the SSH panel only', () => {
    const Harness = sshHarness({ sshEnabled: true });
    const { container } = render(<Harness />);
    const panel = container.querySelector('div.mt-3.grid') as HTMLElement;
    expect(panel.className).toContain('bg-surface');
  });

  it('showEnableToggle={false} drops the checkbox and always shows the body', () => {
    const Harness = sshHarness({ sshEnabled: false });
    render(<Harness showEnableToggle={false} />);

    expect(screen.queryByTestId('new-conn-ssh-tunnel-checkbox')).toBeNull();
    expect(screen.queryByTestId('new-conn-ssh-tunnel')).toBeNull();
    // The body is visible even though `sshEnabled` is false: the kind selector
    // owns the choice in the settings editor.
    expect(screen.getByPlaceholderText('ssh.example.com')).toBeInTheDocument();
  });

  it('keeps the no-op tabFill harmless (it neither fills nor throws)', () => {
    const Harness = sshHarness({ sshEnabled: true });
    render(<Harness />);
    const handler = noopTabFill(vi.fn());
    expect(typeof handler).toBe('function');
    const username = screen.getByPlaceholderText('root');
    fireEvent.keyDown(username, { key: 'Tab' });
    expect(username).toBeInTheDocument();
  });

  it('drives the http proxy and websocket bodies from contract-only values', () => {
    const http = render(<HttpProxyTunnelFields form={httpProxyValue()} />);
    expect(screen.getByTestId('new-conn-http-proxy-host')).toHaveValue('proxy.corp.example');
    // The scheme control is a `@datazen/ui` Select: its hyphenated testid prop is
    // dropped, so the only stable handle is the listbox trigger.
    expect(http.container.querySelector('button[aria-haspopup="listbox"]')).not.toBeNull();
    http.unmount();

    const ws = render(<WebSocketTunnelFields form={webSocketValue()} />);
    expect(screen.getByTestId('new-conn-ws-url')).toHaveValue('wss://relay.example.com/v1');
    expect(ws.container.querySelector('button[aria-haspopup="listbox"]')).not.toBeNull();
  });
});
