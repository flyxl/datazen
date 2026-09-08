/**
 * Built-in high-frequency SQL snippet library (§4.1).
 *
 * Dialect-neutral on purpose: anything dialect-specific belongs in a driver
 * package, not here.
 *
 * Every template ends with an empty terminal tabstop. CodeMirror deactivates a
 * snippet session as soon as the cursor reaches the *last* field, so without a
 * trailing stop the final meaningful placeholder would end the session and
 * Shift-Tab could no longer walk backwards.
 */
import type { SqlSnippetItem } from './types';

export const BUILTIN_SQL_SNIPPETS: readonly SqlSnippetItem[] = Object.freeze([
  {
    id: 'select-all',
    prefix: 'sel*',
    descriptionKey: 'query.editor.snippet.selectAll',
    template: 'SELECT *\nFROM ${1:table_name}\nWHERE ${2:condition};${3}',
  },
  {
    id: 'select-columns',
    prefix: 'selc',
    descriptionKey: 'query.editor.snippet.selectColumns',
    template: 'SELECT ${2:columns}\nFROM ${1:table_name};${3}',
  },
  {
    id: 'insert',
    prefix: 'ins',
    descriptionKey: 'query.editor.snippet.insert',
    template: 'INSERT INTO ${1:table_name} (${2:columns})\nVALUES (${3:values});${4}',
  },
  {
    id: 'update',
    prefix: 'upd',
    descriptionKey: 'query.editor.snippet.update',
    template: 'UPDATE ${1:table_name}\nSET ${2:column} = ${3:value}\nWHERE ${4:condition};${5}',
  },
  {
    id: 'delete',
    prefix: 'del',
    descriptionKey: 'query.editor.snippet.delete',
    template: 'DELETE FROM ${1:table_name}\nWHERE ${2:condition};${3}',
  },
  {
    id: 'join',
    prefix: 'join',
    descriptionKey: 'query.editor.snippet.join',
    template: 'JOIN ${1:table_name} ON ${1:table_name}.${2:id} = ${3:other_table}.${4:fk_id}${5}',
  },
  {
    id: 'count',
    prefix: 'count',
    descriptionKey: 'query.editor.snippet.count',
    template: 'SELECT COUNT(1) AS total\nFROM ${1:table_name};${2}',
  },
]);
