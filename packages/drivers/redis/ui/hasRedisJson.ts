/** True when MODULE LIST includes ReJSON / RedisJSON. */
export function hasRedisJson(modules: string[]): boolean {
  return modules.some((name) => {
    const lower = name.toLowerCase();
    return lower === 'rejson' || lower === 'redisjson';
  });
}

/** True when Redis TYPE indicates a RedisJSON key. */
export function isJsonKeyType(keyType: string): boolean {
  const lower = keyType.toLowerCase();
  return lower === 'json' || lower.includes('rejson');
}

/** True when key detail suggests a module JSON type (unsupported bucket). */
export function looksLikeJsonModuleDetail(detail: {
  keyType: string;
  value: unknown;
}): boolean {
  if (isJsonKeyType(detail.keyType)) return true;
  const v = detail.value;
  if (typeof v === 'object' && v !== null && 'raw' in v) {
    const raw = String((v as { raw: unknown }).raw).toLowerCase();
    return raw.includes('rejson') || /\bjson\b/.test(raw);
  }
  return false;
}
