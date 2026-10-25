import { describe, expect, it, vi, afterEach } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { BinaryViewer, hexRows } from '../index';

afterEach(cleanup);

const PNG_HEAD = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00];

describe('hexRows (DB-13 shared renderer)', () => {
  it('renders 16 bytes per row with an ASCII gutter and an offset', () => {
    const bytes = new Uint8Array([...PNG_HEAD, ...Array.from({ length: 14 }, (_, i) => 0x41 + i)]);
    const rows = hexRows(bytes);
    expect(rows).toHaveLength(2);
    expect(rows[0]).toEqual({
      offset: 0,
      hex: '89 50 4e 47 0d 0a 1a 0a 00 00 41 42 43 44 45 46',
      // Controls render as '.', printable ASCII as itself.
      ascii: '.PNG......ABCDEF',
    });
    expect(rows[1]!.offset).toBe(16);
  });

  it('bounds the rows for multi-MB payloads', () => {
    const rows = hexRows(new Uint8Array(64 * 1024 * 4), 10);
    expect(rows).toHaveLength(10);
  });
});

describe('@datazen/ui BinaryViewer (DB-13)', () => {
  it('renders the hex dump and the byte count', () => {
    render(<BinaryViewer bytes={new Uint8Array([0xde, 0xad, 0xbe, 0xef])} />);

    expect(screen.getByTestId('binary-viewer-hex-bytes')).toHaveTextContent('de ad be ef');
    expect(screen.getByTestId('binary-viewer-size')).toHaveTextContent('binaryViewer.bytes');
    expect(screen.queryByTestId('binary-viewer-image')).toBeNull();
    expect(screen.queryByTestId('binary-viewer-save')).toBeNull();
  });

  it('previews a payload with a known image signature inline', () => {
    render(<BinaryViewer bytes={new Uint8Array(PNG_HEAD)} />);

    const image = screen.getByTestId('binary-viewer-image');
    const img = image.querySelector('img');
    expect(img).not.toBeNull();
    expect(img!.getAttribute('src')).toContain('data:image/png;base64,');
    expect(screen.getByTestId('binary-viewer-kind')).toHaveTextContent('image/png');
  });

  it('hands the bytes and a sniffed file name to the save exit', () => {
    const onSaveAsFile = vi.fn();
    render(
      <BinaryViewer
        bytes={new Uint8Array([0xff, 0xd8, 0xff, 0xe0])}
        fileName="photo"
        onSaveAsFile={onSaveAsFile}
      />,
    );

    fireEvent.click(screen.getByTestId('binary-viewer-save'));
    expect(onSaveAsFile).toHaveBeenCalledOnce();
    const [bytes, name] = onSaveAsFile.mock.calls[0] as [Uint8Array, string];
    expect(bytes).toBeInstanceOf(Uint8Array);
    // The JPEG signature names the extension — the host dialog needs one.
    expect(name).toBe('photo.jpg');
  });

  it('says how much of a huge payload the bounded dump covers', () => {
    render(<BinaryViewer bytes={new Uint8Array(1024 * 1024)} />);

    const truncated = screen.getByTestId('binary-viewer-hex-truncated');
    expect(truncated).toHaveTextContent('binaryViewer.hexTruncated');
  });
});
