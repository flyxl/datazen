import { describe, expect, it, beforeEach, afterEach } from 'vitest';
import { act, render, screen } from '@testing-library/react';
import {
  getLocale,
  getRegisteredTranslations,
  registerTranslations,
  setLocale,
  t,
  useI18n,
} from '../i18n';

function Probe() {
  const { t: translate, language } = useI18n();
  return (
    <div>
      <span data-testid="label">{translate('greeting.hello', { name: 'Ada' })}</span>
      <span data-testid="lang">{language}</span>
    </div>
  );
}

describe('@datazen/ui i18n engine (single implementation)', () => {
  registerTranslations({
    en: { 'greeting.hello': 'Hello {name}!', 'settings.plain': 'Plain' },
    'zh-CN': { 'greeting.hello': '你好，{name}！' },
  });

  afterEach(() => {
    setLocale('en');
  });

  it('resolves the active locale and interpolates {param}', () => {
    expect(t('greeting.hello', { name: 'Ada' })).toBe('Hello Ada!');
    setLocale('zh-CN');
    expect(getLocale()).toBe('zh-CN');
    expect(t('greeting.hello', { name: 'Ada' })).toBe('你好，Ada！');
  });

  it('falls back to en then to the raw key', () => {
    setLocale('zh-CN');
    expect(t('settings.plain')).toBe('Plain');
    expect(t('no.such.key', { x: 1 })).toBe('no.such.key');
    // Empty-string values are real translations, not missing keys.
    registerTranslations({ 'zh-CN': { 'option.separator': '' } });
    expect(t('option.separator')).toBe('');
  });

  it('keeps unmatched {tokens} intact during interpolation', () => {
    registerTranslations({ en: { 'partial.tokens': '{a} and {b}' } });
    expect(t('partial.tokens', { a: 'A' })).toBe('A and {b}');
  });

  it('merges repeated registrations (later wins per key)', () => {
    registerTranslations({ 'zh-CN': { 'settings.plain': '朴素' } });
    registerTranslations({ 'zh-CN': { 'settings.plain': '纯文本' } });
    setLocale('zh-CN');
    expect(t('settings.plain')).toBe('纯文本');
  });

  it('exposes a read-only snapshot of the registered dictionary', () => {
    registerTranslations({ 'xx-XX': { 'snapshot.only': 'Snap' } });
    expect(getRegisteredTranslations('xx-XX')).toEqual({ 'snapshot.only': 'Snap' });
    // Mutating the snapshot must not write back into the registry.
    const snapshot = getRegisteredTranslations('xx-XX');
    snapshot['snapshot.only'] = 'tampered';
    delete snapshot['snapshot.only'];
    expect(getRegisteredTranslations('xx-XX')['snapshot.only']).toBe('Snap');
    setLocale('xx-XX');
    expect(t('snapshot.only')).toBe('Snap');
    expect(getRegisteredTranslations('no-such-locale')).toEqual({});
    // Host snapshot covers every registration so far (host + driver packs).
    expect(getRegisteredTranslations('en')['greeting.hello']).toBe('Hello {name}!');
  });

  it('re-renders useI18n consumers when the locale changes', () => {
    render(<Probe />);
    expect(screen.getByTestId('label').textContent).toBe('Hello Ada!');
    expect(screen.getByTestId('lang').textContent).toBe('en');
    act(() => setLocale('zh-CN'));
    expect(screen.getByTestId('label').textContent).toBe('你好，Ada！');
    expect(screen.getByTestId('lang').textContent).toBe('zh-CN');
    act(() => setLocale('en'));
    expect(screen.getByTestId('label').textContent).toBe('Hello Ada!');
  });
});

/**
 * [tester] r-phase B 表 R-8（O-1 裁定回归项，Wave 4-B 独立补齐）
 *
 * 契约出处：docs/development/driver-api-dependency-boundary.md §2.4.1（唯一运行时）、
 * §2.4.3（驱动 eager 注册全部 10 语言 / 宿主运行时只接线 en + zh-CN / 扩展经宿主
 * `registerLocale()` → 共享 `registerTranslations` 引入第 3 语言）、§2.4.4（前缀互斥）。
 *
 * O-1 保留 10 语言档的收益侧断言：驱动字典里那些「宿主运行时尚未接线」的语言
 * （此处以 `de` 代表）必须在扩展引入该 locale 后立即对驱动 key 生效；且扩展注册
 * 同一 locale 时**不得**清掉驱动已注册的词条 —— 「重复注册为后写覆盖合并」是逐 key
 * 合并，不是整本字典替换。
 *
 * 落点说明：本块必须留在 `i18n.test.tsx` 内。护栏 R2 的豁免是**文件级**精确清单
 * （`R2_FILE_CARVEOUTS`），新建 `packages/**` 测试文件调用 `setLocale()` 会被 R2
 * 直接阻断（Wave 4-B 实测红 8 处），因此按契约「它自己的单测」这一条并入本文件。
 */
const DRIVER_PREFIX = 'redis.testerR8';
const EXTENSION_PREFIX = 'chart.testerR8';

/** 模拟驱动包 `locales/index.ts` 的 eager 自注册：含宿主运行时尚未接线的 `de`。 */
function registerTesterDriverPack(): void {
  registerTranslations({
    en: {
      [`${DRIVER_PREFIX}.console`]: 'Console',
      [`${DRIVER_PREFIX}.flush`]: 'Flush DB',
      [`${DRIVER_PREFIX}.enOnly`]: 'English only',
    },
    'zh-CN': { [`${DRIVER_PREFIX}.console`]: '控制台' },
    // 故意不含 enOnly：用于验证 registry[locale] ?? registry['en'] 回落链。
    de: { [`${DRIVER_PREFIX}.console`]: 'Konsole', [`${DRIVER_PREFIX}.flush`]: 'DB leeren' },
  });
}

/** 模拟扩展走宿主 `registerLocale('de', …)` → 共享 `registerTranslations`。 */
function registerTesterExtensionLocale(): void {
  registerTranslations({ de: { [`${EXTENSION_PREFIX}.title`]: 'Diagramm' } });
}

function TesterProbe() {
  const { t: translate, language } = useI18n();
  return (
    <div>
      <span data-testid="tester-driver-console">{translate(`${DRIVER_PREFIX}.console`)}</span>
      <span data-testid="tester-driver-flush">{translate(`${DRIVER_PREFIX}.flush`)}</span>
      <span data-testid="tester-extension-title">{translate(`${EXTENSION_PREFIX}.title`)}</span>
      <span data-testid="tester-lang">{language}</span>
    </div>
  );
}

describe('[tester] R-8 扩展引入第 3 语言时驱动词条命中', () => {
  // 共享注册表是模块级单例：每个用例前重新灌入驱动包并复位语言，用例之间不留
  // 顺序耦合（否则「逐 key 后写覆盖」用例会污染后续用例的 de.console 期望值）。
  beforeEach(() => {
    registerTesterDriverPack();
    setLocale('en');
  });

  afterEach(() => {
    setLocale('en');
  });

  it('驱动 eager 注册的 de 字典先于扩展存在：de 命中、de 缺失 key 回落 en', () => {
    expect(getLocale()).toBe('en');
    expect(t(`${DRIVER_PREFIX}.console`)).toBe('Console');

    setLocale('de');
    expect(getLocale()).toBe('de');
    expect(t(`${DRIVER_PREFIX}.console`)).toBe('Konsole');
    expect(t(`${DRIVER_PREFIX}.flush`)).toBe('DB leeren');
    // de 字典里没有 enOnly → 按契约回落 en，而不是回显 key。
    expect(t(`${DRIVER_PREFIX}.enOnly`)).toBe('English only');
    // 两个前缀都查不到时原样回显 key（防止把「查不到」误判成「命中」）。
    expect(t('no.such.testerR8.key', { x: 1 })).toBe('no.such.testerR8.key');
  });

  it('扩展注册 de 后：驱动词条未被清掉，两个生产方在同一 locale 下并存命中', () => {
    registerTesterExtensionLocale();

    const dePack = getRegisteredTranslations('de');
    expect(dePack[`${DRIVER_PREFIX}.console`]).toBe('Konsole');
    expect(dePack[`${EXTENSION_PREFIX}.title`]).toBe('Diagramm');

    setLocale('de');
    // 核心断言：驱动自有前缀词条在扩展引入的第 3 语言里命中。
    expect(t(`${DRIVER_PREFIX}.console`)).toBe('Konsole');
    // 扩展词条同时命中 —— 若 registerTranslations 是整本替换，上一行会退化成 key 回显。
    expect(t(`${EXTENSION_PREFIX}.title`)).toBe('Diagramm');
  });

  it('后写覆盖是逐 key 合并：扩展同名 key 只覆盖自己那一条', () => {
    registerTranslations({ de: { [`${DRIVER_PREFIX}.console`]: 'Konsole (ext override)' } });
    setLocale('de');
    expect(t(`${DRIVER_PREFIX}.console`)).toBe('Konsole (ext override)');
    // 同一字典里的其它驱动词条必须原样保留。
    expect(t(`${DRIVER_PREFIX}.flush`)).toBe('DB leeren');
  });

  it('useI18n 消费者随第 3 语言切换重渲染，并可完整跃迁回宿主接线语言', () => {
    registerTesterExtensionLocale();
    render(<TesterProbe />);

    expect(screen.getByTestId('tester-lang').textContent).toBe('en');
    expect(screen.getByTestId('tester-driver-console').textContent).toBe('Console');

    act(() => setLocale('de'));
    expect(screen.getByTestId('tester-lang').textContent).toBe('de');
    expect(screen.getByTestId('tester-driver-console').textContent).toBe('Konsole');
    expect(screen.getByTestId('tester-driver-flush').textContent).toBe('DB leeren');
    expect(screen.getByTestId('tester-extension-title').textContent).toBe('Diagramm');

    // 退出跃迁：切回宿主已接线的 zh-CN —— 驱动中文词条命中；flush 未注册中文
    // ⇒ 回落 en；扩展只有 de 词条且 en 里没有 ⇒ 回显 key，不残留上一语言译文。
    act(() => setLocale('zh-CN'));
    expect(screen.getByTestId('tester-lang').textContent).toBe('zh-CN');
    expect(screen.getByTestId('tester-driver-console').textContent).toBe('控制台');
    expect(screen.getByTestId('tester-driver-flush').textContent).toBe('Flush DB');
    expect(screen.getByTestId('tester-extension-title').textContent).toBe(
      `${EXTENSION_PREFIX}.title`,
    );

    act(() => setLocale('en'));
    expect(screen.getByTestId('tester-lang').textContent).toBe('en');
    expect(screen.getByTestId('tester-driver-console').textContent).toBe('Console');
  });
});
