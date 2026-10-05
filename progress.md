P4 session-client foundation implemented; ready for isolated tester after final gates.
Public API: src/lib/session/index.ts exports factory createEditorSessionController(client,target,paneId), EditorSessionController and ExecutionProjection.
Controller: lazy session, serialized execution/context/close, explicit replacement after lost, host identity, clone requires distinct editor ID, in-memory handles.
Projection: decimal BigInt ordering, notification/chunk dedupe, publication-count gap recovery, original provenance and ephemeral binding invalidation, unsubscribe separate from cancel.
Desktop transport: real Channel AsyncIterable, stop subscription, error/close frames, bounded buffer, kernel request envelope, profile platform_* mapping, JSON bytes normalization.
No Query/Table/panel/schema store consumers changed; those remain next-wave work. No performance or wording audit.
Verified ec162f044: typecheck exit 0; Vitest 2 files / 4 tests passed. Final commit gates pending below.
