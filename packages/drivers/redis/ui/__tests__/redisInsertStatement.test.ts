/**
 * 键头行「复制插入语句」纯函数单测（本轨 E-4，PRD §3.3 键头行 / 裁定 8-4）。
 *
 * `buildRedisInsertStatement` 把 `get_key` 返回的各类 value 形状
 * （`key_value_json`，packages/drivers/redis/src/driver/session.rs 只读镜像）
 * 转成一条 redis-cli 兼容的重建命令；stream/未知形状返回 null（宁可不给，
 * 也不给一条会撒谎的语句）。载荷是服务器数据，永不翻译。
 */
import { describe, expect, it } from 'vitest';
import { buildRedisInsertStatement } from '../value-editors/redisInsertStatement';

function detail(keyType: string, value: unknown) {
  return { key: 'user:1', keyType, value } as Parameters<typeof buildRedisInsertStatement>[0];
}

describe('buildRedisInsertStatement', () => {
  it('builds SET from the string `{ value }` wrapper', () => {
    expect(buildRedisInsertStatement(detail('string', { value: 'hello' }))).toBe(
      'SET user:1 hello',
    );
  });

  it('builds SET from a raw string value', () => {
    expect(buildRedisInsertStatement(detail('string', 'plain text'))).toBe(
      'SET user:1 "plain text"',
    );
  });

  it('quotes values that need it and leaves plain tokens readable', () => {
    expect(buildRedisInsertStatement(detail('string', { value: 'say "hi"' }))).toBe(
      'SET user:1 "say \\"hi\\""',
    );
    expect(buildRedisInsertStatement(detail('string', { value: 'back\\slash' }))).toBe(
      'SET user:1 "back\\\\slash"',
    );
    expect(buildRedisInsertStatement(detail('string', { value: 'a b' }))).toBe('SET user:1 "a b"');
  });

  it('builds HSET with every field/value pair', () => {
    expect(buildRedisInsertStatement(detail('hash', { f1: 'v1', 'f 2': 'v 2' }))).toBe(
      'HSET user:1 f1 v1 "f 2" "v 2"',
    );
  });

  it('returns null for a malformed or empty hash', () => {
    expect(buildRedisInsertStatement(detail('hash', ['not', 'a', 'map']))).toBeNull();
    expect(buildRedisInsertStatement(detail('hash', {}))).toBeNull();
  });

  it('builds RPUSH from a list of strings', () => {
    expect(buildRedisInsertStatement(detail('list', ['a', 'b c']))).toBe('RPUSH user:1 a "b c"');
  });

  it('returns null for an empty or non-string list', () => {
    expect(buildRedisInsertStatement(detail('list', []))).toBeNull();
    expect(buildRedisInsertStatement(detail('list', [1, 2]))).toBeNull();
  });

  it('builds SADD from a set of strings', () => {
    expect(buildRedisInsertStatement(detail('set', ['m1', 'm2']))).toBe('SADD user:1 m1 m2');
  });

  it('builds ZADD with score/member pairs', () => {
    expect(
      buildRedisInsertStatement(
        detail('zset', [
          { member: 'one', score: 1 },
          { member: 'two words', score: 2.5 },
        ]),
      ),
    ).toBe('ZADD user:1 1 one 2.5 "two words"');
  });

  it('returns null for malformed zset entries', () => {
    expect(buildRedisInsertStatement(detail('zset', [{ member: 'x' }]))).toBeNull();
    expect(buildRedisInsertStatement(detail('zset', []))).toBeNull();
  });

  it('refuses stream, json and unknown shapes (no statement that lies)', () => {
    expect(buildRedisInsertStatement(detail('stream', [{ id: '1-0' }]))).toBeNull();
    expect(buildRedisInsertStatement(detail('json', { doc: true }))).toBeNull();
    expect(buildRedisInsertStatement(detail('ReJSON-RL', { doc: true }))).toBeNull();
    expect(buildRedisInsertStatement(detail('string', null))).toBe('SET user:1 ""');
  });
});
