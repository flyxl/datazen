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
