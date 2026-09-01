import { describe, expect, it, vi, afterEach } from 'vitest';
import { render, fireEvent, cleanup, screen } from '@testing-library/react';
import { useState as reactUseState } from 'react';
import { SshTunnelFields } from '../SshTunnelFields';
import type { ConnectionFormState } from '../useConnectionForm';
import type { SshAuthMethod } from '../../../types';

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

vi.mock('../../ui/PathInput', () => ({
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
}));

afterEach(cleanup);

function Harness({ supportsSSH = true }: { supportsSSH?: boolean }) {
  const [sshEnabled, setSshEnabled] = reactUseState(false);
  const [sshAuthMethod, setSshAuthMethod] = reactUseState<SshAuthMethod>('password');
  const [sshPassword, setSshPassword] = reactUseState('');
  const [sshKeyPath, setSshKeyPath] = reactUseState('');
  const [sshPassphrase, setSshPassphrase] = reactUseState('');
  const [sshJumpEnabled, setSshJumpEnabled] = reactUseState(false);
  const [sshJumpHost, setSshJumpHost] = reactUseState('');
  const [sshJumpAuthMethod, setSshJumpAuthMethod] = reactUseState<SshAuthMethod>('password');
  const [sshJumpPassword, setSshJumpPassword] = reactUseState('');
  const [sshJumpKeyPath, setSshJumpKeyPath] = reactUseState('');
  const [sshJumpPassphrase, setSshJumpPassphrase] = reactUseState('');
  const form = {
    supportsSSH,
    sshEnabled,
    setSshEnabled,
    sshHost: '',
    setSshHost: vi.fn(),
    sshPort: '22',
    setSshPort: vi.fn(),
    sshUsername: '',
    setSshUsername: vi.fn(),
    sshAuthMethod,
    setSshAuthMethod,
    sshPassword,
    setSshPassword,
    sshKeyPath,
    setSshKeyPath,
    sshPassphrase,
    setSshPassphrase,
    sshJumpEnabled,
    setSshJumpEnabled,
    sshJumpHost,
    setSshJumpHost,
    sshJumpPort: '22',
    setSshJumpPort: vi.fn(),
    sshJumpUsername: '',
    setSshJumpUsername: vi.fn(),
    sshJumpAuthMethod,
    setSshJumpAuthMethod,
    sshJumpPassword,
    setSshJumpPassword,
    sshJumpKeyPath,
    setSshJumpKeyPath,
    sshJumpPassphrase,
    setSshJumpPassphrase,
    tabFill: () => undefined,
  } as unknown as ConnectionFormState;
  return <SshTunnelFields form={form} />;
}

describe('SshTunnelFields', () => {
  it('hides when the driver does not support SSH', () => {
    const { container } = render(<Harness supportsSSH={false} />);
    expect(container.firstChild).toBeNull();
  });

  it('covers password, key, agent, and jump auth fields', () => {
    render(<Harness />);
    fireEvent.click(screen.getByText('newConn.sshTunnel'));
    fireEvent.change(screen.getByPlaceholderText('ssh.example.com'), {
      target: { value: 'ssh.example.com' },
    });
    fireEvent.change(screen.getAllByPlaceholderText('22')[0], { target: { value: '22' } });
    fireEvent.change(screen.getByPlaceholderText('root'), { target: { value: 'ops' } });

    expect(screen.getByText('newConn.sshPassword')).toBeInTheDocument();
    fireEvent.change(document.querySelectorAll('input[type="password"]')[0], {
      target: { value: 'secret' },
    });

    fireEvent.click(screen.getByText('newConn.authKey'));
    fireEvent.change(screen.getByPlaceholderText('~/.ssh/id_rsa'), {
      target: { value: '/tmp/key' },
    });
    fireEvent.change(screen.getByPlaceholderText('newConn.passphraseHint'), {
      target: { value: 'ph' },
    });

    fireEvent.click(screen.getByText('newConn.authAgent'));
    expect(screen.getByText('newConn.authAgentHint')).toBeInTheDocument();

    fireEvent.click(screen.getByText('newConn.sshJump'));
    fireEvent.change(screen.getByPlaceholderText('bastion.example.com'), {
      target: { value: 'jump.example.com' },
    });
    const ports = screen.getAllByPlaceholderText('22');
    fireEvent.change(ports[ports.length - 1], { target: { value: '2222' } });
    fireEvent.change(screen.getByPlaceholderText('ubuntu'), { target: { value: 'jumpuser' } });

    const passwords = document.querySelectorAll('input[type="password"]');
    fireEvent.change(passwords[passwords.length - 1], { target: { value: 'jump-secret' } });

    fireEvent.click(screen.getAllByText('newConn.authKey')[1]);
    const keyInputs = screen.getAllByPlaceholderText('~/.ssh/id_rsa');
    fireEvent.change(keyInputs[keyInputs.length - 1], { target: { value: '/tmp/jump' } });
    const jumpPasswords = document.querySelectorAll('input[type="password"]');
    fireEvent.change(jumpPasswords[jumpPasswords.length - 1], { target: { value: 'jump-ph' } });

    fireEvent.click(screen.getAllByText('newConn.authAgent')[1]);
  });
});
