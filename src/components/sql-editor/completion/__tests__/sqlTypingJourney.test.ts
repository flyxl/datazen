import { describe, expect, it } from 'vitest';
import { EditorState } from '@codemirror/state';
import { CompletionContext } from '@codemirror/autocomplete';
import {
  inferSqlCompletionKind,
  contextualKeywordCompletion,
  contextualSchemaCompletion,
  filterKeywordsByKind,
  filterCompletionsByKind,
} from '../../../../lib/sqlCompletionContext';
import { resolveCmDialect } from '../../contracts';
import { keywordCompletionSource } from '@codemirror/lang-sql';
import { buildSemanticModel } from '../../semantic/scopeModel';
import { produceSchemaCompletions } from '../schemaCompletion';

describe('SQL Editing Typing Journey (Continuous Typing State Machine)', () => {
  const pgDialect = resolveCmDialect('postgresql');
  const baseKeywordSource = contextualKeywordCompletion(keywordCompletionSource(pgDialect, true));

  const testTypingStep = (sql: string, explicit = true) => {
    const state = EditorState.create({ doc: sql });
    const pos = sql.length;
    const context = new CompletionContext(state, pos, explicit);
    const kind = inferSqlCompletionKind(sql);
    const model = buildSemanticModel(sql, pos, { dialectId: 'postgresql' });
    const kwResult = baseKeywordSource(context);
    const kwOptions = kwResult && 'options' in kwResult ? kwResult.options : [];
    const kwLabels = kwOptions.map((o) => o.label);

    return {
      kind,
      intent: model.cursorIntent.kind,
      kwLabels,
    };
  };

  it('Step 1: typing "SEL" -> suggests SELECT keyword', () => {
    const { kwLabels } = testTypingStep('SEL');
    expect(kwLabels).toContain('SELECT');
  });

  it('Step 2: typing "SELECT * FR" -> suggests FROM keyword', () => {
    const { kwLabels } = testTypingStep('SELECT * FR');
    expect(kwLabels).toContain('FROM');
  });

  it('Step 3: typing "SELECT * FROM ec" -> table context, suppresses system variables', () => {
    const { kind, intent, kwLabels } = testTypingStep('SELECT * FROM ec');
    expect(kind).toBe('table');
    expect(intent).toBe('relation');
    // Suppresses PL/pgSQL exception keywords and internal diagnostics
    expect(kwLabels).not.toContain('PG_EXCEPTION_HINT');
    expect(kwLabels).not.toContain('OCCURRENCES_REGEX');
    expect(kwLabels).not.toContain('PERCENTILE_DISC');
  });

  it('Step 4: typing "SELECT * FROM er_customers " -> table completed, ready for alias or clause', () => {
    const { kind, intent, kwLabels } = testTypingStep('SELECT * FROM er_customers ');
    expect(kind).toBe('any');
    expect(intent).not.toBe('relation');
    // WHERE, JOIN, ORDER, GROUP must be available as subsequent keywords!
    expect(kwLabels).toContain('WHERE');
    expect(kwLabels).toContain('JOIN');
    expect(kwLabels).toContain('ORDER');
  });

  it('Step 5: typing "SELECT * FROM er_customers WHE" -> suggests WHERE keyword without suppression', () => {
    const { kind, intent, kwLabels } = testTypingStep('SELECT * FROM er_customers WHE');
    expect(kind).toBe('any');
    expect(intent).not.toBe('relation');
    expect(kwLabels).toContain('WHERE');
  });

  it('Step 6: typing "SELECT * FROM er_customers ec WHE" -> suggests WHERE after table with alias', () => {
    const { kind, intent, kwLabels } = testTypingStep('SELECT * FROM er_customers ec WHE');
    expect(kind).toBe('any');
    expect(intent).not.toBe('relation');
    expect(kwLabels).toContain('WHERE');
  });

  it('Step 7: typing "SELECT * FROM er_customers WHERE " -> column context for predicates', () => {
    const { kind, intent, kwLabels } = testTypingStep('SELECT * FROM er_customers WHERE ');
    expect(kind).toBe('column');
    expect(intent).toBe('projection');
    // Problem 1: WHERE clause must retain expression operators and clauses
    expect(kwLabels).toContain('AND');
    expect(kwLabels).toContain('OR');
    expect(kwLabels).toContain('NOT');
    expect(kwLabels).toContain('IN');
    expect(kwLabels).toContain('IS');
    expect(kwLabels).toContain('NULL');
    expect(kwLabels).toContain('LIKE');
    expect(kwLabels).toContain('ORDER');
    // But suppress irrelevant DDL and admin keywords
    expect(kwLabels).not.toContain('CREATE');
    expect(kwLabels).not.toContain('DROP');
    expect(kwLabels).not.toContain('ALTER');
    expect(kwLabels).not.toContain('VACUUM');
    expect(kwLabels).not.toContain('GRANT');
    expect(kwLabels).not.toContain('REVOKE');
  });

  it('Step 8: typing "SELECT * FROM er_customers, ord" -> comma starts new table context', () => {
    const { kind, intent, kwLabels } = testTypingStep('SELECT * FROM er_customers, ord');
    expect(kind).toBe('table');
    expect(intent).toBe('relation');
    expect(kwLabels).not.toContain('PG_EXCEPTION_HINT');
  });

  it('Step 9: typing "SELECT * FROM er_customers, orders WHE" -> multi-table completed, suggests WHERE', () => {
    const { kind, intent, kwLabels } = testTypingStep('SELECT * FROM er_customers, orders WHE');
    expect(kind).toBe('any');
    expect(intent).not.toBe('relation');
    expect(kwLabels).toContain('WHERE');
  });

  it('Step 10: Multi-statement editing -> Statement 2 autocompletion is NOT degraded and isolates statement scope', () => {
    const doc = 'SELECT * FROM er_customers;\nSELECT * FROM er_orders o WHERE ';
    const pos = doc.length;
    const kind = inferSqlCompletionKind(doc);
    const model = buildSemanticModel(doc, pos, { dialectId: 'postgresql' });

    expect(kind).toBe('column');
    expect(model.statement.index).toBe(1);
    expect(model.cursorIntent.kind).toBe('projection');

    // Scopes in Statement 2 must have er_orders with alias 'o', NOT er_customers
    const scope =
      model.scopes.find((s) => pos >= s.range.from && pos <= s.range.to) ?? model.scopes[0]!;
    expect(scope.relations).toHaveLength(1);
    expect(scope.relations[0]!.relation.name.name).toBe('er_orders');
    expect(scope.relations[0]!.alias).toBe('o');
  });

  it('Step 11: typing "SELECT * FROM er_customers WHERE na" -> defaults to unquoted, auto-quotes reserved keywords, supports both mode', () => {
    const doc = 'SELECT * FROM er_customers WHERE na';
    const pos = doc.length;
    const model = buildSemanticModel(doc, pos, { dialectId: 'postgresql' });
    const erCustomers = {
      key: 'er_customers',
      identity: { namespacePath: [], name: { name: 'er_customers', quoted: false } },
      kind: 'table' as const,
      columns: [
        { name: 'id', dataType: 'integer', nullable: false, isPrimaryKey: true },
        { name: 'name', dataType: 'character varying', nullable: true, isPrimaryKey: false },
        { name: 'order', dataType: 'character varying', nullable: true, isPrimaryKey: false },
      ],
    };
    const snapshot = {
      dbSessionId: 's1',
      epoch: 1,
      relations: new Map([['er_customers', erCustomers]]),
    };

    // 1. Default policy ('unquoted'): safe identifier is unquoted, reserved keyword is quoted
    const completions = produceSchemaCompletions({ model, snapshot });
    const labels = completions.map((c) => c.label);
    expect(labels).toContain('name'); // safe -> unquoted
    expect(labels).toContain('"order"'); // reserved keyword -> auto-quoted!
    expect(labels).not.toContain('"name"'); // unquoted default does not duplicate unquoted names

    // 2. 'both' policy: provides both unquoted and quoted variants
    const bothCompletions = produceSchemaCompletions({ model, snapshot, quotePolicy: 'both' });
    const bothLabels = bothCompletions.map((c) => c.label);
    expect(bothLabels).toContain('name');
    expect(bothLabels).toContain('"name"');
    const unquotedItem = bothCompletions.find((c) => c.label === 'name');
    const quotedItem = bothCompletions.find((c) => c.label === '"name"');
    expect(unquotedItem?.boost).toBeGreaterThan(quotedItem?.boost ?? 0);
  });

  it('Step 12: typing AN after condition and then AND -> suggests AND keyword and then columns', () => {
    const sqlAN = 'SELECT * FROM er_customers WHERE er_customers."name" = \'张军\' AN';
    const stepAN = testTypingStep(sqlAN, false);
    expect(stepAN.kwLabels).toContain('AND');

    const sqlANDci = 'SELECT * FROM er_customers WHERE er_customers."name" = \'张军\' AND ci';
    const stepANDci = testTypingStep(sqlANDci, false);
    expect(stepANDci.kind).toBe('column');
    expect(stepANDci.intent).toBe('projection');
  });
});
