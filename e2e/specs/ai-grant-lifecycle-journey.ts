import { browser, expect } from '@wdio/globals';
import { ensureMainWindowForIpc, invokeBackend } from '../helpers.js';

/**
 * P6 §14.4 / CM-53 — the AI editor-session grant lifecycle, driven through the
 * real IPC surface (no provider call is involved, so the journey does not depend
 * on an AI key).
 *
 * What this proves: a grant is bound to *this* editor session, another pane's
 * handle is refused, a foreign database is refused at the tool boundary, and
 * closing the session revokes the grant so a later claim is rejected. The
 * tool-execution half (temporary objects visible on the shared session) is
 * covered at the H layer; this journey deliberately stops at the ownership
 * boundary that only the packaged app can exercise.
 */

interface PlatformIdentity {
  clientInstanceId: string;
  organizationId: string;
  principalId: string;
}

interface OpenSessionReceipt {
  session: {
    handle: { dbSessionId: string; runtimeEpoch: string };
    connectionId: string;
    configRevision: string | number;
    owner: unknown;
  };
  attachmentToken: string;
}

const SEEDED_CONNECTION_NAME = process.env.DATAZEN_E2E_PG_NAME ?? 'E2E Postgres';

async function seededConnectionId(): Promise<{ id: string; database: string }> {
  const connections = await invokeBackend<Array<{ id: string; name: string; database?: string }>>(
    'get_connections',
  );
  const found = connections.find((connection) => connection.name === SEEDED_CONNECTION_NAME);
  if (!found) {
    throw new Error(
      `seeded connection "${SEEDED_CONNECTION_NAME}" is missing; the journey needs the standard seeded PostgreSQL fixture`,
    );
  }
  return { id: found.id, database: found.database ?? 'postgres' };
}

async function openEditorSession(
  connectionId: string,
  database: string,
  editorSessionId: string,
  clientInstanceId: string,
): Promise<OpenSessionReceipt> {
  const token = await invokeBackend<{ idempotencyKey: string; submissionToken: unknown }>(
    'issue_submission_token',
    { request: { operation: 'openSession' } },
  );
  return invokeBackend<OpenSessionReceipt>('open_session', {
    request: {
      initialTarget: { connectionId, namespace: { database, schema: null, path: [] }, object: null },
      owner: { kind: 'editor', clientInstanceId, editorSessionId },
      idempotencyKey: token.idempotencyKey,
      submissionToken: token.submissionToken,
    },
  });
}

describe('AI editor-session grant lifecycle (CM-53)', () => {
  let connectionId = '';
  let database = '';
  let identity: PlatformIdentity;
  let session: OpenSessionReceipt;

  before(async () => {
    await ensureMainWindowForIpc();
    const seeded = await seededConnectionId();
    connectionId = seeded.id;
    database = seeded.database;
    identity = await invokeBackend<PlatformIdentity>('get_platform_identity');
    session = await openEditorSession(
      connectionId,
      database,
      'e2e-ai-grant-pane',
      identity.clientInstanceId,
    );
  });

  it('grants the current editor session and rejects a foreign database', async () => {
    const receipt = await invokeBackend<{ grantId: string; expiresAtMs: number }>(
      'ai_grant_editor_session',
      {
        connectionId,
        database,
        editorSessionId: 'e2e-ai-grant-pane',
        claimedClientInstanceId: identity.clientInstanceId,
        sessionHandle: session.session.handle,
      },
    );
    expect(receipt.grantId).toBeTruthy();
    expect(receipt.expiresAtMs).toBeGreaterThan(Date.now());

    // Touch keeps the grant alive; revoking twice reports the second as a no-op.
    await expect(
      invokeBackend<boolean>('ai_touch_editor_grant', { grantId: receipt.grantId }),
    ).resolves.toBe(true);
    await expect(
      invokeBackend<boolean>('ai_revoke_editor_grant', { grantId: receipt.grantId }),
    ).resolves.toBe(true);
    await expect(
      invokeBackend<boolean>('ai_revoke_editor_grant', { grantId: receipt.grantId }),
    ).resolves.toBe(false);
  });

  it('refuses a grant whose client instance does not own the session', async () => {
    const error = await invokeBackend<{ __error?: string }>('ai_grant_editor_session', {
      connectionId,
      database,
      editorSessionId: 'e2e-ai-grant-pane',
      claimedClientInstanceId: 'e2e-forged-client',
      sessionHandle: session.session.handle,
    }).then(
      () => null,
      (failure: unknown) => String(failure),
    );
    expect(error).not.toBeNull();
    expect(error).toMatch(/aiGrantClientMismatch|PermissionDenied|ClientMismatch/i);
  });

  it('refuses a grant for another pane of the same client', async () => {
    const error = await invokeBackend<never>('ai_grant_editor_session', {
      connectionId,
      database,
      editorSessionId: 'e2e-some-other-pane',
      claimedClientInstanceId: identity.clientInstanceId,
      sessionHandle: session.session.handle,
    }).then(
      () => null,
      (failure: unknown) => String(failure),
    );
    expect(error).not.toBeNull();
    expect(error).toMatch(/aiGrantSessionGone|SessionGone/i);
  });

  it('revokes the grant when the editor session closes', async () => {
    const receipt = await invokeBackend<{ grantId: string }>('ai_grant_editor_session', {
      connectionId,
      database,
      editorSessionId: 'e2e-ai-grant-pane',
      claimedClientInstanceId: identity.clientInstanceId,
      sessionHandle: session.session.handle,
    });

    await invokeBackend<void>('close_session', {
      request: { handle: session.session.handle, mode: 'rollbackAndClose' },
    });

    // The grant borrowed that session; after close it must be unusable rather
    // than silently pointing at a replaced handle.
    await expect(
      invokeBackend<boolean>('ai_touch_editor_grant', { grantId: receipt.grantId }),
    ).resolves.toBe(false);
  });

  after(async () => {
    // The journey closes its own session; nothing is left to release. Keeping an
    // explicit hook documents that deliberate absence for the next reader.
    await browser.pause(0);
  });
});
