import { beforeEach, describe, expect, it, vi } from 'vitest';

const mockSaveBase64 = vi.fn().mockResolvedValue(true);
const mockSaveText = vi.fn().mockResolvedValue(true);

vi.mock('../../../commands/file', () => ({
  fileCommands: {
    saveBase64WithDialog: (...args: unknown[]) => mockSaveBase64(...args),
    saveTextWithDialog: (...args: unknown[]) => mockSaveText(...args),
  },
}));

vi.mock('html-to-image', () => ({
  toPng: vi.fn().mockResolvedValue('data:image/png;base64,QUJD'),
}));

describe('exportChartAsPng', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    document.documentElement.style.setProperty('--c-surface', '#1a1a2e');
  });

  it('exports chart element as PNG via file dialog', async () => {
    const { exportChartAsPng } = await import('../export');
    const el = document.createElement('div');
    await exportChartAsPng(el, 'chart-1');
    expect(mockSaveBase64).toHaveBeenCalledWith('QUJD', 'chart-1.png', 'PNG', ['png']);
  });
});

describe('exportChartAsSvg', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.resetModules();
  });

  it('serializes inner svg and saves via dialog', async () => {
    const { exportChartAsSvg } = await import('../export');
    const el = document.createElement('div');
    const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
    svg.innerHTML = '<rect width="10" height="10"/>';
    el.appendChild(svg);
    await exportChartAsSvg(el, 'chart-svg');
    expect(mockSaveText).toHaveBeenCalledWith(
      expect.stringContaining('<svg'),
      'chart-svg.svg',
      'SVG',
      ['svg'],
    );
  });

  it('no-ops when svg is missing', async () => {
    const { exportChartAsSvg } = await import('../export');
    await exportChartAsSvg(document.createElement('div'), 'empty');
    expect(mockSaveText).not.toHaveBeenCalled();
  });
});
