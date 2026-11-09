import { fileCommands } from '../../commands/file';
import { readCssColor } from './colors';

export interface ElementExportOptions {
  /** Background painted behind the element; defaults to the chart surface color. */
  backgroundColor?: string;
  /** PNG only: html-to-image output quality (0–1). Omitted ⇒ html-to-image default. */
  quality?: number;
}

function chartBackground(): string {
  return readCssColor('--c-surface', '#0b0e14');
}

/**
 * The one DOM→PNG path for charts and the ER diagram: html-to-image renders
 * the element, the data URL is split to base64 and the host save dialog writes
 * the file. Callers that need a different backdrop or quality pass them in;
 * the defaults are the chart's.
 */
export async function exportElementAsPng(
  element: HTMLElement,
  filename: string,
  options?: ElementExportOptions,
): Promise<void> {
  const { toPng } = await import('html-to-image');
  const dataUrl = await toPng(element, {
    backgroundColor: options?.backgroundColor ?? chartBackground(),
    ...(options?.quality !== undefined ? { quality: options.quality } : {}),
  });
  const base64 = dataUrl.includes(',') ? dataUrl.split(',')[1]! : dataUrl;
  await fileCommands.saveBase64WithDialog(base64, `${filename}.png`, 'PNG', ['png']);
}

/**
 * The DOM→SVG path for elements that are not a single `<svg>` (the ER
 * diagram's viewport is HTML nodes plus SVG edges): html-to-image renders the
 * element to a data URL and the decoded SVG text goes through the save dialog.
 */
export async function exportElementAsSvg(
  element: HTMLElement,
  filename: string,
  options?: Pick<ElementExportOptions, 'backgroundColor'>,
): Promise<void> {
  const { toSvg } = await import('html-to-image');
  const dataUrl = await toSvg(element, {
    backgroundColor: options?.backgroundColor ?? chartBackground(),
  });
  let svgContent: string;
  if (dataUrl.startsWith('data:image/svg+xml;base64,')) {
    svgContent = atob(dataUrl.slice('data:image/svg+xml;base64,'.length));
  } else if (dataUrl.startsWith('data:image/svg+xml,')) {
    svgContent = decodeURIComponent(dataUrl.slice(dataUrl.indexOf(',') + 1));
  } else {
    svgContent = dataUrl;
  }
  await fileCommands.saveTextWithDialog(svgContent, `${filename}.svg`, 'SVG', ['svg']);
}

export async function exportChartAsPng(chartElement: HTMLElement, filename: string): Promise<void> {
  await exportElementAsPng(chartElement, filename);
}

/**
 * Chart SVG stays on XMLSerializer rather than `exportElementAsSvg`: a chart
 * element IS a single `<svg>`, so serializing it is exact, while the ER
 * diagram's viewport needs html-to-image's `toSvg` to capture its HTML nodes.
 * Same save-dialog call, different rasterizer — deliberately not unified.
 */
export async function exportChartAsSvg(chartElement: HTMLElement, filename: string): Promise<void> {
  const svgEl = chartElement.querySelector('svg');
  if (!svgEl) return;

  const svgData = new XMLSerializer().serializeToString(svgEl);
  await fileCommands.saveTextWithDialog(svgData, `${filename}.svg`, 'SVG', ['svg']);
}
