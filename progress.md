P4 session-client foundation implemented; ready for isolated tester after final gates.
Public API: src/lib/session/index.ts exports factory createEditorSessionController(client,target,paneId), EditorSessionController and ExecutionProjection.
Controller: lazy session, serialized execution/context/close, explicit replacement after lost, host identity, clone requires distinct editor ID, in-memory handles.
Projection: decimal BigInt ordering, notification/chunk dedupe, publication-count gap recovery, original provenance and ephemeral binding invalidation, unsubscribe separate from cancel.
Desktop transport: real Channel AsyncIterable, stop subscription, error/close frames, bounded buffer, kernel request envelope, profile platform_* mapping, JSON bytes normalization.
No Query/Table/panel/schema store consumers changed; those remain next-wave work. No performance or wording audit.
Verified ec162f044: typecheck exit 0; Vitest 2 files / 4 tests passed. Final commit gates pending below.
Final validated code HEAD c938a3d304e0b3310be1c5f3734c6d70792e136e:
TYPECHECK_EXIT=0; VITEST_EXIT=0; Test Files 2 passed (2); Tests 6 passed (6).
Gate start/end HEAD identical; start/end clean-workspace status SHA e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855.
Logs /tmp/p4-session-typecheck-4.log and /tmp/p4-session-vitest-4.log.
READY_FOR_TEST. Runtime/desktop integration depends on peer foundation merges. Query/Table consumer and WDIO journeys remain next wave scope, not implemented here.
