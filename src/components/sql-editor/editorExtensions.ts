/**
 * Central extension assembly for the SQL Editor.
 *
 * Composes all leaf extension factories (S4-A/B/C/D + S5-A) into compartment
 * groups with a fixed priority order. Compartments allow dynamic reconfiguration
 * without duplicating listeners, timers, or tooltips.
 *
 * §Track S6-D: Central Editor / Query Assembly
 */
import type { MutableRefObject } from 'react';
import { EditorView, keymap } from '@codemirror/view';
import { defaultKeymap, history, historyKeymap } from '@codemirror/commands';
import { sql, keywordCompletionSource, type SQLNamespace } from '@codemirror/lang-sql';
import {
  Compartment,
  StateField,
  StateEffect,
  type Extension,
  Transaction,
} from '@codemirror/state';
import {
  autocompletion,
  closeBrackets,
  acceptCompletion,
  type Completion,
  type CompletionSource,
} from '@codemirror/autocomplete';
import { searchKeymap } from '@codemirror/search';
import { lineNumbers, tooltips } from '@codemirror/view';
import {
  applySqlSyntaxPreset,
  editorSyntaxHighlighting,
  readEditorColorsFromElement,
  sqlPropertyNameHighlighting,
} from '../../lib/themeEditorColors';
import {
  resolveEditorFontFamily,
  HOST_DEFAULT_EDITOR_FONT,
} from '../../lib/resolveEditorFontFamily';
import { useSettingsStore } from '../../stores/settingsStore';
import { parseQualifiedPathParents } from '../../lib/sqlPathPrefix';
import {
  contextualSchemaCompletion,
  contextualKeywordCompletion,
  inferSqlCompletionKind,
} from '../../lib/sqlCompletionContext';
import { getStatementAtCursor } from '../../lib/sqlStatementRange';
import { sqlFunctionCompletions } from '../../lib/sqlCompletions';
import {
  resolveCmDialect,
  type SqlSchema,
  type SqlEditorProps,
  type CompletionQuotePolicy,
} from './contracts';
import { getActionShortcut, toCodeMirrorKeyFormat, type KeymapPreset } from '../../lib/keymap';

// ── Leaf factories (S4-A/B/C/D + S5-A) ───────────────────────────────
import {
  statementIndexField,
  executionStateField,
  extensionRegistry,
  sqlEditorEnhancedEP,
  SafeCompartmentWrapper,
  type ExtensionPoint,
} from '@datazen/extension-points';
import { produceSchemaCompletions } from './completion/schemaCompletion';
import { createSnippetCompletionSource, type SqlSnippetItem } from './snippets';
import { formatEditorDocument } from './format/formatEditorDocument';
import { getDialectAdapter } from './semantic/dialectAdapter';
import { buildSemanticModel } from './semantic/scopeModel';
import type { EditorMetadataSnapshot } from './metadata/types';
import type { SqlSemanticModel } from './semantic/types';

/* -------------------------------------------------------------------------- */
/*  Types                                                                      */
/* -------------------------------------------------------------------------- */

export interface ThemeConfig {
  dark: boolean;
  fontSize: number;
  fontFamily: string;
}

export interface UpdateListenerRefs {
  onChange: MutableRefObject<(value: string) => void>;
  onQualifiedPath: MutableRefObject<((parents: string[]) => void) | undefined>;
  lastParents: MutableRefObject<string[]>;
}

/* -------------------------------------------------------------------------- */
/*  Document version field                                                     */
/* -------------------------------------------------------------------------- */

/** Effect to bump document version on external value replacement. */
export const BumpDocumentVersion = StateEffect.define<void>();

/**
 * Monotonically increasing counter that increments on every document change.
 * Used by execution state to ensure gutter spinners only match the correct
 * document revision.
 */
export const documentVersionField = StateField.define<number>({
  create() {
    return 0;
  },
  update(value, tr) {
    if (tr.docChanged || tr.effects.some((e) => e.is(BumpDocumentVersion))) {
      return value + 1;
    }
    return value;
  },
});

/* -------------------------------------------------------------------------- */
/*  Compartments                                                               */
/* -------------------------------------------------------------------------- */

/** §S6-D priority order compartments */
export const compartments = {
  /** Statement frame + gutter (S4-A) */
  statement: new Compartment(),
  /** Completion + signature (S4-B) */
  completion: new Compartment(),
  /** Intention + hint (S4-C) */
  intention: new Compartment(),
  /** Hover + navigation (S4-D) */
  hover: new Compartment(),
  /** Paste + drop + multi-cursor (S5-A) */
  paste: new Compartment(),
  /** Linter / Diagnostics (S4-E) */
  linter: new Compartment(),
};

/* -------------------------------------------------------------------------- */
/*  Theme extensions                                                           */
/* -------------------------------------------------------------------------- */

function makeEditorTheme(
  { dark, fontSize, fontFamily }: ThemeConfig,
  colors: { propertyName?: string },
) {
  const propertyNameColor = colors.propertyName ?? (dark ? '#61afef' : '#2563eb');
  return EditorView.theme(
    {
      '&': {
        height: '100%',
        fontSize: `${fontSize}px`,
      },
      '.cm-content': {
        fontFamily: `${fontFamily}, ui-monospace, SFMono-Regular, Menlo, monospace`,
        padding: '12px 0',
      },
      '.cm-sql-property-name, .cm-sql-property-name *': {
        // Syntax highlighting can render its token span inside a decoration
        // span, so this must win on both the marker and its descendants.
        color: `${propertyNameColor} !important`,
      },
      '.cm-activeLine': {
        backgroundColor: dark ? 'rgba(30,41,59,0.5)' : 'rgba(241,245,249,0.5)',
      },
      '.cm-selectionBackground, &.cm-focused .cm-selectionBackground': {
        // selection color from theme
      },
      '.cm-gutters': {
        backgroundColor: dark ? '#111827' : '#f1f5f9',
        color: dark ? '#9ca3af' : '#64748b',
        border: 'none',
        borderRight: `1px solid ${dark ? '#374151' : '#cbd5e1'}`,
      },
      '.cm-activeLineGutter': {
        backgroundColor: dark ? 'rgba(55,65,81,0.5)' : 'rgba(203,213,225,0.5)',
      },
      '.cm-tooltip': {
        backgroundColor: dark ? '#111827' : '#ffffff',
        border: `1px solid ${dark ? '#374151' : '#cbd5e1'}`,
        color: dark ? '#f3f4f6' : '#111827',
        borderRadius: '6px',
        boxShadow: dark ? '0 4px 12px rgba(0,0,0,0.4)' : '0 4px 12px rgba(0,0,0,0.1)',
      },
      '.cm-tooltip-autocomplete': {
        '& > ul': { maxHeight: '240px' },
        '& > ul > li': {
          padding: '2px 8px',
          fontSize: '12px',
          lineHeight: '1.6',
        },
      },
      '.cm-tooltip-autocomplete ul li[aria-selected]': {
        backgroundColor: dark ? '#202b3a' : '#dbeafe',
        color: dark ? '#f3f4f6' : '#1e3a5f',
      },
      '.cm-completionIcon': {
        width: '1.2em',
        opacity: '0.7',
      },
      '.cm-placeholder': {
        color: dark ? '#64748b' : '#94a3b8',
      },
    },
    { dark },
  );
}

export function currentThemeConfig(): ThemeConfig {
  const { editorFontSize: fs, editorFontFamily: ff } = useSettingsStore.getState().settings;
  const computedVar = getComputedStyle(document.documentElement).getPropertyValue('--font-editor');
  return {
    dark: document.documentElement.classList.contains('dark'),
    fontSize: fs || 14,
    fontFamily: resolveEditorFontFamily(ff || '', computedVar, HOST_DEFAULT_EDITOR_FONT),
  };
}

export function themeExtensions(sqlSyntaxTheme?: string): Extension[] {
  const config = currentThemeConfig();
  const baseColors = readEditorColorsFromElement(document.documentElement);
  const colors = applySqlSyntaxPreset(baseColors, sqlSyntaxTheme, config.dark);
  return [
    makeEditorTheme(config, colors),
    editorSyntaxHighlighting(colors, config.dark),
    sqlPropertyNameHighlighting(),
  ];
}

/* -------------------------------------------------------------------------- */
/*  Base editor extensions                                                     */
/* -------------------------------------------------------------------------- */

/**
 * Create base editor extensions.
 *
 * @param handlerRefs Optional execution handler refs. When provided, Mod-Enter / Mod-Shift-Enter / Mod-s
 *   keymaps are wired to call the handlers directly. When omitted (mount-time call), stubs are used
 *   and the caller is responsible for reconfiguring the compartment with real refs.
 */
export function createBaseEditorExtensions(
  handlerRefs?: {
    onExecute: MutableRefObject<(() => void) | undefined>;
    onExecuteSelection: MutableRefObject<((sql: string) => void) | undefined>;
    onExecuteAll: MutableRefObject<(() => void) | undefined>;
    onSaveQuery: MutableRefObject<(() => void) | undefined>;
  },
  keymapOptions?: {
    preset?: KeymapPreset;
    custom?: Partial<Record<string, string>>;
  },
): Extension[] {
  const executeKey = toCodeMirrorKeyFormat(
    getActionShortcut('execute', keymapOptions?.preset, keymapOptions?.custom),
  );
  const saveKey = toCodeMirrorKeyFormat(
    getActionShortcut('saveQuery', keymapOptions?.preset, keymapOptions?.custom),
  );

  return [
    lineNumbers(),
    history(),
    closeBrackets(),
    // Custom keymaps FIRST so they override defaultKeymap's Mod-Enter etc.
    keymap.of([
      {
        key: executeKey,
        run: (view) => {
          if (handlerRefs) {
            const sel = view.state.sliceDoc(
              view.state.selection.main.from,
              view.state.selection.main.to,
            );
            if (sel.trim()) {
              handlerRefs.onExecuteSelection.current?.(sel);
            } else {
              handlerRefs.onExecute.current?.();
            }
          }
          return true;
        },
      },
      {
        key: saveKey,
        run: () => {
          handlerRefs?.onSaveQuery.current?.();
          return true;
        },
      },
      { key: 'Tab', run: acceptCompletion },
    ]),
    keymap.of([...defaultKeymap, ...historyKeymap, ...searchKeymap]),
    EditorView.lineWrapping,
    documentVersionField,
    tooltips({
      position: 'fixed',
      parent: typeof document !== 'undefined' ? document.body : undefined,
    }),
  ];
}

/* -------------------------------------------------------------------------- */
/*  SQL language extension (compartmented)                                      */
/* -------------------------------------------------------------------------- */

export interface CreateSqlExtensionsOptions {
  databaseType?: string;
  schema?: SqlSchema;
  namespaceLoading?: boolean;
  defaultSchema?: string;
  defaultTable?: string;
}

export function createSqlExtensions(opts: CreateSqlExtensionsOptions): Extension[] {
  return [
    sql({
      dialect: resolveCmDialect(opts.databaseType),
      schema: opts.schema as SQLNamespace | undefined,
      upperCaseKeywords: true,
      defaultSchema: opts.defaultSchema,
      defaultTable: opts.defaultTable,
    }),
    createFormatKeymap(opts.databaseType),
  ];
}

/**
 * §4.2 Beautify shortcut. Lives in the SQL compartment so it always formats
 * with the dialect currently selected in the panel.
 */
export function createFormatKeymap(databaseType?: string): Extension {
  const formatShortcut = toCodeMirrorKeyFormat(
    getActionShortcut(
      'formatSql',
      useSettingsStore.getState().settings.keymapPreset,
      useSettingsStore.getState().settings.customKeymap,
    ),
  );
  return keymap.of([
    {
      key: formatShortcut,
      run: (view) =>
        formatEditorDocument(view, {
          databaseType,
          options: useSettingsStore.getState().settings.sqlFormatOptions,
        }),
    },
  ]);
}

/* -------------------------------------------------------------------------- */
/*  Statement compartment (S4-A): frame + gutter                               */
/* -------------------------------------------------------------------------- */

export interface CreateStatementExtensionsOptions {
  onExecuteStatement?: (sql: string) => void;
  enabled?: boolean;
}

const enhancedSafe = (featureName: string) => ({
  point: sqlEditorEnhancedEP as ExtensionPoint<unknown>,
  featureName,
});

export function createStatementExtensions(opts?: CreateStatementExtensionsOptions): Extension[] {
  const enhanced = extensionRegistry.get(sqlEditorEnhancedEP);
  const enhancedDecorations =
    opts?.enabled !== false
      ? SafeCompartmentWrapper(
          enhancedSafe('createStatementDecorations'),
          () => enhanced.createStatementDecorations?.(opts) ?? [],
          [],
        )
      : [];
  return [statementIndexField(), executionStateField, ...enhancedDecorations];
}

/* -------------------------------------------------------------------------- */
/*  Completion/signature compartment (S4-B)                                    */
/* -------------------------------------------------------------------------- */

export interface CompletionCompartmentOptions {
  databaseType?: string;
  metadataSnapshot?: EditorMetadataSnapshot;
  schema?: SQLNamespace;
  completionQuotePolicy?: CompletionQuotePolicy;
  /** Resolves snippet description i18n keys; omitted in tests and non-UI callers. */
  translate?: (key: string) => string;
  /** Active snippet library (builtin + user snippets) */
  snippets?: readonly SqlSnippetItem[];
}

export function createCompletionExtensions(
  opts: CompletionCompartmentOptions,
  refs: {
    modelRef: MutableRefObject<SqlSemanticModel | null>;
    metadataSnapshotRef?: MutableRefObject<EditorMetadataSnapshot | undefined>;
  },
): Extension[] {
  const enhanced = extensionRegistry.get(sqlEditorEnhancedEP);

  const enhancedCompletionSource: CompletionSource = (context) =>
    SafeCompartmentWrapper(
      enhancedSafe('enhancedCompletionSource'),
      () => {
        const joinSource = enhanced.createJoinCompletionSource?.(opts, refs);
        const joinRes = joinSource ? joinSource(context) : null;
        if (joinRes) return joinRes;

        const colSource = enhanced.createColumnCompletionSource?.(opts, refs);
        const colRes = colSource ? colSource(context) : null;
        if (colRes) return colRes;

        return null;
      },
      null,
    );

  // Wrap Completion[] as a CompletionSource with table context filtering
  const functionCompletionSource: CompletionSource = (context) => {
    const textBefore = context.state.sliceDoc(0, context.pos);
    const kind = inferSqlCompletionKind(textBefore);
    if (kind === 'table') return null;

    const match = context.matchBefore(/[A-Za-z_]\w*/);
    if (!match && !context.explicit) return null;
    const completions = sqlFunctionCompletions(opts.databaseType);
    return {
      from: match?.from ?? context.pos,
      options: completions,
    };
  };

  // Schema-aware completion that reads modelRef and snapshot at CALL time
  // (not at creation time), so it always uses the latest semantic model and
  // metadata snapshot. Falls back to basic relation completions from the
  // snapshot when the model is not yet available.
  const schemaAwareCompletionSource: CompletionSource = (context) => {
    let model = refs.modelRef.current;
    const doc = context.state.doc.toString();
    const pos = context.pos;
    if (
      !model ||
      pos < model.statement.from ||
      pos > model.statement.to ||
      model.cursorIntent.kind === 'unknown'
    ) {
      try {
        model = buildSemanticModel(doc, pos, { dialectId: opts.databaseType });
        refs.modelRef.current = model;
      } catch {
        // ignore
      }
    }
    const snapshot = refs.metadataSnapshotRef?.current ?? opts.metadataSnapshot;
    if (!snapshot && !opts.schema) return null;

    const match = context.matchBefore(/[A-Za-z_$"][\w$"']*$/);
    const isDot = pos > 0 && doc.charAt(pos - 1) === '.';
    if (!match && !isDot && !context.explicit) return null;

    // If we have a valid model, use the full schema-aware completion
    if (model && model.cursorIntent) {
      const completions = produceSchemaCompletions({
        model,
        snapshot: snapshot ?? { dbSessionId: '', epoch: 0, relations: new Map() },
        schema: opts.schema,
        adapter: getDialectAdapter(opts.databaseType ?? 'standard'),
        quotePolicy: opts.completionQuotePolicy,
      });
      if (completions.length === 0) return null;
      return {
        from: match?.from ?? pos,
        options: completions,
        validFor: /^[\w$"']*$/,
      };
    }

    // Fallback: provide basic relation completions from the snapshot
    // even when the semantic model is not yet available.
    const adapter = getDialectAdapter(opts.databaseType ?? 'standard');
    const results: Completion[] = [];
    if (snapshot) {
      for (const [, rel] of snapshot.relations) {
        const name = adapter.quoteIdentifier(rel.identity.name.name);
        results.push({
          label: name,
          type: 'type' as const,
          detail: rel.kind,
          apply: name,
          boost: 10,
        });
      }
    }
    if (results.length === 0) return null;
    return {
      from: match?.from ?? context.pos,
      options: results,
      validFor: /^[A-Za-z_$"][\w$"']*$/,
    };
  };

  const completionSources = autocompletion({
    override: [
      // SQL keyword completions from @codemirror/lang-sql dialect,
      // wrapped with context-aware keyword filtering.
      contextualKeywordCompletion(
        keywordCompletionSource(resolveCmDialect(opts.databaseType), true),
      ),
      // Schema-based table/column completions from @codemirror/lang-sql,
      // filtered by SQL context (columns after SELECT/WHERE, tables after FROM).
      contextualSchemaCompletion({
        dialect: resolveCmDialect(opts.databaseType),
        schema: opts.schema,
      }),
      functionCompletionSource,
      // §4.1: snippet templates with tabstop expansion
      createSnippetCompletionSource({ t: opts.translate, snippets: opts.snippets }),
      // S4-B: schema-aware completion (reads from metadata snapshot)
      schemaAwareCompletionSource,
      // S4-B: Enhanced completion
      enhancedCompletionSource,
    ],
    activateOnTyping: true,
    maxRenderedOptions: 50,
  });

  const signatureHelpExts = SafeCompartmentWrapper(
    enhancedSafe('createSignatureHelpExtensions'),
    () => enhanced.createSignatureHelpExtensions?.(opts.databaseType) ?? [],
    [],
  );

  return [completionSources, ...signatureHelpExts];
}

/* -------------------------------------------------------------------------- */
/*  Intention/hint compartment (S4-C)                                          */
/* -------------------------------------------------------------------------- */

export interface IntentionCompartmentOptions {
  insertValueHints?: boolean;
  databaseType?: string;
  schema?: SqlSchema;
  completionQuotePolicy?: CompletionQuotePolicy;
}

/**
 * Build intention extensions. When insertValueHints is false, only
 * the Alt+Enter intention keymap is included (hints are suppressed).
 */
export function createIntentionExtensions(
  opts: IntentionCompartmentOptions,
  refs: {
    modelRef: MutableRefObject<SqlSemanticModel | null>;
    metadataSnapshotRef: MutableRefObject<EditorMetadataSnapshot | undefined>;
  },
): Extension[] {
  const enhanced = extensionRegistry.get(sqlEditorEnhancedEP);
  return SafeCompartmentWrapper(
    enhancedSafe('createIntentionExtensions'),
    () => enhanced.createIntentionExtensions?.(opts, refs) ?? [],
    [],
  );
}

/* -------------------------------------------------------------------------- */
/*  Hover/navigation compartment (S4-D)                                        */
/* -------------------------------------------------------------------------- */

export interface HoverCompartmentOptions {
  metadataSnapshot?: EditorMetadataSnapshot;
  onNavigateToTable?: SqlEditorProps['onNavigateToTable'];
  onNavigateToStructure?: SqlEditorProps['onNavigateToStructure'];
  onNavigateToDdl?: SqlEditorProps['onNavigateToDdl'];
  databaseType?: string;
  schema?: SqlSchema;
  database?: string;
}

export function createHoverExtensions(
  opts: HoverCompartmentOptions,
  refs: {
    modelRef: MutableRefObject<SqlSemanticModel | null>;
    metadataSnapshotRef?: MutableRefObject<EditorMetadataSnapshot | undefined>;
  },
): Extension[] {
  const enhanced = extensionRegistry.get(sqlEditorEnhancedEP);
  return SafeCompartmentWrapper(
    enhancedSafe('createHoverExtensions'),
    () => enhanced.createHoverExtensions?.(opts, refs) ?? [],
    [],
  );
}

/* -------------------------------------------------------------------------- */
/*  Linter compartment (S4-E)                                                  */
/* -------------------------------------------------------------------------- */

export interface LinterCompartmentOptions {
  databaseType?: string;
  schema?: SqlSchema;
}

export function createLinterExtensions(
  opts: LinterCompartmentOptions,
  refs: {
    modelRef: MutableRefObject<SqlSemanticModel | null>;
    metadataSnapshotRef: MutableRefObject<EditorMetadataSnapshot | undefined>;
  },
): Extension[] {
  const enhanced = extensionRegistry.get(sqlEditorEnhancedEP);
  return SafeCompartmentWrapper(
    enhancedSafe('createLinterExtensions'),
    () => enhanced.createLinterExtensions?.(opts, refs) ?? [],
    [],
  );
}

/* -------------------------------------------------------------------------- */
/*  Paste/drop/multiple selection compartment (S5-A)                           */
/* -------------------------------------------------------------------------- */
export { createPasteExtensions, type PasteCompartmentOptions } from './paste/createPasteExtensions';

/* -------------------------------------------------------------------------- */
/*  Update listener                                                            */
/* -------------------------------------------------------------------------- */

export function createUpdateListener(refs: UpdateListenerRefs): Extension {
  return EditorView.updateListener.of((update) => {
    if (update.docChanged) {
      refs.onChange.current(update.state.doc.toString());
    }
    if (update.docChanged || update.selectionSet) {
      const parents = parseQualifiedPathParents(
        update.state.doc.toString(),
        update.state.selection.main.head,
      );
      if (!parentsEqual(refs.lastParents.current, parents)) {
        refs.lastParents.current = parents;
        refs.onQualifiedPath.current?.(parents);
      }
    }
  });
}

/* -------------------------------------------------------------------------- */
/*  DOM event handlers (keyboard shortcuts + navigation click)                  */
/* -------------------------------------------------------------------------- */

export interface DomEventRefs {
  /** New S6-D naming */
  onContextMenu?: MutableRefObject<((e: MouseEvent, sql: string) => void) | undefined>;
  /** Legacy alias — tests and drop handler still use this */
  onCtxMenu?: MutableRefObject<((e: MouseEvent, selectedSql: string) => void) | undefined>;
  onNavigateToTable?: MutableRefObject<SqlEditorProps['onNavigateToTable']>;
  onNavigateToStructure?: MutableRefObject<SqlEditorProps['onNavigateToStructure']>;
}

export function createDomEventHandlers(refs: DomEventRefs): Extension {
  return EditorView.domEventHandlers({
    contextmenu: (e, view) => {
      const handler = refs.onContextMenu ?? refs.onCtxMenu;
      if (!handler?.current) return false;
      const sel = view.state.selection.main;
      const sqlText = sel.empty
        ? (() => {
            const doc = view.state.doc.toString();
            const stmt = getStatementAtCursor(doc, sel.head);
            return stmt ?? doc.trim();
          })()
        : view.state.sliceDoc(sel.from, sel.to);
      e.preventDefault();
      e.stopPropagation();
      handler.current(e, sqlText);
      return true;
    },
  });
}

/* -------------------------------------------------------------------------- */
/*  Semantic model builder (populates modelRef on doc/cursor changes)          */
/* -------------------------------------------------------------------------- */

/**
 * Creates an EditorView update listener that builds the semantic model
 * from the current document content and cursor position, storing it
 * in `modelRef` for use by completion, hover, and intention extensions.
 *
 * For documents exceeding 1,000 lines, windows to the active statement or
 * ±200 lines around cursor to protect editor performance (AC-19).
 */
export function createModelBuilderExtension(
  modelRef: MutableRefObject<SqlSemanticModel | null>,
  databaseType?: string,
): Extension {
  return EditorView.updateListener.of((update) => {
    if (update.docChanged || update.selectionSet) {
      const doc = update.state.doc;
      const cursor = update.state.selection.main.head;

      let docText = doc.toString();
      let effectiveCursor = cursor;

      // Windowing for very large documents (> 1,000 lines)
      if (doc.lines > 1000) {
        const currentLine = doc.lineAt(cursor).number;
        const startLine = Math.max(1, currentLine - 200);
        const endLine = Math.min(doc.lines, currentLine + 200);
        const fromPos = doc.line(startLine).from;
        const toPos = doc.line(endLine).to;
        docText = doc.sliceString(fromPos, toPos);
        effectiveCursor = Math.max(0, cursor - fromPos);
      }

      try {
        modelRef.current = buildSemanticModel(docText, effectiveCursor, {
          dialectId: databaseType,
        });
      } catch {
        // If model building fails (e.g., incomplete SQL), keep the previous model
      }
    }
  });
}

/* -------------------------------------------------------------------------- */
/*  Internal helpers                                                           */
/* -------------------------------------------------------------------------- */

function parentsEqual(a: readonly string[], b: readonly string[]): boolean {
  return a.length === b.length && a.every((seg, i) => seg === b[i]);
}

/** §EP hot-plug: batch-reconfigure all Pro-driven compartments on a live EditorView. */
export interface ProCompartmentPayload {
  statement: Extension[];
  completion: Extension[];
  intention: Extension[];
  hover: Extension[];
  paste: Extension[];
  linter: Extension[];
}

export function reconfigureProCompartments(view: EditorView, payload: ProCompartmentPayload): void {
  view.dispatch({
    effects: [
      compartments.statement.reconfigure(payload.statement),
      compartments.completion.reconfigure(payload.completion),
      compartments.intention.reconfigure(payload.intention),
      compartments.hover.reconfigure(payload.hover),
      compartments.paste.reconfigure(payload.paste),
      compartments.linter.reconfigure(payload.linter),
    ],
    annotations: Transaction.addToHistory.of(false),
  });
}
