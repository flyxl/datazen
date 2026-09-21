export interface InfoSection {
  name: string;
  entries: Array<{ key: string; value: string }>;
}

/** Parse Redis `INFO` text into named sections with key/value pairs. */
export function parseInfoSections(raw: string): InfoSection[] {
  const sections: InfoSection[] = [];
  let current: InfoSection | null = null;

  for (const line of raw.split(/[\n\r]+/).filter((l) => l.length > 0)) {
    if (line.startsWith('# ')) {
      if (current) {
        sections.push(current);
      }
      current = { name: line.slice(2).trim(), entries: [] };
      continue;
    }
    if (current) {
      const colon = line.indexOf(':');
      if (colon >= 0) {
        current.entries.push({
          key: line.slice(0, colon).trim(),
          value: line.slice(colon + 1).trim(),
        });
      }
    }
  }

  if (current) {
    sections.push(current);
  }

  return sections;
}

export interface FilteredInfoResult {
  sections: InfoSection[];
  totalEntries: number;
  matchedEntries: number;
}

/** Filter parsed INFO sections by optional section name and keyword search. */
export function filterInfoSections(sections: InfoSection[], search?: string): FilteredInfoResult {
  const q = search?.toLowerCase().trim();
  let totalEntries = 0;
  let matchedEntries = 0;
  const filtered: InfoSection[] = [];

  for (const section of sections) {
    totalEntries += section.entries.length;
    const matched = section.entries.filter((e) => {
      if (!q) {
        matchedEntries++;
        return true;
      }
      const hit = e.key.toLowerCase().includes(q) || e.value.toLowerCase().includes(q);
      if (hit) matchedEntries++;
      return hit;
    });
    if (matched.length > 0) {
      filtered.push({ name: section.name, entries: matched });
    }
  }

  return { sections: filtered, totalEntries, matchedEntries };
}
