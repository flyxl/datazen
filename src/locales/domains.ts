/**
 * Locale domain packs.
 *
 * Eager domains are always loaded with the shipping locale pair (en / zh-CN).
 * Lazy domains are code-split and loaded when a feature window opens via
 * {@link ensureLocaleDomains}.
 */

export const EAGER_DOMAINS = [
  'core',
  'connection',
  'schema',
  'query',
  'settings',
  'chart',
  'backup',
  'ai',
] as const;

export const LAZY_DOMAINS = [
  'sync',
  'transfer',
  'schemaDiff',
  'workflows',
  'dashboard',
  'mcp',
] as const;

export type EagerDomain = (typeof EAGER_DOMAINS)[number];
export type LazyDomain = (typeof LAZY_DOMAINS)[number];
export type LocaleDomain = EagerDomain | LazyDomain;

export const ALL_DOMAINS: readonly LocaleDomain[] = [...EAGER_DOMAINS, ...LAZY_DOMAINS];

export function isLazyDomain(d: string): d is LazyDomain {
  return (LAZY_DOMAINS as readonly string[]).includes(d);
}
