import { useI18n } from '../../hooks/useI18n';
import type { TableSchemaDiff } from '../../types';

export function SchemaDiffPanel({ diff }: { diff: TableSchemaDiff }) {
  const { t } = useI18n();
  const missing = diff.missingOnTarget ?? diff.added;
  const extra = diff.extraOnTarget ?? diff.removed;
  const identical =
    missing.length === 0 && extra.length === 0 && diff.changed.length === 0;

  return (
    <div className="space-y-4 text-xs">
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
          <h4 className="mb-1.5 font-semibold text-warning">{t('sync.colChanged')}</h4>
          {diff.changed.map((col) => (
            <div
              key={col.name}
              className="mb-2 rounded border border-edge bg-surface-alt p-2 font-mono text-[11px]"
            >
              <div className="font-medium text-fg">{col.name}</div>
              <div className="mt-1 text-fg-secondary">
                {t('sync.source')}: {col.source.dataType}
                {col.source.nullable ? '' : ', NOT NULL'}
                {col.source.isPrimaryKey ? ', PK' : ''}
              </div>
              <div className="text-fg-secondary">
                {t('sync.target')}: {col.target.dataType}
                {col.target.nullable ? '' : ', NOT NULL'}
                {col.target.isPrimaryKey ? ', PK' : ''}
              </div>
              <div className="mt-1 text-fg-muted">{col.changes.join(', ')}</div>
            </div>
          ))}
        </section>
      )}
      {identical && <div className="text-fg-muted">{t('sync.schemaIdentical')}</div>}
    </div>
  );
}

/** Plain-text summary for clipboard / ALTER hints. */
export function formatSchemaDiffText(diff: TableSchemaDiff): string {
  const missing = diff.missingOnTarget ?? diff.added;
  const extra = diff.extraOnTarget ?? diff.removed;
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
  if (lines.length === 1) lines.push('(identical)');
  return lines.join('\n');
}
