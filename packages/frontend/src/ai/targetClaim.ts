/**
 * AI runtime lifecycle wire contracts (P6 §14.4).
 *
 * These are the shapes the frontend and the host agree on for:
 * - the target a chat run claims (`AiTargetClaim`),
 * - the receipt of a granted editor session (`AiGrantReceipt`),
 * - the three-state answer to `ai_cancel` (`CancelReceipt`).
 *
 * Keeping them in one dependency-free module lets the state machines
 * (`streamRegistry`, `grantState`, `sessionCancel`) and the IPC wrapper in
 * `commands/ai.ts` share the contract without importing Tauri.
 */

/**
 * Receipt for `ai_cancel`.
 *
 * - `marked`: the backend accepted the cancel and will stop the run.
 * - `alreadyFinished`: the run reached its terminal state before the cancel.
 * - `forbidden`: this caller/target may not cancel the run.
 */
export type CancelReceipt =
  | { kind: 'marked'; requestId: string }
  | { kind: 'alreadyFinished'; requestId: string }
  | { kind: 'forbidden'; requestId: string };

/**
 * What a chat run is allowed to observe.
 *
 * `independent` means the run owns its own runtime context. `editorGrant`
 * binds it to an editor session the user explicitly shared, identified by the
 * server-issued grant id and the exact client/editor pair that claimed it.
 */
export type AiTargetClaim =
  | { kind: 'independent' }
  | {
      kind: 'editorGrant';
      grantId: string;
      claimedClientInstanceId: string;
      editorSessionId: string;
    };

/**
 * Opaque runtime handle the editor controller already holds. The frontend
 * passes it through verbatim; it must never be reconstructed from parts.
 */
export interface AiSessionHandle {
  dbSessionId: string;
  runtimeEpoch: string;
}

/** Arguments for `ai_grant_editor_session` (camelCase; Tauri maps to snake_case). */
export interface AiGrantRequest {
  connectionId: string;
  database: string | null;
  editorSessionId: string;
  claimedClientInstanceId: string;
  sessionHandle: AiSessionHandle;
}

/** Receipt returned by `ai_grant_editor_session`. */
export interface AiGrantReceipt {
  grantId: string;
  connectionId: string;
  database: string | null;
  configRevision: number;
  expiresAtMs: number;
}

/** Minimal attribution metadata retained for a run/message list. */
export type AiRunAttribution =
  | { kind: 'independent' }
  | { kind: 'editorGrant'; grantId: string; editorSessionId: string };

/** The default, unshared target. */
export function independent(): AiTargetClaim {
  return { kind: 'independent' };
}

/** Build a claim from an issued grant receipt. */
export function editorGrant(
  receipt: Pick<AiGrantReceipt, 'grantId'>,
  claimedClientInstanceId: string,
  editorSessionId: string,
): AiTargetClaim {
  return {
    kind: 'editorGrant',
    grantId: receipt.grantId,
    claimedClientInstanceId,
    editorSessionId,
  };
}

/** Narrow a claim to its editor-grant arm. */
export function isEditorGrant(
  claim: AiTargetClaim | null | undefined,
): claim is Extract<AiTargetClaim, { kind: 'editorGrant' }> {
  return claim?.kind === 'editorGrant';
}

/** Project a claim into the non-secret attribution metadata kept by the UI. */
export function attributionOf(claim: AiTargetClaim | null | undefined): AiRunAttribution {
  if (isEditorGrant(claim)) {
    return { kind: 'editorGrant', grantId: claim.grantId, editorSessionId: claim.editorSessionId };
  }
  return { kind: 'independent' };
}
