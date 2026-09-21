import { useCallback, useMemo, useState } from 'react';
import { ChevronDown, ChevronRight, Search, X } from 'lucide-react';
import { Button, cn } from '@datazen/ui';
import { useI18n } from '@datazen/ui';
import { redisCommandInvoke } from '../shared/redisInvoke';
import {
  parseInfoSections,
  filterInfoSections,
  type InfoSection,
  type FilteredInfoResult,
} from './infoParse';

export interface SearchableInfoPanelProps {
  dbSessionId: string;
  section?: string;
  pinnedNodeAddr?: string;
}

function highlight(text: string, query?: string): { __html: string } {
  if (!query) return { __html: escapeHtml(text) };
  const escaped = escapeHtml(text);
  const q = escapeHtml(query);
  const regex = new RegExp(`(${q.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')})`, 'gi');
  return {
    __html: escaped.replace(
      regex,
      '<mark class="bg-yellow-600/30 text-yellow-200 rounded px-0.5">$1</mark>',
    ),
  };
}

function escapeHtml(s: string): string {
  return s
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

export function SearchableInfoPanel({
  dbSessionId,
  section,
  pinnedNodeAddr,
}: SearchableInfoPanelProps) {
  const { t } = useI18n();
  const [rawInfo, setRawInfo] = useState<string>('');
  const [search, setSearch] = useState('');
  const [expandedSections, setExpandedSections] = useState<Set<string>>(new Set());
  const [loading, setLoading] = useState(false);

  const sections = useMemo(() => parseInfoSections(rawInfo), [rawInfo]);
  const filtered: FilteredInfoResult = useMemo(
    () => filterInfoSections(sections, search || undefined),
    [sections, search],
  );

  const fetchInfo = useCallback(async () => {
    setLoading(true);
    try {
      const result = await redisCommandInvoke('redis', 'info_filtered', {
        dbSessionId,
        section: section || undefined,
        search: search || undefined,
        nodeAddr: pinnedNodeAddr || undefined,
      });
      // Server returns { sections, totalEntries, matchedEntries }
      if (result && typeof result === 'object' && 'sections' in result) {
        const raw = reconstructInfo(result as FilteredInfoResult);
        setRawInfo(raw);
      } else {
        setRawInfo(typeof result === 'string' ? result : JSON.stringify(result));
      }
    } catch {
      try {
        const raw = await redisCommandInvoke('redis', 'info', {
          dbSessionId,
          section: section || undefined,
          nodeAddr: pinnedNodeAddr || undefined,
        });
        setRawInfo(typeof raw === 'string' ? raw : JSON.stringify(raw));
      } catch {
        setRawInfo('');
      }
    } finally {
      setLoading(false);
    }
  }, [dbSessionId, section, pinnedNodeAddr, search]);

  const toggleSection = useCallback((name: string) => {
    setExpandedSections((prev) => {
      const next = new Set(prev);
      if (next.has(name)) next.delete(name);
      else next.add(name);
      return next;
    });
  }, []);

  return (
    <div className="flex flex-col gap-2 p-2">
      {/* Search bar */}
      <div className="flex items-center gap-2">
        <div className="relative flex-1">
          <Search className="absolute left-2 top-1/2 -translate-y-1/2 h-3.5 w-3.5 text-muted-foreground" />
          <input
            type="text"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            placeholder={t('redis.monitor.infoSearchPlaceholder', 'Search INFO keys/values...')}
            className="w-full rounded border border-border bg-background pl-7 pr-7 py-1.5 text-xs outline-none focus:ring-1 focus:ring-ring"
          />
          {search && (
            <button
              onClick={() => setSearch('')}
              className="absolute right-2 top-1/2 -translate-y-1/2 text-muted-foreground hover:text-foreground"
            >
              <X className="h-3.5 w-3.5" />
            </button>
          )}
        </div>
        <Button
          variant="outline"
          size="sm"
          onClick={fetchInfo}
          disabled={loading}
          className="shrink-0"
        >
          {loading ? '...' : t('redis.monitor.refresh', 'Refresh')}
        </Button>
      </div>

      {/* Stats */}
      {rawInfo && (
        <div className="text-xs text-muted-foreground px-1">
          {search
            ? `${filtered.matchedEntries} / ${filtered.totalEntries} ${t('redis.monitor.infoMatched', 'matched')}`
            : `${filtered.totalEntries} ${t('redis.monitor.infoEntries', 'entries')}`}
          {filtered.sections.length > 0 &&
            ` · ${filtered.sections.length} ${t('redis.monitor.infoSections', 'sections')}`}
        </div>
      )}

      {/* Section list */}
      <div className="flex flex-col gap-1 overflow-y-auto max-h-[600px]">
        {filtered.sections.map((sec) => (
          <SectionCard
            key={sec.name}
            section={sec}
            expanded={expandedSections.has(sec.name)}
            onToggle={() => toggleSection(sec.name)}
            searchQuery={search || undefined}
          />
        ))}
        {rawInfo && filtered.sections.length === 0 && (
          <div className="text-xs text-muted-foreground text-center py-4">
            {t('redis.monitor.infoNoMatch', 'No matching entries')}
          </div>
        )}
        {!rawInfo && !loading && (
          <div className="text-xs text-muted-foreground text-center py-4">
            {t('redis.monitor.infoHint', 'Click Refresh to load INFO data')}
          </div>
        )}
      </div>
    </div>
  );
}

function SectionCard({
  section,
  expanded,
  onToggle,
  searchQuery,
}: {
  section: InfoSection;
  expanded: boolean;
  onToggle: () => void;
  searchQuery?: string;
}) {
  return (
    <div className="rounded border border-border overflow-hidden">
      <button
        onClick={onToggle}
        className="flex items-center gap-2 w-full px-3 py-2 text-xs font-medium bg-muted/50 hover:bg-muted/80 text-left"
      >
        {expanded ? (
          <ChevronDown className="h-3.5 w-3.5 shrink-0" />
        ) : (
          <ChevronRight className="h-3.5 w-3.5 shrink-0" />
        )}
        <span>{section.name}</span>
        <span className="ml-auto text-muted-foreground font-normal">{section.entries.length}</span>
      </button>
      {expanded && (
        <div className="divide-y divide-border">
          {section.entries.map((entry, i) => (
            <div key={`${entry.key}-${i}`} className="flex items-baseline px-3 py-1 text-xs gap-2">
              <span
                className="font-mono text-blue-400 shrink-0"
                dangerouslySetInnerHTML={highlight(entry.key, searchQuery)}
              />
              <span className="text-muted-foreground shrink-0">=</span>
              <span
                className="font-mono text-foreground break-all"
                dangerouslySetInnerHTML={highlight(entry.value, searchQuery)}
              />
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

function reconstructInfo(result: FilteredInfoResult): string {
  const lines: string[] = [];
  for (const sec of result.sections) {
    lines.push(`# ${sec.name}`);
    for (const [k, v] of sec.entries) {
      lines.push(`${k}:${v}`);
    }
  }
  return lines.join('\n');
}
