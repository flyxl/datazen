import { describe, it, expect } from 'vitest';
import {
  classifyDangerLevel,
  requiresConfirmation,
  dangerBadgeColor,
} from '../console/redisConsoleDanger';

describe('redisConsoleDanger', () => {
  describe('classifyDangerLevel', () => {
    it('classifies safe commands', () => {
      expect(classifyDangerLevel('GET foo')).toBe('safe');
      expect(classifyDangerLevel('INFO')).toBe('safe');
      expect(classifyDangerLevel('PING')).toBe('safe');
      expect(classifyDangerLevel('TTL key')).toBe('safe');
      expect(classifyDangerLevel('TYPE key')).toBe('safe');
      expect(classifyDangerLevel('EXISTS key')).toBe('safe');
      expect(classifyDangerLevel('SCAN 0')).toBe('safe');
    });

    it('classifies write commands', () => {
      expect(classifyDangerLevel('SET foo bar')).toBe('write');
      expect(classifyDangerLevel('LPUSH list a')).toBe('write');
      expect(classifyDangerLevel('HSET h f v')).toBe('write');
      expect(classifyDangerLevel('ZADD z 1 m')).toBe('write');
      expect(classifyDangerLevel('SADD s a')).toBe('write');
      expect(classifyDangerLevel('XADD stream * k v')).toBe('write');
      expect(classifyDangerLevel('INCR counter')).toBe('write');
    });

    it('classifies danger commands', () => {
      expect(classifyDangerLevel('DEL key')).toBe('danger');
      expect(classifyDangerLevel('EXPIRE key 100')).toBe('danger');
      expect(classifyDangerLevel('RENAME a b')).toBe('danger');
      expect(classifyDangerLevel('PERSIST key')).toBe('danger');
      expect(classifyDangerLevel('SUBSCRIBE ch')).toBe('danger');
      expect(classifyDangerLevel('CLIENT LIST')).toBe('danger');
    });

    it('classifies ultra-danger commands', () => {
      expect(classifyDangerLevel('FLUSHDB')).toBe('ultra-danger');
      expect(classifyDangerLevel('FLUSHALL')).toBe('ultra-danger');
      expect(classifyDangerLevel('KEYS *')).toBe('ultra-danger');
      expect(classifyDangerLevel('CONFIG SET')).toBe('ultra-danger');
      expect(classifyDangerLevel('SHUTDOWN NOSAVE')).toBe('ultra-danger');
    });

    it('is case-insensitive', () => {
      expect(classifyDangerLevel('get foo')).toBe('safe');
      expect(classifyDangerLevel('Set foo bar')).toBe('write');
      expect(classifyDangerLevel('del key')).toBe('danger');
      expect(classifyDangerLevel('flushdb')).toBe('ultra-danger');
    });

    it('handles empty command', () => {
      expect(classifyDangerLevel('')).toBe('safe');
    });
  });

  describe('requiresConfirmation', () => {
    it('requires confirmation for danger and ultra-danger', () => {
      expect(requiresConfirmation('danger')).toBe(true);
      expect(requiresConfirmation('ultra-danger')).toBe(true);
    });

    it('does not require confirmation for safe and write', () => {
      expect(requiresConfirmation('safe')).toBe(false);
      expect(requiresConfirmation('write')).toBe(false);
    });
  });

  describe('dangerBadgeColor', () => {
    it('returns correct colors', () => {
      expect(dangerBadgeColor('ultra-danger')).toContain('bg-red');
      expect(dangerBadgeColor('danger')).toContain('bg-orange');
      expect(dangerBadgeColor('write')).toContain('bg-yellow');
      expect(dangerBadgeColor('safe')).toContain('bg-green');
    });
  });
});
