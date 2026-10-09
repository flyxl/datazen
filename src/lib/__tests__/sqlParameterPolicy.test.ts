import { describe, expect, it } from 'vitest';
import { resolveSqlParameterPolicy } from '../sqlDialects/sqlParameterPolicy';
import { parseSqlParams } from '../sqlBindParams';
import type { SqlParameterStrategy } from '../sqlDialects/types';

describe('resolveSqlParameterPolicy', () => {
  it('uses the selected driver policy and strategy', () => {
    const strategy: SqlParameterStrategy = {
      filterOccurrences: (_sql, occurrences) => occurrences,
    };
    const policy = resolveSqlParameterPolicy('sqlserver', {
      sqlserver: {
        sqlDialectProfile: {
          parameterPolicy: {
            atNamed: true,
            question: false,
            dollarPositional: false,
            template: false,
          },
        },
        sqlParameterStrategy: strategy,
      },
    });

    expect(policy).toEqual({
      enableAt: true,
      enableQuestion: false,
      enableDollarPositional: false,
      enableDollarNamed: false,
      enableTemplate: false,
      strategy,
    });
  });

  it('allows a focused driver policy when there is no full dialect profile', () => {
    expect(
      resolveSqlParameterPolicy('turso', {
        turso: {
          sqlParameterPolicy: {
            atNamed: true,
            question: true,
            dollarPositional: false,
            dollarNamed: true,
            template: false,
          },
        },
      }),
    ).toEqual({
      enableAt: true,
      enableQuestion: true,
      enableDollarPositional: false,
      enableDollarNamed: true,
      enableTemplate: false,
      strategy: undefined,
    });
  });

  it('uses only shared placeholder forms when a driver declares no native policy', () => {
    expect(resolveSqlParameterPolicy('generic', {})).toEqual({
      enableAt: false,
      enableQuestion: false,
      enableDollarPositional: false,
      enableDollarNamed: false,
      enableTemplate: true,
      strategy: undefined,
    });
  });

  it('does not interpret MySQL session variables as editor parameters', () => {
    const policy = resolveSqlParameterPolicy('mysql', {
      mysql: {
        sqlDialectProfile: {
          parameterPolicy: {
            atNamed: false,
            question: true,
            dollarPositional: false,
            template: false,
          },
        },
      },
    });

    expect(parseSqlParams('SELECT @session_var, ?', policy).map((param) => param.stableId)).toEqual(
      ['question:1'],
    );
  });

  it('does not interpret PostgreSQL JSON question operators as bind markers', () => {
    const policy = resolveSqlParameterPolicy('postgresql', {
      postgresql: {
        sqlDialectProfile: {
          parameterPolicy: {
            atNamed: false,
            question: false,
            dollarPositional: true,
            template: false,
          },
        },
      },
    });

    expect(
      parseSqlParams("SELECT data ? 'key', $1", policy).map((param) => param.stableId),
    ).toEqual(['dollar:1']);
  });
});
