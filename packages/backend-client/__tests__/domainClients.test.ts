import { describe, expect, it } from 'vitest';

import {
  createBackendClient,
  createBackendGuard,
  createClients,
  toId,
  toTimestamp,
  type ProfileDraft,
} from '../src/index';
import { createFakeTransport } from './client.test';

/**
 * Lightweight wire contracts for the domains whose facades had no dedicated
 * pre-F3 suite: the method name on the wire and the envelope shape, one test
 * per method. These are the F5 contract-test seed tables — F5 grows them into
 * the full method × request × response tables the plan asks for.
 */

const token = () => ({
  idempotencyKey: 'idem-x',
  expiresAt: mustNarrow(toTimestamp(1_700_000_000_000)),
});

function mustNarrow<T>(narrowed: T | undefined): T {
  if (narrowed === undefined) throw new Error('fixture did not narrow');
  return narrowed;
}

describe('Settings domain client', () => {
  it('getSettings: method name and empty payload on the wire', async () => {
    const fake = createFakeTransport();
    fake.responses.set('getSettings', { theme: 'system' });
    const client = createBackendClient('desktop', fake.transport);

    await client.getSettings();

    expect(fake.calls[0]).toEqual({ method: 'getSettings', payload: undefined });
  });

  it('saveSettings: settings travel under their documented key', async () => {
    const fake = createFakeTransport();
    const settings = { theme: 'dark' };
    fake.responses.set('saveSettings', settings);
    const client = createBackendClient('desktop', fake.transport);

    await client.saveSettings(settings as never);

    expect(fake.calls[0]).toEqual({ method: 'saveSettings', payload: { settings } });
  });
});

describe('Identity domain client', () => {
  it('getPlatformIdentity: method name and empty payload on the wire', async () => {
    const fake = createFakeTransport();
    fake.responses.set('getPlatformIdentity', {
      clientInstanceId: 'c-1',
      organizationId: 'o-1',
      principalId: 'p-1',
    });
    const client = createBackendClient('desktop', fake.transport);

    await client.getPlatformIdentity();

    expect(fake.calls[0]).toEqual({ method: 'getPlatformIdentity', payload: undefined });
  });

  it('issueSubmissionToken: operation and handle in one flat envelope', async () => {
    const fake = createFakeTransport();
    fake.responses.set('issueSubmissionToken', token());
    const client = createBackendClient('desktop', fake.transport);
    const handle = {
      dbSessionId: mustNarrow(toId('sess-1')),
      runtimeEpoch: mustNarrow(toId('epoch-1')),
    };

    await client.issueSubmissionToken('session.open', handle);

    expect(fake.calls[0]).toEqual({
      method: 'issueSubmissionToken',
      payload: { operation: 'session.open', handle },
    });
  });

  it('issueSubmissionToken: scope spreads into the envelope, not nested', async () => {
    const fake = createFakeTransport();
    fake.responses.set('issueSubmissionToken', token());
    const client = createBackendClient('desktop', fake.transport);

    await client.issueSubmissionToken('connection.create', null, {
      connectionId: 'conn-1',
      owner: { kind: 'editor', clientInstanceId: 'c-1', editorSessionId: 'e-1' } as never,
    });

    expect(fake.calls[0]?.payload).toMatchObject({ connectionId: 'conn-1' });
    expect(fake.calls[0]?.payload).not.toHaveProperty('scope');
  });
});

describe('Connection domain client', () => {
  it('listConnections: method name and empty payload on the wire', async () => {
    const fake = createFakeTransport();
    fake.responses.set('listConnections', []);
    const client = createBackendClient('desktop', fake.transport);

    await client.listConnections();

    expect(fake.calls[0]).toEqual({ method: 'listConnections', payload: undefined });
  });

  it('createConnection: draft and token folded into one envelope', async () => {
    const fake = createFakeTransport();
    fake.responses.set('createConnection', {});
    const client = createBackendClient('desktop', fake.transport);
    const draft = { name: 'pg', driverType: 'postgres' } as unknown as ProfileDraft;

    await client.createConnection(draft, token());

    expect(fake.calls[0]?.payload).toEqual({ ...draft, submissionToken: token() });
  });

  it('disableConnection: id and expected revision in the envelope', async () => {
    const fake = createFakeTransport();
    fake.responses.set('disableConnection', {});
    const client = createBackendClient('desktop', fake.transport);
    const connectionId = mustNarrow(toId('conn-9'));

    await client.disableConnection(connectionId, 3);

    expect(fake.calls[0]).toEqual({
      method: 'disableConnection',
      payload: { connectionId, expectedRevision: 3 },
    });
  });
});

describe('Artifact domain client', () => {
  it('readArtifact: request passes through untouched', async () => {
    const fake = createFakeTransport();
    fake.responses.set('readArtifact', { bytes: new Uint8Array(), eof: true });
    const client = createBackendClient('desktop', fake.transport);
    const request = { artifactId: mustNarrow(toId('art-1')), offset: 0n, limit: 4096 } as never;

    await client.readArtifact(request);

    expect(fake.calls[0]).toEqual({ method: 'readArtifact', payload: request });
  });
});

describe('DriverGateway domain client (transitional)', () => {
  it('get_driver_commands: legacy command name and flat payload', async () => {
    const fake = createFakeTransport();
    fake.responses.set('get_driver_commands', []);
    const guard = createBackendGuard('desktop', {});
    const { driverGateway } = createClients(fake.transport, guard);
    await driverGateway.get_driver_commands('postgres');

    expect(fake.calls[0]).toEqual({
      method: 'get_driver_commands',
      payload: { driverType: 'postgres' },
    });
  });

  it('execute_driver_command: request forwarded untouched, envelope preserved', async () => {
    const fake = createFakeTransport();
    fake.responses.set('execute_driver_command', { data: { rows: [] } });
    const guard = createBackendGuard('desktop', {});
    const { driverGateway } = createClients(fake.transport, guard);
    const request = { commandId: 'exec-sql', args: {} };
    await expect(driverGateway.execute_driver_command(request)).resolves.toEqual({
      data: { rows: [] },
    });
    expect(fake.calls[0]).toEqual({ method: 'execute_driver_command', payload: request });
  });
});
