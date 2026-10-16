import { realpathSync } from 'node:fs';
import { pathToFileURL } from 'node:url';

/** Recognize CLI entry paths, including symlinks and URL-escaped characters. */
export function isDirectInvocation(moduleUrl, entry = process.argv[1]) {
  if (!entry) return false;
  try {
    return moduleUrl === pathToFileURL(realpathSync(entry)).href ||
      moduleUrl === pathToFileURL(entry).href;
  } catch {
    return moduleUrl === pathToFileURL(entry).href;
  }
}
