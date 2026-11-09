/**
 * Base64 of the wire's byte payloads (DBR-05).
 *
 * The native file dialogs exchange binary as base64 because the value has to
 * cross the IPC boundary as JSON text; the grid's own byte payloads travel
 * as untagged number arrays. These two helpers are the only places that
 * conversion happens, so the padding rules cannot drift between callers.
 */

export function bytesToBase64(bytes: Uint8Array): string {
  let binary = '';
  const chunk = 0x8000;
  for (let i = 0; i < bytes.length; i += chunk) {
    binary += String.fromCharCode(...bytes.subarray(i, i + chunk));
  }
  return btoa(binary);
}

export function base64ToBytes(dataBase64: string): Uint8Array {
  const binary = atob(dataBase64);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i += 1) {
    bytes[i] = binary.charCodeAt(i);
  }
  return bytes;
}
