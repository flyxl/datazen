import { beforeEach, describe, expect, it, vi } from 'vitest';

const mockSaveBase64 = vi.fn().mockResolvedValue(true);
const mockSaveText = vi.fn().mockResolvedValue(true);
const mockToPng = vi.fn().mockResolvedValue('data:image/png;base64,QUJD');
const mockToSvg = vi.fn().mockResolvedValue('data:image/svg+xml;base64,PHN2Zz48L3N2Zz4=');

vi.mock('../../../commands/file', () => ({
  fileCommands: {
    saveBase64WithDialog: (...args: unknown[]) => mockSaveBase64(...args),
    saveTextWithDialog: (...args: unknown[]) => mockSaveText(...args),
  },
}));

vi.mock('html-to-image', () => ({
  toPng: (...args: unknown[]) => mockToPng(...args),
  toSvg: (...args: unknown[]) => mockToSvg(...args),
}));

import {
  exportChartAsPng,
  exportChartAsSvg,
  exportElementAsPng,
  exportElementAsSvg,
} from '../export';

describe('exportChartAsPng', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    document.documentElement.style.setProperty('--c-surface', '#0b1220');
  });

  it('exports chart element as PNG via file dialog', async () => {
    const el = document.createElement('div');
    await exportChartAsPng(el, 'chart-1');
    expect(mockToPng).toHaveBeenCalledWith(el, { backgroundColor: '#0b1220' });
    expect(mockSaveBase64).toHaveBeenCalledWith('QUJD', 'chart-1.png', 'PNG', ['png']);
  });
});

describe('exportElementAsPng', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    document.documentElement.style.setProperty('--c-surface', '#0b1220');
    mockToPng.mockResolvedValue('data:image/png;base64,QUJD');
  });

  it('forwards a caller background and quality (ER diagram path)', async () => {
    const el = document.createElement('div');
    await exportElementAsPng(el, 'er-diagram-db', {
      backgroundColor: '#1a1a2e',
      quality: 1,
    });
    expect(mockToPng).toHaveBeenCalledWith(el, {
      backgroundColor: '#1a1a2e',
      quality: 1,
    });
    expect(mockSaveBase64).toHaveBeenCalledWith('QUJD', 'er-diagram-db.png', 'PNG', ['png']);
  });

  it('defaults to the chart surface color when no background is given', async () => {
    await exportElementAsPng(document.createElement('div'), 'chart-1');
    expect(mockToPng).toHaveBeenCalledWith(expect.anything(), {
      backgroundColor: '#0b1220',
    });
  });

  it('saves a data URL without a comma verbatim', async () => {
    mockToPng.mockResolvedValueOnce('rawbase64');
    await exportElementAsPng(document.createElement('div'), 'chart-1');
    expect(mockSaveBase64).toHaveBeenCalledWith('rawbase64', 'chart-1.png', 'PNG', ['png']);
  });
});

describe('exportElementAsSvg', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    document.documentElement.style.setProperty('--c-surface', '#0b1220');
    mockToSvg.mockResolvedValue('data:image/svg+xml;base64,PHN2Zz48L3N2Zz4=');
  });

  it('decodes a base64 SVG data URL and saves the text (ER diagram path)', async () => {
    const el = document.createElement('div');
    await exportElementAsSvg(el, 'er-diagram-db', { backgroundColor: '#1a1a2e' });
    expect(mockToSvg).toHaveBeenCalledWith(el, { backgroundColor: '#1a1a2e' });
    expect(mockSaveText).toHaveBeenCalledWith('<svg></svg>', 'er-diagram-db.svg', 'SVG', [
      'svg',
    ]);
  });

  it('decodes a percent-encoded SVG data URL', async () => {
    mockToSvg.mockResolvedValueOnce('data:image/svg+xml,%3Csvg%3E%3C%2Fsvg%3E');
    await exportElementAsSvg(document.createElement('div'), 'er-diagram-db');
    expect(mockSaveText).toHaveBeenCalledWith('<svg></svg>', 'er-diagram-db.svg', 'SVG', [
      'svg',
    ]);
  });

  it('saves a non-data-URL result verbatim', async () => {
    mockToSvg.mockResolvedValueOnce('<svg>raw</svg>');
    await exportElementAsSvg(document.createElement('div'), 'er-diagram-db');
    expect(mockSaveText).toHaveBeenCalledWith('<svg>raw</svg>', 'er-diagram-db.svg', 'SVG', [
      'svg',
    ]);
  });

  it('defaults to the chart surface color when no background is given', async () => {
    await exportElementAsSvg(document.createElement('div'), 'er-diagram-db');
    expect(mockToSvg).toHaveBeenCalledWith(expect.anything(), {
      backgroundColor: '#0b1220',
    });
  });
});

describe('exportChartAsSvg', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('serializes inner svg and saves via dialog', async () => {
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
    await exportChartAsSvg(document.createElement('div'), 'empty');
    expect(mockSaveText).not.toHaveBeenCalled();
  });
});
