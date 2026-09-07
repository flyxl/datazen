/**
 * Alias-aware schema completion: given a semantic model and metadata snapshot,
 * produces CodeMirror completions filtered by cursor intent.
 *
 * - `alias.` → only that relation's columns (with type/nullable/comment).
 *   If the alias is ambiguous, no guess is made and no completions are returned.
 * - Bare cursor in column context → all visible relations' columns.
 * - Table context → relations in the current scope's database/schema.
 *
 * This module does NOT perform any IPC; it reads synchronously from the
 * metadata snapshot and semantic model.
 */

import type { Completion, CompletionResult } from '@codemirror/autocomplete';
import type { SQLNamespace } from '@codemirror/lang-sql';
import type { ColumnSchema } from '../../../types';
import type { CompletionQuotePolicy } from '../contracts';
import type { EditorMetadataSnapshot, EditorRelationMetadata } from '../metadata/types';
import type {
  SqlDialectAdapter,
  SqlRelationBinding,
  SqlSemanticModel,
  SqlToken,
} from '../semantic/types';
import { SqlTokenKind } from '../semantic/tokens';
import { getDialectAdapter } from '../semantic/dialectAdapter';
import { foldSegment } from '../semantic/quoteHelper';
import { listVisibleRelations, resolveRelation } from '../semantic/relationResolver';
import { findRelationMetadata } from '../metadata/findRelation';

/* -------------------------------------------------------------------------- */
/*  Types                                                                     */
/* -------------------------------------------------------------------------- */

export type SchemaCompletionItem = Completion & { filterText?: string };

export interface SchemaCompletionOptions {
  model: SqlSemanticModel;
  snapshot: EditorMetadataSnapshot;
  adapter?: SqlDialectAdapter;
  /** Full editor schema tree (from `useSchemaStore.columnMap` via `buildEditorSchema()`). */
  schema?: SQLNamespace;
  /** Identifier quoting policy ('unquoted' | 'always' | 'both'). Default 'unquoted'. */
  quotePolicy?: CompletionQuotePolicy;
}

/* -------------------------------------------------------------------------- */
/*  Helpers                                                                   */
/* -------------------------------------------------------------------------- */

function formatColumnDetail(col: ColumnSchema): string {
  const parts: string[] = [col.dataType];
  if (col.isPrimaryKey) parts.push('PK');
  if (col.nullable) parts.push('nullable');
  if (col.comment) parts.push(col.comment);
  return parts.join(' · ');
}

function matchRelation(
  binding: SqlRelationBinding,
  metadata: EditorMetadataSnapshot,
  adapter: SqlDialectAdapter,
  schemaTree?: SQLNamespace,
): EditorRelationMetadata | undefined {
  return findRelationMetadata(metadata, binding.relation, adapter, schemaTree as any);
}

/**
 * Build column completions from a single relation's metadata.
 * Supports configurable quoting policy:
 * - 'unquoted' (default): normal identifiers unquoted (e.g. name), reserved words/special quoted (e.g. "order").
 * - 'both': provides unquoted + quoted for safe identifiers, quoted for reserved/special.
 * - 'always': all columns quoted.
 */
function columnCompletionsFromRelation(
  rel: EditorRelationMetadata,
  qualifierPrefix: string,
  adapter: SqlDialectAdapter,
  quotePolicy: CompletionQuotePolicy = 'unquoted',
): SchemaCompletionItem[] {
  const results: SchemaCompletionItem[] = [];
  for (const col of rel.columns) {
    const quotedLabel = adapter.quoteIdentifier(col.name);
    const unquotedLabel = col.name;
    const detail = formatColumnDetail(col);
    const mustQuote = adapter.shouldQuoteIdentifier(col.name);

    if (quotePolicy === 'always') {
      results.push({
        label: quotedLabel,
        filterText: col.name,
        type: 'property' as const,
        detail,
        apply: qualifierPrefix ? `${qualifierPrefix}.${quotedLabel}` : quotedLabel,
        boost: 10,
      });
    } else if (quotePolicy === 'both') {
      if (mustQuote) {
        results.push({
          label: quotedLabel,
          filterText: col.name,
          type: 'property' as const,
          detail,
          apply: qualifierPrefix ? `${qualifierPrefix}.${quotedLabel}` : quotedLabel,
          boost: 12,
        });
      } else {
        results.push({
          label: unquotedLabel,
          filterText: col.name,
          type: 'property' as const,
          detail,
          apply: qualifierPrefix ? `${qualifierPrefix}.${unquotedLabel}` : unquotedLabel,
          boost: 12,
        });
        results.push({
          label: quotedLabel,
          filterText: col.name,
          type: 'property' as const,
          detail,
          apply: qualifierPrefix ? `${qualifierPrefix}.${quotedLabel}` : quotedLabel,
          boost: 10,
        });
      }
    } else {
      // 'unquoted' (default): smart quote reserved keywords or special names, unquoted for standard
      if (mustQuote) {
        results.push({
          label: quotedLabel,
          filterText: col.name,
          type: 'property' as const,
          detail,
          apply: qualifierPrefix ? `${qualifierPrefix}.${quotedLabel}` : quotedLabel,
          boost: 12,
        });
      } else {
        results.push({
          label: unquotedLabel,
          filterText: col.name,
          type: 'property' as const,
          detail,
          apply: qualifierPrefix ? `${qualifierPrefix}.${unquotedLabel}` : unquotedLabel,
          boost: 12,
        });
      }
    }
  }
  return results;
}

/**
 * Build table/relation completions from the metadata snapshot.
 * Filters to relations in the current scope's database/schema context.
 */
function relationCompletionsFromSnapshot(
  metadata: EditorMetadataSnapshot,
  adapter: SqlDialectAdapter,
  qualifierParts: readonly string[],
): Completion[] {
  const results: Completion[] = [];
  for (const [, rel] of metadata.relations) {
    // If qualifier parts are provided, filter by the last qualifier segment.
    if (qualifierParts.length > 0) {
      const lastQualifier = qualifierParts[qualifierParts.length - 1]!;
      const namespaceNames = rel.identity.namespacePath.map((s) => s.name);
      // Check if the last qualifier matches the schema or database level.
      if (qualifierParts.length === 1) {
        const matchesSchema =
          namespaceNames.length >= 1 &&
          adapter.compareIdentifiers(namespaceNames[namespaceNames.length - 1]!, lastQualifier);
        const matchesDatabase =
          namespaceNames.length >= 2 &&
          adapter.compareIdentifiers(namespaceNames[namespaceNames.length - 2]!, lastQualifier);
        if (!matchesSchema && !matchesDatabase) continue;
      }
    }

    const name = adapter.quoteIdentifier(rel.identity.name.name);
    results.push({
      label: name,
      type: 'type' as const,
      detail: rel.kind,
      apply: name,
      boost: 10,
    });
  }
  return results;
}

/**
 * Find the EditorRelationKey for a completion label by matching against the
 * snapshot. Used to deduplicate filtered vs. remaining relation completions.
 */
function relationKeyFromLabel(
  label: string,
  adapter: SqlDialectAdapter,
  snapshot: EditorMetadataSnapshot,
): string | null {
  for (const [key, rel] of snapshot.relations) {
    const quotedName = adapter.quoteIdentifier(rel.identity.name.name);
    if (quotedName === label) return key;
  }
  return null;
}

/**
 * Produce column completions for ALL columns across ALL relations in the
 * snapshot. Used as a fallback when no FROM clause is present (projection
 * context without visible relations).
 *
 * Deduplicates by (label + qualifier) so that columns appearing in multiple
 * tables are listed once per table (qualifier differs), but identical
 * columns within the same table are never duplicated.
 */
function allColumnsFromSnapshot(
  snapshot: EditorMetadataSnapshot,
  adapter: SqlDialectAdapter,
  quotePolicy: CompletionQuotePolicy = 'unquoted',
): SchemaCompletionItem[] {
  const results: SchemaCompletionItem[] = [];
  const seen = new Set<string>();

  for (const [, relMeta] of snapshot.relations) {
    if (relMeta.columns.length === 0) continue;

    const tableName = adapter.quoteIdentifier(relMeta.identity.name.name);
    const qualifier = relMeta.identity.name.name;

    for (const col of relMeta.columns) {
      const quotedLabel = adapter.quoteIdentifier(col.name);
      const unquotedLabel = col.name;
      const detail = `${formatColumnDetail(col)} · ${tableName}`;
      const mustQuote = adapter.shouldQuoteIdentifier(col.name);

      const addUnquoted = () => {
        const key = `${adapter.foldUnquotedIdentifier(unquotedLabel)}|${adapter.foldUnquotedIdentifier(qualifier)}|unquoted`;
        if (!seen.has(key)) {
          seen.add(key);
          results.push({
            label: unquotedLabel,
            filterText: col.name,
            type: 'property' as const,
            detail,
            apply: `${tableName}.${unquotedLabel}`,
            boost: 12,
          });
        }
      };

      const addQuoted = (boost = 10) => {
        const key = `${adapter.foldUnquotedIdentifier(quotedLabel)}|${adapter.foldUnquotedIdentifier(qualifier)}|quoted`;
        if (!seen.has(key)) {
          seen.add(key);
          results.push({
            label: quotedLabel,
            filterText: col.name,
            type: 'property' as const,
            detail,
            apply: `${tableName}.${quotedLabel}`,
            boost,
          });
        }
      };

      if (quotePolicy === 'always') {
        addQuoted(10);
      } else if (quotePolicy === 'both') {
        if (mustQuote) {
          addQuoted(12);
        } else {
          addUnquoted();
          addQuoted(10);
        }
      } else {
        // 'unquoted' (default): quote if reserved/special, otherwise unquoted
        if (mustQuote) {
          addQuoted(12);
        } else {
          addUnquoted();
        }
      }
    }
  }

  return results;
}

/**
 * Produce column completions for ALL columns from the full editor schema tree.
 * Used as a projection fallback when no FROM clause is present AND the
 * metadata snapshot is empty (i.e. `allColumnsFromSnapshot` returns nothing).
 *
 * The `schema` is the `SqlNamespace` tree built from `useSchemaStore.columnMap`
 * via `buildEditorSchema()` — it contains ALL tables and columns loaded when
 * the connection opens, regardless of what's referenced in the current SQL.
 *
 * Tree structure:
 * ```
 * { "tableName": ["col1", "col2", ...] }            // flat
 * { "schema": { "table": ["col1", "col2"] } }       // with namespaces
 * ```
 */
function allColumnsFromEditorSchema(
  schema: SQLNamespace,
  adapter: SqlDialectAdapter,
  quotePolicy: CompletionQuotePolicy = 'unquoted',
): SchemaCompletionItem[] {
  const results: SchemaCompletionItem[] = [];

  const walk = (node: SQLNamespace, tableName: string | null) => {
    if (Array.isArray(node)) {
      // Leaf: `node` is an array of column names for `tableName`.
      if (tableName && node.length > 0) {
        const quotedTable = adapter.quoteIdentifier(tableName);
        for (const colName of node) {
          const quotedLabel = adapter.quoteIdentifier(colName);
          const unquotedLabel = colName;
          const mustQuote = adapter.shouldQuoteIdentifier(colName);

          const addUnquoted = () => {
            results.push({
              label: unquotedLabel,
              filterText: colName,
              type: 'property' as const,
              detail: tableName,
              apply: `${quotedTable}.${unquotedLabel}`,
              boost: 12,
            });
          };

          const addQuoted = (boost = 10) => {
            results.push({
              label: quotedLabel,
              filterText: colName,
              type: 'property' as const,
              detail: tableName,
              apply: `${quotedTable}.${quotedLabel}`,
              boost,
            });
          };

          if (quotePolicy === 'always') {
            addQuoted(10);
          } else if (quotePolicy === 'both') {
            if (mustQuote) {
              addQuoted(12);
            } else {
              addUnquoted();
              addQuoted(10);
            }
          } else {
            // 'unquoted'
            if (mustQuote) {
              addQuoted(12);
            } else {
              addUnquoted();
            }
          }
        }
      }
      return;
    }

    // Branch: each key is either a table (leaf child) or a schema (branch child).
    for (const [key, child] of Object.entries(node)) {
      if (Array.isArray(child)) {
        walk(child, key);
      } else {
        // Nested namespace (schema/catalog) — recurse without setting tableName
        // so that we reach the table leaves within.
        walk(child, tableName);
      }
    }
  };

  walk(schema, null);
  return results;
}

/**
 * Produce relation completions from the editor schema tree.
 * Used when the metadata snapshot is empty or does not contain all relations.
 */
function relationCompletionsFromEditorSchema(
  schema: SQLNamespace,
  adapter: SqlDialectAdapter,
  qualifierParts: readonly string[],
): Completion[] {
  const results: Completion[] = [];
  const seen = new Set<string>();

  const addTable = (name: string, detail = 'table') => {
    const quoted = adapter.quoteIdentifier(name);
    const key = adapter.foldUnquotedIdentifier(name);
    if (seen.has(key)) return;
    seen.add(key);
    results.push({
      label: quoted,
      type: 'type' as const,
      detail,
      apply: quoted,
      boost: 10,
    });
  };

  const addSchema = (name: string) => {
    const quoted = adapter.quoteIdentifier(name);
    const key = adapter.foldUnquotedIdentifier(name);
    if (seen.has(key)) return;
    seen.add(key);
    results.push({
      label: quoted,
      type: 'type' as const,
      detail: 'schema',
      apply: quoted,
      boost: 8,
    });
  };

  if (qualifierParts.length > 0) {
    const lastQualifier = qualifierParts[qualifierParts.length - 1]!;
    for (const [key, child] of Object.entries(schema)) {
      if (!Array.isArray(child) && adapter.compareIdentifiers(key, lastQualifier)) {
        for (const [tblKey] of Object.entries(child)) {
          addTable(tblKey);
        }
      }
    }
    return results;
  }

  for (const [key, child] of Object.entries(schema)) {
    if (Array.isArray(child)) {
      addTable(key);
    } else {
      addSchema(key);
      for (const [tblKey] of Object.entries(child)) {
        addTable(tblKey);
      }
    }
  }

  return results;
}

/**
 * Extract unqualified column names from the SELECT clause of the semantic
 * model's tokens. Looks for identifiers between the SELECT keyword and the
 * next FROM/WHERE/GROUP/ORDER/LIMIT/HAVING/UNION keyword or end of tokens.
 *
 * Returns bare column names (unquoted fold-cased for case-insensitive
 * comparison).
 */
export function extractSelectColumns(model: SqlSemanticModel): string[] {
  const tokens = model.tokens;
  if (tokens.length === 0) return [];

  // Find the SELECT keyword token
  let selectIdx = -1;
  for (let i = 0; i < tokens.length; i++) {
    const t = tokens[i]!;
    if (t.kind === SqlTokenKind.Other && t.text.toUpperCase() === 'SELECT' && t.parenDepth === 0) {
      selectIdx = i;
      break;
    }
  }
  if (selectIdx === -1) return [];

  // Find the terminator (FROM, WHERE, GROUP, ORDER, LIMIT, HAVING, UNION, ...)
  const terminators = new Set([
    'FROM',
    'WHERE',
    'GROUP',
    'ORDER',
    'LIMIT',
    'HAVING',
    'UNION',
    'INTERSECT',
    'EXCEPT',
    'MINUS',
    'INTO',
    'ON',
    'SET',
    'VALUES',
    'RETURNING',
    'WINDOW',
    'FETCH',
    'FOR',
    'LOCK',
    'LIMIT',
  ]);

  let endIdx = tokens.length;
  for (let i = selectIdx + 1; i < tokens.length; i++) {
    const t = tokens[i]!;
    if (
      t.kind === SqlTokenKind.Other &&
      terminators.has(t.text.toUpperCase()) &&
      t.parenDepth === 0
    ) {
      endIdx = i;
      break;
    }
  }

  // Extract identifier tokens between SELECT and terminator
  const columns: string[] = [];
  const skipKeywords = new Set([
    'DISTINCT',
    'ALL',
    'BETWEEN',
    'IN',
    'NOT',
    'LIKE',
    'ILIKE',
    'ESCAPE',
  ]);

  let skipNextIdent = false; // after seeing AS, skip the next identifier (alias)

  for (let i = selectIdx + 1; i < endIdx; i++) {
    const t = tokens[i]!;
    // Skip commas, parentheses, and whitespace
    if (t.text === ',' || t.text === '(' || t.text === ')') continue;
    if (t.kind === SqlTokenKind.Whitespace) continue;

    // Skip known SQL keywords that can appear in SELECT expressions
    if (t.kind === SqlTokenKind.Other && skipKeywords.has(t.text.toUpperCase())) continue;

    // After AS keyword, skip the next identifier (it's an alias)
    if (t.kind === SqlTokenKind.Other && t.text.toUpperCase() === 'AS') {
      skipNextIdent = true;
      continue;
    }

    if (skipNextIdent) {
      // Skip the alias identifier (bare or double-quoted) after AS
      if (t.kind === SqlTokenKind.Other || t.kind === SqlTokenKind.DoubleQuoted) {
        skipNextIdent = false;
        continue;
      }
      skipNextIdent = false;
    }

    // Collect identifiers (unquoted or double-quoted)
    let identText: string | null = null;
    if (t.kind === SqlTokenKind.Other) {
      // Bare identifier: must look like an identifier (starts with letter/_)
      const first = t.text.charCodeAt(0);
      if ((first >= 65 && first <= 90) || (first >= 97 && first <= 122) || first === 95) {
        identText = t.text.toLowerCase();
      }
    } else if (t.kind === SqlTokenKind.DoubleQuoted) {
      const unquoted = t.text.slice(1, -1).replace(/""/g, '"');
      identText = unquoted.toLowerCase();
    }

    if (identText) {
      // Check next non-whitespace token — if it's '(' this is a function call, skip it
      let nextNonWs: SqlToken | undefined;
      for (let j = i + 1; j < endIdx; j++) {
        if (tokens[j]!.kind !== SqlTokenKind.Whitespace) {
          nextNonWs = tokens[j];
          break;
        }
      }
      if (nextNonWs?.text === '(') {
        // This is a function call like count(*), skip it and the entire paren group
        continue;
      }

      columns.push(identText);
    }
  }

  return columns;
}

/**
 * Filter snapshot relations to only those containing ALL of the given column
 * names (case-insensitive). Returns empty if columns list is empty or no
 * table matches all columns.
 */
function filterRelationsByColumns(
  snapshot: EditorMetadataSnapshot,
  selectColumns: string[],
  adapter: SqlDialectAdapter,
): EditorRelationMetadata[] {
  if (selectColumns.length === 0) return [];

  const required = new Set(selectColumns.map((c) => c.toLowerCase()));
  const results: EditorRelationMetadata[] = [];

  for (const [, relMeta] of snapshot.relations) {
    const colNames = new Set(relMeta.columns.map((c) => adapter.foldUnquotedIdentifier(c.name)));
    const hasAll = [...required].every((req) => colNames.has(req));
    if (hasAll) results.push(relMeta);
  }

  return results;
}

/* -------------------------------------------------------------------------- */
/*  Public API                                                                */
/* -------------------------------------------------------------------------- */

/**
 * Resolve `alias.` prefix → column completions for the uniquely-bound relation.
 *
 * Returns `null` if:
 * - The prefix doesn't match an `alias.` pattern.
 * - The alias is ambiguous or unresolved.
 * - No metadata is available for the relation.
 */
export function resolveAliasDotCompletions(
  qualifierParts: readonly string[],
  model: SqlSemanticModel,
  snapshot: EditorMetadataSnapshot,
  adapter?: SqlDialectAdapter,
  schemaTree?: SQLNamespace,
  quotePolicy: CompletionQuotePolicy = 'unquoted',
): SchemaCompletionItem[] | null {
  if (qualifierParts.length !== 1) return null;

  const qualifier = qualifierParts[0]!;
  const dialect = adapter ?? getDialectAdapter('standard');
  const result = resolveRelation(
    qualifier,
    model,
    model.cursorIntent.replacementRange.from,
    dialect,
  );

  if (result.status !== 'unique' || !result.binding) return null;
  // Don't produce column completions for CTEs/subqueries — only for real
  // relations whose metadata is available in the snapshot.
  if (result.binding.sourceKind !== 'table' && result.binding.sourceKind !== 'view') return null;

  const relMeta = matchRelation(result.binding, snapshot, dialect, schemaTree);
  if (!relMeta) return null;

  return columnCompletionsFromRelation(relMeta, '', dialect, quotePolicy);
}

/**
 * Produce completions for the current cursor intent.
 *
 * Strategy:
 * 1. If `qualified_column` with a single qualifier → resolve alias.
 * 2. If `relation` or `join_target` → table completions from snapshot.
 * 3. If `projection` / column context → all visible columns.
 * 4. Fallback → relation completions.
 */
export function produceSchemaCompletions(options: SchemaCompletionOptions): SchemaCompletionItem[] {
  const { model, snapshot, adapter: adapterOverride, quotePolicy = 'unquoted' } = options;
  const adapter = adapterOverride ?? getDialectAdapter('standard');
  const intent = model.cursorIntent;

  switch (intent.kind) {
    case 'qualified_column': {
      const completions = resolveAliasDotCompletions(
        intent.qualifierParts,
        model,
        snapshot,
        adapter,
        options.schema,
        quotePolicy,
      );
      if (completions) return completions;
      // If qualifier is ambiguous or unresolved, return nothing — don't guess.
      return [];
    }

    case 'relation':
    case 'join_target': {
      // Enhancement 2: FROM hint — if SELECT has column names, suggest tables
      // that contain ALL those columns (filtered list appears first).
      const selectColumns = extractSelectColumns(model);
      if (selectColumns.length > 0) {
        const matchingTables = filterRelationsByColumns(snapshot, selectColumns, adapter);
        if (matchingTables.length > 0) {
          const filteredCompletions: Completion[] = matchingTables.map((relMeta) => {
            const name = adapter.quoteIdentifier(relMeta.identity.name.name);
            return {
              label: name,
              type: 'type' as const,
              detail: `${relMeta.kind} (matched columns)`,
              apply: name,
              boost: 15,
            };
          });
          // Also include remaining tables as secondary suggestions
          const filteredKeys = new Set(matchingTables.map((r) => r.key));
          const remainingCompletions = relationCompletionsFromSnapshot(
            snapshot,
            adapter,
            intent.qualifierParts,
          ).filter((c) => {
            const k = relationKeyFromLabel(c.label, adapter, snapshot);
            return k === null || !filteredKeys.has(k);
          });
          return [...filteredCompletions, ...remainingCompletions];
        }
      }

      const fromSnapshot = relationCompletionsFromSnapshot(
        snapshot,
        adapter,
        intent.qualifierParts,
      );
      if (fromSnapshot.length > 0) {
        // If snapshot has tables, also merge any extra tables from schema that aren't in snapshot yet
        if (options.schema) {
          const fromSchema = relationCompletionsFromEditorSchema(
            options.schema,
            adapter,
            intent.qualifierParts,
          );
          const seenLabels = new Set(fromSnapshot.map((c) => c.label));
          for (const item of fromSchema) {
            if (!seenLabels.has(item.label)) {
              fromSnapshot.push(item);
              seenLabels.add(item.label);
            }
          }
        }
        return fromSnapshot;
      }

      // If snapshot is empty, fall back to schema tree
      if (options.schema) {
        return relationCompletionsFromEditorSchema(options.schema, adapter, intent.qualifierParts);
      }

      return [];
    }

    case 'projection': {
      // Column completions from all visible relations, plus aliases.
      const visible = listVisibleRelations(model, intent.replacementRange.from, adapter);
      if (visible.length > 0) {
        const results: Completion[] = [];
        const seenQualifiers = new Set<string>();

        // 1. Add alias completions so typing e.g. "ON e" suggests "eo" and "ec"
        for (const binding of visible) {
          if (binding.alias) {
            const tableName = binding.relation.name.name;
            results.push({
              label: binding.alias,
              type: 'variable',
              detail: `alias for ${tableName}`,
              boost: 8,
            });
          }
        }

        // 2. Add column completions from each visible relation
        for (const binding of visible) {
          const relMeta = matchRelation(binding, snapshot, adapter);
          if (!relMeta) continue;

          const qualifier = binding.alias ?? foldSegment(binding.relation.name, adapter);
          if (seenQualifiers.has(adapter.foldUnquotedIdentifier(qualifier))) continue;
          seenQualifiers.add(adapter.foldUnquotedIdentifier(qualifier));

          results.push(...columnCompletionsFromRelation(relMeta, qualifier, adapter, quotePolicy));
        }

        // Deduplicate completions by label
        const seenLabels = new Set<string>();
        return results.filter((item) => {
          if (seenLabels.has(item.label)) return false;
          seenLabels.add(item.label);
          return true;
        });
      }

      // FALLBACK: no visible relations — show ALL columns from ALL tables.
      // First try the metadata snapshot (tables referenced in the SQL).
      const fromSnapshot = allColumnsFromSnapshot(snapshot, adapter, quotePolicy);
      if (fromSnapshot.length > 0) return fromSnapshot;

      // Second fallback: the full editor schema tree (all tables loaded at
      // connection time). This handles the common case of typing "SELECT ord"
      // with no FROM clause, where the snapshot is empty but the schema tree
      // contains all tables and columns.
      if (options.schema) {
        return allColumnsFromEditorSchema(options.schema, adapter, quotePolicy);
      }

      return [];
    }

    default: {
      // Fallback: relation completions from snapshot.
      return relationCompletionsFromSnapshot(snapshot, adapter, intent.qualifierParts);
    }
  }
}

/**
 * Wrap `produceSchemaCompletions` as a CodeMirror CompletionSource.
 */
export function createSchemaCompletionSource(options: SchemaCompletionOptions) {
  return (): CompletionResult | null => {
    const completions = produceSchemaCompletions(options);
    if (completions.length === 0) return null;
    return {
      from: options.model.cursorIntent.replacementRange.from,
      options: completions,
      validFor: /^[A-Za-z_$"][\w$"']*$/,
    };
  };
}
