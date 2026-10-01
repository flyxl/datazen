import { transitionalPlatformServices, transitionalTransport } from './desktopBinding';

export interface OpenedBinaryFile {
  fileName: string;
  dataBase64: string;
}

/**
 * Native-dialog file IO shared by drivers (e.g. Redis dump import/export).
 * Dialog + read/write happen atomically on the Rust side; paths never reach JS.
 *
 * Thin re-export over `PlatformServices` (§7.2 薄再导出过渡): the exported
 * names, parameter lists and return types are unchanged, so every caller is
 * untouched. Positional parameters are assembled into the named input objects
 * `PlatformServices` declares.
 *
 * `openBase64WithDialog` is the one exception and is routed through the
 * transport rather than `PlatformServices`: §7.1 declares text-open and
 * directory-open only, so there is no binary-open capability to delegate to.
 * Adding a fake one here would put an operation in the contract that the
 * architecture document does not have.
 */
export const fileCommands = {
  /** Save UTF-8 text via native OS dialog. Returns false if cancelled. */
  saveTextWithDialog: (
    contents: string,
    defaultFileName: string,
    filterName: string,
    extensions: string[],
  ) =>
    transitionalPlatformServices().saveTextWithDialog({
      contents,
      defaultFileName,
      filterName,
      extensions,
    }),

  /** Save base64 bytes via native OS dialog. Returns false if cancelled. */
  saveBase64WithDialog: (
    dataBase64: string,
    defaultFileName: string,
    filterName: string,
    extensions: string[],
  ) =>
    transitionalPlatformServices().saveBinaryWithDialog({
      dataBase64,
      defaultFileName,
      filterName,
      extensions,
    }),

  /** Open a binary file via native dialog; returns basename + base64 (no path). */
  openBase64WithDialog: (filterName: string, extensions: string[]) =>
    transitionalTransport().call('open_base64_with_dialog', {
      filterName,
      extensions,
    }) as Promise<OpenedBinaryFile | null>,
};