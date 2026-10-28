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
import { EditorState, Compartment, Transaction } from '@codemirror/state';
import { snippet } from '@codemirror/autocomplete';
import {
  buildSemanticModel,
  sqlEditorEnhancedEP,
  useIsExtensionEnhanced,
} from '@datazen/extension-points';
import { parseQualifiedPathParents } from '../../lib/sqlPathPrefix';
import { toggleSqlLineComments } from '../../lib/sqlEditorContextMenu';
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
  documentVersionField,
  BumpDocumentVersion,
  createStatementExtensions,
  createCompletionExtensions,
  createIntentionExtensions,
  createHoverExtensions,
  createPasteExtensions,
  createLinterExtensions,
} from './editorExtensions';
import {
  createProExtraExtensions,
  createProKeymapExtension,
  mountProCompartments,
  proSettingFlag,
  readProSettingsBag,
  reconfigureProCompartments,
  type ProCompartmentPayload,
  type ProSettingsBag,
} from './proCompartments';
import { createFoldCompartmentExtensions } from './fold/foldCompartment';
import { BUILTIN_SQL_SNIPPETS } from './snippets';
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
    completionIncludeTablePrefix = true,
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
  const modelRef = useRef<import('@datazen/extension-points').SqlSemanticModel | null>(null);

  const { t } = useI18n();
  // Snippet descriptions are i18n keys; the completion source needs a resolver.
  const translate = useCallback((key: string) => t(key as I18nKey), [t]);

  // Settings are consumed internally by themeExtensions() in editorExtensions.ts
  const userSnippets = useSettingsStore((s) => s.settings.sqlSnippets);
  const allSnippets = useMemo(
    () => [...BUILTIN_SQL_SNIPPETS, ...(userSnippets ?? [])],
    [userSnippets],
  );

  const keymapPreset = useSettingsStore((s) => s.settings.keymapPreset);
  const customKeymap = useSettingsStore((s) => s.settings.customKeymap);
  const sqlSyntaxTheme = useSettingsStore((s) => s.settings.sqlSyntaxTheme);
  // ── §G3: privileged settings bag (generic path) ─────────────────
  // Read wholesale instead of key by key: the bag is forwarded into every
  // compartment factory and is a dependency of each one, so any key an
  // extension declares in `settingsContributions` reaches the editor. Reading
  // individual keys here is what used to make most settings persist and render
  // while having no effect at all.
  const proSettings: ProSettingsBag = useSettingsStore((s) =>
    readProSettingsBag(s.settings.driverSettings),
  );

  // Host-side gates. These stay host-owned because they decide whether the
  // host wires a capability at all; the extension reads the same bag for the
  // keys only it knows about.
  const statementGutterEnabled = proSettingFlag(proSettings, 'statementGutter');
  const tableHoverEnabled = proSettingFlag(proSettings, 'tableHover');
  const insertValueHintsEnabled = proSettingFlag(proSettings, 'insertValueHints');
  const intentionActionsEnabled = proSettingFlag(proSettings, 'intentionActions', false);

  // §EP hot-plug: re-render when enhanced extension registers/unregisters at runtime
  const isSqlEditorEnhanced = useIsExtensionEnhanced(sqlEditorEnhancedEP);

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
    getCursorOffset: () => {
      const view = viewRef.current;
      if (!view) return 0;
      return view.state.selection.main.head;
    },
  }));

  // ── Extension creation (memoized per compartment) ────────────────
  // Every memo below lists `proSettings` as a dependency on purpose: that is
  // what turns an arbitrary extension setting into a live reconfiguration
  // instead of a persisted-but-inert value.
  const statementExts = useMemo(
    () =>
      createStatementExtensions({
        enabled: statementGutterEnabled,
        proSettings,
        onExecuteStatement: (sql) => {
          onExecuteSelectionRef.current?.(sql);
        },
      }),
    [statementGutterEnabled, proSettings, isSqlEditorEnhanced],
  );

  const completionExts = useMemo(
    () =>
      createCompletionExtensions(
        {
          databaseType,
          metadataSnapshot,
          schema,
          completionQuotePolicy,
          completionIncludeTablePrefix,
          translate,
          snippets: allSnippets,
          proSettings,
        },
        { modelRef, metadataSnapshotRef },
      ),
    [
      databaseType,
      metadataSnapshot,
      schema,
      completionQuotePolicy,
      completionIncludeTablePrefix,
      translate,
      allSnippets,
      proSettings,
      isSqlEditorEnhanced,
    ],
  );

  const intentionExts = useMemo(
    () =>
      createIntentionExtensions(
        {
          insertValueHints: insertValueHintsEnabled,
          databaseType,
          schema,
          completionQuotePolicy,
          proSettings,
        },
        { modelRef, metadataSnapshotRef },
      ),
    [
      insertValueHintsEnabled,
      databaseType,
      schema,
      completionQuotePolicy,
      proSettings,
      isSqlEditorEnhanced,
    ],
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
              proSettings,
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
      proSettings,
      isSqlEditorEnhanced,
    ],
  );

  const pasteExts = useMemo(
    () =>
      createPasteExtensions({
        connectionId,
        onDrop: onDropTable,
        proSettings,
      }),
    [connectionId, onDropTable, proSettings, isSqlEditorEnhanced],
  );

  const linterExts = useMemo(
    () =>
      createLinterExtensions(
        {
          databaseType,
          schema,
          completionQuotePolicy,
          intentionActions: intentionActionsEnabled,
          proSettings,
        },
        { modelRef, metadataSnapshotRef },
      ),
    [
      databaseType,
      schema,
      completionQuotePolicy,
      intentionActionsEnabled,
      proSettings,
      isSqlEditorEnhanced,
    ],
  );

  // Generic slots (contract 1.1.0): `createExtraKeymap` and `createExtraExtensions`.
  const keymapExts = useMemo(
    () => [createProKeymapExtension({ proSettings })],
    [proSettings, isSqlEditorEnhanced],
  );

  const extraExts = useMemo(
    () => createProExtraExtensions({ proSettings }),
    [proSettings, isSqlEditorEnhanced],
  );

  // `fold` sits in its own compartment because folding carries a keymap: it
  // needs a known slot in the mount order, and toggling the setting must not
  // rewrite the generic `extra` bucket in the same transaction. The gate is
  // read off the same bag as every other privileged key.
  const foldExts = useMemo(
    () => createFoldCompartmentExtensions({ proSettings }),
    [proSettings, isSqlEditorEnhanced],
  );

  /**
   * The single source of truth for every privileged slot.
   *
   * Its identity changes whenever any of the memos above does, which is what
   * the reconfigure effect below keys on. Because the payload is a plain
   * `Record`, adding a slot is a one-line change here and needs no change to
   * the reconfiguration logic.
   */
  const proPayload: ProCompartmentPayload = useMemo(
    () => ({
      statement: statementExts,
      completion: completionExts,
      intention: intentionExts,
      hover: hoverExts,
      paste: pasteExts,
      linter: linterExts,
      fold: foldExts,
      keymap: keymapExts,
      extra: extraExts,
    }),
    [
      statementExts,
      completionExts,
      intentionExts,
      hoverExts,
      pasteExts,
      linterExts,
      foldExts,
      keymapExts,
      extraExts,
    ],
  );

  /**
   * The payload currently installed in the live view, used to make the
   * reconfigure effect below idempotent across the mount commit.
   *
   * It is declared *before* the mount effect because the mount effect has to
   * seed it. React runs effects in declaration order, so in the mount commit
   * the two effects run back to back against the same `proPayload` object:
   * the mount effect installs that exact payload, and the reconfigure effect
   * must then recognise the identity and stay quiet. Left at its initial
   * `null`, the guard `appliedPayloadRef.current === proPayload` never matched
   * and every mount paid for a redundant full 8-slot reconfiguration
   * transaction (ep-hooks-settings-BUG-002).
   */
  const appliedPayloadRef = useRef<ProCompartmentPayload | null>(null);

  // ── Editor mount ─────────────────────────────────────────────────
  useEffect(() => {
    if (!containerRef.current) return;

    const state = EditorState.create({
      doc: value,
      extensions: [
        // Keep the initial theme in the same compartment that is reconfigured
        // when the user changes the SQL syntax theme or theme pack.
        themeCompartment.current.of(themeExtensions(sqlSyntaxTheme)),
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
        sqlCompartment.current.of([]),
        // §S6-D: compartment groups in priority order. `mountProCompartments`
        // registers any extension-owned slot before the view exists, which is
        // the only point at which a new Compartment can enter the state.
        ...mountProCompartments(proPayload),
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
    view.dom.setAttribute('data-testid', 'sql-editor');
    view.contentDOM.setAttribute('data-testid', 'sql-editor-content');

    viewRef.current = view;
    (view.dom as any).cmView = { view };
    (view.dom as any).__cmView = view;

    // The state built above already carries exactly this payload. Record it so
    // the reconfigure effect — declared below, hence run right after this one
    // in the same commit — short-circuits instead of re-dispatching it.
    appliedPayloadRef.current = proPayload;

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

  // ── §EP: atomic batch reconfiguration of every privileged slot ───
  // One dispatch, one transaction: previously each compartment was
  // reconfigured by its own `view.dispatch`, so a single settings change
  // produced N transactions and an observer could see a frame where, say, the
  // linter had been updated but the keymap had not. All slots — including the
  // ones the host has never heard of — now land together.
  useEffect(() => {
    const view = viewRef.current;
    // The mount effect above seeded `appliedPayloadRef` with this exact
    // `proPayload`; re-applying it would only add a redundant transaction.
    if (!view || appliedPayloadRef.current === proPayload) return;
    appliedPayloadRef.current = proPayload;
    reconfigureProCompartments(view, proPayload);
  }, [proPayload]);

  // ── Theme-pack change listener (reconfigure theme compartment) ───
  useEffect(() => {
    const reconfigure = () => {
      const view = viewRef.current;
      if (!view) return;
      view.dispatch({
        effects: themeCompartment.current.reconfigure(
          themeExtensions(useSettingsStore.getState().settings.sqlSyntaxTheme),
        ),
        annotations: Transaction.addToHistory.of(false),
      });
    };
    document.addEventListener('datazen:theme-pack-changed', reconfigure);
    return () => document.removeEventListener('datazen:theme-pack-changed', reconfigure);
  }, []);

  // ── SQL syntax theme preset change ──────────────────────────────
  useEffect(() => {
    const view = viewRef.current;
    if (!view) return;
    view.dispatch({
      effects: themeCompartment.current.reconfigure(themeExtensions(sqlSyntaxTheme)),
      annotations: Transaction.addToHistory.of(false),
    });
  }, [sqlSyntaxTheme]);

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
      annotations: Transaction.addToHistory.of(false),
    });
  }, [
    schema,
    databaseType,
    namespaceLoading,
    defaultSchema,
    defaultTable,
    keymapPreset,
    customKeymap,
  ]);

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
