import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { GrantState, type GrantSnapshot } from '../ai/grantState';
import { editorGrant, independent, type AiTargetClaim } from '../ai/targetClaim';
import { aiCommands } from '../commands/ai';
import {
  getQueryPanelSessionRegistry,
  queryPanelClientInstanceId,
} from '../lib/session/queryPanelSessionRoot';

export interface UseAiEditorGrantParams {
  connectionId?: string;
  database?: string;
  /** Stable owner id of the editor pane whose session may be shared. */
  editorSessionId?: string;
}

const INDEPENDENT: GrantSnapshot = {
  phase: 'independent',
  receipt: null,
  expiresAtMs: null,
  error: null,
};

/**
 * Resolve the opaque runtime handle of the pane's editor session.
 *
 * The controller is the one QueryPanel already created for this pane key; it
 * is only `peek`ed, never duplicated. `ensureSession` opens the runtime
 * session if the pane has not executed yet — the user explicitly asked to
 * share that session, so binding it is the requested effect.
 */
async function resolveSessionHandle(editorSessionId: string) {
  const controller = getQueryPanelSessionRegistry().peek(editorSessionId);
  if (!controller) throw new Error('Editor session is not ready');
  const session = await controller.ensureSession();
  return session.handle;
}

/**
 * Owns the panel's editor-session sharing lifecycle: grant, touch, revoke and
 * local expiry. Defaults to `independent`; a denied grant leaves the machine
 * independent with a visible error instead of silently sending without a
 * claim.
 */
export function useAiEditorGrant({
  connectionId,
  database,
  editorSessionId,
}: UseAiEditorGrantParams) {
  const [snapshot, setSnapshot] = useState<GrantSnapshot>(INDEPENDENT);
  const claimedClientInstanceIdRef = useRef<string | null>(null);
  const canShare = Boolean(connectionId && editorSessionId);

  const state = useMemo(() => {
    if (!connectionId || !editorSessionId) return null;
    return new GrantState({
      grant: async (input) => {
        const [claimedClientInstanceId, sessionHandle] = await Promise.all([
          queryPanelClientInstanceId(),
          resolveSessionHandle(input.editorSessionId),
        ]);
        claimedClientInstanceIdRef.current = claimedClientInstanceId;
        return aiCommands.grantEditorSession({
          connectionId: input.connectionId,
          database: input.database,
          editorSessionId: input.editorSessionId,
          claimedClientInstanceId,
          sessionHandle: {
            dbSessionId: String(sessionHandle.dbSessionId),
            runtimeEpoch: String(sessionHandle.runtimeEpoch),
          },
        });
      },
      revoke: (grantId) => aiCommands.revokeEditorGrant(grantId),
      touch: (grantId) => aiCommands.touchEditorGrant(grantId),
      onWarn: (message, error) => console.warn(`[AI] ${message}`, error),
    });
  }, [connectionId, editorSessionId]);

  useEffect(() => {
    if (!state) {
      setSnapshot(INDEPENDENT);
      return;
    }
    setSnapshot(state.snapshot);
    const unsubscribe = state.subscribe(() => setSnapshot(state.snapshot));
    return () => {
      unsubscribe();
      state.dispose();
    };
  }, [state]);

  /** Turn sharing on/off. Grant failures surface through `snapshot.error`. */
  const setSharing = useCallback(
    async (next: boolean): Promise<void> => {
      if (!state) return;
      if (!next) {
        await state.revoke();
        return;
      }
      await state.grant({
        connectionId: connectionId as string,
        database: database ?? null,
        editorSessionId: editorSessionId as string,
      });
    },
    [state, connectionId, database, editorSessionId],
  );

  /** The claim to attach to the next `ai_chat` call. */
  const claimForSend = useCallback((): AiTargetClaim => {
    const receipt = snapshot.receipt;
    const claimedClientInstanceId = claimedClientInstanceIdRef.current;
    if (
      snapshot.phase === 'granted' &&
      receipt &&
      claimedClientInstanceId &&
      editorSessionId
    ) {
      return editorGrant(receipt, claimedClientInstanceId, editorSessionId);
    }
    return independent();
  }, [snapshot, editorSessionId]);

  return {
    canShare,
    sharing: snapshot.phase === 'granting' || snapshot.phase === 'granted',
    phase: snapshot.phase,
    error: snapshot.error,
    setSharing,
    claimForSend,
  };
}
