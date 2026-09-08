/**
 * Central SQL Editor component — S6-D assembly.
 *
 * Composes all leaf extension factories (S4-A/B/C/D + S5-A) into
 * compartment groups with fixed priority order:
 *   statement → completion/signature → intention/hint → hover/navigation → paste/drop/multipleSelection
 *
 * §Track S6-D: Central Editor / Query Assembly
 */
import {
  useCallback,
  useEffect,
  useImperativeHandle,
  useRef,
  useMemo,
  forwardRef,
  type MutableRefObject,
} from 'react';
import { EditorView, placeholder as cmPlaceholder } from '@codemirror/view';
import { EditorState, Compartment } from '@codemirror/state';
import { snippet } from '@codemirror/autocomplete';
import { parseQualifiedPathParents } from '../../lib/sqlPathPrefix';
import { toggleSqlLineComments } from '../../lib/sqlEditorContextMenu';
import { buildSemanticModel } from './semantic/scopeModel';
import { useSettingsStore } from '../../stores/settingsStore';
import { useI18n } from '../../hooks/useI18n';
import type { I18nKey } from '../../locales';
import type { SqlEditorHandle, SqlEditorProps } from './contracts';
import {
  createBaseEditorExtensions,
  createDomEventHandlers,
  createSqlExtensions,
  createUpdateListener,
  createModelBuilderExtension,
  themeExtensions,
  compartments,
  documentVersionField,
  BumpDocumentVersion,
  createStatementExtensions,
  createCompletionExtensions,
  createIntentionExtensions,
  createHoverExtensions,
  createPasteExtensions,
} from './editorExtensions';
import { formatEditorDocument } from './format/formatEditorDocument';
import { StartExecutionEffect, FinishExecutionEffect } from './extensions/executionState';

export const SqlEditor = forwardRef<SqlEditorHandle, SqlEditorProps>(function SqlEditor(
  {
    value,
    onChange,
    onExecute,
    onExecuteSelection,
    onExecuteAll,
    onSaveQuery,
    onContextMenu: onCtxMenu,
    onQualifiedPath,
    placeholder,
    schema,
    databaseType,
    database,
    namespaceLoading,
    defaultSchema,
    defaultTable,
    className,
    onDropTable,
    // S6-D new props
    metadataSnapshot,
    executionStatus,
    executingRange,
    connectionId,
    onNavigateToTable,
    onNavigateToStructure,
    onNavigateToDdl,
    completionQuotePolicy = 'unquoted',
  },
  ref,
) {
  const containerRef = useRef<HTMLDivElement>(null);
  const viewRef = useRef<EditorView | null>(null);
  const themeCompartment = useRef(new Compartment());
  const sqlCompartment = useRef(new Compartment());

  // ── Refs for callbacks (stable reference identity) ────────────────
  const onChangeRef = useRef(onChange);
  const onExecuteRef = useRef(onExecute);
  const onExecuteSelectionRef = useRef(onExecuteSelection);
  const onExecuteAllRef = useRef(onExecuteAll);
  const onSaveQueryRef = useRef(onSaveQuery);
  const onCtxMenuRef = useRef(onCtxMenu);
  const onQualifiedPathRef = useRef(onQualifiedPath);
  const onDropTableRef = useRef(onDropTable);
  const onNavigateToTableRef = useRef(onNavigateToTable);
  const onNavigateToStructureRef = useRef(onNavigateToStructure);
  const onNavigateToDdlRef = useRef(onNavigateToDdl);
  const lastParentsRef = useRef<string[]>([]);

  // ── S6-D: Snapshot & model refs for completion/hover ──────────────
  const metadataSnapshotRef = useRef(metadataSnapshot);
  const modelRef = useRef<import('./semantic/types').SqlSemanticModel | null>(null);

  const { t } = useI18n();
  // Snippet descriptions are i18n keys; the completion source needs a resolver.
  const translate = useCallback((key: string) => t(key as I18nKey), [t]);

  // Settings are consumed internally by themeExtensions() in editorExtensions.ts
  const keymapPreset = useSettingsStore((s) => s.settings.keymapPreset);
  const customKeymap = useSettingsStore((s) => s.settings.customKeymap);
  const proSettings = useSettingsStore(
    (s) => s.settings.pluginSettings?.['sql-editor-pro'] as Record<string, unknown> | undefined,
  );
  const statementGutterEnabled = proSettings?.statementGutter !== false;
  const tableHoverEnabled = proSettings?.tableHover !== false;
  const insertValueHintsEnabled = proSettings?.insertValueHints !== false;

  // ── Sync callback refs ───────────────────────────────────────────
  onChangeRef.current = onChange;
  onExecuteRef.current = onExecute;
  onExecuteSelectionRef.current = onExecuteSelection;
  onExecuteAllRef.current = onExecuteAll;
  onSaveQueryRef.current = onSaveQuery;
  onCtxMenuRef.current = onCtxMenu;
  onQualifiedPathRef.current = onQualifiedPath;
  onDropTableRef.current = onDropTable;
  onNavigateToTableRef.current = onNavigateToTable;
  onNavigateToStructureRef.current = onNavigateToStructure;
  onNavigateToDdlRef.current = onNavigateToDdl;
  metadataSnapshotRef.current = metadataSnapshot;

  // ── Imperative handle ────────────────────────────────────────────
  useImperativeHandle(ref, () => ({
    getSelection: () => {
      const view = viewRef.current;
      if (!view) return '';
      return view.state.sliceDoc(view.state.selection.main.from, view.state.selection.main.to);
    },
    toggleLineComment: () => {
      const view = viewRef.current;
      if (!view) return;
      const { state } = view;
      const sel = state.selection.main;
      const fromLine = state.doc.lineAt(sel.from);
      const toLine = state.doc.lineAt(sel.to > sel.from ? sel.to - 1 : sel.to);
      const from = fromLine.from;
      const to = toLine.to;
      const original = state.sliceDoc(from, to);
      const next = toggleSqlLineComments(original);
      if (next === original) return;
      view.dispatch({
        changes: { from, to, insert: next },
        selection: { anchor: from, head: from + next.length },
      });
    },
    insertAt: (text: string, pos?: number | null) => {
      const view = viewRef.current;
      if (!view) return;
      const docLen = view.state.doc.length;
      const docStr = view.state.doc.toString();
      if (docStr.trim().length === 0) {
        view.dispatch({
          changes: { from: 0, to: docLen, insert: text },
          selection: { anchor: text.length },
        });
        return;
      }
      const targetPos = pos != null ? Math.max(0, Math.min(pos, docLen)) : docLen;
      const before = docStr.slice(0, targetPos);
      const after = docStr.slice(targetPos);
      const needLeadingNewline = before.length > 0 && !before.endsWith('\n\n');
      const prefix = needLeadingNewline ? (before.endsWith('\n') ? '\n' : '\n\n') : '';
      const needTrailingNewline = after.length > 0 && !after.startsWith('\n\n');
      const suffix = needTrailingNewline ? (after.startsWith('\n') ? '\n' : '\n\n') : '';
      const insertText = `${prefix}${text}${suffix}`;
      view.dispatch({
        changes: { from: targetPos, insert: insertText },
        selection: { anchor: targetPos + insertText.length },
      });
    },
    getDocumentVersion: () => {
      const view = viewRef.current;
      if (!view) return 0;
      return view.state.field(documentVersionField);
    },
    getDocument: () => {
      const view = viewRef.current;
      if (!view) return '';
      return view.state.doc.toString();
    },
    rawInsert: (text: string, pos: number) => {
      const view = viewRef.current;
      if (!view) return;
      const docLen = view.state.doc.length;
      const targetPos = Math.max(0, Math.min(pos, docLen));
      view.dispatch({
        changes: { from: targetPos, insert: text },
        selection: { anchor: targetPos + text.length },
      });
    },
    focus: () => {
      viewRef.current?.focus();
    },
    formatDocument: () => {
      const view = viewRef.current;
      if (!view) return;
      formatEditorDocument(view, {
        databaseType,
        options: useSettingsStore.getState().settings.sqlFormatOptions,
      });
      view.focus();
    },
    insertSnippet: (template: string) => {
      const view = viewRef.current;
      if (!view) return;
      const { from, to } = view.state.selection.main;
      // Applying via `snippet()` (rather than a plain insert) is what activates
      // the tabstop session, so Tab / Shift-Tab traverse the placeholders.
      snippet(template)(view, null, from, to);
      view.focus();
    },
  }));

  // ── Extension creation (memoized per compartment) ────────────────
  const statementExts = useMemo(
    () =>
      createStatementExtensions({
        enabled: statementGutterEnabled,
        onExecuteStatement: (sql) => {
          onExecuteSelectionRef.current?.(sql);
        },
      }),
    [statementGutterEnabled],
  );

  const completionExts = useMemo(
    () =>
      createCompletionExtensions(
        { databaseType, metadataSnapshot, schema, completionQuotePolicy, translate },
        { modelRef, metadataSnapshotRef },
      ),
    [databaseType, metadataSnapshot, schema, completionQuotePolicy, translate],
  );

  const intentionExts = useMemo(
    () =>
      createIntentionExtensions(
        { insertValueHints: insertValueHintsEnabled, databaseType, schema },
        { modelRef, metadataSnapshotRef },
      ),
    [insertValueHintsEnabled, databaseType, schema],
  );

  const hoverExts = useMemo(
    () =>
      tableHoverEnabled
        ? createHoverExtensions(
            {
              metadataSnapshot,
              onNavigateToTable,
              onNavigateToStructure,
              onNavigateToDdl,
              databaseType,
              database,
              schema,
            },
            { modelRef, metadataSnapshotRef },
          )
        : [],
    [
      tableHoverEnabled,
      metadataSnapshot,
      onNavigateToTable,
      onNavigateToStructure,
      onNavigateToDdl,
      databaseType,
      database,
      schema,
    ],
  );

  const pasteExts = useMemo(
    () =>
      createPasteExtensions({
        connectionId,
        onDrop: onDropTable,
      }),
    [connectionId, onDropTable],
  );

  // ── Editor mount ─────────────────────────────────────────────────
  useEffect(() => {
    if (!containerRef.current) return;

    const state = EditorState.create({
      doc: value,
      extensions: [
        ...themeExtensions(),
        ...createBaseEditorExtensions(
          {
            onExecute: onExecuteRef as MutableRefObject<(() => void) | undefined>,
            onExecuteSelection: onExecuteSelectionRef as MutableRefObject<
              ((sql: string) => void) | undefined
            >,
            onExecuteAll: onExecuteAllRef as MutableRefObject<(() => void) | undefined>,
            onSaveQuery: onSaveQueryRef as MutableRefObject<(() => void) | undefined>,
          },
          {
            preset: keymapPreset,
            custom: customKeymap,
          },
        ),
        ...createSqlExtensions({
          databaseType,
          schema,
          namespaceLoading,
          defaultSchema,
          defaultTable,
        }),
        themeCompartment.current.of([]),
        sqlCompartment.current.of([]),
        // §S6-D: compartment groups in priority order
        compartments.statement.of(statementExts),
        compartments.completion.of(completionExts),
        compartments.intention.of(intentionExts),
        compartments.hover.of(hoverExts),
        compartments.paste.of(pasteExts),
        // §S6-D: DOM event handlers (contextmenu + navigation click)
        createDomEventHandlers({
          onCtxMenu: onCtxMenuRef as MutableRefObject<
            ((e: MouseEvent, selectedSql: string) => void) | undefined
          >,
        }),
        // §S6-D: update listener (onChange + qualified path)
        createUpdateListener({
          onChange: onChangeRef,
          onQualifiedPath: onQualifiedPathRef,
          lastParents: lastParentsRef,
        }),
        // §S6-D: semantic model builder (populates modelRef for completion/hover)
        createModelBuilderExtension(modelRef, databaseType),
        // §S6-D: placeholder
        ...(placeholder ? [cmPlaceholder(placeholder)] : []),
      ],
    });

    const view = new EditorView({
      state,
      parent: containerRef.current,
    });

    viewRef.current = view;

    try {
      modelRef.current = buildSemanticModel(
        view.state.doc.toString(),
        view.state.selection.main.head,
        { dialectId: databaseType },
      );
    } catch {
      // ignore
    }

    // Fire initial onQualifiedPath
    const initialParents = parseQualifiedPathParents(
      view.state.doc.toString(),
      view.state.selection.main.head,
    );
    lastParentsRef.current = initialParents;
    onQualifiedPathRef.current?.(initialParents);

    return () => {
      view.destroy();
      viewRef.current = null;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // ── §S6-D: Reconfigure statement compartment ─────────────────────
  useEffect(() => {
    const view = viewRef.current;
    if (!view) return;
    view.dispatch({
      effects: compartments.statement.reconfigure(statementExts),
    });
  }, [statementExts]);

  // ── §S6-D: Reconfigure completion compartment ────────────────────
  useEffect(() => {
    const view = viewRef.current;
    if (!view) return;
    view.dispatch({
      effects: compartments.completion.reconfigure(completionExts),
    });
  }, [completionExts]);

  // ── §S6-D: Reconfigure intention compartment (INSERT hint toggle) ─
  useEffect(() => {
    const view = viewRef.current;
    if (!view) return;
    view.dispatch({
      effects: compartments.intention.reconfigure(intentionExts),
    });
  }, [intentionExts]);

  // ── §S6-D: Reconfigure hover compartment ─────────────────────────
  useEffect(() => {
    const view = viewRef.current;
    if (!view) return;
    view.dispatch({
      effects: compartments.hover.reconfigure(hoverExts),
    });
  }, [hoverExts]);

  // ── §S6-D: Reconfigure paste compartment ─────────────────────────
  useEffect(() => {
    const view = viewRef.current;
    if (!view) return;
    view.dispatch({
      effects: compartments.paste.reconfigure(pasteExts),
    });
  }, [pasteExts]);

  // ── Theme-pack change listener (reconfigure theme compartment) ───
  useEffect(() => {
    const reconfigure = () => {
      const view = viewRef.current;
      if (!view) return;
      view.dispatch({
        effects: themeCompartment.current.reconfigure(themeExtensions()),
      });
    };
    document.addEventListener('datazen:theme-pack-changed', reconfigure);
    return () => document.removeEventListener('datazen:theme-pack-changed', reconfigure);
  }, []);

  // ── §S6-D: Reconfigure SQL compartment on schema/type change ─────
  useEffect(() => {
    const view = viewRef.current;
    if (!view) return;
    view.dispatch({
      effects: sqlCompartment.current.reconfigure(
        createSqlExtensions({
          databaseType,
          schema,
          namespaceLoading,
          defaultSchema,
          defaultTable,
        }),
      ),
    });
  }, [schema, databaseType, namespaceLoading, defaultSchema, defaultTable]);

  // ── §S6-D: External value replacement (with documentVersion bump) ─
  useEffect(() => {
    const view = viewRef.current;
    if (!view) return;
    const current = view.state.doc.toString();
    if (current !== value) {
      view.dispatch({
        changes: { from: 0, to: current.length, insert: value },
        effects: [BumpDocumentVersion.of()],
      });
    }
  }, [value]);

  // ── §S6-D: Dispatch execution state effects ──────────────────────
  useEffect(() => {
    const view = viewRef.current;
    if (!view) return;
    if (executionStatus === 'running') {
      view.dispatch({
        effects: StartExecutionEffect.of({
          targetRange: executingRange ?? null,
          documentVersion: view.state.field(documentVersionField),
        }),
      });
    } else if (executionStatus === 'idle' || executionStatus === 'cancelling') {
      view.dispatch({
        effects: FinishExecutionEffect.of(),
      });
    }
  }, [executionStatus, executingRange]);

  return (
    <div
      ref={containerRef}
      className={`h-full w-full overflow-hidden${className ? ` ${className}` : ''}`}
    />
  );
});
