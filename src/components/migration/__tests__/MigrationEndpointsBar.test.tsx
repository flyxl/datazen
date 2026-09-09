import { describe, expect, it, beforeAll } from 'vitest';
import { render, screen } from '@testing-library/react';
import { MigrationEndpointsBar } from '../MigrationEndpointsBar';
import { ensureAllLazyDomains } from '../../../locales/lazyPacks';
import { useSettingsStore } from '../../../stores/settingsStore';

describe('MigrationEndpointsBar i18n resolution', () => {
  beforeAll(async () => {
    await ensureAllLazyDomains('zh-CN');
    await ensureAllLazyDomains('en');
    useSettingsStore.setState({
      settings: {
        ...useSettingsStore.getState().settings,
        language: 'zh-CN',
      },
    });
  });

  it('renders translated labels and placeholders for schema-diff prefix without raw keys', () => {
    const { container } = render(
      <MigrationEndpointsBar
        testIdPrefix="schema-diff"
        sourceId=""
        targetId=""
        sourceDatabase=""
        targetDatabase=""
        sourceDatabases={[]}
        targetDatabases={[]}
        connOptions={[]}
        targetOptions={[]}
        onSourceChange={() => {}}
        onTargetChange={() => {}}
        onSourceDatabaseChange={() => {}}
        onTargetDatabaseChange={() => {}}
      />,
    );

    const textContent = container.textContent ?? '';
    // Labels must NOT be raw keys like SCHEMADIFF.SOURCE or SCHEMADIFF.TARGET
    expect(textContent).not.toContain('SCHEMADIFF.SOURCE');
    expect(textContent).not.toContain('SCHEMADIFF.TARGET');
    expect(textContent).not.toContain('schemaDiff.source');
    expect(textContent).not.toContain('schemaDiff.target');

    // Placeholders must NOT be raw keys
    const rawKeys = container.querySelectorAll('*');
    for (const el of Array.from(rawKeys)) {
      const placeholder = el.getAttribute('placeholder');
      if (placeholder) {
        expect(placeholder).not.toContain('schemaDiff.');
        expect(placeholder).not.toContain('sync.');
        expect(placeholder).not.toContain('transfer.');
      }
    }

    // Verify correct translations in zh-CN
    expect(screen.getByText('源数据库')).toBeDefined();
    expect(screen.getByText('目标数据库')).toBeDefined();
    expect(screen.getAllByText('选择数据库').length).toBeGreaterThanOrEqual(2);
  });

  it('renders translated labels and placeholders for data-sync prefix without raw keys', () => {
    const { container } = render(
      <MigrationEndpointsBar
        testIdPrefix="data-sync"
        sourceId=""
        targetId=""
        sourceDatabase=""
        targetDatabase=""
        sourceDatabases={[]}
        targetDatabases={[]}
        connOptions={[]}
        targetOptions={[]}
        onSourceChange={() => {}}
        onTargetChange={() => {}}
        onSourceDatabaseChange={() => {}}
        onTargetDatabaseChange={() => {}}
      />,
    );

    const textContent = container.textContent ?? '';
    expect(textContent).not.toContain('SYNC.DATABASE');
    expect(textContent).not.toContain('sync.database');

    for (const el of Array.from(container.querySelectorAll('*'))) {
      const placeholder = el.getAttribute('placeholder');
      if (placeholder) {
        expect(placeholder).not.toBe('sync.selectDatabase');
      }
    }
  });

  it('renders translated labels and placeholders for data-transfer prefix without raw keys', () => {
    const { container } = render(
      <MigrationEndpointsBar
        testIdPrefix="data-transfer"
        sourceId=""
        targetId=""
        sourceDatabase=""
        targetDatabase=""
        sourceDatabases={[]}
        targetDatabases={[]}
        connOptions={[]}
        targetOptions={[]}
        onSourceChange={() => {}}
        onTargetChange={() => {}}
        onSourceDatabaseChange={() => {}}
        onTargetDatabaseChange={() => {}}
      />,
    );

    const textContent = container.textContent ?? '';
    expect(textContent).not.toContain('TRANSFER.DATABASE');
    expect(textContent).not.toContain('transfer.database');

    for (const el of Array.from(container.querySelectorAll('*'))) {
      const placeholder = el.getAttribute('placeholder');
      if (placeholder) {
        expect(placeholder).not.toBe('transfer.selectDatabase');
      }
    }
  });
});
