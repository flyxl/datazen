import { useCallback, useEffect, useState } from 'react';
import { Check, Copy, Loader2 } from 'lucide-react';
import { getCachedDDL } from '../../lib/schemaCache';
import { SqlCodeBlock } from '../../components/SqlCodeBlock';
import { Button } from '../../components/ui/Button';
import { useI18n } from '../../hooks/useI18n';
import { DB_REGISTRY } from '../../lib/databaseTypes';
import type { DatabaseType } from '../../types';

interface DDLViewProps {
  connectionId: string;
  tableName: string;
  databaseType?: string;
}

export function DDLView({ connectionId, tableName, databaseType }: DDLViewProps) {
  const { t } = useI18n();
  const [ddl, setDdl] = useState('');
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setError(null);
    setDdl('');

    const dialect = DB_REGISTRY[databaseType as DatabaseType]?.sqlDialect;
    const isMySQLDialect = dialect === 'mysql';
    const sql = isMySQLDialect
      ? `SHOW CREATE TABLE \`${tableName}\``
      : `
      SELECT
        'CREATE TABLE ' || quote_ident(schemaname) || '.' || quote_ident(tablename) || E' (\\n' ||
        string_agg(
          '  ' || quote_ident(attname) || ' ' || format_type(atttypid, atttypmod) ||
          CASE WHEN NOT attnotnull THEN '' ELSE ' NOT NULL' END ||
          CASE WHEN pg_get_expr(adbin, adrelid) IS NOT NULL
               THEN ' DEFAULT ' || pg_get_expr(adbin, adrelid)
               ELSE '' END,
          E',\\n' ORDER BY attnum
        ) || E'\\n);' AS ddl
      FROM pg_tables t
      JOIN pg_attribute a ON a.attrelid = (quote_ident(schemaname) || '.' || quote_ident(tablename))::regclass
      LEFT JOIN pg_attrdef d ON d.adrelid = a.attrelid AND d.adnum = a.attnum
      WHERE a.attnum > 0 AND NOT a.attisdropped
        AND tablename = '${tableName}'
      GROUP BY schemaname, tablename;
    `;

    getCachedDDL(connectionId, tableName, sql, (rows) => {
      const row = rows[0];
      const val = isMySQLDialect ? row?.[1] : row?.[0];
      return typeof val === 'string' ? val : val != null ? String(val) : `-- ${t('ddl.getFailed')}`;
    })
      .then((result) => {
        if (!cancelled) { setDdl(result); setLoading(false); }
      })
      .catch((e) => {
        if (!cancelled) {
          setError(typeof e === 'string' ? e : e instanceof Error ? e.message : t('ddl.loadFailed'));
          setLoading(false);
        }
      });

    return () => { cancelled = true; };
  }, [connectionId, tableName, databaseType, t]);

  const handleCopy = useCallback(async () => {
    try {
      await navigator.clipboard.writeText(ddl);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch {
      // clipboard not available
    }
  }, [ddl]);

  if (loading) {
    return (
      <div className="flex flex-1 items-center justify-center gap-2 text-fg-muted">
        <Loader2 className="h-5 w-5 animate-spin" />
        {t('ddl.generating')}
      </div>
    );
  }

  if (error) {
    return <div className="flex flex-1 items-center justify-center text-sm text-red-400">{error}</div>;
  }

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="flex items-center justify-between px-4 py-3">
        <div className="flex items-baseline gap-2">
          <span className="text-base font-semibold text-fg">{tableName}</span>
          <span className="text-sm text-fg-muted">· DDL</span>
        </div>
        <Button variant="secondary" className="h-7 gap-1 px-2 text-xs" onClick={() => void handleCopy()}>
          {copied ? <Check className="h-3.5 w-3.5 text-green-400" /> : <Copy className="h-3.5 w-3.5" />}
          {copied ? t('common.copied') : t('common.copy')}
        </Button>
      </div>

      <div className="min-h-0 flex-1 overflow-auto bg-surface">
        <SqlCodeBlock code={ddl} dialect={DB_REGISTRY[databaseType as DatabaseType]?.sqlDialect} />
      </div>
    </div>
  );
}
