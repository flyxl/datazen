/**
 * SQL snippet template contracts (§4.1).
 *
 * Templates use CodeMirror's native snippet syntax: `${1:defaultText}` marks an
 * ordered tabstop whose default text is selected on arrival.
 */

export interface SqlSnippetItem {
  /** Stable identity, also used as the import/export key. */
  id: string;
  /** Typed prefix that expands the template (e.g. `sel*`). */
  prefix: string;
  /** i18n key for the human-readable description. */
  descriptionKey: string;
  /** CodeMirror snippet template with `${n:default}` tabstops. */
  template: string;
}
