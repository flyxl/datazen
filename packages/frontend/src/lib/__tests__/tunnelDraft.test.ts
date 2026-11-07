import { describe, expect, it } from 'vitest';
import {
  DEFAULT_HTTP_PROXY_PORT,
  DEFAULT_SSH_PORT,
  DEFAULT_TUNNEL_TIMEOUT,
  SAVED_TUNNEL_KINDS,
  TUNNEL_DRAFT_PROBLEM_KEYS,
  copyTunnelName,
  draftToSavedTunnel,
  emptyTunnelDraft,
  isTunnelDraftValid,
  savedTunnelToDraft,
  tunnelDraftProblem,
  type TunnelDraft,
} from '../tunnelDraft';
import type { SavedTunnel } from '../../types';

const sshEntity: SavedTunnel = {
  id: 'tun_ssh',
  name: 'Bastion',
  kind: 'ssh',
  ssh: {
    enabled: true,
    host: 'bastion.example.com',
    port: 2222,
    username: 'ops',
    authMethod: 'private_key',
    privateKeyPath: '/home/ops/.ssh/id_ed25519',
    passphrase: 'secret',
    jump: {
      enabled: true,
      host: 'jump.example.com',
      port: 22,
      username: 'gate',
      authMethod: 'password',
      password: 'pw',
    },
  },
};

describe('tunnelDraft', () => {
  it('starts from a blank, valid-by-shape ssh draft', () => {
    const draft = emptyTunnelDraft();
    expect(draft.id).toBeNull();
    expect(draft.kind).toBe('ssh');
    expect(draft.ssh.port).toBe(DEFAULT_SSH_PORT);
    expect(draft.httpProxy.port).toBe(DEFAULT_HTTP_PROXY_PORT);
    expect(draft.httpProxy.timeout).toBe(DEFAULT_TUNNEL_TIMEOUT);
    expect(draft.websocket.mode).toBe('datazen_v1');
    expect(draft.ssh.jump.enabled).toBe(false);
    // Only the name is missing — the shape itself is complete.
    expect(tunnelDraftProblem(draft)).toBe('name');
  });

  it('covers exactly the three saved kinds (never "none")', () => {
    expect([...SAVED_TUNNEL_KINDS]).toEqual(['ssh', 'httpProxy', 'websocket']);
  });

  it('round-trips an ssh entity through the draft without losing credentials', () => {
    const draft = savedTunnelToDraft(sshEntity);
    expect(draft.id).toBe('tun_ssh');
    expect(draft.name).toBe('Bastion');
    expect(draft.kind).toBe('ssh');
    expect(draft.ssh.port).toBe('2222');
    expect(draft.ssh.authMethod).toBe('private_key');
    expect(draft.ssh.keyPath).toBe('/home/ops/.ssh/id_ed25519');
    expect(draft.ssh.passphrase).toBe('secret');
    expect(draft.ssh.jump.enabled).toBe(true);
    expect(draft.ssh.jump.host).toBe('jump.example.com');
    expect(draft.ssh.jump.password).toBe('pw');

    expect(draftToSavedTunnel(draft, 'tun_ssh')).toEqual(sshEntity);
  });

  it('keeps port/timeout as clearable strings while editing', () => {
    const draft = savedTunnelToDraft(sshEntity);
    draft.ssh.port = '';
    expect(tunnelDraftProblem(draft)).toBe('sshPort');
    draft.ssh.port = '0';
    expect(tunnelDraftProblem(draft)).toBe('sshPort');
    draft.ssh.port = '70000';
    expect(tunnelDraftProblem(draft)).toBe('sshPort');
    draft.ssh.port = '22.5';
    expect(tunnelDraftProblem(draft)).toBe('sshPort');
    draft.ssh.port = '22';
    expect(tunnelDraftProblem(draft)).toBeNull();
  });

  it('emits only the block matching the selected kind', () => {
    const draft: TunnelDraft = {
      ...savedTunnelToDraft(sshEntity),
      kind: 'websocket',
      websocket: {
        url: 'wss://relay.example.com/v1',
        mode: 'raw_binary',
        authToken: 'tok',
        timeout: '15',
      },
    };
    const entity = draftToSavedTunnel(draft, 'tun_ws');
    expect(entity).toEqual({
      id: 'tun_ws',
      name: 'Bastion',
      kind: 'websocket',
      websocket: {
        enabled: true,
        url: 'wss://relay.example.com/v1',
        mode: 'raw_binary',
        authToken: 'tok',
        connectTimeoutSecs: 15,
      },
    });
    expect(entity.ssh).toBeUndefined();
    expect(entity.httpProxy).toBeUndefined();
  });

  it('maps http proxy drafts onto the stored config, dropping empty credentials', () => {
    const draft: TunnelDraft = {
      ...emptyTunnelDraft(),
      name: 'Corp proxy',
      kind: 'httpProxy',
      httpProxy: {
        host: 'proxy.corp.example',
        port: '3128',
        scheme: 'https',
        username: '',
        password: '',
        timeout: '',
      },
    };
    expect(draftToSavedTunnel(draft, 'tun_p')).toEqual({
      id: 'tun_p',
      name: 'Corp proxy',
      kind: 'httpProxy',
      httpProxy: {
        enabled: true,
        host: 'proxy.corp.example',
        port: 3128,
        scheme: 'https',
        username: undefined,
        password: undefined,
        connectTimeoutSecs: Number(DEFAULT_TUNNEL_TIMEOUT),
      },
    });
  });

  it('requires the credential that matches the selected ssh auth method', () => {
    const base = { ...emptyTunnelDraft(), name: 'B', kind: 'ssh' as const };
    base.ssh.host = 'h';
    base.ssh.username = 'u';

    const agent = draftToSavedTunnel({ ...base, ssh: { ...base.ssh, authMethod: 'agent' } }, 't');
    expect(agent.ssh?.password).toBeUndefined();
    expect(agent.ssh?.privateKeyPath).toBeUndefined();

    const password = draftToSavedTunnel(
      { ...base, ssh: { ...base.ssh, authMethod: 'password', password: 'pw' } },
      't',
    );
    expect(password.ssh?.password).toBe('pw');
    expect(password.ssh?.privateKeyPath).toBeUndefined();

    const key = draftToSavedTunnel(
      {
        ...base,
        ssh: { ...base.ssh, authMethod: 'private_key', keyPath: '/k', passphrase: 'pp' },
      },
      't',
    );
    expect(key.ssh?.privateKeyPath).toBe('/k');
    expect(key.ssh?.passphrase).toBe('pp');
    expect(key.ssh?.password).toBeUndefined();
  });

  it('omits the jump hop entirely when it is disabled', () => {
    const draft = { ...emptyTunnelDraft(), name: 'B' };
    draft.ssh.host = 'h';
    draft.ssh.username = 'u';
    draft.ssh.jump.enabled = false;
    draft.ssh.jump.host = 'ignored';
    expect(draftToSavedTunnel(draft, 't').ssh?.jump).toBeUndefined();
  });

  it('reports the first unmet requirement per kind', () => {
    const draft = emptyTunnelDraft();
    expect(tunnelDraftProblem(draft)).toBe('name');

    draft.name = 'x';
    expect(tunnelDraftProblem(draft)).toBe('sshHost');
    draft.ssh.host = 'h';
    expect(tunnelDraftProblem(draft)).toBe('sshUsername');
    draft.ssh.username = 'u';
    expect(tunnelDraftProblem(draft)).toBeNull();
    expect(isTunnelDraftValid(draft)).toBe(true);

    draft.ssh.jump.enabled = true;
    expect(tunnelDraftProblem(draft)).toBe('sshJumpHost');
    draft.ssh.jump.host = 'jh';
    expect(tunnelDraftProblem(draft)).toBe('sshJumpUsername');
    draft.ssh.jump.username = 'ju';
    expect(tunnelDraftProblem(draft)).toBeNull();

    draft.kind = 'httpProxy';
    expect(tunnelDraftProblem(draft)).toBe('httpProxyHost');
    draft.httpProxy.host = 'p';
    expect(tunnelDraftProblem(draft)).toBeNull();

    draft.kind = 'websocket';
    expect(tunnelDraftProblem(draft)).toBe('wsUrl');
    draft.websocket.url = 'wss://relay';
    expect(tunnelDraftProblem(draft)).toBeNull();
  });

  it('exposes a translation key for every problem it can report', () => {
    const draft = emptyTunnelDraft();
    draft.name = 'x';
    draft.ssh.jump.enabled = true;
    const problems = new Set<string>();
    for (const mutate of [
      () => {
        draft.ssh.host = '';
      },
      () => {
        draft.ssh.host = 'h';
        draft.ssh.port = '';
      },
      () => {
        draft.ssh.port = '22';
        draft.ssh.username = '';
      },
      () => {
        draft.ssh.username = 'u';
        draft.ssh.jump.host = '';
      },
      () => {
        draft.ssh.jump.host = 'jh';
        draft.ssh.jump.port = '';
      },
      () => {
        draft.ssh.jump.port = '22';
        draft.ssh.jump.username = '';
      },
    ]) {
      mutate();
      const problem = tunnelDraftProblem(draft);
      if (problem) problems.add(TUNNEL_DRAFT_PROBLEM_KEYS[problem]);
    }
    expect(problems.size).toBeGreaterThan(0);
    for (const key of problems) expect(key.startsWith('settings.tunnels.validation.')).toBe(true);
  });

  it('appends the copy suffix to the name', () => {
    expect(copyTunnelName('Bastion', ' (copy)')).toBe('Bastion (copy)');
  });
});
