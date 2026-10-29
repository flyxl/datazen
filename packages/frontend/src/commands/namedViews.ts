/**
 * The command layer as the saved-views menu drives it.
 *
 * The menu holds no command layer: it takes its three calls and its two file
 * calls as props, so the component can be exercised without an IPC and
 * without a native dialog. This module is the adapter that hands the real
 * ones over.
 *
 * It lives in its own file — next to, not inside, `database.ts` and
 * `file.ts` — so a caller that replaces either of those modules does not
 * have to know about this one.
 */
import type {
  NamedViewFileOperations,
  NamedViewOperations,
} from '../components/filter/NamedViewMenu';
import { databaseCommands } from './database';
import { fileCommands } from './file';

/** The three host commands the menu drives. */
export const namedViewOperations: NamedViewOperations = {
  /** The views of a scope. */
  list: (scope) => databaseCommands.listNamedViews(scope),
  /** Save a view; the answer carries the minted id and the timestamps. */
  save: (view) => databaseCommands.saveNamedView(view),
  /** Delete a view; the answer carries the remaining ones. */
  remove: (id) => databaseCommands.deleteNamedView(id),
};

/** The two file commands the export and the import drive. */
export const namedViewFiles: NamedViewFileOperations = {
  /** Write the export file. Resolves `false` when the user cancelled. */
  write: (fileName, content) =>
    fileCommands.saveTextWithDialog(content, fileName, 'JSON', ['json']),
  /** Read an import file. Resolves `null` when the user cancelled. */
  read: () => fileCommands.openTextWithDialog('JSON', ['json']),
};
