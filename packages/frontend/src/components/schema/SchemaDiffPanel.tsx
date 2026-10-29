import { useI18n } from '../../hooks/useI18n';
import type { TableSchemaDiff } from '../../types';

export function SchemaDiffPanel({ diff }: { diff: TableSchemaDiff }) {
  const { t } = useI18n();
  const missing = diff.missingOnTarget ?? diff.added;
  const extra = diff.extraOnTarget ?? diff.removed;
  const missingChecks = diff.missingCheckConstraints ?? [];
  const extraChecks = diff.extraCheckConstraints ?? [];
  const tableOptions = diff.tableOptions;
  const identical =
    !diff.targetOnly &&
    missing.length === 0 &&
    extra.length === 0 &&
    diff.changed.length === 0 &&
    missingChecks.length === 0 &&
    extraChecks.length === 0 &&
    !tableOptions;

  return (
    <div className="space-y-4 text-xs">
      {diff.targetOnly && (
        <section
          data-testid="schema-diff-target-only-detail"
          className="rounded border border-danger/30 bg-danger/10 p-3 text-danger"
        >
          {t('schemaDiff.targetOnlyDetail')}
        </section>
      )}
      {missing.length > 0 && (
        <section>
          <h4 className="mb-1.5 font-semibold text-success">{t('schemaDiff.missingOnTarget')}</h4>
          {missing.map((col) => (
            <div key={col.name} className="mb-1 font-mono text-fg-secondary">
              + {col.name} ({col.dataType}
              {col.nullable ? '' : ', NOT NULL'}
              {col.isPrimaryKey ? ', PK' : ''})
            </div>
          ))}
        </section>
      )}
      {extra.length > 0 && (
        <section>
          <h4 className="mb-1.5 font-semibold text-danger">{t('schemaDiff.extraOnTarget')}</h4>
          {extra.map((col) => (
            <div key={col.name} className="mb-1 font-mono text-fg-secondary">
              - {col.name} ({col.dataType}
              {col.nullable ? '' : ', NOT NULL'}
              {col.isPrimaryKey ? ', PK' : ''})
            </div>
          ))}
        </section>
      )}
      {diff.changed.length > 0 && (
        <section>
          <h4 className="mb-1.5 font-semibold text-warning">{t('schemaDiff.colChanged')}</h4>
          {diff.changed.map((col) => (
            <div
              key={col.name}
              className="mb-2 rounded border border-edge bg-surface-alt p-2 font-mono text-[11px]"
            >
              <div className="font-medium text-fg">{col.name}</div>
              <div className="mt-1 text-fg-secondary">
                {t('schemaDiff.source')}: {col.source.dataType}
                {col.source.nullable ? '' : ', NOT NULL'}
                {col.source.isPrimaryKey ? ', PK' : ''}
              </div>
              <div className="text-fg-secondary">
                {t('schemaDiff.target')}: {col.target.dataType}
                {col.target.nullable ? '' : ', NOT NULL'}
                {col.target.isPrimaryKey ? ', PK' : ''}
              </div>
              <div className="mt-1 text-fg-muted">{col.changes.join(', ')}</div>
            </div>
          ))}
        </section>
      )}
      {missingChecks.length > 0 && (
        <section>
          <h4 className="mb-1.5 font-semibold text-success">{t('schemaDiff.checkMissing')}</h4>
          {missingChecks.map((constraint) => (
            <div key={constraint.name} className="mb-1 font-mono text-fg-secondary">
              + {constraint.name}: CHECK ({constraint.expression})
            </div>
          ))}
        </section>
      )}
      {extraChecks.length > 0 && (
        <section>
          <h4 className="mb-1.5 font-semibold text-danger">{t('schemaDiff.checkExtra')}</h4>
          {extraChecks.map((constraint) => (
            <div key={constraint.name} className="mb-1 font-mono text-fg-secondary">
              - {constraint.name}: CHECK ({constraint.expression})
            </div>
          ))}
        </section>
      )}
      {tableOptions && (
        <section>
          <h4 className="mb-1.5 font-semibold text-warning">{t('schemaDiff.tableOptions')}</h4>
          {tableOptions.changes.map((change) => {
            const source = tableOptions.source[change as keyof typeof tableOptions.source];
            const target = tableOptions.target[change as keyof typeof tableOptions.target];
            return (
              <div key={change} className="mb-1 font-mono text-fg-secondary">
                ~ {change}: {String(target ?? '')} -&gt; {String(source ?? '')}
              </div>
            );
          })}
        </section>
      )}
      {identical && <div className="text-fg-muted">{t('schemaDiff.schemaIdentical')}</div>}
    </div>
  );
}

/** Plain-text summary for clipboard / ALTER hints. */
export function formatSchemaDiffText(diff: TableSchemaDiff): string {
  const missing = diff.missingOnTarget ?? diff.added;
  const extra = diff.extraOnTarget ?? diff.removed;
  const missingChecks = diff.missingCheckConstraints ?? [];
  const extraChecks = diff.extraCheckConstraints ?? [];
  const tableOptions = diff.tableOptions;
  const lines: string[] = [`-- Schema diff: ${diff.table}`];
  for (const col of missing) {
    lines.push(`+ ${col.name} ${col.dataType}${col.nullable ? '' : ' NOT NULL'}`);
  }
  for (const col of extra) {
    lines.push(`- ${col.name} ${col.dataType}`);
  }
  for (const col of diff.changed) {
    lines.push(
      `~ ${col.name}: ${col.target.dataType} -> ${col.source.dataType} (${col.changes.join(', ')})`,
    );
  }
  for (const constraint of missingChecks) {
    lines.push(`+ ${constraint.name}: CHECK (${constraint.expression})`);
  }
  for (const constraint of extraChecks) {
    lines.push(`- ${constraint.name}: CHECK (${constraint.expression})`);
  }
  if (tableOptions) {
    for (const change of tableOptions.changes) {
      const source = tableOptions.source[change as keyof typeof tableOptions.source];
      const target = tableOptions.target[change as keyof typeof tableOptions.target];
      lines.push(`~ table ${change}: ${String(target ?? '')} -> ${String(source ?? '')}`);
    }
  }
  if (lines.length === 1) lines.push('(identical)');
  return lines.join('\n');
}
