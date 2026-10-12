# Published query result export

After a SQL query finishes, its result area offers **Export published result (.jsonl)** when the backend has finalized a published result. This exports the original execution artifact; the existing table export remains available for CSV, SQL and other table formats.

The Host opens the native save dialog. Select a new `.jsonl` destination. Dismissing the dialog leaves the result available for another export attempt. Existing files are protected by exclusive creation; select a new filename if saving reports an error.

Cancelled or failed queries can retain a finalized partial result. The result area labels it as partial, and the Host requires confirmation before saving. Its JSON Lines export starts with an explicit truncation marker. Pending artifacts cannot be exported.

The file contains published execution chunks, including their output and provenance, rather than only the currently selected result table. Reading or exporting this retained result does not execute SQL again or require the original database session to remain usable. Backend ownership, expiry and lifecycle checks still apply.

Starting a new query removes the previous execution's artifact export action immediately. Result pinning and the existing table export retain their own behavior.

Implementation: `runEditorQuery` publishes finalized artifact references before reporting success, failure or cancellation. The pane retains those references and `QueryArtifactExport` calls `export_artifact` with only the artifact id. Identity, partial-result confirmation and destination authorization are supplied by Host code.
