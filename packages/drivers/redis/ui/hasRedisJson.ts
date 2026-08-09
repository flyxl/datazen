/** True when MODULE LIST includes ReJSON / RedisJSON. */
export function hasRedisJson(modules: string[]): boolean {
  return modules.some((name) => {
    const lower = name.toLowerCase();
    return lower === 'rejson' || lower === 'redisjson';
  });
}
